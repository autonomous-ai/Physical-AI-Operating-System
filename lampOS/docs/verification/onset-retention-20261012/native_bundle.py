"""What the device's microphone, canceller and detector did with a cached
clip played over the lamp's own reply. python3.12 -I with NumPy and SciPy.
argv: RUNTIME_DIR LAB_FEATURES.tsv COFFEE.wav COFFEE_SHA SUNSET.wav SUNSET_SHA COFFEE_NOMINAL_START_LAMP_US OUT.json SECONDS_BETWEEN_THE_TWO_SOURCE_STARTS
Reads only. Nothing is uploaded and no clip is changed."""
import sys, json, wave, hashlib
import numpy as np
from scipy import signal
runtime, features, coffee_path, coffee_sha, sunset_path, sunset_sha, nominal, out = sys.argv[1:9]
nominal = int(nominal)
BETWEEN_SOURCE_STARTS_S = float(sys.argv[9])
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
json.dump(res, open(out, 'w'), indent=1)
print(json.dumps(res, indent=1))
