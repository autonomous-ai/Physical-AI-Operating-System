#!/usr/bin/env bash
# Compare selection and execution without relying on aggregate nextest counts.
# The inventory fields and status records are pinned to cargo-nextest 0.9.148.
set -euo pipefail
export LC_ALL=C

if [[ $# -ne 2 || ( "$1" != inventory && "$1" != executed ) ]]; then
    printf 'usage: %s inventory|executed FILE\n' "$0" >&2
    exit 2
fi
if [[ ! -f "$2" || ! -r "$2" ]]; then
    printf 'test inventory: cannot read %s\n' "$2" >&2
    exit 1
fi

if [[ "$1" == inventory ]]; then
    # Normal JSON parsing overwrites duplicate object keys. Check the stream
    # first so a repeated test record cannot silently replace its predecessor.
    jq --stream -n -e '
        [inputs | select(length == 2) | .[0]]
        | if length == (unique | length) then true
          else error("duplicate inventory field or test identity") end
    ' "$2" >/dev/null
    jq -e -r -s '
        def require($condition; $message):
            if $condition then . else error($message) end;
        def identity:
            type == "string" and length > 0
            and (test("[\\s\\x00-\\x1f\\x7f]") | not);
        def text: type == "string" and length > 0;
        def selected_rows:
            require(length == 1; "expected one inventory JSON document") | .[0]
            | require(type == "object"; "inventory must be an object")
            | require(keys == ["rust-build-meta", "rust-suites", "test-count"];
                "unknown inventory schema")
            | require(."rust-build-meta" | type == "object";
                "missing Rust build metadata")
            | require(."rust-build-meta"."target-directory" | text;
                "missing target directory")
            | require(."rust-suites" | type == "object";
                "rust-suites must be an object")
            | require(."test-count" | type == "number" and . >= 0 and . == floor;
                "invalid test-count")
            | . as $inventory
            | [."rust-suites" | to_entries[]
                | .key as $binary
                | .value
                | require(type == "object"; "test suite must be an object")
                | require(keys == ["binary-id", "binary-name", "binary-path",
                    "build-platform", "cwd", "kind", "package-id", "package-name",
                    "status", "testcases"]; "unknown test suite schema")
                | require((."binary-id" | identity) and ."binary-id" == $binary;
                    "invalid or inconsistent binary identity")
                | require(all([."binary-name", ."binary-path", .cwd,
                    ."package-id", ."package-name"][]; text);
                    "invalid test suite metadata")
                | require(."build-platform" == "host" or ."build-platform" == "target";
                    "unknown build platform")
                | require(.kind as $kind | ["lib", "bin", "test", "example",
                    "bench", "proc-macro"] | index($kind) != null;
                    "unknown binary kind")
                | require(.status == "listed" or .status == "skipped"
                    or .status == "skipped-default-filter"; "unknown suite status")
                | require(.testcases | type == "object";
                    "testcases must be an object")
                | require(.status == "listed" or (.testcases | length == 0);
                    "skipped suite contains test records")
                | .testcases | to_entries[]
                | require(.key | identity; "invalid test identity")
                | .key as $test
                | .value
                | require(type == "object"; "test record must be an object")
                | require(keys == ["filter-match", "ignored", "kind"];
                    "unknown test record schema")
                | require(.kind == "test" or .kind == "bench";
                    "unknown test kind")
                | require(.ignored | type == "boolean"; "ignored must be boolean")
                | require(."filter-match" | type == "object";
                    "filter-match must be an object")
                | require(."filter-match" |
                    if .status == "matches" then keys == ["status"]
                    elif .status == "mismatch" then
                        keys == ["reason", "status"]
                        and (.reason as $reason | ["not-benchmark", "ignored",
                            "string", "expression", "partition", "rerun-already-passed",
                            "default-filter"] | index($reason) != null)
                    else false end; "unknown filter-match schema")
                | {binary: $binary, test: $test,
                    selected: (.ignored == false and ."filter-match".status == "matches")}
            ]
            | require(length == $inventory."test-count";
                "test-count disagrees with test records")
            | map(select(.selected) | [.binary, .test]);
        selected_rows | sort
        | require(length > 0; "inventory selects no tests")
        | require(length == (unique | length); "duplicate selected test identity")
        | .[] | @tsv
    ' "$2"
else
    # Accept a raw nextest log or gh run view --log, whose two tab-separated
    # fields and timestamp precede each original log line. Do not search for
    # PASS inside arbitrary output or confuse progress counters with binary IDs.
    jq -e -r -R -s '
        def require($condition; $message):
            if $condition then . else error($message) end;
        def original_line:
            sub("\r$"; "")
            | sub("^[^\t]+\t[^\t]+\t\uFEFF?[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}(\\.[0-9]+)?Z[ ]"; "");
        ("^[ \\t]*(PASS|LEAK)[ \\t]+\\[[ \\t]*[0-9]+(\\.[0-9]+)?s\\][ \\t]+"
            + "(?:\\([ \\t]*(?<index>[1-9][0-9]*)/(?<total>[1-9][0-9]*)\\)[ \\t]+)?"
            + "(?<binary>[^\\s\\x00-\\x1f\\x7f]+)[ \\t]+(?<test>[^\\s\\x00-\\x1f\\x7f]+)[ \\t]*$")
            as $record
        | [split("\n")[] | original_line
            | select(test("^[ \\t]*(PASS|LEAK)([^A-Za-z_]|$)"))
            | require(test($record); "malformed nextest PASS/LEAK record: " + .)
            | capture($record)
            | require(.index == null or ((.index | tonumber) <= (.total | tonumber));
                "invalid nextest progress counter")
            | [.binary, .test]]
        | sort
        | require(length > 0; "log contains no executed tests")
        | require(length == (unique | length); "duplicate executed test identity")
        | .[] | @tsv
    ' "$2"
fi
