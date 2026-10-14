use lamp_surround::{
    atomic,
    config::Secret,
    operator, runtime, setup, startup,
    worker::{Worker, WorkerConfig},
};
use std::{
    collections::BTreeSet,
    io,
    path::PathBuf,
    process::ExitCode,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tokio::sync::{mpsc, oneshot};

const USAGE: &str = "lamp-surround serve --config FILE [--enable] [--clock-id ID] [--listen-loopback PORT] [--seconds N]\n\
lamp-surround doctor --config FILE\n\
lamp-surround plan --config FILE [--enable] [--clock-id ID]\n\
lamp-surround clock [--clock-id ID]\n\n\
Maintenance requires BOTH config.enabled=true and --enable. Network ownership has its own enable switch.\n\
Default HTTP transport is a private Unix socket; loopback TCP is an explicit local preview option.\n\
doctor and plan do not connect to owners, start services or change networking.\n\
Configuration, credentials and the setup code are private files, never command-line values.\n\
Offline written update scenarios: cargo run --offline -p lamp-surround --example update_replay\n\
No command supplies missing physical privacy, quiet, rest, exit or readiness evidence.";

#[derive(Default)]
struct Args {
    command: String,
    config: Option<PathBuf>,
    enable: bool,
    clock: Option<String>,
    port: Option<u16>,
    seconds: Option<u64>,
}
fn parse(values: impl IntoIterator<Item = String>) -> io::Result<Option<Args>> {
    let mut values = values.into_iter();
    let Some(command) = values.next() else {
        return Err(io::Error::other("a command is required"));
    };
    if command == "--help" || command == "-h" {
        return Ok(None);
    }
    if !["serve", "doctor", "plan", "clock"].contains(&command.as_str()) {
        return Err(io::Error::other("unknown command"));
    }
    let mut args = Args {
        command,
        ..Args::default()
    };
    let mut seen = BTreeSet::new();
    while let Some(flag) = values.next() {
        if !seen.insert(flag.clone()) {
            return Err(io::Error::other("duplicate option"));
        }
        let allowed = match args.command.as_str() {
            "serve" => [
                "--config",
                "--enable",
                "--clock-id",
                "--listen-loopback",
                "--seconds",
            ]
            .contains(&flag.as_str()),
            "plan" => ["--config", "--enable", "--clock-id"].contains(&flag.as_str()),
            "doctor" => flag == "--config",
            _ => flag == "--clock-id",
        };
        if !allowed {
            return Err(io::Error::other("unsupported option"));
        }
        if flag == "--enable" {
            args.enable = true;
            continue;
        }
        let value = values
            .next()
            .ok_or_else(|| io::Error::other("option needs a value"))?;
        match flag.as_str() {
            "--config" => args.config = Some(value.into()),
            "--clock-id" => args.clock = Some(value),
            "--listen-loopback" => {
                args.port = Some(
                    value
                        .parse::<u16>()
                        .ok()
                        .filter(|p| *p != 0)
                        .ok_or_else(|| io::Error::other("invalid loopback port"))?,
                )
            }
            "--seconds" => {
                args.seconds = Some(
                    value
                        .parse::<u64>()
                        .ok()
                        .filter(|s| (1..=86_400).contains(s))
                        .ok_or_else(|| io::Error::other("invalid duration"))?,
                )
            }
            _ => return Err(io::Error::other("unsupported option")),
        }
    }
    if args.command != "clock" && args.config.is_none() {
        return Err(io::Error::other("--config is required"));
    }
    Ok(Some(args))
}

fn print(value: &impl serde::Serialize) -> io::Result<()> {
    serde_json::to_writer(std::io::stdout().lock(), value)?;
    println!();
    Ok(())
}

async fn shutdown(seconds: Option<u64>) {
    let duration = async {
        match seconds {
            Some(n) => tokio::time::sleep(Duration::from_secs(n)).await,
            None => std::future::pending::<()>().await,
        }
    };
    #[cfg(unix)]
    {
        let terminate = async {
            match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
                Ok(mut signal) => {
                    signal.recv().await;
                }
                Err(_) => std::future::pending::<()>().await,
            }
        };
        tokio::select! { _ = tokio::signal::ctrl_c() => {}, _ = terminate => {}, _ = duration => {} }
    }
    #[cfg(not(unix))]
    {
        tokio::select! { _ = tokio::signal::ctrl_c() => {}, _ = duration => {} }
    }
}

