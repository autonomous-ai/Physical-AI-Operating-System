import json, sys
for p in sys.argv[1:]:
    r = json.load(open(p)); print('=' * 12, p.split('/')[-1])
    for k in ('reply', 'clip_starts_seconds_into_the_second_copy', 'clip_alone_in_the_lamp_pause', 'original_request_lamp_silent', 'lamp_own_voice_alone', 'method_floor'):
        print(k, json.dumps(r[k]))
    for row in r['clip_over_the_second_copy']:
        print(' -', row['stretch'], row['clip_ms'], 'at', row['seconds_into_the_copy'], '| expected: sunset gain', row['clip_level_expected_from_the_sunset_gain_dbfs'], 'own gain in pause', row['clip_level_expected_from_its_own_gain_in_the_lamp_pause_dbfs'], '| lamp own', row['lamp_own_voice_dbfs'])
        print('     before:', json.dumps(row['before_cancellation']))
        print('     after :', json.dumps(row['after_cancellation']))
        print('     score :', json.dumps(row['device_score_blocks']))
    print('pause', json.dumps(r['clip_in_the_lamp_pause_after_the_second_copy']))
