"""A cached clip played over a fixed reply that the lamp plays three times: what of the clip is in the
microphone signal while the lamp itself sounds. python3.12 -I with NumPy and SciPy.

    micpair.py RUNTIME_DIR NATIVE_BUNDLE.json COFFEE.wav OUT.json

The reply is one 4.49 s recording played three times with 300 ms of zeros between. The clip arrives during
the second copy. The first copy is the same lamp sound with nobody speaking, sample for sample at the render,
so it is taken from the second: what is left is the clip plus whatever differs between two playings. The
third copy, after the clip has ended, is taken apart the same way and holds no clip: it is the control and
gives the floor of the method. Reads only."""
import json
import sys
import wave

import numpy as np
from scipy import signal

runtime, bundle_path, coffee_path, out = sys.argv[1:5]
bundle = json.load(open(bundle_path))
COPY, GAP = 107_760, 7_200          # render frames at 24 kHz
PERIOD = COPY + GAP
POST_LAG = 144                      # the canceller's output is this many samples behind its input


def db(x):
    return float(10 * np.log10(max(float(np.mean(np.square(x))), 1e-3) / 32768.0 ** 2)) if len(x) else None


rows = []
for line in open(runtime + '/capture-audio/events.jsonl'):
    if '"capture"' not in line:
        continue
    e = json.loads(line)
    if e.get('kind') == 'capture':
        rows.append((e['meta']['reference']['analysed_through'], e['meta'].get('vad_score') or 0.0))
cursor = np.array([r[0] for r in rows], dtype=np.float64)
vad = np.array([r[1] for r in rows])
pre = np.fromfile(runtime + '/capture-audio/pre_aec.pcm16le', dtype='<i2').astype(np.float64)
post = np.fromfile(runtime + '/capture-audio/post_aec.pcm16le', dtype='<i2').astype(np.float64)
first = None
frames = 0
for line in open(runtime + '/render-audio/events.jsonl'):
    if '"render_accepted"' not in line:
        continue
    e = json.loads(line)
    if e.get('kind') == 'render_accepted' and e['accepted_silence_kind'] is None and e['frames']:
        first = e['meta']['first_sample'] if first is None else min(first, e['meta']['first_sample'])
        frames += e['frames']
assert frames == 3 * COPY + 2 * GAP, frames
render = np.fromfile(runtime + '/render-audio/render_accepted.pcm16le', dtype='<i2').astype(np.float64)
reply = render[first:first + COPY]
reply16 = signal.resample_poly(reply, 2, 3)

# capture sample at which the canceller had analysed the render up to the start of each copy
grow = np.nonzero(np.diff(cursor) > 0)[0] + 1
at = lambda c: float(np.interp(c, cursor[grow], grow * 160.0))
nominal = [int(round(at(first + k * PERIOD))) for k in range(3)]

# the lamp's own voice in the microphone: delay from the render cursor to the microphone signal
T = int(5.2 * 16000)
a_nom = pre[nominal[0]:nominal[0] + T + 3200]
corr = signal.correlate(a_nom[:len(reply16) + 3200], reply16, mode='valid')
delay = int(np.argmax(np.abs(corr)))


def lag_between(k, w0, w1, reach=480):
    """Samples by which copy k sits later than its nominal start, measured against copy 1 over [w0, w1)."""
    a = pre[nominal[0] + w0:nominal[0] + w1]
    b = pre[nominal[k] + w0 - reach:nominal[k] + w1 + reach]
    c = signal.correlate(b, a, mode='valid')
    i = int(np.argmax(c))
    frac = 0.0
    if 0 < i < len(c) - 1:
        y0, y1, y2 = c[i - 1], c[i], c[i + 1]
        frac = float(0.5 * (y0 - y2) / (y0 - 2 * y1 + y2))
    peak = float(c[i] / np.sqrt(np.sum(a * a) * np.sum(b[i:i + len(a)] ** 2)))
    return i - reach + frac, peak


def shifted(x, start, length, lag):
    """x[start + lag + t] for t in [0, length), lag fractional, by a phase ramp."""
    whole = int(np.floor(lag))
    frac = lag - whole
    pad = 2048
    seg = x[start + whole - pad:start + whole + length + pad]
    spectrum = np.fft.rfft(seg)
    spectrum *= np.exp(2j * np.pi * np.fft.rfftfreq(len(seg)) * frac)
    return np.fft.irfft(spectrum, len(seg))[pad:pad + length]


