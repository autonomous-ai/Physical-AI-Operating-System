"""The lamp's own voice before and after cancellation in a lab replay, and what the detector scored.
argv: LAB_DIR OUT_DIR(of the lab) PAIR.json BUNDLE.json"""
import json, sys
import numpy as np
lab, out, pair, bundle = sys.argv[1:5]
p = json.load(open(pair)); b = json.load(open(bundle))
pre = np.fromfile(lab + '/pre.pcm16le', dtype='<i2').astype(np.float64)
post = np.fromfile(out + '/post.pcm16le', dtype='<i2').astype(np.float64)
rows = [l.split('\t') for l in open(out + '/features.tsv').read().splitlines()[1:]]
vad = np.zeros(len(pre) // 160); 
for r in rows: vad[int(r[0])] = float(r[1])
db = lambda x: 10 * np.log10(max(float(np.mean(x * x)), 1e-3) / 32768.0 ** 2)
n = [int(round(s * 16000)) for s in p['reply']['copies_start_at_capture_s']]
n3 = n[2] + int(round(p['reply']['copy_3_against_copy_1']['lag_samples']))
res = {}
for name, a, z in (('copy 1 alone', n[0] + 1600, n[0] + int(4.4 * 16000)), ('copy 3 alone', n3 + int(0.8 * 16000), n3 + int(4.4 * 16000))):
    v = vad[a // 160:z // 160]
    res[name] = (round(db(pre[a:z]), 1), round(db(post[a + 144:z + 144]), 1), round(db(pre[a:z]) - db(post[a + 144:z + 144]), 1), int((v >= 0.8).sum()), len(v))
s0 = b['sunset_in_the_clear']['found_at_sample'] // 160
sv = vad[s0:s0 + 188]
print('   '.join('%s: before %.1f after %.1f removed %.1f dB, scored>=0.8 %d/%d' % ((k,) + v) for k, v in res.items()), '  sunset scored>=0.8 %d/188' % int((sv >= 0.8).sum()))
