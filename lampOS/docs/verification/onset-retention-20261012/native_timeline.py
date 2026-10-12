"""Where each admitted request of the native serve run began, against where
the speech of its cached stimulus began. python3.12 -I with NumPy.
argv: AUDIT.json METADATA_DIR OUT_DIR
Reads the independent audit (step timing, stimulus paths and digests), the
native trace (events-*.jsonl) and the cached stimulus clips. Writes
stimuli-segments.json and native-timeline.json. Nothing is uploaded and no
clip is changed or regenerated."""
import sys, json, wave, hashlib, glob
import numpy as np
audit_path, metadata, out = sys.argv[1:4]
audit = json.load(open(audit_path))
events = []
for path in sorted(glob.glob(metadata + '/events-*.jsonl')):
    for line in open(path):
        line = line.strip()
        if line:
            try: events.append(json.loads(line))
            except ValueError: pass
def segments(path, digest):
    raw = open(path, 'rb').read()
    assert hashlib.sha256(raw).hexdigest() == digest, 'stimulus digest'
    w = wave.open(path, 'rb'); rate = w.getframerate()
    x = np.frombuffer(w.readframes(w.getnframes()), dtype='<i2').astype(np.float64)
    hop = rate // 100; m = len(x) // hop
    db = 20 * np.log10((np.sqrt((x[:m * hop].reshape(m, hop) ** 2).mean(axis=1)) + 1e-9) / 32768.0)
    act = db > max(db.max() - 35.0, np.percentile(db, 10) + 10.0)
    segs = []; i = 0
    while i < m:
        if act[i]:
            j = i
            while j < m and (act[j] or (j + 15 < m and act[j:j + 15].any())): j += 1   # gaps under 150 ms are inside a segment
            if (j - i) * 10 >= 60: segs.append([i * 10, j * 10])
            i = j
        else: i += 1
    return {'rate': rate, 'duration_s': len(x) / rate, 'peak_dbfs': round(float(db.max()), 1), 'segments_ms': segs}
stimuli = {s['step']: dict(text=s['cached_text'][0], sha256=s['source']['sha256'], **segments(s['source']['path'], s['source']['sha256'])) for s in audit['steps']}
json.dump(stimuli, open(out + '/stimuli-segments.json', 'w'), indent=1)
candidate = {e['candidate']['id']['serial']: e['candidate'] for e in events if e.get('kind') == 'input_candidate'}
admitted = {e['turn']: e for e in events if e.get('kind') == 'input_admitted'}
endpoint = {e['turn']: e for e in events if e.get('kind') == 'local_endpoint'}
rows = []
for turn, s in enumerate(audit['steps'], start=1):
    offset = s['trigger']['runner_us'] - s['trigger']['event_at_us']         # runner clock minus lamp clock
    clip_start = s['first_source_copy_host_ns'] / 1000 - offset               # first source copy, on the lamp clock
    seg = stimuli[s['step']]['segments_ms']; onset = seg[0][0] * 1000
    c = candidate[turn]; first, trigger = c['first_read_at_us'], c['trigger_read_at_us']
    rows.append({'turn': turn, 'step': s['step'], 'scene': s['scene'], 'displaced_an_owner': bool(c['displaced_owner']),
        'clip_start_lamp_us': round(clip_start), 'clip_speech_onset_ms': seg[0][0],
        'trigger_sequence': c['trigger_sequence'],
        'trigger_after_speech_onset_ms': round((trigger - clip_start - onset) / 1000, 1),
        'request_reaches_back_before_trigger_ms': round((trigger - first) / 1000, 1),
        'request_starts_after_speech_onset_ms': round((first - clip_start - onset) / 1000, 1),
        'request_starts_at_clip_ms': round((first - clip_start) / 1000, 1),
        'admitted_after_trigger_ms': round((admitted[turn]['at_us'] - trigger) / 1000, 1),
        'endpoint_last_block_after_clip_speech_end_ms': round((endpoint[turn]['last_block_host_read_us'] - clip_start - seg[-1][1] * 1000) / 1000, 1)})
y = [e for e in events if e.get('kind') == 'duplex_yield']; cm = [e for e in events if e.get('kind') == 'duplex_commit']
coffee = rows[3]
json.dump({'method': 'Clip start on the lamp clock is the first source copy on the runner clock minus (runner minus lamp) from that step\'s trigger mapping. Speech onset is the first 10 ms block of the cached clip within 35 dB of its peak. No output-device latency or acoustic path is added, so the speech reached the microphone later than these figures say by that unknown amount; the endpoint column bounds it.',
    'rows': rows,
    'coffee': {'yield_at_clip_ms': round((y[0]['last_block_host_read_us'] - coffee['clip_start_lamp_us']) / 1000, 1),
               'commit_at_clip_ms': round((cm[0]['last_block_host_read_us'] - coffee['clip_start_lamp_us']) / 1000, 1),
               'hearing_margin_db_at_yield': y[0]['hearing_margin_db'], 'hearing_margin_db_at_commit': cm[0]['hearing_margin_db'],
               'reply_frames_still_queued_at_yield': y[0]['reply_queued_frames'], 'duplex_yields_in_run': len(y)}},
    open(out + '/native-timeline.json', 'w'), indent=1)
print('%-8s %-9s %9s %9s %10s' % ('step', 'displaced', 'trigger', 'reach', 'start'))
for r in rows:
    print('%-8s %-9s %9.1f %9.1f %10.1f' % (r['step'], r['displaced_an_owner'], r['trigger_after_speech_onset_ms'], r['request_reaches_back_before_trigger_ms'], r['request_starts_after_speech_onset_ms']))
