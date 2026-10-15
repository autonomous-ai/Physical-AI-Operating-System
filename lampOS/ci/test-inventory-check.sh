#!/usr/bin/env bash
# Focused format and failure checks; no workspace tests or build are needed.
set -euo pipefail
export LC_ALL=C

ci_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
scratch=$(mktemp -d "${TMPDIR:-/tmp}/lampos-inventory.XXXXXX")
trap 'rm -rf "$scratch"' EXIT

fail() {
    printf 'inventory check failed: %s\n' "$*" >&2
    exit 1
}

reject() {
    local label=$1
    shift
    if bash "$ci_dir/test-inventory.sh" "$@" >"$scratch/actual" 2>"$scratch/error"; then
        fail "$label was accepted"
    fi
    [[ ! -s "$scratch/actual" ]] || fail "$label emitted a partial inventory"
    [[ -s "$scratch/error" ]] || fail "$label did not explain the failure"
}

jq -n '
    def testcase($ignored; $filter):
        {kind: "test", ignored: $ignored, "filter-match": $filter};
    def suite($id; $kind; $tests):
        {"package-name": "lamp-alpha", "binary-id": $id,
         "binary-name": "alpha", "package-id": "path+file:///fixture#lamp-alpha@0.1.0",
         kind: $kind, "binary-path": "/fixture/target/release/deps/alpha",
         "build-platform": "target", cwd: "/fixture", status: "listed", testcases: $tests};
    {"rust-build-meta": {"target-directory": "/fixture/target"}, "test-count": 5,
     "rust-suites": {
        "lamp-alpha": suite("lamp-alpha"; "lib"; {
            "z::last": testcase(false; {status: "matches"}),
            ignored: testcase(true; {status: "matches"}),
            other_partition: testcase(false; {status: "mismatch", reason: "partition"})}),
        "lamp-alpha::bin/lamp-alpha": suite("lamp-alpha::bin/lamp-alpha"; "bin"; {
            "same::name": testcase(false; {status: "matches"})}),
        "lamp-alpha::integration": suite("lamp-alpha::integration"; "test"; {
            "same::name": testcase(false; {status: "matches"})}),
        "lamp-ipc::relay_signals": (suite("lamp-ipc::relay_signals"; "test"; {})
            | .status = "skipped")}}
' >"$scratch/inventory.json"
printf 'lamp-alpha\tz::last\nlamp-alpha::bin/lamp-alpha\tsame::name\nlamp-alpha::integration\tsame::name\n' >"$scratch/expected"
bash "$ci_dir/test-inventory.sh" inventory "$scratch/inventory.json" >"$scratch/actual"
cmp "$scratch/expected" "$scratch/actual"

cat >"$scratch/executed.log" <<'EOF'
Nextest run ID example
        PASS [   0.003s] (   1/3) lamp-alpha z::last
        SKIP [         ] lamp-alpha ignored
        SLOW [  60.000s] lamp-alpha::integration same::name
Summary: PASS in this line is not a test record
EOF
printf '        LEAK [   0.100s] lamp-alpha::integration same::name\r\n' >>"$scratch/executed.log"
printf 'ARM64 (1/4)\ttest\t2026-10-15T07:30:09.7155803Z         PASS [   1.000s] (   3/3) lamp-alpha::bin/lamp-alpha same::name\n' >>"$scratch/executed.log"
bash "$ci_dir/test-inventory.sh" executed "$scratch/executed.log" >"$scratch/actual"
cmp "$scratch/expected" "$scratch/actual"

for mutation in \
    'del(."rust-suites")' \
    '."rust-suites" = []' \
    '."test-count" = 4' \
    '."rust-suites"."lamp-alpha".status = "unknown"' \
    'del(."rust-suites"."lamp-alpha".testcases)' \
    '."rust-suites"."lamp-alpha".testcases."z::last".ignored = "false"' \
    '."rust-suites"."lamp-alpha".testcases."z::last"."filter-match".status = "unknown"' \
    '."rust-suites"."lamp-alpha".testcases.other_partition."filter-match".reason = "unknown"' \
    '."rust-suites"."lamp-alpha".testcases."z::last".new_field = true' \
    '."rust-suites"."lamp-alpha"."binary-id" = "other-binary"' \
    '."rust-suites"."lamp-alpha::integration"."binary-id" = "lamp-alpha::bin/lamp-alpha"' \
    '."rust-suites"."lamp-alpha".testcases["bad\ttest"] = ."rust-suites"."lamp-alpha".testcases."z::last" | ."test-count" = 6' \
    '."rust-suites" |= with_entries(.value.testcases |= with_entries(.value.ignored = true))'; do
    jq "$mutation" "$scratch/inventory.json" >"$scratch/invalid.json"
    reject "$mutation" inventory "$scratch/invalid.json"
done

jq -c . "$scratch/inventory.json" | sed 's/"z::last":/"z::last":{"kind":"test","ignored":false,"filter-match":{"status":"matches"}},"z::last":/' >"$scratch/invalid.json"
reject 'duplicate JSON test key' inventory "$scratch/invalid.json"
cat "$scratch/inventory.json" "$scratch/inventory.json" >"$scratch/invalid.json"
reject 'multiple JSON documents' inventory "$scratch/invalid.json"
printf '{' >"$scratch/invalid.json"
reject 'truncated JSON' inventory "$scratch/invalid.json"
printf '' >"$scratch/empty"
reject 'empty inventory' inventory "$scratch/empty"
reject 'empty log' executed "$scratch/empty"
reject 'missing file' inventory "$scratch/missing"
reject 'unknown command' other "$scratch/inventory.json"

for record in \
    'PASS [ 0.003s] lamp-alpha' \
    'LEAK [ nonsense] lamp-alpha test' \
    'PASS [ 0.003s] ( 4/3) lamp-alpha test' \
    'PASS [ 0.003s] ( 0/3) lamp-alpha test' \
    'PASS [ 0.003s] ( 1/3 lamp-alpha test' \
    'PASS [ 0.003s] lamp-alpha test extra'; do
    cp "$scratch/executed.log" "$scratch/invalid.log"
    printf '%s\n' "$record" >>"$scratch/invalid.log"
    reject "$record" executed "$scratch/invalid.log"
done
cat "$scratch/executed.log" "$scratch/executed.log" >"$scratch/invalid.log"
reject 'duplicate executed test' executed "$scratch/invalid.log"

printf 'test-inventory checks passed\n'
