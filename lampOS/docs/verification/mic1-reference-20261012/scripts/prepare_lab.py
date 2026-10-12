"""Prepare a lab directory from a diagnostic bundle. python3.12 -I.
argv: RUNTIME_DIR LAB_DIR"""
import json, os, sys, shutil
R, L = sys.argv[1], sys.argv[2]
os.makedirs(L, exist_ok=True)
rows = []; resets = 0
for l in open(R + '/capture-audio/events.jsonl'):
    if '"kind":"reset"' in l: resets += 1
    if '"kind":"capture"' not in l: continue
    e = json.loads(l); m = e['meta']
    assert e['pre_aec_byte_offset'] == len(rows) * 320, 'capture blocks are not contiguous'
    rows.append((m['reference']['analysed_through'], m['aec_queue_delay_ms']))
assert resets == 1, f'{resets} resets: this lab handles one epoch'
open(L + '/schedule.tsv', 'w').write('# render cursor analysed before the block, delay hint ms\n' + ''.join(f'{a}\t{h}\n' for a, h in rows))
for src, dst in (('capture-audio/pre_aec.pcm16le', 'pre.pcm16le'), ('render-audio/render_accepted.pcm16le', 'render.pcm16le'), ('capture-audio/post_aec.pcm16le', 'device_post.pcm16le')):
    if os.path.lexists(L + '/' + dst): os.remove(L + '/' + dst)
    os.symlink(R + '/' + src, L + '/' + dst)
print(len(rows), 'blocks')
