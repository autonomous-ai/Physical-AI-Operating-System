"""Pool the coffee-clip lab trials and measure how much of the added speech is
in the canceller's output before the lamp first yields. python3.12 -I.
argv: OUT_DIR SESSION..."""
import sys, json, statistics
import numpy as np
out = sys.argv[1]; sessions = sys.argv[2:]
SEG = [(0, 590), (770, 1140), (1370, 3070), (3280, 4560)]      # active segments of the clip, ms
def blocks(x): return x[:len(x) // 160 * 160].astype(np.float64).reshape(-1, 160)
pooled = {'sessions': {}, 'trials': 0, 'excluded': 0}
rows = []
for s in sessions:
    r = json.load(open(f'{out}/{s}.json'))
    pooled['sessions'][s] = {'replies': len(r['replies']), 'negative_control_yields': r['negative_control']['yields'],
                             'negative_control_commits': r['negative_control']['commits'],
                             'session_prompt_p90_dbfs_median': r['session_prompt_p90_dbfs_median']}
    for t in r['trials']:
        if t.get('excluded'): pooled['excluded'] += 1; continue
        pooled['trials'] += 1
        g = t['gain_db']; at = t['onset_block']
        seg = np.fromfile(f'{out}/{s}.seg.{g:+.0f}.pcm16le', dtype='<i2')
        post = np.fromfile(f"{out}/{s}.r{t['reply']}.g{g:+.0f}/post.pcm16le", dtype='<i2')
        first_yield = next((d for st, d in t['steps'] if st == 'yield'), None)
        end_ms = first_yield if first_yield is not None else min(len(seg) // 16, 4560)
        end_ms = min(end_ms, t['overlap_blocks_before_reply_end'] * 10)
        row = {'session': s, 'reply': t['reply'], 'gain_db': g, 'first_yield_ms': first_yield,
               'steps': t['steps'], 'committed': t['committed'], 'heard_before_yield': t['heard_before_yield']}
        LAG = 144                                                # the canceller's own delay here: 9.0 ms, measured with the lamp silent
        def kept(lo_ms, hi_ms):
            idx = np.concatenate([np.arange(max(lo, lo_ms) * 16, min(hi, hi_ms) * 16) for lo, hi in SEG if min(hi, hi_ms) > max(lo, lo_ms)] or [np.arange(0)])
            idx = idx[idx + at * 160 + LAG < len(post)]
            if len(idx) < 3200: return None                      # under 200 ms of speech: not measured
            a = seg[idx].astype(np.float64); b = post[idx + at * 160 + LAG].astype(np.float64)
            g_ = (a * b).sum() / ((a * a).sum() + 1e-9); proj = g_ * a; rest = b - proj
            return {'speech_ms': int(len(idx) // 16),
                    'level_kept_db': round(float(10 * np.log10(max((proj ** 2).sum(), 1e-9) / (a ** 2).sum())), 1),
                    'against_everything_else_db': round(float(10 * np.log10(max((proj ** 2).sum(), 1e-9) / max((rest ** 2).sum(), 1e-9))), 1)}
        m_ = kept(0, end_ms)
        if m_: row['added_speech_in_cancelled_signal_before_yield'] = dict(window_ms=end_ms, **m_)
        # the same measure where the lamp is silent in the lab: the last sentence, if the commit came before it
        if t['committed'] and t['committed']['commit_delay_ms'] <= 2900:
            c_ = kept(3280, 4560)
            if c_: row['added_speech_in_cancelled_signal_lamp_silent'] = c_
        rows.append(row)
def pool(rs):
    c = [r for r in rs if r['committed']]
    lost = sorted(r['committed']['opening_not_retained_ms'] for r in c)
    fy = sorted(r['first_yield_ms'] for r in rs if r['first_yield_ms'] is not None)
    def q(v, p): return v[min(len(v) - 1, int(p * len(v)))] if v else None
    h = {k: sum(r['heard_before_yield'][k] for r in rs) for k in ('under', 'over', 'own', 'unknown', 'clear_or_still')}
    sounding = h['under'] + h['over'] + h['own'] + h['unknown']
    m = [r['added_speech_in_cancelled_signal_before_yield'] for r in rs if 'added_speech_in_cancelled_signal_before_yield' in r]
    return {
     'trials': len(rs), 'committed_within_clip_plus_1s': len(c), 'not_committed': len(rs) - len(c),
     'never_yielded': sum(1 for r in rs if r['first_yield_ms'] is None),
     'whole_request_retained': sum(x == 0 for x in lost),
     'opening_not_retained_ms': {'n': len(lost), 'median': statistics.median(lost) if lost else None, 'p25': q(lost, .25), 'p75': q(lost, .75),
                                 'max': lost[-1] if lost else None, 'at_least_1000': sum(x >= 1000 for x in lost), 'at_least_2000': sum(x >= 2000 for x in lost)},
     'first_yield_ms': {'n': len(fy), 'median': statistics.median(fy) if fy else None, 'within_290': sum(x <= 290 for x in fy), 'max': fy[-1] if fy else None},
     'first_yield_then_resume': sum(1 for r in rs if len(r['steps']) >= 2 and r['steps'][1][0] == 'resume'),
     'commit_after_one_given_up_yield': sum(1 for r in c if r['committed']['yields_before_it'] == 1),
     'commit_after_two_given_up_yields': sum(1 for r in c if r['committed']['yields_before_it'] >= 2),
     'committed_yield_between_2500_and_2650_ms': [[r['session'], r['reply'], r['gain_db'], r['committed']['its_yield_delay_ms'], r['committed']['commit_delay_ms'], r['committed']['opening_not_retained_ms']] for r in c if 2500 <= r['committed']['its_yield_delay_ms'] <= 2650],
     'heard_before_first_yield': h, 'lamp_sounding_blocks': sounding,
     'heard_while_lamp_sounding_percent': {k: round(100 * h[k] / sounding, 1) for k in ('under', 'over', 'own', 'unknown')} if sounding else None,
     'added_speech_in_cancelled_signal_before_yield': {'trials': len(m),
        'level_kept_db_median': statistics.median(x['level_kept_db'] for x in m) if m else None,
        'level_kept_db_range': [min(x['level_kept_db'] for x in m), max(x['level_kept_db'] for x in m)] if m else None,
        'against_everything_else_db_median': statistics.median(x['against_everything_else_db'] for x in m) if m else None}}
for r in rows:
    r['lamp_sounded_at_every_block_before_the_first_yield'] = r['heard_before_yield']['clear_or_still'] == 0
pooled['the_trials_that_count'] = 'Those in which the lamp sounded at every block from the clip\'s onset to the first yield. Where the recorded reply paused or ended first, the person would be admitted by the ordinary path, which this lab does not model; those trials are pooled apart and not used.'
pooled['lamp_sounding_throughout'] = pool([r for r in rows if r['lamp_sounded_at_every_block_before_the_first_yield']])
pooled['reply_paused_or_ended_first_not_used'] = pool([r for r in rows if not r['lamp_sounded_at_every_block_before_the_first_yield']])
pooled['all_trials_as_first_published'] = pool(rows)
pooled['production_differs'] = 'The lab applies the yield rule without the mask\'s rule that a reply stops yielding after two fruitless yields. A commit after two given-up yields would not happen in production: the reply would play on.'
b_ = [r['added_speech_in_cancelled_signal_lamp_silent'] for r in rows if 'added_speech_in_cancelled_signal_lamp_silent' in r]
pooled['added_speech_in_cancelled_signal_lamp_silent'] = {'trials': len(b_),
    'level_kept_db_median': statistics.median(x['level_kept_db'] for x in b_) if b_ else None,
    'level_kept_db_range': [min(x['level_kept_db'] for x in b_), max(x['level_kept_db'] for x in b_)] if b_ else None,
    'against_everything_else_db_median': statistics.median(x['against_everything_else_db'] for x in b_) if b_ else None}
pooled['measure'] = 'one least-squares gain of the added speech in the cancelled signal over the speech-active samples of the window, at the canceller delay of 144 samples; a linear measure of a non-linear canceller, so compare the two rows with each other, not with zero'
json.dump({'pooled': pooled, 'trials': rows}, open(f'{out}/pooled.json', 'w'), indent=1)
print(json.dumps({k: pooled[k] for k in ('lamp_sounding_throughout', 'reply_paused_or_ended_first_not_used', 'added_speech_in_cancelled_signal_lamp_silent')}, indent=1))
