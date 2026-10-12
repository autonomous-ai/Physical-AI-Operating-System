"""Per capture row of a bundle: the frame's own read time against when it was processed, and the reference
counters recorded with it. argv: RUNTIME"""
import json, sys
import numpy as np
rows = []
for line in open(sys.argv[1] + '/capture-audio/events.jsonl'):
    if '"capture"' not in line: continue
    e = json.loads(line)
    if e.get('kind') == 'capture':
        m = e['meta']; r = m['reference']
        rows.append((m['frame_sequence'], m['read_completed_at_us'], m['processing_completed_at_us'], r['capture_blocks_processed'], r['analysed_through'], r['accepted_through'], m['aec_queue_delay_ms'], -1 if m.get('aec_internal_alignment_ms') is None else m['aec_internal_alignment_ms'], e['pre_aec_byte_offset']))
a = np.array(rows, dtype=np.int64)
seq, read, done, counted = a[:, 0], a[:, 1], a[:, 2], a[:, 3]
wait = done - read
pc = lambda v: [int(x) for x in np.percentile(v, [0, 5, 50, 95, 100])]
out = {'rows': len(a), 'first_sequence': int(seq[0]), 'sequences_contiguous': bool((np.diff(seq) == 1).all()),
       'audio_offsets_contiguous': bool((a[:, 8] == np.arange(len(a)) * 320).all()),
       'read_times_increasing': bool((np.diff(read) > 0).all()),
       'between_reads_us_min_5_50_95_max': pc(np.diff(read)),
       'processed_after_own_read_us_min_5_50_95_max': pc(wait),
       'reference_capture_blocks_processed_minus_frame_sequence_values': sorted(set(int(v) for v in counted - seq)),
       'accepted_minus_analysed_frames_5_50_95': [int(x) for x in np.percentile(a[:, 5] - a[:, 4], [5, 50, 95])],
       'delay_hint_ms_values': sorted(set(int(v) for v in a[:, 6]))[:8],
       'aec_internal_alignment_ms_values': sorted(set(int(v) for v in a[:, 7]))[:10]}
print(json.dumps(out, indent=1))
