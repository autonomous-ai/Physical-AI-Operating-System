"""Two device runs of the sunset/coffee pair side by side. python3.12 -I.
argv: NAME_A A.json NAME_B B.json   (the files native_bundle.py writes)"""
import sys, json, statistics
na, a, nb, b = sys.argv[1], json.load(open(sys.argv[2])), sys.argv[3], json.load(open(sys.argv[4]))
def mixer(d):
    m = (d.get('mixer_snapshot') or {}).get('devices', {})
    return '; '.join(f"{k}: gain control {v.get('auto_gain_control')}, capture {v.get('mic_capture_volume')}" for k, v in m.items()) or 'not given'
def floors(d):
    rows = d['microphone_per_second']['rows']
    first_loud = next((r['second'] for r in rows if r['loud_dbfs'] > -40), len(rows))
    before = [r['floor_dbfs'] for r in rows[:first_loud]]
    reply = next((r['second'] for r in rows if r['reduction_db'] > 5), None)
    after = [r['floor_dbfs'] for r in rows[(reply or 0):] if r['loud_dbfs'] < -60]
    return (round(statistics.median(before), 1) if before else None, round(min(after), 1) if after else None, round(after[-1], 1) if after else None)
def clip_seconds(d):
    c0 = d['coffee_over_the_reply']['placed_at_sample']; end = c0 + d['clips']['coffee_speech_ms'][-1][1] * 16
    rows = d['microphone_per_second']['rows']
    during = [r['reduction_db'] for r in rows if c0 // 16000 + 1 <= r['second'] < end // 16000]
    cal = d.get('calibration') or {}
    s = cal.get('lamp_alone_from_second')
    after = [r['reduction_db'] for r in rows if s is not None and int(s) + 1 <= r['second'] <= int(s) + 4]
    return during, after
def row(label, fa, fb=None):
    fb = fb or fa
    try: va = fa(a)
    except Exception as e: va = f'n/a ({type(e).__name__})'
    try: vb = fb(b)
    except Exception as e: vb = f'n/a ({type(e).__name__})'
    print(f'| {label} | {va} | {vb} |')
print(f'| | {na} | {nb} |'); print('|---|---|---|')
row('Microphone mixer', mixer)
row('Clipped samples in the recording', lambda d: d['microphone_per_second']['clipped_samples'])
row('Quiet floor before any playback, dBFS', lambda d: floors(d)[0])
row('Quiet floor after the lamp has spoken: lowest second, last quiet second, dBFS', lambda d: f'{floors(d)[1]}, {floors(d)[2]}')
row('Sunset question, lamp silent: microphone level, dBFS', lambda d: d['sunset_in_the_clear']['microphone_level_dbfs'])
row('Sunset: matched filter', lambda d: d['sunset_in_the_clear']['matched_filter_peak'])
row('Sunset: device score at 0.5 or over, blocks', lambda d: f"{d['sunset_in_the_clear']['device_score']['from_0_5']} of {d['sunset_in_the_clear']['device_score']['blocks']}")
row('Microphone minus clip level, sunset in the clear, dB', lambda d: d['microphone_minus_clip_level']['sunset_lamp_silent']['median_db'])
row('The same at the end of the coffee clip, median of the 50 ms steps, dB', lambda d: round(statistics.median(x[1] for x in d['microphone_minus_clip_level']['coffee_last_400_ms']), 1))
row('The part of the coffee clip the lamp spoke through, ms', lambda d: d['coffee_while_the_lamp_spoke']['window_ms'][1])
row('Microphone level there, dBFS', lambda d: d['coffee_while_the_lamp_spoke']['microphone_level_dbfs'])
row('Level expected for the clip there from the sunset gain, dBFS (and against everything else, dB)', lambda d: f"{d['coffee_while_the_lamp_spoke']['person_level_predicted_from_the_sunset_gain_dbfs']} ({d['coffee_while_the_lamp_spoke']['person_against_everything_else_predicted_db']})")
row('Coffee: matched filter (and with nothing added, calibration)', lambda d: f"{d['calibration']['real_overlap']['matched_filter']} ({d['calibration']['rows'][0]['matched_filter']})")
row('Coffee: coherent share of the microphone, 32 ms / 128 ms windows, dB', lambda d: f"{d['calibration']['real_overlap']['coherent_share_db_512']} / {d['calibration']['real_overlap']['coherent_share_db_2048']}")
row('Calibration: clip added at the level nearest the coherent share measured (32 ms windows)', lambda d: (lambda rows, real: min(((abs(r['coherent_share_db_512'] - real), f"{r['added_dbfs']} dBFS gives {r['matched_filter']}, {r['coherent_share_db_512']} / {r['coherent_share_db_2048']}") for r in rows if r['added_dbfs'] is not None))[1])(d['calibration']['rows'], d['calibration']['real_overlap']['coherent_share_db_512']))
row('After the canceller, the part the lamp spoke through: level dBFS, coherent share 32 / 128 ms, dB', lambda d: f"{d['coffee_while_the_lamp_spoke']['after_cancellation']['level_dbfs']}, {d['coffee_while_the_lamp_spoke']['after_cancellation']['coherent_share_db_512']} / {d['coffee_while_the_lamp_spoke']['after_cancellation']['coherent_share_db_2048']}")
row('Canceller reduction in the whole seconds of the clip, dB', lambda d: clip_seconds(d)[0])
row('Canceller reduction just after, lamp alone, dB', lambda d: clip_seconds(d)[1])
row('Before the yield: device score under 0.5 in the clip\'s speech blocks', lambda d: f"{d['before_the_yield']['device_score_under_0_5']} of {d['before_the_yield']['speech_blocks_of_the_clip']}")
row('Before the yield, replay hearing: nothing speech-like / own / person / no alignment', lambda d: '{under} / {own} / {over} / {unknown}'.format(**d['before_the_yield']['replay_hearing']))
row('Lamp pauses of 60 ms or more inside the clip speech before the yield: (ms after arrival, length)', lambda d: [(q['ms_after_the_clip_arrived'], q['length_ms']) for q in d['lamp_pauses_before_the_yield']['pauses'] if q['length_ms'] >= 60 and q['inside_the_clip_speech']])
row('Clip position fixed by', lambda d: f"{d['coffee_over_the_reply']['alignment']['by']} ({d['coffee_over_the_reply']['alignment']['shift_ms']:+} ms)")
row('Longest run of device score at 0.8 or over, blocks', lambda d: d['per_block_over_the_clip']['device_score']['longest_run_from_0_8'])
row('Replay hearing over the clip: nothing speech-like / own / person / no alignment', lambda d: '{under} / {own} / {over} / {unknown}'.format(**d['per_block_over_the_clip']['replay_hearing']))
row('Longest run heard as a person, blocks', lambda d: d['per_block_over_the_clip']['replay_hearing']['longest_over_run'])
row('Yields, commits and resumes: ms after the clip arrived (margin dB)', lambda d: '; '.join(f"{s['kind'][7:]} {s['ms_after_the_clip_arrived']} ({s['hearing_margin_db']})" for s in d['trace']['duplex_steps']) or 'none')
row('Clip speech ends at, ms', lambda d: d['trace']['clip_speech_ends_ms'])
row('Candidates rejected / admitted', lambda d: f"{[(r['serial'], r['reason']) for r in d['trace']['rejected']]} / {[(r['serial'], r['turn']) for r in d['trace']['admitted']]}")
row('What the service transcribed as input', lambda d: d['trace']['input_transcripts'])
