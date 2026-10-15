import collections
import decimal
import hashlib
import json
import pathlib
import re
import subprocess

ROOT = pathlib.Path('/private/tmp/tests-823-native-20261016')
PREFIX = '909-38820115320'
TREE = pathlib.Path('/private/tmp/lampos-tests-serve-release')
HEAD = '8af53fd459795029f6277679ded49bf7826def73'
CURRENT = 'bce32d696fe4bb90db172bfab580619dbf5f11c8'
BASE = 'd60124f8280f9c76576d6ca7bf859bc5c3983828'
EVIDENCE = 'lampOS/docs/verification/serve-all-cases-20261016/'
D = decimal.Decimal

def date():
    return subprocess.check_output(['date', '+%Y-%m-%d %H:%M:%S %z'], text=True).strip()

def sha(data):
    return hashlib.sha256(data).hexdigest()

def git(*args):
    return subprocess.check_output(['git', *args], cwd=TREE)

def blob(path, ref=HEAD):
    return git('show', ref + ':' + path)

def load(suffix):
    return json.loads((ROOT / (PREFIX + suffix)).read_bytes())

def save(suffix, value):
    p = ROOT / (PREFIX + suffix)
    with p.open('x') as f:
        json.dump(value, f, indent=2)
        f.write('\n')
    return {'path': str(p), 'sha256': sha(p.read_bytes()), 'bytes': p.stat().st_size}

started = date()
raw_path = ROOT / (PREFIX + '.retrieved.log')
raw = raw_path.read_bytes()
lines = [re.sub(r'\x1b\[[0-9;]*[A-Za-z]', '', x) for x in raw.decode().splitlines()]
run, api, jobs, commit = [load('.retrieved.' + k + '.json') for k in ('run-final', 'run-api', 'jobs-api', 'commit-api')]
assert run['headSha'] == api['head_sha'] == commit['sha'] == HEAD
assert run['attempt'] == api['run_attempt'] == 1
assert run['status'] == api['status'] == 'completed'
assert run['conclusion'] == api['conclusion'] == 'success'
assert run['event'] == api['event'] == 'workflow_dispatch'
assert jobs['total_count'] == len(jobs['jobs']) == 5
assert commit['tree']['sha'] == git('rev-parse', HEAD + '^{tree}').decode().strip()

rows, checkouts, commands, starts, summaries = [], [], {}, {}, {}
row_pattern = re.compile(r'\b(PASS|FAIL|LEAK)\s+\[\s*([\d.]+)s\]\s+\(\s*(\d+)\s*/\s*(\d+)\s*\)\s+(\S+)\s+(\S+)\s*$')
start_pattern = re.compile(r'Starting (\d+) tests across (\d+) binaries \((\d+) tests and (\d+) binaries skipped\)')
summary_pattern = re.compile(r'Summary \[\s*([\d.]+)s\] (\d+) tests run: (\d+) passed, (\d+) skipped')
for i, line in enumerate(lines, 1):
    sm = re.search(r'\((\d)/4\)', line.split('\t')[0])
    if not sm:
        continue
    shard = int(sm[1])
    payload = line.split('\t', 2)[2]
    timestamp = payload.split(' ', 1)[0]
    if '[command]/usr/bin/git log -1 --format=%H' in line:
        following = lines[i]
        checkout_head = following.rsplit(' ', 1)[-1]
        assert checkout_head == HEAD
        checkouts.append({'shard': shard, 'command_line': i, 'head_line': i + 1, 'head': checkout_head, 'raw_command': line, 'raw_result': following})
    if '##[group]Run echo ' in line and 'cargo nextest run' in line:
        assert shard not in commands
        assert '--profile default' in line and '-j 1' in line and f'--partition hash:{shard}/4' in line
        assert '--retries' not in line
        commands[shard] = {'raw_line': line, 'line': i}
    m = start_pattern.search(line)
    if m:
        assert shard not in starts
        starts[shard] = {'tests': int(m[1]), 'binaries': int(m[2]), 'skipped_tests': int(m[3]), 'skipped_binaries': int(m[4]), 'timestamp': timestamp, 'line': i}
    m = summary_pattern.search(line)
    if m:
        assert shard not in summaries
        summaries[shard] = {'seconds': m[1], 'tests': int(m[2]), 'passed': int(m[3]), 'skipped': int(m[4]), 'timestamp': timestamp, 'line': i}
    m = row_pattern.search(line)
    if m:
        rows.append({'binary': m[5], 'test': m[6], 'status': m[1], 'seconds': m[2], 'shard': shard, 'ordinal': int(m[3]), 'total': int(m[4]), 'line': i, 'timestamp': timestamp})

