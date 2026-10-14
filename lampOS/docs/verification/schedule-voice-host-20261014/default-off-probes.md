# Default-off comparison probes

The baseline capture at `7fbb03e1f2e61ad6dfaf184cd768bad6524c2d76`
and the final implementation comparison both passed all 72 cases: 144 exact
stdout byte comparisons, all with empty stderr. These Rust programs ran only
in the two temporary source copies; they were not added to the product. Both
copies used these exact helper bytes, the same baseline fixture paths and the
per-case environments in `default-off-baseline.json`. No stdout was normalized.
External Cargo dependencies came from the host cache.

The capture covers Actions catalog bytes, Voice environment arguments,
declared tools, route choices and ordinary Written serialization. It starts
no provider, Schedule service or audio. The two existing-tool profiles are
ordinary tools with Memory and clock (14 declared tools), and the same profile
with Tasks declarations (16 declared tools). Declaring or routing Tasks tools
does not execute them.

Each profile uses the independent 6 by 6 Cartesian product of absent, `off`,
`ON`, `true`, `1`, and ` on ` values for `LAMP_SCHEDULE_TOOLS` and
`LAMP_SCHEDULE_SPEAK`: 36 cases per profile, 72 total. Both Schedule paths are
deliberately invalid relative paths and remained inert. Every helper exited
zero with empty stderr; all 36 readings within each profile had identical
bytes for each helper. The 144 stdout readings have three distinct payloads.

The previous d5 baseline report, reading map and this probe document are
preserved byte-for-byte under `earlier-d5d765862/`. The archived reading map's
`retained_file` paths remain relative to this verification directory; its
payloads are still retained in `default-off-bytes/`.

## crates/actions/examples/schedule_off_actions.rs

SHA-256: `8fc55e05703d1aa5ca58348132e3f4b0d7eed202e443f608f0d5af562c8f35e1`.

```rust
// Scratch-only baseline helper: no service, socket, hardware or tool execution.
fn main() {
    println!("{}", lamp_actions::catalog_file(&lamp_actions::catalog(true, true)));
}
```

## crates/voice/examples/schedule_off_voice.rs

SHA-256: `12d36f2d201b1117df21a55574855c8b44bcd2ec06f7b6fa980603e809b41695`.

```rust
// Scratch-only baseline helper: reads only the fixture tools.json.
// Declaring/routing existing Tasks tools does not start or contact an agent.
use lamp_voice::{provider_worker::ProviderInput, tools};
use serde_json::json;
fn main() {
    let mut cli = std::env::args().skip(1);
    let path = cli.next().expect("catalog directory");
    let profile = cli.next().expect("ordinary or tasks profile");
    assert!(cli.next().is_none());
    let args = tools::arguments_from_environment();
    let (settings, rest) = tools::take(&args).expect("ordinary settings");
    assert!(rest.is_empty());
    assert_eq!(settings.exec.as_deref(), Some(std::path::Path::new(&path)));
    assert!(settings.decline && settings.clock && settings.memory.is_some());
    assert_eq!(settings.tasks.is_some(), profile == "tasks");
    let names = [
        "not_for_me", "the_time", "remember", "recall", "forget",
        "volume_up", "volume_down", "volume_mute", "volume_unmute",
        "light_on", "light_off", "light_colour", "light_brighter", "light_dimmer",
        "start_agent_task", "confirm_agent_task", "ordinary_unknown_tool",
    ];
    let routes: Vec<_> = names.iter().map(|name| json!({"name":name,"route":format!("{:?}", tools::route(&settings,name))})).collect();
    let wire: ProviderInput = serde_json::from_str(r#"{"kind":"written","request":7,"session":9,"privacy_generation":11,"text":"fixture","look":false}"#).unwrap();
    println!("{}", json!({"arguments":args,"tools":tools::declared(&settings),"routes":routes,"written":serde_json::to_string(&wire).unwrap()}));
}
```

## Reproduction

The completed baseline uses the source-only archive of commit
`7fbb03e1f2e61ad6dfaf184cd768bad6524c2d76`, extracted to:

```text
/private/tmp/lamp-third-schedule-7fbb03e1f-baseline-kg0lzf0c/lampOS
```

Its 1,436 original source files and the two added helper files are hashed in
`default-off-baseline.json`. Offline Cargo metadata resolved 34 local package
manifests, all inside that copy. The first helper build passed with this
working directory and `CARGO_TARGET_DIR` set to its own `target` subdirectory:

```sh
cargo build --offline --locked -p lamp-actions -p lamp-voice --example schedule_off_actions --example schedule_off_voice
```

