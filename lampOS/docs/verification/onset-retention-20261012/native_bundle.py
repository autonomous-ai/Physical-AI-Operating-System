"""What the device's microphone, canceller and detector did with a cached
clip played over the lamp's own reply. python3.12 -I with NumPy and SciPy.
argv: RUNTIME_DIR LAB_FEATURES.tsv COFFEE.wav COFFEE_SHA SUNSET.wav SUNSET_SHA COFFEE_NOMINAL_START_LAMP_US OUT.json SECONDS_BETWEEN_THE_TWO_SOURCE_STARTS [LAMP_ALONE_FROM_SECOND|-] [MIXERS.json]
Reads only. Nothing is uploaded and no clip is changed."""
import sys, json, wave, hashlib
import numpy as np
from scipy import signal
runtime, features, coffee_path, coffee_sha, sunset_path, sunset_sha, nominal, out = sys.argv[1:9]
nominal = int(nominal)
BETWEEN_SOURCE_STARTS_S = float(sys.argv[9])
LAMP_ALONE_ARG = sys.argv[10] if len(sys.argv) > 10 else '-'
MIXERS = sys.argv[11] if len(sys.argv) > 11 else None
def clip(path, sha):
    raw = open(path, 'rb').read(); assert hashlib.sha256(raw).hexdigest() == sha, 'clip digest ' + path
    w = wave.open(path, 'rb'); assert (w.getframerate(), w.getnchannels(), w.getsampwidth()) == (16000, 1, 2)
    return np.frombuffer(w.readframes(w.getnframes()), dtype='<i2').astype(np.float64)
