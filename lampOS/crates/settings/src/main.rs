//! Local operator CLI; all mutations require an explicit compare-and-set guard.
use lamp_settings::{
    client,
    engine::Engine,
    model::{Catalog, Entry, Guard, Mutation},
    protocol::{MAX_REQUEST_BYTES, Request, refusal},
    runtime::{self, Config},
    store::{FileStore, MAX_STATE_BYTES},
};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    env,
    io::{self, Write},
    path::PathBuf,
    process::ExitCode,
    sync::{Arc, atomic::AtomicBool},
    time::{Duration, Instant},
};

const USAGE: &str = "\
usage: lamp-settings COMMAND [OPTIONS]
  catalog
      Print the first three proposed declarations. No owner reads these yet.
  serve [--enable] --dir ABSOLUTE_PRIVATE_DIR --catalog ABSOLUTE_PRIVATE_FILE
        [--bus-root ABSOLUTE_DIR] [--seconds N]
      Without --enable: no file, socket, worker or Bus activity.
      With it: keep desired settings in DIR/settings.json and answer on
      DIR/settings.sock. The existing directory and catalog must be 0700/0600.
  read --socket ABSOLUTE_PATH
      Read the complete declaration and last durably saved values.
  status --socket ABSOLUTE_PATH
      Read health and progress without setting values.
  set --socket ABSOLUTE_PATH --name OWNER.KEY --value TYPED_JSON
      --store-id ID --revision N --request-id ID
      Atomically save one value using the store and revision previously read.
  apply --socket ABSOLUTE_PATH --file ABSOLUTE_PRIVATE_FILE
      Save one atomic Mutation JSON document containing guard and changes.

No command applies a value to hardware or another component. Save success is
durability, not consumer adoption. After a lost reply, reuse the SAME request
ID, revision and values. Never guess success or retry with a new guard.
";

#[derive(Default)]
struct Arguments {
    options: BTreeMap<String, String>,
    enabled: bool,
}

impl Arguments {
    fn parse(mut given: impl Iterator<Item = String>) -> Result<Self, String> {
        let mut result = Self::default();
        while let Some(key) = given.next() {
            if key == "--enable" {
                if result.enabled {
                    return Err("--enable given twice".into());
                }
                result.enabled = true;
                continue;
            }
            if ![
                "--dir",
                "--catalog",
                "--bus-root",
                "--seconds",
                "--socket",
                "--name",
                "--value",
                "--store-id",
                "--revision",
                "--request-id",
                "--file",
            ]
            .contains(&key.as_str())
            {
                return Err("unknown argument".into());
            }
            let value = given
                .next()
                .ok_or_else(|| format!("{key} requires a value"))?;
            if result.options.insert(key.clone(), value).is_some() {
                return Err(format!("{key} given twice"));
            }
        }
        Ok(result)
    }
    fn take(&mut self, name: &str) -> Option<String> {
        self.options.remove(name)
    }
    fn required(&mut self, name: &str) -> Result<String, String> {
        self.take(name).ok_or_else(|| format!("{name} is required"))
    }
    fn finish(self) -> Result<(), String> {
        if !self.options.is_empty() || self.enabled {
            return Err("argument does not belong to this command".into());
        }
        Ok(())
    }
}

fn output(body: &Value) -> io::Result<()> {
    let mut stdout = io::stdout().lock();
    serde_json::to_writer(&mut stdout, body)?;
    writeln!(stdout)
}

