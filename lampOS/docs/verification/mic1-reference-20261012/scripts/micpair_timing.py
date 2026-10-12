"""When the lamp's own voice reaches the microphone signal, against when the canceller was given that render.
argv: RUNTIME PAIR.json. Reads only."""
import json, sys
import numpy as np
from scipy import signal
from scipy.linalg import solve_toeplitz
rt, pair = sys.argv[1:3]
p = json.load(open(pair))
first = p['reply']['first_render_sample']; COPY = 107_760
cap = []
for line in open(rt + '/capture-audio/events.jsonl'):
    if '"capture"' not in line: continue
    e = json.loads(line)
    if e.get('kind') == 'capture':
        m = e['meta']; r = m['reference']
        cap.append((m['read_completed_at_us'], r['analysed_through'], r['accepted_through'], r['analysed_at_us'], r['queued_frames'], m['aec_queue_delay_ms'], m.get('aec_internal_alignment_ms'), m['delayed_frames']))
cap = np.array([[np.nan if v is None else v for v in row] for row in cap], dtype=np.float64)
ren = []
for line in open(rt + '/render-audio/events.jsonl'):
    if '"render_accepted"' not in line: continue
    e = json.loads(line)
    if e.get('kind') == 'render_accepted' and e['frames']:
        ren.append((e['meta']['first_sample'], e['meta']['accepted_at_us'], e['meta']['queued_frames'], 0 if e['accepted_silence_kind'] is None else 1))
ren = np.array(ren, dtype=np.float64)
pre = np.fromfile(rt + '/capture-audio/pre_aec.pcm16le', dtype='<i2').astype(np.float64)
render = np.fromfile(rt + '/render-audio/render_accepted.pcm16le', dtype='<i2').astype(np.float64)
reply = render[first:first + COPY]
lv = lambda x: 10 * np.log10(np.maximum((x[:len(x) // 160 * 160].reshape(-1, 160) ** 2).mean(axis=1), 1e-3) / 32768.0 ** 2)
dpre = lv(pre)
# first render block of the reply that is not near-silent
rl = 10 * np.log10(np.maximum((reply[:len(reply) // 240 * 240].reshape(-1, 240) ** 2).mean(axis=1), 1e-3) / 32768.0 ** 2)
onset_block = int(np.argmax(rl > -40)); onset_frame = first + onset_block * 240
acc_row = ren[np.searchsorted(ren[:, 0], onset_frame, side='right') - 1]
accepted_us = acc_row[1] + (onset_frame - acc_row[0]) / 24000 * 1e6 * 0   # the block's acceptance time
i_analysed = int(np.argmax(cap[:, 1] > onset_frame))      # first capture block processed after the canceller had this render
i_accepted = int(np.argmax(cap[:, 2] > onset_frame))
start = int(np.argmax(cap[:, 1] >= first))
idle = np.percentile(dpre[max(start - 200, 0):start - 20], 50)
j = start - 30 + int(np.argmax(dpre[start - 30:start + 60] > idle + 15))   # first capture block where the lamp's voice is in the microphone
out = {'reply_first_loud_render_frame': onset_frame, 'into_the_reply_ms': onset_block * 10,
       'render_block_accepted_at_us': int(acc_row[1]), 'speaker_queue_at_acceptance_frames': int(acc_row[2]),
       'first_capture_block_the_canceller_had_it_for': {'index': i_analysed, 'read_completed_at_us': int(cap[i_analysed, 0])},
       'first_capture_block_with_the_lamp_voice': {'index': j, 'read_completed_at_us': int(cap[j, 0]), 'level_dbfs': round(float(dpre[j]), 1), 'idle_dbfs': round(float(idle), 1)},
       'voice_in_the_microphone_before_the_canceller_had_its_render_blocks': i_analysed - j,
       'voice_block_read_after_render_acceptance_ms': round((cap[j, 0] - acc_row[1]) / 1000, 1),
       'canceller_given_the_render_after_acceptance_ms': round((cap[i_analysed, 0] - acc_row[1]) / 1000, 1),
       'accepted_minus_analysed_frames_percentiles_5_50_95': [float(v) for v in np.percentile((cap[:, 2] - cap[:, 1])[start:start + 1400], [5, 50, 95])],
       'delay_hint_ms_percentiles_5_50_95': [float(v) for v in np.percentile(cap[start:start + 1400, 5], [5, 50, 95])],
       'aec_internal_alignment_ms_values': sorted(set(int(v) for v in cap[start:start + 1400, 6] if not np.isnan(v)))[:12],
       'capture_delayed_frames_percentiles_5_50_95': [float(v) for v in np.percentile(cap[start:start + 1400, 7], [5, 50, 95])]}
print('levels around the onset, capture block: level dBFS, analysed_through - onset frame')
print([(int(k), round(float(dpre[k]), 1), int(cap[k, 1] - onset_frame)) for k in range(j - 4, j + 8)])
# least-squares filter over a wide window either side of the cursor
reply16 = signal.resample_poly(reply, 2, 3)
n0 = int(round(p['reply']['copies_start_at_capture_s'][0] * 16000))
L, LEAD = 4096, 2048
x = reply16[:len(reply16) - 1600]; N = len(x)
y = pre[n0:n0 + N]
r = signal.correlate(x, x, mode='full')[N - 1:N - 1 + L]
pc = signal.correlate(np.concatenate([np.zeros(L), y, np.zeros(L)]), x, mode='valid')
h = solve_toeplitz(r + np.r_[r[0] * 1e-6, np.zeros(L - 1)], pc[L - LEAD:L - LEAD + L])
est = signal.fftconvolve(x, h)[LEAD:LEAD + N]
db = lambda v: 10 * np.log10(max(float(np.mean(v * v)), 1e-3) / 32768.0 ** 2)
e = (y - est)[4800:N - 4800]
energy = np.cumsum(h * h) / np.sum(h * h)
out['linear_filter_256ms_window'] = {'removed_db': round(db(y[4800:N - 4800]) - db(e), 1), 'peak_ms_after_the_cursor': round((int(np.argmax(np.abs(h))) - LEAD) / 16, 1),
                                     'half_of_its_energy_by_ms': round((int(np.argmax(energy >= 0.5)) - LEAD) / 16, 1),
                                     'tenth_of_its_energy_by_ms': round((int(np.argmax(energy >= 0.1)) - LEAD) / 16, 1)}
print(json.dumps(out, indent=1))