fn start_diagnostics(
    root: Option<PathBuf>,
    shared: operator::Shared,
    running: Arc<AtomicBool>,
) -> io::Result<()> {
    let Some(root) = root else {
        return Ok(());
    };
    std::thread::Builder::new()
        .name("surround-diagnostics".into())
        .spawn(move || {
            let mut publisher = None;
            let mut failure_reported = false;
            while running.load(Ordering::Acquire) {
                if publisher.is_none() {
                    publisher = lamp_bus::Bus::new(root.clone())
                        .and_then(|bus| {
                            bus.publisher_declared(
                                lamp_msgs::Diagnostic::TOPIC,
                                &lamp_msgs::Diagnostic::declaration(),
                                "surround",
                            )
                        })
                        .ok();
                }
                let report = shared
                    .view
                    .read()
                    .ok()
                    .map(|view| runtime::diagnostic(&view, lamp_ipc::monotonic_us()));
                let okay = match (&mut publisher, report) {
                    (Some(publisher), Some(report)) => publisher
                        .publish_json(lamp_ipc::monotonic_us(), &report)
                        .is_ok(),
                    _ => false,
                };
                if !okay {
                    publisher = None;
                    if !failure_reported {
                        eprintln!("surround: diagnostics unavailable; operator service continues");
                        failure_reported = true;
                    }
                }
                std::thread::sleep(Duration::from_secs(1));
            }
        })?;
    Ok(())
}

async fn serve(mut config: runtime::Config, args: Args) -> io::Result<()> {
    config.enabled &= args.enable;
    let clock_id = runtime::clock_id(args.clock.as_deref())?;
    let clock: operator::Clock = Arc::new(lamp_ipc::monotonic_us);
    let session = operator::random_id()?;
    let mut code = atomic::read(&config.operator_code_file, 128)?
        .ok_or_else(|| io::Error::other("operator setup code is missing"))?;
    if code.ends_with(b"\n") {
        code.pop();
    }
    let code = Secret::new(code)?;
    let shared = operator::Shared::new(config.enabled);
    let (tx, rx) = mpsc::channel(1);
    let router = operator::router(
        code,
        &config.operator_origin,
        shared.clone(),
        tx,
        clock.clone(),
    )?;
    enum Listener {
        Private(tokio::net::UnixListener, runtime::ListenerGuard),
        Loopback(tokio::net::TcpListener),
    }
    // Reserve the endpoint before starting recovery or an OS adapter. A second
    // process or invalid preview origin cannot trigger maintenance on its way out.
    let listener = if let Some(port) = args.port {
        if ![
            format!("http://127.0.0.1:{port}"),
            format!("http://localhost:{port}"),
        ]
        .contains(&config.operator_origin)
        {
            return Err(io::Error::other(
                "loopback preview requires its exact HTTP origin in configuration",
            ));
        }
        Listener::Loopback(
            tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port)).await?,
        )
    } else {
        let (listener, guard) = runtime::listen(&config.socket).await?;
        Listener::Private(listener, guard)
    };
    let network = config.network_adapter(clock.clone())?;
    let worker_config = WorkerConfig {
        enabled: config.enabled,
        state_dir: config.state_dir.clone(),
        configuration_dir: config.configuration_dir.clone(),
        owner_socket: config.owner_socket.clone(),
        release: config
            .release
            .as_ref()
            .map(runtime::ReleaseSource::config)
            .transpose()?,
        selection_dir: config.release.as_ref().map(|r| r.selection_dir.clone()),
        network_mode: config
            .network
            .as_ref()
            .map_or(setup::SetupMode::Wired, |n| match n.kind {
                setup::LinkKind::Wired => setup::SetupMode::Wired,
                setup::LinkKind::Wifi => setup::SetupMode::Wifi,
            }),
        reset_stores: config.reset_stores.clone(),
    };
    let (started, start_rx) = oneshot::channel();
    let (ended, end_rx) = oneshot::channel();
    let worker_shared = shared.clone();
    // Local fsync/hash work lives on this thread's executor, not on the HTTP
    // executor. There is one maintenance worker, independent of request count.
    std::thread::Builder::new()
        .name("surround-maintenance".into())
        .spawn(move || {
            let result = (|| {
                let executor = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()?;
                let worker = Worker::open(
                    worker_config,
                    worker_shared,
                    clock,
                    &clock_id,
                    &session,
                    network,
                )?;
                Ok::<_, io::Error>((executor, worker))
            })();
            match result {
                Ok((executor, worker)) => {
                    let _ = started.send(true);
                    let _ = ended.send(executor.block_on(worker.run(rx)).is_ok());
                }
                Err(_) => {
                    let _ = started.send(false);
                    let _ = ended.send(false);
                }
            }
        })?;
    if !start_rx.await.unwrap_or(false) {
        return Err(io::Error::other(
            "maintenance state could not be opened; inspect private records before retrying",
        ));
    }
    let failed_shared = shared.clone();
    tokio::spawn(async move {
        let _ = end_rx.await;
        if let Ok(mut view) = failed_shared.view.write() {
            view.enabled = false;
            view.runtime = Some(setup::RuntimeState::Unknown);
            view.runtime_observed_us = None;
            view.voice_capability = None;
            if let Some(operation) = &mut view.operation
                && operation.phase == operator::Phase::Working
            {
                operation.phase = operator::Phase::Interrupted;
                operation.error = Some(operator::ErrorCode::Unavailable);
            }
        }
        failed_shared.finished();
    });
    let running = Arc::new(AtomicBool::new(true));
    start_diagnostics(config.bus_root.clone(), shared, running.clone())?;
    let result = match listener {
        Listener::Loopback(listener) => {
            eprintln!(
                "surround: operator page available on loopback; maintenance enabled={}",
                config.enabled
            );
            axum::serve(
                lamp_surround::http_limit::LimitedListener::new(listener),
                router,
            )
            .with_graceful_shutdown(shutdown(args.seconds))
            .await
        }
        Listener::Private(listener, _guard) => {
            eprintln!(
                "surround: operator page available on private socket; maintenance enabled={}",
                config.enabled
            );
            axum::serve(
                lamp_surround::http_limit::LimitedListener::new(listener),
                router,
            )
            .with_graceful_shutdown(shutdown(args.seconds))
            .await
        }
    };
    running.store(false, Ordering::Release);
    result
}