clip_wave = wave.open(coffee_path, 'rb')
clip = np.frombuffer(clip_wave.readframes(clip_wave.getnframes()), dtype='<i2').astype(np.float64)
placed = bundle['coffee_over_the_reply'].get('placed_at_sample_after_alignment',
                                              bundle['coffee_over_the_reply']['placed_at_sample'])
sunset_gain = bundle['microphone_minus_clip_level']['sunset_lamp_silent']['median_db']

own_from, own_to = delay + 1600, delay + int(0.5 * 16000)           # copy 2 before the clip arrives
tail_from, tail_to = delay + int(0.8 * 16000), delay + int(4.4 * 16000)  # copy 3 after the clip has ended
lag2, peak2 = lag_between(1, own_from, own_to)
lag3, peak3 = lag_between(2, tail_from, tail_to)
a = pre[nominal[0]:nominal[0] + T]
a_post = post[nominal[0] + POST_LAG:nominal[0] + POST_LAG + T]
copies = {}
for k, lag in ((2, lag2), (3, lag3)):
    b = shifted(pre, nominal[k - 1], T, lag)
    b_post = shifted(post, nominal[k - 1] + POST_LAG, T, lag)
    w = slice(own_from, own_to) if k == 2 else slice(tail_from, tail_to)
    gain = float(np.dot(b[w], a[w]) / np.dot(a[w], a[w]))
    copies[k] = {'pre': b, 'post': b_post, 'residual': b - gain * a, 'gain': gain,
                 'start': nominal[k - 1] + lag, 'window': w}
clip_at = placed - copies[2]['start']       # clip sample 0 in the time of a copy


def correlation(x, reference, reach=80, where=False):
    """Largest normalised correlation of x with the reference within reach samples either way."""
    assert len(x) == len(reference), (len(x), len(reference))
    if len(x) < 800 or not np.any(reference):
        return None
    padded = np.concatenate([np.zeros(reach), x, np.zeros(reach)])
    c = np.abs(signal.correlate(padded, reference, mode='valid'))
    value = float(np.max(c) / np.sqrt(np.sum(x * x) * np.sum(reference * reference) + 1e-9))
    return (value, int(np.argmax(c)) - reach) if where else value


# The clip alone, in the lamp's 300 ms of zeros after the second copy, says where the clip really sits
# (the players' clocks place it to within tens of ms) and how well the dry clip matches what the
# microphone makes of it when nothing else sounds.
# The 300 ms of zeros after a copy, found in the microphone signal of the first copy itself (no clip
# there): the longest run of 10 ms blocks 25 dB under the lamp's own level between 4.2 s and 5.1 s.
levels = np.array([db(a[i:i + 160]) for i in range(int(4.2 * 16000), int(5.1 * 16000), 160)])
quiet = levels < db(a[delay + 1600:delay + int(4.4 * 16000)]) - 25
runs, i = [], 0
while i < len(quiet):
    if quiet[i]:
        j = i
        while j < len(quiet) and quiet[j]:
            j += 1
        runs.append((j - i, i, j))
        i = j
    else:
        i += 1
_, r0, r1 = max(runs)
gap_from, gap_to = int(4.2 * 16000) + r0 * 160 + 480, int(4.2 * 16000) + r1 * 160 - 160
g0 = gap_from - int(round(clip_at))
alone, moved = correlation(copies[2]['pre'][gap_from:gap_to], clip[g0:g0 + gap_to - gap_from], reach=960, where=True)
clip_at += moved
g0 = gap_from - int(round(clip_at))
gap_gain = db(copies[2]['pre'][gap_from:gap_to]) - db(clip[g0:g0 + gap_to - gap_from])