fn run() -> Result<bool, String> {
    let mut given = env::args().skip(1);
    let Some(command) = given.next() else {
        return Err("no command".into());
    };
    if command == "--help" || command == "-h" {
        print!("{USAGE}");
        return Ok(true);
    }
    let mut arguments = Arguments::parse(given)?;
    if command == "catalog" {
        arguments.finish()?;
        output(&serde_json::to_value(Catalog::first_three()).unwrap())
            .map_err(|e| e.to_string())?;
        return Ok(true);
    }
    if command == "serve" {
        let enabled = std::mem::take(&mut arguments.enabled);
        let dir = arguments.take("--dir").map(PathBuf::from);
        let catalog = arguments.take("--catalog").map(PathBuf::from);
        let bus_root = arguments.take("--bus-root").map(PathBuf::from);
        let seconds = arguments
            .take("--seconds")
            .map(|v| v.parse::<u64>())
            .transpose()
            .map_err(|_| "invalid seconds")?;
        arguments.finish()?;
        if !enabled {
            output(&json!({"enabled":false,"reason":"--enable was not set"}))
                .map_err(|e| e.to_string())?;
            return Ok(true);
        }
        if seconds.is_some_and(|s| s == 0 || s > 86_400) {
            return Err("--seconds must be 1..86400".into());
        }
        let dir = dir.ok_or("--dir is required with --enable")?;
        let catalog = catalog.ok_or("--catalog is required with --enable")?;
        let socket = dir.join("settings.sock");
        client::validate_socket(&socket).map_err(|e| e.to_string())?;
        if let Some(root) = &bus_root {
            lamp_bus::Bus::new(root).map_err(|e| e.to_string())?;
        }
        // The same hardened bounded reader used by Records. A malformed or
        // unsafe declaration is rejected before the store is opened.
        let catalog: Catalog =
            lamp_records::json_file::read(&catalog, MAX_STATE_BYTES).map_err(|e| e.to_string())?;
        catalog.check()?;
        let store = FileStore::open(&dir).map_err(|e| e.to_string())?;
        let engine = Engine::open(store, catalog).map_err(|e| e.to_string())?;
        let stop = Arc::new(AtomicBool::new(false));
        let term = signal_hook::flag::register(signal_hook::consts::SIGTERM, stop.clone())
            .map_err(|e| e.to_string())?;
        let int = signal_hook::flag::register(signal_hook::consts::SIGINT, stop.clone())
            .map_err(|e| e.to_string())?;
        let result = runtime::serve(
            engine,
            Config {
                socket,
                bus_root,
                run_for: seconds.map(Duration::from_secs),
            },
            stop,
        );
        signal_hook::low_level::unregister(term);
        signal_hook::low_level::unregister(int);
        let finished = result.map_err(|e| e.to_string())?;
        output(&serde_json::to_value(&finished).unwrap()).map_err(|e| e.to_string())?;
        return Ok(finished.owner_stopped && finished.reporter_stopped && finished.reliable);
    }
    let socket = PathBuf::from(arguments.required("--socket")?);
    let request = match command.as_str() {
        "read" => Request::Read {},
        "status" => Request::Status {},
        "set" => {
            let name = arguments.required("--name")?;
            let raw = arguments.required("--value")?;
            if raw.len() > MAX_REQUEST_BYTES {
                return Err("value exceeds request bound".into());
            }
            let value = serde_json::from_str(&raw).map_err(|_| "invalid typed value")?;
            let store_id = arguments.required("--store-id")?;
            let revision = arguments
                .required("--revision")?
                .parse()
                .map_err(|_| "invalid revision")?;
            let request_id = arguments.required("--request-id")?;
            Request::Set {
                mutation: Mutation {
                    guard: Guard {
                        store_id,
                        revision,
                        request_id,
                    },
                    changes: vec![Entry { name, value }],
                },
            }
        }
        "apply" => {
            let path = PathBuf::from(arguments.required("--file")?);
            let mutation = lamp_records::json_file::read(&path, MAX_REQUEST_BYTES)
                .map_err(|e| e.to_string())?;
            Request::Set { mutation }
        }
        _ => return Err("unknown command".into()),
    };
    arguments.finish()?;
    let body = match client::exchange(
        &socket,
        &serde_json::to_value(request).unwrap(),
        Instant::now() + Duration::from_millis(1600),
    ) {
        Ok(body) => body,
        Err(error) => {
            let mut body = refusal(
                error.code(),
                "Local exchange failed. A possibly sent mutation may still commit; reconcile using the same request ID and guard.",
            );
            body["may_have_sent"] = json!(error.may_have_sent);
            output(&body).map_err(|e| e.to_string())?;
            return Ok(false);
        }
    };
    output(&body).map_err(|e| e.to_string())?;
    Ok(body["ok"] == true && body["reliable"] != false)
}

fn main() -> ExitCode {
    match run() {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(error) => {
            eprintln!("lamp-settings: {error}\n{USAGE}");
            ExitCode::from(2)
        }
    }
}