async fn run(args: Args) -> io::Result<()> {
    if args.command == "clock" {
        return print(
            &serde_json::json!({"clock_id":runtime::clock_id(args.clock.as_deref())?,"at_us":lamp_ipc::monotonic_us(),"unit":"CLOCK_MONOTONIC microseconds"}),
        );
    }
    let config = runtime::Config::read(args.config.as_ref().unwrap())?;
    match args.command.as_str() {
        "serve" => serve(config, args).await,
        "doctor" => print(
            &serde_json::json!({"schema_version":1,"configuration_valid":true,"configured_enabled":config.enabled,"requires_enable_argument":true,"network_feature_built":cfg!(feature="network-manager"),"linux_network_adapter_supported":cfg!(all(feature="network-manager",target_os="linux")),"network_selected":config.network.is_some(),"supervisor_configured":config.owner_socket.is_some(),"signed_feed_configured":config.release.is_some(),"startup_declarations":config.startup.components,"device_checked":false,"owner_connected":false,"readiness_proved":false}),
        ),
        "plan" => {
            let clock = runtime::clock_id(args.clock.as_deref())?;
            let now = lamp_ipc::monotonic_us();
            let mut startup = config.startup;
            startup.enabled &= args.enable && config.enabled;
            let mut monitor =
                startup::Monitor::new(startup, &clock, &clock, now).map_err(io::Error::other)?;
            let plan = monitor.plan(now).map_err(io::Error::other)?;
            print(
                &serde_json::json!({"dry_run":true,"launches":plan,"status":monitor.snapshot(now).map_err(io::Error::other)?,"actions_executed":0}),
            )
        }
        _ => Err(io::Error::other("unknown command")),
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    match parse(std::env::args().skip(1)) {
        Ok(None) => {
            println!("{USAGE}");
            ExitCode::SUCCESS
        }
        Ok(Some(args)) => match run(args).await {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("surround: {error}");
                ExitCode::FAILURE
            }
        },
        Err(error) => {
            eprintln!("surround: {error}\n{USAGE}");
            ExitCode::FAILURE
        }
    }
}