assert len(checkouts) == 4 and {c['shard'] for c in checkouts} == {1, 2, 3, 4}
assert len(commands) == len(starts) == len(summaries) == 4
identities = collections.Counter((r['binary'], r['test']) for r in rows)
duplicates = [list(k) for k, v in identities.items() if v != 1]
assert not duplicates
assert len(rows) == 5205 and all(r['status'] == 'PASS' for r in rows)
shards = []
for shard in range(1, 5):
    rs = [r for r in rows if r['shard'] == shard]
    n = starts[shard]['tests']
    assert [r['ordinal'] for r in rs] == list(range(1, n + 1))
    assert all(r['total'] == n for r in rs)
    assert n == len(rs) == summaries[shard]['tests'] == summaries[shard]['passed']
    assert starts[shard]['skipped_tests'] == summaries[shard]['skipped']
    assert n + starts[shard]['skipped_tests'] == 5242
    job = next(j for j in jobs['jobs'] if j['name'].endswith(f'({shard}/4)'))
    assert job['conclusion'] == 'success' and job['status'] == 'completed'
    shards.append({'shard': shard, 'job_id': job['id'], 'started_at_api': job['started_at'], 'completed_at_api': job['completed_at'], 'command': commands[shard], 'start': starts[shard], 'summary': summaries[shard], 'rows': n, 'unique': n, 'ordinals_complete_in_order': True, 'first_ordinal': 1, 'last_ordinal': n, 'per_case_seconds_sum': str(sum((D(r['seconds']) for r in rs), D(0)))})

fixed_path = ROOT / '919-root-independent-full-audit.json'
fixed = json.loads(fixed_path.read_bytes())
fixed_ids = {(r['binary'], r['test']) for r in fixed['original123_results']}
inventory_ref = '4596d2cac9e25e31bc2b3509ad565ffde66a5ab1'
inventory_path = 'lampOS/docs/verification/realtime-inventory-20261015/inventory.json'
inventory_bytes = blob(inventory_path, inventory_ref)
inventory = json.loads(inventory_bytes)
assert sha(inventory_bytes) == fixed['original_inventory_sha256']
assert fixed_ids == {(r['binary'], r['test']) for r in inventory['tests']} and len(fixed_ids) == 123
identity_bytes = blob(EVIDENCE + 'current/root-identity-proof.json')
identity = json.loads(identity_bytes)
new_ids = {tuple(r) for r in identity['new']}
assert len(new_ids) == 121
serve_bytes = blob(EVIDENCE + 'integration/original-serve-preservation.json')
serve = json.loads(serve_bytes)
serve_ids = {('lamp-voice-eval::serve_session', n) for n in serve['unignored_unconditional_identities']}
assert len(serve_ids) == 14 and serve_ids <= fixed_ids
list_bytes = blob(EVIDENCE + 'current/root-list.log')
compiled = json.loads(next(l for l in list_bytes.decode().splitlines() if l.startswith('{')))
compiled_ids = {(suite['binary-id'], test) for suite in compiled['rust-suites'].values() for test in suite['testcases']}
assert len(compiled_ids) == compiled['test-count'] == 498
assert all(not tc['ignored'] for suite in compiled['rust-suites'].values() for tc in suite['testcases'].values())
assert {k for k in identities if k[0].startswith('lamp-voice-eval')} == compiled_ids
assert new_ids <= compiled_ids

def selection(ids):
    selected = sorted((r for r in rows if (r['binary'], r['test']) in ids), key=lambda r: (r['binary'], r['test']))
    assert len(selected) == len(ids) and all(identities[k] == 1 for k in ids)
    return {'count': len(selected), 'all_once': True, 'all_pass': all(r['status'] == 'PASS' for r in selected), 'per_case_seconds_sum': str(sum((D(r['seconds']) for r in selected), D(0))), 'results': selected}

original, added, serve_group, evaluator = map(selection, [fixed_ids, new_ids, serve_ids, compiled_ids])
recorder_names = {'a_room_recording_that_ends_early_withholds_the_rest', 'a_recorder_that_exits_zero_mid_conversation_withholds_the_rest_and_certifies_nothing', 'a_declared_capture_end_before_the_next_prompt_and_its_window_withholds_it'}
recorder_ids = {k for k in serve_ids if k[1] in recorder_names}
slow_ids = {k for k in serve_ids if k[1] == 'startup_cues_stay_fresh_through_a_slow_transport_and_a_slow_first_preparer'}
serve_subgroups = {'original_ten': selection(serve_ids - recorder_ids - slow_ids), 'recorder_three': selection(recorder_ids), 'slow_transport_one': selection(slow_ids)}
assert [serve_subgroups[k]['count'] for k in ('original_ten', 'recorder_three', 'slow_transport_one')] == [10, 3, 1]
by_binary = {binary: selection({k for k in fixed_ids if k[0] == binary}) for binary in sorted({k[0] for k in fixed_ids})}

