"""Per 100 ms of capture: render level at the canceller's cursor, microphone before and after cancellation,
blocks scored 0.8 or over. argv: RUNTIME FROM_S TO_S [CLIP_START_SAMPLE]"""
import json, sys
import numpy as np
rt, a, b = sys.argv[1], float(sys.argv[2]), float(sys.argv[3]); clip0 = int(sys.argv[4]) if len(sys.argv) > 4 else None
cur, vad = [], []
for line in open(rt + '/capture-audio/events.jsonl'):
    if '"capture"' not in line: continue
    e = json.loads(line)
    if e.get('kind') == 'capture': cur.append(e['meta']['reference']['analysed_through']); vad.append(e['meta'].get('vad_score') or 0.0)
cur, vad = np.array(cur), np.array(vad)
pre = np.fromfile(rt + '/capture-audio/pre_aec.pcm16le', dtype='<i2').astype(np.float64)
post = np.fromfile(rt + '/capture-audio/post_aec.pcm16le', dtype='<i2').astype(np.float64)
render = np.fromfile(rt + '/render-audio/render_accepted.pcm16le', dtype='<i2').astype(np.float64)
db = lambda x: 10 * np.log10(max(float(np.mean(x * x)), 1e-3) / 32768.0 ** 2) if len(x) else -99
for t in np.arange(a, b, 0.1):
    i = int(round(t * 100)); c0, c1 = int(cur[i]), int(cur[min(i + 10, len(cur) - 1)])
    mark = ''
    if clip0 is not None and clip0 <= i * 160 < clip0 + 73281: mark = 'clip %4d ms' % ((i * 160 - clip0) // 16)
    print('%6.1f  render %6.1f  mic before %6.1f  after %6.1f  scored>=0.8 %2d/10  %s' % (t, db(render[c0:c1]), db(pre[i * 160:(i + 10) * 160]), db(post[i * 160:(i + 10) * 160]), int((vad[i:i + 10] >= 0.8).sum()), mark))