The capture runner command was executed from
`/Users/d/code/autonomous-os`:

```sh
python3 /private/tmp/lamp-third-schedule-7fbb03e1f-baseline-capture.py --parent-cargo-released > /private/tmp/lamp-third-schedule-7fbb03e1f-baseline-capture-first.txt 2>&1
```

Only provisioned `HOME`, `PATH`, `LANG`, `LC_ALL`, `TMPDIR`, `CARGO_HOME` and
`RUSTUP_HOME` values were inherited when present. The helpers inherited no
provider or account configuration. Each case then supplied these values:

```text
LAMP_TOOLS=on
LAMP_ACTIONS_DIR=/private/tmp/lamp-third-schedule-7fbb03e1f-baseline-kg0lzf0c/fixed-comparison-context/executive
LAMP_MEMORY_SOCKET=/private/tmp/lamp-third-schedule-7fbb03e1f-baseline-kg0lzf0c/fixed-comparison-context/absent-memory.sock
LAMP_CLOCK=on
LAMP_SCHEDULE_SOCKET=relative/invalid-schedule.sock
LAMP_SCHEDULE_DELIVERY_DIR=relative/invalid-delivery
```

The Tasks profile also supplied:

```text
LAMP_TASKS_DIR=/private/tmp/lamp-third-schedule-7fbb03e1f-baseline-kg0lzf0c/fixed-comparison-context/absent-tasks
LAMP_TASKS_TARGET=office
```

For each of the 72 environments, run `target/debug/examples/schedule_off_actions`
from that copy, write its exact stdout to `tools.json` under the common
fixture catalog directory, and run
`target/debug/examples/schedule_off_voice <that-directory> ordinary` or
`tasks`. Each invocation has a 10-second process timeout. Require exit zero,
empty stderr and no created relative Schedule paths. The per-case report
records the exact argv, Schedule environment values, stdout path, byte count
and SHA-256. An absent switch is omitted from the environment, not set to an
empty string. The spaces in ` on ` are literal.

For the final comparison, build the reviewed implementation in its own
source-only copy with its own target directory and the exact helpers above.
Keep the same baseline fixture catalog and all recorded environment path
values. Compare each Actions stdout to its matching baseline before running
Voice with the unchanged baseline catalog. Compare Voice stdout byte-for-byte,
and require the same process and path checks. Using another fixture path
changes serialized path bytes, so it cannot establish exact equivalence to
these retained readings. The completed comparison is retained in
`default-off-comparison.json`, with the exact source-copy report in
`standalone-final.json` and all final readings in `default-off-final-files.json`.

The retained stdout payloads are deduplicated by SHA-256, with all 144 original
readings mapped in `default-off-baseline-files.json`. No payload was normalized
or edited. The first capture, build and preparation reports are retained as
`raw/lamp-third-schedule-7fbb03e1f-baseline-{capture,build,prepare}-first.txt`.

## Completed final comparison

The final source-only copy was `/private/tmp/lamp-third-schedule-voice-7f-standalone-ttewwzqn/lampOS`. It contains 1,454
original source files after 46 exact task overlays, plus the two scratch helpers.
All 675 frozen Rust/manifest paths matched the source map before and after
checks; all source/helper bytes remained unchanged. The exact source-map
SHA-256 is `0312a1dbbef6fd5a3374272454123dfc1b5d7eaf6633a98b948aa8ccf79a48d4`. The exact baseline-report SHA-256 is
`6304d8fb2486646e7bd4320a048b01a4ad05a5220f8827f85e74a613c45e8926`. Both helper hashes above are unchanged.

The authorized runner command, from `/Users/d/code/autonomous-os`, was:

```sh
python3 /private/tmp/lamp-third-schedule-voice-7f-standalone-run.py --parent-cargo-released > /private/tmp/lamp-third-schedule-voice-7f-standalone-run-first.txt 2>&1
```

`standalone-summary.md` records the six build/test commands, scope and counts.
`standalone-evidence-index.json` maps original report/log/reading paths to
retained files, and explicitly identifies excluded archives and build outputs.
The run used only its own target directory; all 34 resolved local package
manifests stayed inside the standalone source copy. The two optional timing
probes were ignored in this test run and are not a latency measurement.

## Not shown

This metadata capture on Darwin 25.6.0, x86_64, Rust 1.96.0 does not demonstrate
runtime latency, acoustic behavior, provider acceptance, enabled Tasks
execution or any result on a lamp. The helper build duration is not an
interaction measurement. The default-off metadata bytes matched across these two revisions. The full
host qualification remains red because of the separately retained inherited
motor-test failures; this passing subset does not override that result.