proof_bytes = blob(EVIDENCE + 'current/final-source-proof.json')
proof = json.loads(proof_bytes)
source_files = []
for path, expected in proof['final_owned_sha256'].items():
    actual = blob(path)
    assert sha(actual) == expected
    if path.endswith('.rs'):
        current = blob(path, CURRENT)
        assert current == actual
        source_files.append({'path': path, 'sha256': expected, 'git_blob_8af': git('rev-parse', HEAD + ':' + path).decode().strip(), 'bytes': len(actual), 'current_bce_bytes_exact': True})
assert len(source_files) == 25
workflow = blob('.github/workflows/lampos-arm64.yml')
nextest = blob('lampOS/.config/nextest.toml')
assert workflow == blob('.github/workflows/lampos-arm64.yml', BASE)
assert nextest == blob('lampOS/.config/nextest.toml', BASE)
assert not re.search(rb'^\s*retries\s*=', nextest, re.M)
ignored = git('grep', '-n', '-E', r'#\[ignore', HEAD, '--', 'lampOS/crates').decode().splitlines()
base_ignored = git('grep', '-n', '-E', r'#\[ignore', BASE, '--', 'lampOS/crates').decode().splitlines()
assert [l.split(':', 1)[1] for l in ignored] == [l.split(':', 1)[1] for l in base_ignored]
assert len(ignored) == 37

