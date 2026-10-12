"""Onset retention in the offline duplex lab: one cached stimulus clip added
to the microphone over each recorded reply of a session. python3.12 -I.
argv: LAB_BINARY LAB_ROOT SESSION CLIP.wav CLIP_SHA256 OUT_DIR GAINS(csv) [START_AFTER_BLOCKS]
Nothing here is a device result: the clip is the clean cached file, mixed in
software before cancellation, and the canceller starts cold on this host."""
import sys, os, json, subprocess, wave, hashlib
import numpy as np
BIN, ROOT, session, clip_path, clip_sha, out_root = sys.argv[1:7]
gains = [float(g) for g in sys.argv[7].split(',')]
start_after = int(sys.argv[8]) if len(sys.argv) > 8 else 120
PRE_ROLL = 30            # crates/live/src/activity.rs PRE_ROLL_BLOCKS
ONSET_BLOCKS = 6         # Confirmation::EVALUATED.onset_blocks, START_BLOCKS
def read_features(path):
    rows = [l.rstrip('\n').split('\t') for l in open(path)]
    head = rows[0]; data = np.array([[float(x) for x in r] for r in rows[1:]])
    return {h: data[:, i] for i, h in enumerate(head)}
def blk_db(x):
    x = x[:len(x) // 160 * 160].astype(np.float64).reshape(-1, 160)
    p = (x * x).mean(axis=1)
    return 10 * np.log10(np.maximum(p, 1e-3) / 32768.0 ** 2)
raw = open(clip_path, 'rb').read()
assert hashlib.sha256(raw).hexdigest() == clip_sha, 'clip digest'
w = wave.open(clip_path, 'rb'); assert w.getframerate() == 16000 and w.getnchannels() == 1 and w.getsampwidth() == 2
clip = np.frombuffer(w.readframes(w.getnframes()), dtype='<i2').astype(np.float64)
db = blk_db(clip.astype(np.int16)); active = np.nonzero(db > db.max() - 35)[0]
clip = clip[: (int(active[-1]) + 1 + 10) * 160]                  # through 100 ms after the last active block
clip_blocks = len(clip) // 160
clip_p90 = float(np.percentile(blk_db(clip.astype(np.int16))[db[:clip_blocks] > db.max() - 35], 90))
lab = f'{ROOT}/{session}'
base = read_features(lab + '/base/features.tsv')
n = len(base['vad']); playing = base['render_active'] > 0
quiet = (~playing) & (base['vad'] < 0.1)
best = None
for k in range(100, n - 200):
    if quiet[k - 50:k + 150].all():
        lvl = base['capture_tenths'][k:k + 100].max()
        if best is None or lvl < best[0]: best = (lvl, k)
room = best[1]
replies = []; i = 0
while i < n:
    if playing[i]:
        j = i
        while j < n and playing[j]: j += 1
        if j - i >= 150: replies.append((i, j - 50))
        i = j
    else: i += 1
levels = []
for seed in (1, 2, 3):
    p = f'{lab}/near_0_s{seed}/truth.tsv'
    if os.path.exists(p):
        levels += [float(l.split('\t')[6]) for l in open(p) if not l.startswith('#')]
target = float(np.median(levels))                                # how loud this session's own prompts arrived at the microphone
os.makedirs(out_root, exist_ok=True)
def run(out, extra):
    os.makedirs(out, exist_ok=True)
    r = subprocess.run([BIN, lab, out, '--duplex', '--room', str(room)] + extra, capture_output=True, text=True)
    if r.returncode != 0: raise SystemExit(r.stderr[-2000:])
    rows = []
    for l in open(out + '/decisions.tsv').read().splitlines()[1:]:
        a, b, step = l.split('\t'); rows.append((int(a), int(b), step))
    return rows, read_features(out + '/features.tsv')
neg, _ = run(f'{out_root}/{session}.negative', [])
result = {'session': session, 'blocks': n, 'room_block': room, 'replies': [[a, b] for a, b in replies],
          'clip_sha256': clip_sha, 'clip_blocks': clip_blocks, 'clip_p90_dbfs': round(clip_p90, 1),
          'session_prompt_p90_dbfs_median': round(target, 1), 'start_after_reply_blocks': start_after,
          'negative_control': {'what': 'the recorded replies with nothing added', 'yields': sum(s == 'yield' for _, _, s in neg), 'commits': sum(s == 'commit' for _, _, s in neg)},
          'trials': []}
def longest(mask):
    best = run_ = 0
    for m in mask:
        run_ = run_ + 1 if m else 0; best = max(best, run_)
    return best
for g in gains:
    scale = 10 ** ((target - clip_p90 + g) / 20)
    seg = np.clip(np.round(clip * scale), -32768, 32767).astype('<i2')
    segf = f'{out_root}/{session}.seg.{g:+.0f}.pcm16le'; seg.tofile(segf)
    for r, (a, b) in enumerate(replies):
        at = a + start_after
        rows, f = run(f'{out_root}/{session}.r{r}.g{g:+.0f}', ['--segment', segf, str(at)])
        trial = {'gain_db': g, 'reply': r, 'reply_blocks': b - a, 'onset_block': at, 'clip_ends_block': at + clip_blocks,
                 'overlap_blocks_before_reply_end': max(0, min(b, at + clip_blocks) - at)}
        if any(wb < at - 5 and st == 'yield' and a - 5 <= sb <= b + 60 for wb, sb, st in rows):
            trial['excluded'] = 'the lamp yielded in this reply before the clip began'
            result['trials'].append(trial); continue
        first = next(((wb, st) for wb, sb, st in rows if st == 'yield' and at - 5 <= wb <= at + clip_blocks + 30), None)
        heard = f['heard'].astype(int); vad = f['vad']
        if first is None:
            trial.update({'yielded': False})
            span = slice(at, min(len(heard), at + clip_blocks))
        else:
            y = first[0]
            after = next(((wb, st) for wb, sb, st in rows if wb > y and st in ('commit', 'resume')), None)
            start = y - (PRE_ROLL - 1)                           # the trigger block is the last of the pre-roll
            trial.update({'yielded': True, 'yield_delay_ms': (y - at) * 10,
                          'outcome': after[1] if after else None, 'outcome_delay_ms': (after[0] - at) * 10 if after else None,
                          'request_starts_ms_after_onset': (start - at) * 10, 'opening_not_retained_ms': max(0, (start - at) * 10)})
            span = slice(at, y + 1)
        # everything the decision did from the clip's onset to its end plus one second
        steps = [(wb, st) for wb, sb, st in rows if at - 5 <= wb <= at + clip_blocks + 100]
        trial['steps'] = [[st, (wb - at) * 10] for wb, st in steps]
        commit = next(((wb, i) for i, (wb, st) in enumerate(steps) if st == 'commit'), None)
        if commit is not None:
            ywb = max(wb for wb, st in steps[:commit[1]] if st == 'yield')
            trial['committed'] = {'commit_delay_ms': (commit[0] - at) * 10, 'its_yield_delay_ms': (ywb - at) * 10,
                                  'request_starts_ms_after_onset': (ywb - (PRE_ROLL - 1) - at) * 10,
                                  'opening_not_retained_ms': max(0, (ywb - (PRE_ROLL - 1) - at) * 10),
                                  'yields_before_it': sum(st == 'yield' for wb, st in steps[:commit[1]]) - 1}
        else:
            trial['committed'] = None
        h = heard[span]
        trial['heard_before_yield'] = {'blocks': int(len(h)), 'over': int((h == 4).sum()), 'own': int((h == 3).sum()),
                                       'under': int((h == 2).sum()), 'unknown': int((h == 5).sum()), 'clear_or_still': int(((h == 0) | (h == 1)).sum()),
                                       'longest_over_run': longest(h == 4), 'first_over_ms': int(np.argmax(h == 4)) * 10 if (h == 4).any() else None,
                                       'speech_scored_blocks': int((vad[span] >= 0.5).sum())}
        trial['heard_string'] = ''.join('SCuOVx'[c] for c in h)   # S still, C clear, u under, O own, V over, x unknown
        result['trials'].append(trial)
json.dump(result, open(f'{out_root}/{session}.json', 'w'), indent=1)
print(session, 'replies', len(replies), 'room', room, 'target p90', round(target, 1), 'clip p90', round(clip_p90, 1), 'negative yields', result['negative_control']['yields'])
for t in result['trials']:
    if t.get('excluded'): print(f"  g{t['gain_db']:+.0f} r{t['reply']}: excluded"); continue
    c = t.get('committed')
    print(f"  g{t['gain_db']:+.0f} r{t['reply']} steps {t['steps']} | " + (f"committed +{c['commit_delay_ms']} ms after {c['yields_before_it']} earlier yield(s); opening not retained {c['opening_not_retained_ms']} ms" if c else 'no commit within the clip plus 1 s'))
    hb = t['heard_before_yield']
    if t['yielded']:
        print(f"  g{t['gain_db']:+.0f} r{t['reply']} reply {t['reply_blocks']*10} ms: yield +{t['yield_delay_ms']} ms -> {t['outcome']} +{t['outcome_delay_ms']}; opening not retained {t['opening_not_retained_ms']} ms; before yield over {hb['over']} own {hb['own']} under {hb['under']} longest over run {hb['longest_over_run']} first over {hb['first_over_ms']}")
    else:
        print(f"  g{t['gain_db']:+.0f} r{t['reply']} reply {t['reply_blocks']*10} ms: never yielded; over {hb['over']} own {hb['own']} under {hb['under']} longest over run {hb['longest_over_run']}")
