# Default-off comparison probes

These Rust programs ran only in the two temporary source copies. They were
not added to the product. Both copies used these exact helper bytes, the same
fixture catalog path, and the exact per-case environments recorded in
`default-off-baseline.json`. The second copy compared raw stdout without
normalization. Existing external Cargo dependencies came from the host cache.

The comparison covers Actions catalog bytes, Voice environment arguments,
declared tools, route choices and ordinary Written serialization. It starts
no provider, Schedule service or audio. Separate runtime tests cover the absent
speaking switch, optional child absence, admission and delivery behavior.

Each of two profiles (ordinary tools with Memory/clock; the same with Tasks)
uses the 6 by 6 Cartesian product of absent, `off`, `ON`, `true`, `1`, and
` on ` values for the two Schedule switches: 72 cases. Both Schedule paths
are deliberately invalid relative paths and must remain inert.

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

Create source-only copies at the baseline commit and the reviewed task commit.
Add each helper above at its named example path. Build each copy with:

```sh
cargo build --offline --locked -p lamp-actions -p lamp-voice --example schedule_off_actions --example schedule_off_voice
```

For each recorded environment, run `schedule_off_actions`, write its exact
stdout as `tools.json` under one common fixture catalog directory, and run
`schedule_off_voice <that-directory> ordinary` or `tasks`. Compare raw stdout
and require exit zero, empty stderr and no created relative Schedule paths.
Using another fixture path changes serialized path bytes; use the same path
for both revisions. The retained raw payloads are deduplicated by SHA-256,
with every original reading mapped in `default-off-baseline-files.json` and
the final comparison report. No payload was normalized or edited.

## Not shown

This metadata comparison does not demonstrate timing, acoustic behavior,
provider acceptance, enabled Tasks execution or any result on a lamp.
