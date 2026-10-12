"""At each of the three starts of the reply: the capture blocks around it, their level, and how far the
canceller's render cursor was from the first loud render frame of that copy. argv: RUNTIME PAIR.json"""
import json, sys
import numpy as np
rt, pair = sys.argv[1:3]
p = json.load(open(pair)); first = p['reply']['first_render_sample']; COPY, PERIOD = 107_760, 114_960
cur = []
for line in open(rt + '/capture-audio/events.jsonl'):
    if '"capture"' not in line: continue
    e = json.loads(line)
    if e.get('kind') == 'capture': cur.append(e['meta']['reference']['analysed_through'])
cur = np.array(cur)
pre = np.fromfile(rt + '/capture-audio/pre_aec.pcm16le', dtype='<i2').astype(np.float64)
render = np.fromfile(rt + '/render-audio/render_accepted.pcm16le', dtype='<i2').astype(np.float64)
lv = lambda x, h: 10 * np.log10(np.maximum((x[:len(x) // h * h].reshape(-1, h) ** 2).mean(axis=1), 1e-3) / 32768.0 ** 2)
dpre = lv(pre, 160); rl = lv(render[first:first + COPY], 240)
loud = int(np.argmax(rl > -30))          # first render block of a copy over -30 dBFS
out = []
for k in range(3):
    frame = first + k * PERIOD + loud * 240
    i = int(np.argmax(cur > frame))       # first capture block cancelled with that render analysed
    own = np.median(dpre[i + 20:i + 120])
    j = i - 12 + int(np.argmax(dpre[i - 12:i + 12] > own - 12))   # first block within 12 dB of the lamp's own level
    out.append({'copy': k + 1, 'render_block_level_dbfs': round(float(rl[loud]), 1), 'first_capture_block_with_that_render_analysed': i,
                'first_capture_block_within_12_db_of_the_lamp_level': j, 'voice_leads_the_render_by_blocks': i - j,
                'levels_dbfs_from_4_blocks_before': [round(float(v), 1) for v in dpre[i - 4:i + 3]],
                'cursor_minus_that_frame': [int(v - frame) for v in cur[i - 4:i + 3]]})
print(json.dumps(out, indent=1))