doc_summaries, doc_cases, harness_cases = [], [], []
doc_mode = False
doc_crate = None
for i, line in enumerate(lines, 1):
    if '(1/4)' not in line.split('\t')[0]:
        continue
    if 'Doc-tests ' in line:
        doc_mode = True
        doc_crate = line.rsplit('Doc-tests ', 1)[1]
    if 'Running tests/relay_signals.rs' in line:
        doc_mode = False
    if doc_mode and (m := re.search(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;', line)):
        doc_summaries.append({'crate': doc_crate, 'passed': int(m[1]), 'failed': int(m[2]), 'ignored': int(m[3]), 'line': i})
    if doc_mode and re.search(r' test .* \.\.\. ok\s*$', line):
        doc_cases.append({'line': i, 'raw': line})
    if m := re.search(r' test (\S+) \.\.\. ok \((\d+) ms\)$', line):
        harness_cases.append({'test': m[1], 'milliseconds': int(m[2]), 'line': i})
assert sum(d['passed'] for d in doc_summaries) == len(doc_cases) == 4
assert all(d['failed'] == d['ignored'] == 0 for d in doc_summaries)
assert len(harness_cases) == len({r['test'] for r in harness_cases}) == 12
relay_ok = [{'line': i, 'raw': line} for i, line in enumerate(lines, 1) if line.endswith('relay_signals: ok')]
process_ok = [{'line': i, 'raw': line} for i, line in enumerate(lines, 1) if line.endswith('test result: 12 passed; 0 failed')]
assert len(relay_ok) == len(process_ok) == 1

release_manifest = pathlib.Path('/private/tmp/tests-823-serve-release-publication-files.json')
release = json.loads(release_manifest.read_bytes())
assert len(release['manifest']) == 214
for item in release['manifest']:
    assert sha((TREE / item['path']).read_bytes()) == item['sha256']
    assert sha(blob(item['path'], CURRENT)) == item['sha256']

captures = {k: load('.capture-' + k + '.json') for k in ('log', 'run-final', 'run-api', 'jobs-api', 'commit-api')}
retrieved = {k: load('.retrieved.capture-' + k + '.json') for k in captures}
assert all(c['exit_code'] == 1 and c['stdout_bytes'] == 0 for c in captures.values())
assert all(c['exit_code'] == 0 and c['stdout_bytes'] > 0 for c in retrieved.values())
for capture in [*captures.values(), *retrieved.values()]:
    for stream in ('stdout', 'stderr'):
        assert sha(pathlib.Path(capture[stream]).read_bytes()) == capture[stream + '_sha256']

source_proof = {'audited_from_date': started, 'head': HEAD, 'local_tree': git('rev-parse', HEAD + '^{tree}').decode().strip(), 'github_commit_tree': commit['tree']['sha'], 'all_four_checkout_heads_exact': True, 'checkouts': sorted(checkouts, key=lambda r: r['shard']), 'source_files': source_files, 'all25_current_bce_rust_files_exact_to_8af': True, 'current_head': CURRENT, 'qualification_limit': 'This full workflow ran 8af, not bce. The current integration inherited changed Voice, Memory and lockfile inputs and needs its own native qualification.', 'workflow_sha256': sha(workflow), 'nextest_config_sha256': sha(nextest), 'workflow_and_nextest_unchanged_from_d601': True, 'retry_settings_added': False, 'ignored_attribute_source_count': 37, 'ignored_source_attributes_exact_to_d601': True, 'ignored_source_locations': ignored, 'static_ignore_limit': 'The 37 unchanged source ignore attributes are consistent with the 5242-list/5205-result difference; no complete native compiled manifest of those 37 identities was captured by this workflow.', 'all214_current_publication_paths_match_manifest_and_bce_commit': True, 'release_publication_manifest_sha256': sha(release_manifest.read_bytes())}
save('.source-proof.json', source_proof)
save('.results.json', {'raw_log_sha256': sha(raw), 'method': row_pattern.pattern, 'rows': rows})
audit = {'audited_started_from_date': started, 'audited_finished_from_date': date(), 'run': {k: run[k] for k in ('databaseId', 'headSha', 'attempt', 'status', 'conclusion', 'event', 'createdAt', 'startedAt', 'updatedAt', 'url')}, 'raw_log': str(raw_path), 'raw_log_bytes': len(raw), 'raw_log_sha256': sha(raw), 'raw_log_lines': len(lines), 'method': 'Parse complete unedited gh log with explicit result ordinals; no summary-only qualification; match original inventory and pinned compiled evaluator identities.', 'pass_rows': len(rows), 'fail_rows': 0, 'leak_rows': 0, 'unique_result_identities': len(identities), 'duplicates': duplicates, 'all_shard_ordinals_complete': True, 'shards': shards, 'native_listed_count_from_each_shard': 5242, 'unexecuted_listed_difference': 37, 'skip_limit': source_proof['static_ignore_limit'], 'original123': original, 'original123_by_binary': by_binary, 'new121': added, 'serve14': serve_group, 'serve_subgroups': serve_subgroups, 'evaluator498': {'count': evaluator['count'], 'all_once': True, 'all_pass': True, 'per_case_seconds_sum': evaluator['per_case_seconds_sum']}, 'membership_sources': {'original123_authoritative_path': str(fixed_path), 'original123_authoritative_sha256': sha(fixed_path.read_bytes()), 'frozen_inventory_commit': inventory_ref, 'frozen_inventory_path': inventory_path, 'frozen_inventory_sha256': sha(inventory_bytes), 'membership_same_as_frozen_inventory': True, 'new121_commit': HEAD, 'new121_path': EVIDENCE + 'current/root-identity-proof.json', 'new121_sha256': sha(identity_bytes), 'compiled498_path': EVIDENCE + 'current/root-list.log', 'compiled498_sha256': sha(list_bytes), 'serve14_path': EVIDENCE + 'integration/original-serve-preservation.json', 'serve14_sha256': sha(serve_bytes)}, 'additional_tests': {'doc_passed': 4, 'doc_summaries': doc_summaries, 'doc_cases': doc_cases, 'relay_signals': relay_ok, 'turn_end_wire_process_passed': 12, 'turn_end_wire_process_cases': harness_cases, 'turn_end_wire_process_summary': process_ok}, 'initial_failed_captures_preserved': captures, 'successful_distinct_retrievals': retrieved, 'source_proof_file': PREFIX + '.source-proof.json', 'timing_limit': 'Per-test nextest durations summed across four concurrent, internally serial shards; not one standalone serial elapsed measurement, not a paired before/after experiment, and not a device result.', 'current_head_limit': source_proof['qualification_limit'], 'first_full_failure_preserved': {'run': 38801026642, 'head_evidence_commit': 'd9cc28b141e317c51e71a0d06b450be628103cc7', 'path_prefix': 'lampOS/docs/verification/clock-migration-integration-20261016-0350/native/909-', 'limit': 'This subsequent successful first attempt does not establish a fix for Body #914.'}, 'original123_standalone_serial_under_two_minutes_proved': False, 'no_workflow_rerun': True, 'no_source_edits_or_git_mutations': True}
output = save('.audit.json', audit)
print(json.dumps({'audit': output, 'original123_sum': original['per_case_seconds_sum'], 'serve14_sum': serve_group['per_case_seconds_sum'], 'subgroups': {k: v['per_case_seconds_sum'] for k, v in serve_subgroups.items()}, 'new121_sum': added['per_case_seconds_sum'], 'finished': audit['audited_finished_from_date']}, indent=2))