def blocks(copy, t0, t1):
    start = int(round(copies[copy]['start'])) if copy != 1 else nominal[0]
    return vad[(start + t0) // 160:(start + t1) // 160]


stretches = [('first words', s, e) for s, e in bundle['clips']['coffee_speech_ms'][:3]]
stretches.append(('last sentence, lamp still sounding', bundle['clips']['coffee_speech_ms'][3][0], None))
table = []
pause_from, pause_to = gap_from, gap_to
for name, s_ms, e_ms in stretches:
    c0 = s_ms * 16
    c1 = e_ms * 16 if e_ms is not None else min(bundle['clips']['coffee_speech_ms'][3][1] * 16,
                                                 delay + int(4.49 * 16000) - int(clip_at))
    t0, t1 = int(round(clip_at)) + c0, int(round(clip_at)) + c1
    if t1 - t0 < 800:
        continue
    reference = clip[c0:c1]
    r2, r3 = copies[2]['residual'][t0:t1], copies[3]['residual'][t0:t1]
    rho2, rho3 = correlation(r2, reference), correlation(r3, reference)
    v2, v1, v3 = blocks(2, t0, t1), blocks(1, t0, t1), blocks(3, t0, t1)
    table.append({
        'stretch': name, 'clip_ms': [s_ms, c1 // 16], 'seconds_into_the_copy': [round(t0 / 16000, 2), round(t1 / 16000, 2)],
        'clip_level_expected_from_the_sunset_gain_dbfs': round(db(reference) + sunset_gain, 1),
        'clip_level_expected_from_its_own_gain_in_the_lamp_pause_dbfs': round(db(reference) + gap_gain, 1),
        'lamp_own_voice_dbfs': round(db(a[t0:t1]), 1),
        'before_cancellation': {
            'left_after_taking_the_first_copy_away_dbfs': round(db(r2), 1),
            'same_in_the_copy_without_the_clip_dbfs': round(db(r3), 1),
            'correlation_with_the_clip': round(rho2, 3), 'same_in_the_copy_without_the_clip': round(rho3, 3),
            'part_that_follows_the_clip_dbfs': round(db(r2) + 20 * np.log10(max(rho2, 1e-6) / alone), 1),
            'same_in_the_copy_without_the_clip_part_dbfs': round(db(r3) + 20 * np.log10(max(rho3, 1e-6) / alone), 1)},
        'after_cancellation': {
            'with_the_clip_dbfs': round(db(copies[2]['post'][t0:t1]), 1),
            'first_copy_same_place_dbfs': round(db(a_post[t0:t1]), 1),
            'third_copy_same_place_dbfs': round(db(copies[3]['post'][t0:t1]), 1),
            'correlation_with_the_clip': round(correlation(copies[2]['post'][t0:t1], reference), 3),
            'same_in_the_first_copy': round(correlation(a_post[t0:t1], reference), 3),
            'same_in_the_third_copy': round(correlation(copies[3]['post'][t0:t1], reference), 3)},
        'device_score_blocks': {'n': int(len(v2)),
                                'with_the_clip_from_0_8': int((v2 >= 0.8).sum()), 'with_the_clip_from_0_5': int((v2 >= 0.5).sum()),
                                'first_copy_from_0_8': int((v1 >= 0.8).sum()), 'first_copy_from_0_5': int((v1 >= 0.5).sum()),
                                'third_copy_from_0_8': int((v3 >= 0.8).sum()), 'third_copy_from_0_5': int((v3 >= 0.5).sum())}})

# the lamp's 300 ms of zeros after the second copy: the clip with the lamp just silent, against the same
# 300 ms after the first copy, where there is no clip
c0, c1 = pause_from - int(round(clip_at)), pause_to - int(round(clip_at))
inside = 0 <= c0 and c1 <= len(clip)
pause = {'seconds_into_the_copy': [round(pause_from / 16000, 2), round(pause_to / 16000, 2)],
         'clip_ms': [c0 // 16, c1 // 16] if inside else None,
         'clip_level_expected_from_the_sunset_gain_dbfs': round(db(clip[c0:c1]) + sunset_gain, 1) if inside else None,
         'before_cancellation_with_the_clip_dbfs': round(db(copies[2]['pre'][pause_from:pause_to]), 1),
         'before_cancellation_after_the_first_copy_dbfs': round(db(a[pause_from:pause_to]), 1),
         'correlation_with_the_clip': round(correlation(copies[2]['pre'][pause_from:pause_to], clip[c0:c1]), 3) if inside else None,
         'same_after_the_first_copy': round(correlation(a[pause_from:pause_to], clip[c0:c1]), 3) if inside else None,
         'after_cancellation_with_the_clip_dbfs': round(db(copies[2]['post'][pause_from:pause_to]), 1),
         'after_cancellation_after_the_first_copy_dbfs': round(db(a_post[pause_from:pause_to]), 1),
         'device_score_from_0_8_with_the_clip': int((blocks(2, pause_from, pause_to) >= 0.8).sum()),
         'device_score_from_0_8_after_the_first_copy': int((blocks(1, pause_from, pause_to) >= 0.8).sum()),
         'blocks': int(len(blocks(2, pause_from, pause_to)))}

own = slice(delay + 1600, delay + int(4.4 * 16000))


def linear_share(x, start):
    """How much of the microphone signal a fixed linear filter of the render can account for, in dB of
    what such a filter would leave: from the coherence of render and microphone over the lamp's own speech."""
    y = x[start + delay + 1600:start + delay + int(4.4 * 16000)]
    r = reply16[1600:1600 + len(y)]
    f, coherence = signal.coherence(r, y[:len(r)], fs=16000, nperseg=2048, noverlap=1024)
    _, power = signal.welch(y[:len(r)], fs=16000, nperseg=2048, noverlap=1024)
    left = float(np.sum((1 - coherence) * power) / np.sum(power))
    return round(float(-10 * np.log10(max(left, 1e-9))), 1)


idle = pre[max(nominal[0] - 16000, 0):nominal[0] - 1600]
sun = bundle['sunset_in_the_clear']
result = {
    'runtime': runtime,
    'reply': {'first_render_sample': first, 'copies_start_at_capture_s': [round(n / 16000, 3) for n in nominal],
              'render_to_microphone_delay_ms': round(delay / 16, 1),
              'copy_2_against_copy_1': {'lag_samples': round(lag2, 2), 'correlation': round(peak2, 4), 'gain': round(copies[2]['gain'], 3)},
              'copy_3_against_copy_1': {'lag_samples': round(lag3, 2), 'correlation': round(peak3, 4), 'gain': round(copies[3]['gain'], 3)}},
    'clip_starts_seconds_into_the_second_copy': round(clip_at / 16000, 3),
    'clip_alone_in_the_lamp_pause': {'pause_found_seconds_into_a_copy': [round((int(4.2 * 16000) + r0 * 160) / 16000, 2), round((int(4.2 * 16000) + r1 * 160) / 16000, 2)],
                                     'correlation_with_the_dry_clip': round(alone, 3), 'placement_moved_ms': round(moved / 16, 1),
                                     'microphone_minus_clip_level_db': round(gap_gain, 1)},
    'original_request_lamp_silent': {'microphone_level_dbfs': sun['microphone_level_dbfs'], 'idle_floor_dbfs': sun['room_floor_dbfs'],
                                     'loudspeaker_to_microphone_gain_db': sunset_gain, 'matched_filter_peak': sun['matched_filter_peak'],
                                     'device_score_blocks': sun['device_score']['blocks'], 'from_0_8': sun['device_score']['from_0_8']},
    'lamp_own_voice_alone': {
        'idle_floor_before_dbfs': round(db(idle), 1),
        'first_copy_before_cancellation_dbfs': round(db(a[own]), 1), 'first_copy_after_cancellation_dbfs': round(db(a_post[own]), 1),
        'third_copy_before_cancellation_dbfs': round(db(copies[3]['pre'][tail_from:tail_to]), 1),
        'third_copy_after_cancellation_dbfs': round(db(copies[3]['post'][tail_from:tail_to]), 1),
        'first_copy_device_score_from_0_8': int((blocks(1, own.start, own.stop) >= 0.8).sum()),
        'third_copy_device_score_from_0_8': int((blocks(3, tail_from, tail_to) >= 0.8).sum()),
        'first_copy_blocks': int(len(blocks(1, own.start, own.stop))), 'third_copy_blocks': int(len(blocks(3, tail_from, tail_to))),
        'most_a_fixed_linear_filter_of_the_render_could_remove_db': {
            'first_copy': linear_share(pre, nominal[0]), 'third_copy': linear_share(pre, int(round(copies[3]['start'])))}},
    'method_floor': {
        'what': 'one playing taken from another where neither holds the clip: level left, and how far under the lamp it is',
        'copy_2_before_the_clip_left_dbfs': round(db(copies[2]['residual'][own_from:own_to]), 1),
        'copy_2_before_the_clip_lamp_dbfs': round(db(a[own_from:own_to]), 1),
        'copy_3_after_the_clip_left_dbfs': round(db(copies[3]['residual'][tail_from:tail_to]), 1),
        'copy_3_after_the_clip_lamp_dbfs': round(db(a[tail_from:tail_to]), 1)},
    'clip_over_the_second_copy': table,
    'clip_in_the_lamp_pause_after_the_second_copy': pause,
}
json.dump(result, open(out, 'w'), indent=1)
print(json.dumps(result, indent=1))