def blk_db(x, hop=160):
    x = x[:len(x) // hop * hop].reshape(-1, hop); return 10 * np.log10(np.maximum((x * x).mean(axis=1), 1e-3) / 32768.0 ** 2)
def stretches(x):
    db = blk_db(x); act = db > db.max() - 35; segs = []; i = 0; m = len(db)
    while i < m:
        if act[i]:
            j = i
            while j < m and (act[j] or (j + 15 < m and act[j:j + 15].any())): j += 1
            if j - i >= 6: segs.append((i * 10, j * 10))
            i = j
        else: i += 1
    return segs
coffee, sunset = clip(coffee_path, coffee_sha), clip(sunset_path, sunset_sha)
cseg, sseg = stretches(coffee), stretches(sunset)
coffee = coffee[:cseg[-1][1] * 16]; sunset = sunset[:sseg[-1][1] * 16]
rows = []
for line in open(runtime + '/capture-audio/events.jsonl'):
    if 'capture' not in line: continue
    e = json.loads(line)
    if e.get('kind') != 'capture': continue
    m = e['meta']; rows.append((m['frame_sequence'], m['read_completed_at_us'], m.get('vad_score'), e['pre_aec_byte_offset']))
seq = np.array([r[0] for r in rows]); read = np.array([r[1] for r in rows], dtype=np.int64); vad = np.array([np.nan if r[2] is None else r[2] for r in rows])
pre = np.fromfile(runtime + '/capture-audio/pre_aec.pcm16le', dtype='<i2').astype(np.float64)
post = np.fromfile(runtime + '/capture-audio/post_aec.pcm16le', dtype='<i2').astype(np.float64)
f = [l.rstrip('\n').split('\t') for l in open(features)]; head = f[0]; F = {h: np.array([float(r[i]) for r in f[1:]]) for i, h in enumerate(head)}
def sample_of(lamp_us):                                  # sample index in the recordings of a lamp-clock time
    k = int(np.clip(np.searchsorted(read, lamp_us), 0, len(read) - 1))
    return rows[k][3] // 2 + 160 - int(round((read[k] - lamp_us) * 16 / 1000))
def find(x, lo, hi):
    """Where x sits in the microphone signal between samples lo and hi: the peak of the matched filter."""
    seg = pre[lo:hi + len(x)]
    c = signal.correlate(seg, x, mode='valid', method='fft')
    e = np.sqrt(signal.correlate(seg ** 2, np.ones(len(x)), mode='valid', method='fft').clip(1e-9)) * np.sqrt((x ** 2).sum())
    r = c / e; k = int(np.argmax(r)); srt = np.sort(r)
    return lo + k, float(r[k]), float(r[k] / max(srt[int(0.99 * len(srt))], 1e-9))
def powers(x, y, nper):
    """Power of y, and the part of it coherent with x (bias of the estimate taken off)."""
    n = min(len(x), len(y)); x, y = x[:n], y[:n]
    fr, cxy = signal.coherence(x, y, fs=16000, nperseg=nper, noverlap=nper // 2)
    _, pyy = signal.welch(y, fs=16000, nperseg=nper, noverlap=nper // 2)
    k = max(1, (n - nper // 2) // (nper // 2)); bias = 1.0 / k
    g = np.clip((cxy - bias) / (1 - bias), 0, 1); band = (fr >= 200) & (fr <= 4000)
    return float(pyy[band].sum()), float((g * pyy)[band].sum()), k
def db(a, b): return round(float(10 * np.log10(max(a, 1e-12) / max(b, 1e-12))), 1)
res = {'clips': {'coffee_speech_ms': cseg, 'sunset_speech_ms': sseg}}
# the sunset question, with the lamp silent: where it is, and how loud the same loudspeaker arrives at the microphone
s0, peak, sharp = find(sunset, 0, sample_of(nominal) - len(sunset) if sample_of(nominal) > len(sunset) else 16000 * 8)
noise = float(np.median((pre[max(0, s0 - 16000):s0 - 1600].reshape(-1, 160) ** 2).mean(axis=1))) if s0 > 17600 else float(np.percentile((pre[:len(pre)//160*160].reshape(-1,160)**2).mean(axis=1), 5))
sp = float((pre[s0:s0 + len(sunset)] ** 2).mean()); cp = float((sunset ** 2).mean())
gain = (sp - noise) / cp
res['sunset_in_the_clear'] = {'found_at_sample': s0, 'frame_sequence': int(seq[min(len(seq) - 1, s0 // 160)]), 'matched_filter_peak': round(peak, 3), 'peak_over_99th_percentile': round(sharp, 1),
    'microphone_level_dbfs': db(sp, 32768.0 ** 2), 'room_floor_dbfs': db(noise, 32768.0 ** 2), 'loudspeaker_to_microphone_gain_db': db(gain, 1.0)}
for nper in (512, 2048):
    a = powers(sunset, pre[s0:s0 + len(sunset)], nper); b = powers(sunset, post[s0 + 144:s0 + 144 + len(sunset)], nper)
    res['sunset_in_the_clear'][f'coherent_{nper}'] = {'share_of_microphone_db': db(a[1], a[0]), 'share_after_cancellation_db': db(b[1], b[0]),
        'canceller_changes_the_speech_by_db': db(b[1], a[1]), 'segments': a[2]}
# the coffee clip, with the lamp speaking
n0 = sample_of(nominal)
# The two clips came from the same player chain: the coffee clip is placed from the sunset clip's
# measured position and the difference of their source-start times on the host clock.
free, free_peak, free_sharp = find(coffee, n0 - 1600, n0 + 16000)
c0 = s0 + int(round(BETWEEN_SOURCE_STARTS_S * 16000))
near, peak, sharp = find(coffee, c0 - 480, c0 + 480)
res['coffee_over_the_reply'] = {'nominal_start_sample': n0, 'placed_at_sample': c0, 'placed_after_nominal_ms': round((c0 - n0) / 16, 1),
    'placement': 'sunset position plus the difference of the two source-start times on the host clock',
    'matched_filter_peak_within_30ms_of_the_placement': round(peak, 3), 'at_ms_from_the_placement': round((near - c0) / 16, 1),
    'unconstrained_search': {'found_ms_after_nominal': round((free - n0) / 16, 1), 'peak': round(free_peak, 3), 'peak_over_99th_percentile': round(free_sharp, 1),
                             'note': 'no clear peak anywhere within a second: the clip cannot be located from the microphone signal alone'},
    'first_frame_sequence': int(seq[c0 // 160])}
tot = float((pre[c0:c0 + len(coffee)] ** 2).mean()); person = gain * float((coffee ** 2).mean())
res['coffee_over_the_reply'].update({'microphone_level_dbfs': db(tot, 32768.0 ** 2),
    'person_level_predicted_from_the_sunset_gain_dbfs': db(person, 32768.0 ** 2),
    'person_against_everything_else_at_the_microphone_db': db(person, max(tot - person, 1e-9)),
    'after_cancellation_level_dbfs': db(float((post[c0 + 144:c0 + 144 + len(coffee)] ** 2).mean()), 32768.0 ** 2)})
for nper in (512, 2048):
    a = powers(coffee, pre[c0:c0 + len(coffee)], nper); b = powers(coffee, post[c0 + 144:c0 + 144 + len(coffee)], nper)
    res['coffee_over_the_reply'][f'coherent_{nper}'] = {'share_of_microphone_db': db(a[1], a[0]), 'share_after_cancellation_db': db(b[1], b[0]),
        'canceller_changes_the_speech_by_db': db(b[1], a[1]), 'segments': a[2]}
# per block over the clip: the device's own score, and the replay's hearing
b0 = c0 // 160; b1 = min(len(seq), (c0 + len(coffee)) // 160 + 1)
act = np.array([any(lo <= (k - b0) * 10 < hi for lo, hi in cseg) for k in range(b0, b1)])
v = vad[b0:b1]; h = F['heard'][b0:b1].astype(int); lv = F['vad'][b0:b1]
def longest(mask):
    best = run = 0
    for x in mask:
        run = run + 1 if x else 0; best = max(best, run)
    return best
res['per_block_over_the_clip'] = {'frame_sequences': [int(seq[b0]), int(seq[b1 - 1])], 'blocks': int(b1 - b0), 'speech_blocks_of_the_clip': int(act.sum()),
    'device_score': {'under_0_5': int((v < 0.5).sum()), 'from_0_5': int((v >= 0.5).sum()), 'from_0_8': int((v >= 0.8).sum()),
                     'under_0_5_in_speech_blocks': int(((v < 0.5) & act).sum()), 'longest_run_from_0_5': longest(v >= 0.5), 'longest_run_from_0_8': longest(v >= 0.8),
                     'string': ''.join('.' if x < 0.5 else ('s' if x < 0.8 else 'S') for x in v)},
    'replay_hearing': {'under': int((h == 2).sum()), 'own': int((h == 3).sum()), 'over': int((h == 4).sum()), 'unknown': int((h == 5).sum()), 'clear_or_still': int(((h == 0) | (h == 1)).sum()),
                       'longest_over_run': longest(h == 4), 'string': ''.join('SCuOVx'[c] for c in h),
                       'capture_explained_percent_median_where_speech_scored': float(np.median(F['capture_explained'][b0:b1][lv >= 0.5])) if (lv >= 0.5).any() else None},
    'replay_score_against_device_score': {'correlation': round(float(np.corrcoef(v, lv)[0, 1]), 3), 'both_from_0_5': int(((v >= 0.5) & (lv >= 0.5)).sum()),
                                          'device_only': int(((v >= 0.5) & (lv < 0.5)).sum()), 'replay_only': int(((v < 0.5) & (lv >= 0.5)).sum())},
    'clip_string': ''.join('#' if a else ' ' for a in act), 'legend': 'device: . under 0.5, s 0.5 to 0.8, S 0.8 and over; replay: S still, C clear, u under, O own, V over, x unknown; clip: # speech'}
# the same detector on the sunset question in the clear, for comparison
sb0 = s0 // 160; sb1 = (s0 + len(sunset)) // 160 + 1; sv = vad[sb0:sb1]
res['sunset_in_the_clear']['device_score'] = {'blocks': int(sb1 - sb0), 'from_0_5': int((sv >= 0.5).sum()), 'from_0_8': int((sv >= 0.8).sum()), 'string': ''.join('.' if x < 0.5 else ('s' if x < 0.8 else 'S') for x in sv)}
# --- the part of the clip the lamp spoke through: to the first yield inside the clip, else all of it
OVERLAP_MS = cseg[-1][1]
_trace = runtime + '/events.jsonl'
import os
if os.path.exists(_trace):
    for line in open(_trace):
        if 'duplex_yield' in line:
            e = json.loads(line)
            if e.get('kind') == 'duplex_yield':
                t = (e['frame_sequence'] - int(seq[c0 // 160])) * 10
                if 300 < t < cseg[-1][1]: OVERLAP_MS = t; break
# Where the lamp yielded with at least 0.6 s of the clip still to come, that remainder is heard with the lamp
# silent: its position fixes the clip's place better than the players' clocks do.
res['coffee_over_the_reply']['alignment'] = {'by': 'players\' clocks only', 'shift_ms': 0.0}
if OVERLAP_MS + 150 + 600 <= cseg[-1][1]:
    t0_ = OVERLAP_MS + 150; tail = coffee[t0_ * 16:]
    k_, peak_, sharp_ = find(tail, c0 + t0_ * 16 - 3200, c0 + t0_ * 16 + 3200)
    if sharp_ >= 2.5:
        shift = k_ - (c0 + t0_ * 16); c0 = c0 + shift
        res['coffee_over_the_reply']['alignment'] = {'by': 'the part of the clip heard after the lamp yielded', 'from_clip_ms': t0_, 'shift_ms': round(shift / 16, 1),
                                                     'matched_filter': round(peak_, 3), 'peak_over_99th_percentile': round(sharp_, 1),
                                                     'coherent_share_db_512': db(*reversed(powers(tail, pre[c0 + t0_ * 16:c0 + t0_ * 16 + len(tail)], 512)[:2])),
                                                     'microphone_level_dbfs': db(float((pre[c0 + t0_ * 16:c0 + t0_ * 16 + len(tail)] ** 2).mean()), 32768.0 ** 2),
                                                     'clip_level_dbfs': db(float((tail ** 2).mean()), 32768.0 ** 2)}
res['coffee_over_the_reply']['placed_at_sample_after_alignment'] = c0
part = coffee[:OVERLAP_MS * 16]
ptot = float((pre[c0:c0 + len(part)] ** 2).mean()); pperson = gain * float((part ** 2).mean())
_, ppeak, _ = find(part, c0 - 480, c0 + 480)
res['coffee_while_the_lamp_spoke'] = {'window_ms': [0, OVERLAP_MS], 'microphone_level_dbfs': db(ptot, 32768.0 ** 2),
    'person_level_predicted_from_the_sunset_gain_dbfs': db(pperson, 32768.0 ** 2), 'person_against_everything_else_predicted_db': db(pperson, max(ptot - pperson, 1e-9)),
    'matched_filter': round(ppeak, 3),
    'coherent_share_db_512': db(*reversed(powers(part, pre[c0:c0 + len(part)], 512)[:2])), 'coherent_share_db_2048': db(*reversed(powers(part, pre[c0:c0 + len(part)], 2048)[:2])),
    'after_cancellation': {'level_dbfs': db(float((post[c0 + 144:c0 + 144 + len(part)] ** 2).mean()), 32768.0 ** 2),
                           'coherent_share_db_512': db(*reversed(powers(part, post[c0 + 144:c0 + 144 + len(part)], 512)[:2])), 'coherent_share_db_2048': db(*reversed(powers(part, post[c0 + 144:c0 + 144 + len(part)], 2048)[:2]))}}
# --- the lamp's own pauses while the clip was speaking: render level under -45 dBFS
ren = F['render_tenths'] / 10; fb = c0 // 160; yb = fb + OVERLAP_MS // 10
qr = ren[fb:yb + 1] < -45; pauses = []; i = 0
while i < len(qr):
    if qr[i]:
        j = i
        while j < len(qr) and qr[j]: j += 1
        in_speech = any(lo <= i * 10 < hi for lo, hi in cseg) and any(lo < j * 10 <= hi for lo, hi in cseg)
        pauses.append({'ms_after_the_clip_arrived': i * 10, 'length_ms': (j - i) * 10, 'inside_the_clip_speech': bool(in_speech)})
        i = j
    else: i += 1
res['lamp_pauses_before_the_yield'] = {'what': 'stretches where the lamp rendered under -45 dBFS, from the clip\'s arrival to the first yield (or the clip\'s end)',
    'pauses': pauses, 'yield_ms': OVERLAP_MS if OVERLAP_MS < cseg[-1][1] else None,
    'render_dbfs_in_the_30_blocks_to_the_yield': [int(round(v)) for v in ren[max(yb - 29, 0):yb + 1]]}
bsp = [k for k in range(fb, yb) if any(lo <= (k - fb) * 10 < hi for lo, hi in cseg)]
res['before_the_yield'] = {'speech_blocks_of_the_clip': len(bsp), 'device_score_under_0_5': int(sum(1 for k in bsp if vad[k] < 0.5)),
    'replay_hearing': {n: int((F['heard'][fb:yb].astype(int) == c).sum()) for n, c in (('under', 2), ('own', 3), ('over', 4), ('unknown', 5))}}
# --- the microphone's own gain: quiet floor and loud level second by second, clipping, and the canceller's reduction
nsec = min(len(pre), len(post) - 144) // 16000
lvl = lambda v: round(float(10 * np.log10(max(float((v ** 2).mean()), 1e-3) / 32768.0 ** 2)), 1)
per_second = []
for sec in range(nsec):
    a = pre[sec * 16000:(sec + 1) * 16000]; b = post[sec * 16000 + 144:(sec + 1) * 16000 + 144]
    blocks = 10 * np.log10(np.maximum((a.reshape(-1, 160) ** 2).mean(axis=1), 1e-3) / 32768.0 ** 2)
    per_second.append({'second': sec, 'floor_dbfs': round(float(np.percentile(blocks, 10)), 1), 'loud_dbfs': round(float(np.percentile(blocks, 90)), 1),
                       'before_dbfs': lvl(a), 'after_dbfs': lvl(b), 'reduction_db': round(lvl(a) - lvl(b), 1), 'clipped_samples': int((np.abs(a) >= 32000).sum())})
res['microphone_per_second'] = {'what': 'floor and loud are the 10th and 90th percentile of the 10 ms block levels before cancellation; reduction is the mean level before minus after the device canceller',
                                'rows': per_second, 'peak_abs_sample': int(np.abs(pre).max()), 'clipped_samples': int((np.abs(pre) >= 32000).sum())}
# --- the clip's level against its own, where the lamp is silent: the sunset question, and the end of the coffee clip
def against_clip(start, x, lo_ms, hi_ms, step=50):
    rows_ = []
    for t in range(lo_ms, hi_ms, step):
        a = x[t * 16:(t + step) * 16]; b = pre[start + t * 16:start + (t + step) * 16]
        if len(a) == step * 16 and len(b) == step * 16 and lvl(a) > -45: rows_.append([t, round(lvl(b) - lvl(a), 1)])
    return rows_
sun_rows = against_clip(s0, sunset, 0, len(sunset) // 16)
res['microphone_minus_clip_level'] = {'what': 'level of the microphone signal minus the level of the clip itself, per 50 ms where the clip is above -45 dBFS; a change between the two rows is a change of gain on the way to the microphone signal',
    'sunset_lamp_silent': {'median_db': float(np.median([r[1] for r in sun_rows])), 'steps': len(sun_rows)},
    'coffee_last_400_ms': against_clip(c0, coffee, len(coffee) // 16 - 410, len(coffee) // 16)}
# --- what these measures can see: the clean clip added in software to a stretch of this recording where only the lamp speaks
def find_in(sig, x, lo, hi):
    seg = sig[lo:hi + len(x)]
    r = signal.correlate(seg, x, mode='valid', method='fft') / (np.sqrt(signal.correlate(seg ** 2, np.ones(len(x)), mode='valid', method='fft').clip(1e-9)) * np.sqrt((x ** 2).sum()))
    return round(float(r.max()), 3)
def share_db(x, y, nper):
    a = powers(x, y, nper); return db(a[1], a[0])
# where only the lamp speaks: given, or the stretch from 0.55 s after the clip if the lamp renders through most of it
LAMP_ALONE_AT_S = None
if LAMP_ALONE_ARG == 'auto':
    need = len(coffee) // 160 + 10; ra = F['render_active'] > 0
    taken = [(c0 // 160 - 20, (c0 + len(coffee)) // 160 + 50), (s0 // 160 - 20, (s0 + len(sunset)) // 160 + 50)]
    quiet_person = F['heard'] != 4                      # not where the replay heard a person over the lamp
    for k in range(0, len(ra) - need, 10):
        if any(lo < k + need and k < hi for lo, hi in taken): continue
        if float(ra[k:k + need].mean()) > 0.8 and float(quiet_person[k:k + need].mean()) > 0.98: LAMP_ALONE_AT_S = round(k / 100, 2); break
elif LAMP_ALONE_ARG != '-':
    LAMP_ALONE_AT_S = float(LAMP_ALONE_ARG)
if LAMP_ALONE_AT_S is not None:
    b0_ = max(int(LAMP_ALONE_AT_S * 16000), 800); host = pre[b0_ - 800:b0_ + len(part) + 800]
    cal = [{'added_dbfs': None, 'matched_filter': find_in(host, part, 320, 1280), 'coherent_share_db_512': share_db(part, host[800:800 + len(part)], 512), 'coherent_share_db_2048': share_db(part, host[800:800 + len(part)], 2048)}]
    for target in (-10, -15, -20, -25, -30, -35, -40, -45):
        y = host.copy(); y[800:800 + len(part)] += part * 10 ** ((target - lvl(part)) / 20)
        cal.append({'added_dbfs': target, 'matched_filter': find_in(y, part, 320, 1280), 'coherent_share_db_512': share_db(part, y[800:800 + len(part)], 512), 'coherent_share_db_2048': share_db(part, y[800:800 + len(part)], 2048)})
    res['calibration'] = {'what': 'the part of the clean coffee clip the lamp spoke through, added in software at a chosen level to a stretch where only the lamp speaks; the first row has nothing added', 'lamp_alone_from_second': LAMP_ALONE_AT_S,
                          'lamp_alone_level_dbfs': lvl(host), 'rows': cal,
                          'window_ms': [0, OVERLAP_MS],
                          'real_overlap': {'matched_filter': find_in(pre, part, c0 - 480, c0 + 480), 'coherent_share_db_512': share_db(part, pre[c0:c0 + len(part)], 512), 'coherent_share_db_2048': share_db(part, pre[c0:c0 + len(part)], 2048)}}
# --- what the coordinator decided, from the trace beside the audio
import os
trace_path = runtime + '/events.jsonl'
if os.path.exists(trace_path):
    tr = []
    for line in open(trace_path):
        line = line.strip()
        if line:
            try: tr.append(json.loads(line))
            except ValueError: pass
    first_frame = int(seq[c0 // 160])
    def rel(e): return (e['frame_sequence'] - first_frame) * 10 if 'frame_sequence' in e else None
    res['trace'] = {
        'run_start': {k: v for e in tr if e.get('kind') == 'run_start' for k, v in e.items() if k in ('turn_taking', 'reference_clock', 'provider_continuation', 'audio_diagnostics_requested', 'software_processing_requested', 'mode')},
        'duplex_steps': [{'kind': e['kind'], 'frame_sequence': e['frame_sequence'], 'ms_after_the_clip_arrived': rel(e), 'hearing_margin_db': None if e.get('hearing_margin_db') is None else round(e['hearing_margin_db'], 1)}
                         for e in tr if e.get('kind') in ('duplex_yield', 'duplex_commit', 'duplex_resume')],
        'candidates': [{'serial': e['candidate']['id']['serial'], 'trigger_sequence': e['candidate']['trigger_sequence'], 'trigger_ms_after_the_clip_arrived': (e['candidate']['trigger_sequence'] - first_frame) * 10,
                        'displaced_an_owner': bool(e['candidate']['displaced_owner'])} for e in tr if e.get('kind') == 'input_candidate'],
        'rejected': [{'serial': e['candidate']['serial'], 'reason': e.get('reason')} for e in tr if e.get('kind') == 'input_candidate_rejected'],
        'admitted': [{'serial': e['candidate']['serial'], 'turn': e.get('turn'), 'basis': e.get('admission_basis')} for e in tr if e.get('kind') == 'input_admitted'],
        'turns': [{'turn': i + 1, 'outcome': e.get('outcome'), 'playback_gaps': e.get('playback_gaps')} for i, e in enumerate(x for x in tr if x.get('kind') == 'turn_finished')],
        'input_transcripts': [e.get('text') for e in tr if e.get('kind') == 'transcript' and e.get('turn') is None],
        'clip_speech_ends_ms': cseg[-1][1]}
if MIXERS:
    found = {}
    def walk(node, name=''):
        if isinstance(node, dict):
            for k, v in node.items(): walk(v, k)
        elif isinstance(node, list):
            for v in node: walk(v, name)
        elif isinstance(node, str) and "name='Auto Gain Control'" in node:
            entry = {'auto_gain_control': node.split("name='Auto Gain Control'")[1].split(': values=')[1].split('\n')[0].strip()}
            if "name='Mic Capture Volume'" in node:
                entry['mic_capture_volume'] = node.split("name='Mic Capture Volume'")[1].split(': values=')[1].split('\n')[0].strip()
            found[name] = entry
    walk(json.load(open(MIXERS)))
    res['mixer_snapshot'] = {'path': MIXERS, 'devices': found}
json.dump(res, open(out, 'w'), indent=1)
print(json.dumps({k: v for k, v in res.items() if k not in ('per_block_over_the_clip', 'microphone_per_second')}, indent=1))
