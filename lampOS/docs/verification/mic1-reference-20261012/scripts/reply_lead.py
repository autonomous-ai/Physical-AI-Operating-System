"""At every start of lamp speech after at least 300 ms of render silence: how many capture blocks the lamp's
voice is in the microphone before the canceller has that render. argv: RUNTIME"""
import json, sys
import numpy as np
rt = sys.argv[1]
cur = []
for line in open(rt + '/capture-audio/events.jsonl'):
    if '"capture"' not in line: continue
    e = json.loads(line)
    if e.get('kind') == 'capture': cur.append(e['meta']['reference']['analysed_through'])
cur = np.array(cur)
pre = np.fromfile(rt + '/capture-audio/pre_aec.pcm16le', dtype='<i2').astype(np.float64)
post = np.fromfile(rt + '/capture-audio/post_aec.pcm16le', dtype='<i2').astype(np.float64)
render = np.fromfile(rt + '/render-audio/render_accepted.pcm16le', dtype='<i2').astype(np.float64)
lv = lambda x, h: 10 * np.log10(np.maximum((x[:len(x) // h * h].reshape(-1, h) ** 2).mean(axis=1), 1e-3) / 32768.0 ** 2)
dpre, dpost, rl = lv(pre, 160), lv(post, 160), lv(render, 240)
out = []; k = 30
while k < len(rl) - 60:
    if rl[k] > -30 and (rl[k - 30:k] < -60).all():
        frame = k * 240; i = int(np.argmax(cur > frame))
        if 12 < i < len(dpre) - 130:
            own = np.median(dpre[i + 20:i + 120]); before = np.median(dpre[i - 12:i - 4])
            if own - before > 15:
                j = i - 12 + int(np.argmax(dpre[i - 12:i + 12] > own - 12))
                out.append({'render_s': round(k / 100, 2), 'capture_block': i, 'voice_leads_by_blocks': i - j, 'own_dbfs': round(float(own), 1),
                            'levels': [round(float(v), 1) for v in dpre[i - 3:i + 2]], 'cursor_minus_frame': [int(v - frame) for v in cur[i - 3:i + 2]]})
        k += 60
    else: k += 1
print(json.dumps(out))
speak = np.array([rl[min(int(c // 240) - 1, len(rl) - 1)] > -40 and rl[max(int(c // 240) - 6, 0)] > -40 for c in cur[:len(dpre)]])
n = min(len(speak), len(dpre), len(dpost))
if speak[:n].any():
    m = lambda d: 10 * np.log10(np.mean(10 ** (d[:n][speak[:n]] / 10)))
    print('lamp sounding, %d blocks: before %.1f dBFS, after %.1f dBFS, removed %.1f dB' % (int(speak[:n].sum()), m(dpre), m(dpost), m(dpre) - m(dpost)))
