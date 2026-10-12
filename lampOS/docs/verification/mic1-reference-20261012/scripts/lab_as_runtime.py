"""Lay a lab replay out as a runtime directory, so that the analysis of a device bundle can be run on it:
the device's own events with the lab's score per block, the device's microphone signal, the lab's
cancelled signal. argv: DEVICE_RUNTIME LAB_OUT NEW_RUNTIME"""
import json, os, sys
import numpy as np
rt, lab, new = sys.argv[1:4]
os.makedirs(new + '/capture-audio', exist_ok=True)
vad = {}
for l in open(lab + '/features.tsv').read().splitlines()[1:]:
    c = l.split('\t'); vad[int(c[0])] = float(c[1])
i = 0
with open(new + '/capture-audio/events.jsonl', 'w') as f:
    for line in open(rt + '/capture-audio/events.jsonl'):
        if '"kind":"capture"' in line:
            e = json.loads(line); e['meta']['vad_score'] = vad.get(i, 0.0); i += 1
            line = json.dumps(e, separators=(',', ':')) + '\n'
        f.write(line)
pre = np.fromfile(rt + '/capture-audio/pre_aec.pcm16le', dtype='<i2')
post = np.fromfile(lab + '/post.pcm16le', dtype='<i2')
out = np.zeros(len(pre), dtype='<i2'); out[:min(len(post), len(pre))] = post[:len(pre)]
out.tofile(new + '/capture-audio/post_aec.pcm16le')
for name in ('capture-audio/pre_aec.pcm16le', 'render-audio'):
    if os.path.lexists(new + '/' + name): os.remove(new + '/' + name)
    os.symlink(rt + '/' + name, new + '/' + name)
print(i, 'blocks;', len(vad), 'scored by the lab')
