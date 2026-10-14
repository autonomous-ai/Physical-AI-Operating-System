//! One bounded maintenance worker. HTTP admission is acknowledged only after
//! its sanitized intent is durable; slow adapters never hold the status lock.
//! Service ownership, privacy and physical rest remain supervisor duties.
use crate::{
    atomic::{self, DirectoryLock},
    configuration::Configurations,
    network::{self, NetworkAdapter, NetworkCommand, NetworkResult, OperationId},
    operator::{self, Command, ErrorCode, Operation, Phase, Submission},
    owner, release, reset,
    selection::Selector,
    setup, startup, update,
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, io, path::PathBuf, sync::atomic::Ordering, time::Duration};
use tokio::sync::mpsc;

/// Recent IDs are retained for retry deduplication. HTTP assigns random IDs;
/// they are not an externally supplied lifelong replay namespace.
pub const MAX_OPERATIONS: usize = 128;
const MAX_JOURNAL_BYTES: usize = 16_384;
const MAX_UPDATE_BYTES: usize = 65_536;
const POLL: Duration = Duration::from_millis(20);
const NETWORK_CALL: Duration = Duration::from_secs(10);
pub const RESET_CONFIRMATION: &str = "erase-personal-data";

#[derive(Clone, Debug)]
pub struct WorkerConfig {
    pub enabled: bool,
    pub state_dir: PathBuf,
    pub configuration_dir: PathBuf,
    pub owner_socket: Option<PathBuf>,
    pub release: Option<release::ReleaseConfig>,
    pub selection_dir: Option<PathBuf>,
    pub network_mode: setup::SetupMode,
    pub reset_stores: Vec<reset::Store>,
}
impl Default for WorkerConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            state_dir: PathBuf::new(),
            configuration_dir: PathBuf::new(),
            owner_socket: None,
            release: None,
            selection_dir: None,
            network_mode: setup::SetupMode::Wired,
            reset_stores: Vec::new(),
        }
    }
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Journal {
    schema_version: u32,
    admission_generation: u64,
    used_ids: Vec<String>,
    operation: Option<Operation>,
}

pub struct Worker {
    config: WorkerConfig,
    shared: operator::Shared,
    clock: operator::Clock,
    clock_id: String,
    last_us: u64,
    clock_failed: bool,
    _lock: Option<DirectoryLock>,
    owner: Option<owner::Client>,
    network: Box<dyn NetworkAdapter + Send>,
    network_inflight: bool,
    idle_sequence: u64,
    journal: Journal,
    setup: Option<setup::SetupStore>,
    update: update::Coordinator,
    reset: Option<reset::Reset>,
    recovered: bool,
}

fn invalid() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        "invalid maintenance state or configuration",
    )
}
fn operation_id(value: &str) -> bool {
    value.len() == 32
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

impl Worker {
    pub fn open(
        config: WorkerConfig,
        shared: operator::Shared,
        clock: operator::Clock,
        clock_id: &str,
        session: &str,
        network: Box<dyn NetworkAdapter + Send>,
    ) -> io::Result<Self> {
        let mut roots = vec![
            config.state_dir.as_path(),
            config.configuration_dir.as_path(),
        ];
        if let Some(release) = &config.release {
            roots.extend([release.state_dir.as_path(), release.release_dir.as_path()]);
        }
        if let Some(selection) = &config.selection_dir {
            roots.push(selection.as_path());
        }
        if roots.iter().any(|path| {
            !path.is_absolute()
                || path.components().any(|part| {
                    !matches!(
                        part,
                        std::path::Component::RootDir | std::path::Component::Normal(_)
                    )
                })
        }) || roots.iter().enumerate().any(|(index, path)| {
            roots[index + 1..]
                .iter()
                .any(|other| path.starts_with(other) || other.starts_with(path))
        }) {
            return Err(invalid());
        }
        if !setup::valid_id(clock_id)
            || !setup::valid_id(session)
            || config.state_dir == config.configuration_dir
            || config.reset_stores.len() > 7
            || config.reset_stores.iter().collect::<BTreeSet<_>>().len()
                != config.reset_stores.len()
            || (!config.reset_stores.is_empty() && config.reset_stores[0] != reset::Store::Voice)
            || config
                .release
                .as_ref()
                .is_some_and(|r| r.state_dir == config.state_dir)
            || config.selection_dir.as_ref() == Some(&config.state_dir)
        {
            return Err(invalid());
        }
        let lock = if config.enabled {
            Some(DirectoryLock::acquire(&config.state_dir)?)
        } else {
            None
        };
        let exists = match atomic::directory(&config.state_dir) {
            Ok(_) => true,
            Err(error) if error.kind() == io::ErrorKind::NotFound => false,
            Err(error) => return Err(error),
        };
        let now = clock();
        let path = config.state_dir.join("operations.json");
        let mut journal: Journal = match if exists {
            atomic::read(&path, MAX_JOURNAL_BYTES)?
        } else {
            None
        } {
            Some(bytes) => serde_json::from_slice(&bytes).map_err(|_| invalid())?,
            None => Journal {
                schema_version: 2,
                ..Journal::default()
            },
        };
        if journal.schema_version != 2
            || journal.used_ids.len() > MAX_OPERATIONS
            || journal.admission_generation < journal.used_ids.len() as u64
            || (journal.admission_generation == 0) != journal.operation.is_none()
            || journal.used_ids.iter().any(|id| !operation_id(id))
            || journal.used_ids.iter().collect::<BTreeSet<_>>().len() != journal.used_ids.len()
            || journal.operation.as_ref().is_some_and(|op| {
                op.schema_version != 1
                    || !journal.used_ids.contains(&op.id)
                    || (op.phase == Phase::Complete && op.error.is_some())
                    || (op.phase == Phase::Working && op.error.is_some())
            })
        {
            return Err(invalid());
        }
        if let Some(operation) = &mut journal.operation
            && operation.phase == Phase::Working
        {
            operation.phase = Phase::Interrupted;
            operation.error = Some(ErrorCode::Interrupted);
            operation.updated_us = now;
            if config.enabled {
                atomic::write(&path, &serde_json::to_vec(&journal)?, MAX_JOURNAL_BYTES)?;
            }
        }
        let update_state = match if exists {
            atomic::read(&config.state_dir.join("update.json"), MAX_UPDATE_BYTES)?
        } else {
            None
        } {
            Some(bytes) => {
                serde_json::from_slice::<update::State>(&bytes).map_err(|_| invalid())?
            }
            None => update::State::default(),
        };
        let coordinator = update::Coordinator::new(config.enabled, clock_id, session, update_state)
            .map_err(|_| invalid())?;
        let reset = if config.enabled {
            Some(reset::Reset::open(
                true,
                &config.state_dir.join("reset.json"),
                clock_id,
                session,
                now,
            )?)
        } else {
            None
        };
        let setup = if config.enabled {
            Some(setup::SetupStore::open(
                &config.state_dir.join("setup.json"),
                clock_id,
                now,
            )?)
        } else {
            None
        };
        let owner = config
            .owner_socket
            .as_ref()
            .map(|path| {
                owner::Client::new(
                    path.clone(),
                    config.enabled,
                    clock_id.into(),
                    session.into(),
                )
            })
            .transpose()?;
        let mut worker = Self {
            config,
            shared,
            clock,
            clock_id: clock_id.into(),
            last_us: now,
            clock_failed: false,
            _lock: lock,
            owner,
            network,
            network_inflight: false,
            idle_sequence: 0,
            journal,
            setup,
            update: coordinator,
            reset,
            recovered: false,
        };
        worker.refresh_view()?;
        if worker.config.enabled {
            worker.refresh_configurations()?;
        }
        Ok(worker)
    }

    /// Processes a bounded channel serially. Startup reconciles an authorized
    /// interrupted update before admitting unrelated maintenance.
    pub async fn run(mut self, mut rx: mpsc::Receiver<Submission>) -> io::Result<()> {
        self.recover_startup().await?;
        let mut idle = tokio::time::interval(Duration::from_millis(500));
        idle.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                submission=rx.recv()=>match submission {Some(submission)=>self.run_one(submission).await?,None=>return Ok(())},
                _=idle.tick()=>{
                    // Read-only refresh may wait on an adapter, but a new
                    // command always preempts it before durable admission.
                    let incoming=tokio::select! {
                        biased;
                        submission=rx.recv()=>Some(submission),
                        result=self.refresh_idle()=>{result?;None},
                    };
                    match incoming {
                        Some(Some(submission))=>self.run_one(submission).await?,
                        Some(None)=>return Ok(()),
                        None=>{},
                    }
                }
            }
        }
    }

    pub async fn run_one(&mut self, submission: Submission) -> io::Result<()> {
        self.recover_startup().await?;
        self.shared.busy.store(true, Ordering::Release);
        let result = self.run_submission(submission).await;
        self.shared.finished();
        result
    }

    async fn run_submission(&mut self, submission: Submission) -> io::Result<()> {
        let Submission {
            id,
            command,
            admitted,
        } = submission;
        let rejection = if !self.config.enabled {
            Some(ErrorCode::Disabled)
        } else if !operation_id(&id) {
            Some(ErrorCode::InvalidInput)
        } else if self.journal.used_ids.contains(&id) {
            Some(ErrorCode::CheckStatus)
        } else if self.exclusive_conflict(&command) {
            Some(ErrorCode::Busy)
        } else {
            None
        };
        if let Some(error) = rejection {
            let _ = admitted.send(Err(error));
            return Ok(());
        }
        let now = match self.now() {
            Ok(now) => now,
            Err(error) => {
                let _ = admitted.send(Err(error));
                return Ok(());
            }
        };
        let mut next = self.journal.clone();
        let Some(generation) = next.admission_generation.checked_add(1) else {
            let _ = admitted.send(Err(ErrorCode::Storage));
            return Ok(());
        };
        next.admission_generation = generation;
        if next.used_ids.len() == MAX_OPERATIONS {
            next.used_ids.remove(0);
        }
        next.used_ids.push(id.clone());
        next.operation = Some(Operation {
            schema_version: 1,
            id: id.clone(),
            kind: command.kind(),
            phase: Phase::Working,
            updated_us: now,
            error: None,
        });
        if self.save_journal(next).is_err() {
            let _ = admitted.send(Err(ErrorCode::Storage));
            return Err(io::Error::other("cannot persist maintenance admission"));
        }
        self.refresh_view()?;
        let _ = admitted.send(Ok(()));
        let mut result = self.execute(&id, command).await;
        let finished_us = match self.now() {
            Ok(now) => now,
            Err(error) => {
                result = Err(error);
                self.last_us
            }
        };
        let mut next = self.journal.clone();
        let operation = next.operation.as_mut().unwrap();
        operation.updated_us = finished_us;
        operation.phase = if result.is_ok() {
            Phase::Complete
        } else {
            Phase::Failed
        };
        operation.error = result.err();
        if self.save_journal(next).is_err() {
            if let Ok(mut view) = self.shared.view.write()
                && let Some(operation) = &mut view.operation
            {
                operation.phase = Phase::Interrupted;
                operation.error = Some(ErrorCode::Storage);
            }
            return Err(io::Error::other(
                "cannot persist maintenance outcome; inspect durable state",
            ));
        }
        self.refresh_view()
    }

    fn exclusive_conflict(&self, command: &Command) -> bool {
        let reset_active = self
            .reset
            .as_ref()
            .and_then(|r| r.state().operation.as_ref())
            .is_some_and(|op| op.phase != reset::Phase::Complete);
        let update_active = self.update.state().active()
            || matches!(
                self.update.state().outcome,
                update::Outcome::RecoveryFailed { .. }
            );
        (reset_active && !matches!(command, Command::ResumeReset {}))
            || (update_active && !matches!(command, Command::RetryUpdate {}))
    }

    fn now(&mut self) -> Result<u64, ErrorCode> {
        let now = (self.clock)();
        if self.clock_failed || now < self.last_us {
            self.clock_failed = true;
            return Err(ErrorCode::Unavailable);
        }
        self.last_us = now;
        Ok(now)
    }
    fn save_journal(&mut self, next: Journal) -> io::Result<()> {
        atomic::write(
            &self.config.state_dir.join("operations.json"),
            &serde_json::to_vec(&next)?,
            MAX_JOURNAL_BYTES,
        )?;
        self.journal = next;
        Ok(())
    }
    fn refresh_view(&mut self) -> io::Result<()> {
        let mut view = self.shared.view.write().map_err(|_| invalid())?;
        view.enabled = self.config.enabled;
        view.operation = self.journal.operation.clone();
        view.setup = self.setup.as_ref().map(|s| s.progress().clone());
        view.update_outcome = Some(self.update.state().outcome.clone());
        view.reset_phase = self
            .reset
            .as_ref()
            .and_then(|r| r.state().operation.as_ref())
            .map(|o| o.phase);
        Ok(())
    }
    fn refresh_configurations(&mut self) -> io::Result<()> {
        let _lock = DirectoryLock::acquire(&self.config.configuration_dir)?;
        let configurations = Configurations::open(&self.config.configuration_dir)?;
        let candidate = configurations.candidate()?;
        let current = configurations.selection()?.current;
        let mut view = self.shared.view.write().map_err(|_| invalid())?;
        view.candidate_configuration = candidate;
        view.current_configuration = current;
        Ok(())
    }
    async fn call_owner(
        &mut self,
        id: &str,
        command: owner::Command,
    ) -> Result<owner::Reply, ErrorCode> {
        self.owner
            .as_ref()
            .ok_or(ErrorCode::Unavailable)?
            .call(id, command, self.clock.clone())
            .await
            .map_err(|error| match error {
                owner::Error::Unavailable | owner::Error::Timeout | owner::Error::ClockChanged => {
                    ErrorCode::Unavailable
                }
                _ => ErrorCode::OwnerRefused,
            })
    }
    async fn probe_owner(&mut self, id: &str) -> Result<owner::Probe, ErrorCode> {
        let owner::Reply::Probe(probe) = self.call_owner(id, owner::Command::Probe).await? else {
            return Err(ErrorCode::OwnerRefused);
        };
        let now = self.now()?;
        if probe.observed_us > now || now - probe.observed_us >= owner::RECEIPT_AGE_US {
            return Err(ErrorCode::OwnerRefused);
        }
        let voice = probe
            .startup
            .iter()
            .find(|receipt| receipt.component == startup::Component::Voice);
        let mut view = self.shared.view.write().map_err(|_| ErrorCode::Storage)?;
        view.runtime = Some(probe.runtime);
        view.runtime_observed_us = Some(voice.map_or(probe.observed_us, |receipt| {
            receipt.observed_us.min(probe.observed_us)
        }));
        view.voice_capability = voice.map(|receipt| receipt.capability);
        Ok(probe)
    }

    async fn execute(&mut self, id: &str, command: Command) -> Result<(), ErrorCode> {
        if matches!(
            &command,
            Command::ApplyConfiguration { .. }
                | Command::ApplyRelease { .. }
                | Command::RetryUpdate {}
                | Command::Reset { .. }
                | Command::ResumeReset {}
        ) {
            self.withdraw_runtime().map_err(|_| ErrorCode::Storage)?;
            self.refresh_view().map_err(|_| ErrorCode::Storage)?;
        }
        match command {
            Command::SaveProvider {
                document,
                credential,
                instructions,
            } => {
                let _lock = DirectoryLock::acquire(&self.config.configuration_dir)
                    .map_err(|_| ErrorCode::Storage)?;
                let configurations = Configurations::open(&self.config.configuration_dir)
                    .map_err(|_| ErrorCode::Storage)?;
                let generation = configurations
                    .stage(id, document, credential.0, instructions.as_deref())
                    .map_err(|_| ErrorCode::InvalidInput)?;
                self.shared
                    .view
                    .write()
                    .map_err(|_| ErrorCode::Storage)?
                    .candidate_configuration = Some(generation);
                Ok(())
            }
            Command::ApplyConfiguration { target } => {
                let expected = {
                    let _lock = DirectoryLock::acquire(&self.config.configuration_dir)
                        .map_err(|_| ErrorCode::Storage)?;
                    let configurations = Configurations::open(&self.config.configuration_dir)
                        .map_err(|_| ErrorCode::Storage)?;
                    configurations
                        .verify(&target)
                        .map_err(|_| ErrorCode::Verification)?;
                    configurations
                        .selection()
                        .map_err(|_| ErrorCode::Storage)?
                        .current
                };
                if expected.as_ref() == Some(&target) {
                    return Err(ErrorCode::CheckStatus);
                }
                let owner::Reply::Configure(receipt) = self
                    .call_owner(
                        id,
                        owner::Command::Configure {
                            operation_id: id.into(),
                            expected,
                            target,
                        },
                    )
                    .await?
                else {
                    return Err(ErrorCode::OwnerRefused);
                };
                self.refresh_configurations()
                    .map_err(|_| ErrorCode::Storage)?;
                if !receipt.applied() {
                    return Err(ErrorCode::OwnerRefused);
                }
                let owner::ConfigurationOutcome::Applied { ready, .. } = receipt.outcome else {
                    return Err(ErrorCode::OwnerRefused);
                };
                if self
                    .shared
                    .view
                    .read()
                    .map_err(|_| ErrorCode::Storage)?
                    .current_configuration
                    .as_ref()
                    != Some(&ready.generation)
                {
                    return Err(ErrorCode::OwnerRefused);
                }
                let mut view = self.shared.view.write().map_err(|_| ErrorCode::Storage)?;
                view.runtime = Some(setup::RuntimeState::Ready);
                view.runtime_observed_us = Some(ready.observed_us);
                view.voice_capability = Some(match ready.capability {
                    owner::VoiceCapability::Listening => startup::Capability::Listening,
                    owner::VoiceCapability::Inhibited => startup::Capability::Inhibited,
                });
                Ok(())
            }
            Command::StageRelease { manifest_target } => {
                let probe = self.probe_owner(id).await?;
                if !probe.time_synchronized {
                    return Err(ErrorCode::Verification);
                }
                let config = self.config.release.as_ref().ok_or(ErrorCode::Unavailable)?;
                let selector = Selector::open(
                    config.clone(),
                    self.config
                        .selection_dir
                        .clone()
                        .ok_or(ErrorCode::Unavailable)?,
                    true,
                )
                .map_err(|_| ErrorCode::Storage)?;
                let protected = self
                    .shared
                    .view
                    .read()
                    .map_err(|_| ErrorCode::Storage)?
                    .staged_release
                    .clone()
                    .into_iter()
                    .collect::<Vec<_>>();
                selector
                    .collect(&protected)
                    .map_err(|_| ErrorCode::Storage)?;
                let verified = release::verify_and_stage(
                    config,
                    release::TrustedTime {
                        unix_seconds: probe.unix_seconds,
                        synchronized: true,
                    },
                    &manifest_target,
                )
                .await
                .map_err(|_| ErrorCode::Verification)?;
                self.shared
                    .view
                    .write()
                    .map_err(|_| ErrorCode::Storage)?
                    .staged_release = Some(verified.set_sha256().into());
                Ok(())
            }
            Command::ApplyRelease {
                set_sha256,
                rollback,
            } => {
                let config = self.config.release.as_ref().ok_or(ErrorCode::Unavailable)?;
                let verified = release::recover_verified(config, &set_sha256)
                    .map_err(|_| ErrorCode::Verification)?;
                let selector = Selector::open(
                    config.clone(),
                    self.config
                        .selection_dir
                        .clone()
                        .ok_or(ErrorCode::Unavailable)?,
                    false,
                )
                .map_err(|_| ErrorCode::Storage)?;
                let current = selector.current().map_err(|_| ErrorCode::Verification)?;
                let request = update::Request {
                    operation_id: id.into(),
                    kind: if rollback {
                        update::RequestKind::ManualRollback
                    } else {
                        update::RequestKind::Update
                    },
                    target: update::ReleaseId(verified.set_sha256().into()),
                    expected_current: current
                        .map(|current| update::ReleaseId(current.current_set().into())),
                };
                let now = self.now()?;
                self.update
                    .begin(now, request)
                    .map_err(|_| ErrorCode::InvalidInput)?;
                self.persist_update()?;
                self.drive_update().await?;
                match self.update.state().outcome {
                    update::Outcome::Succeeded { .. } => Ok(()),
                    _ => Err(ErrorCode::OwnerRefused),
                }
            }
            Command::RetryUpdate {} => {
                let now = self.now()?;
                self.update
                    .retry_recovery(now)
                    .map_err(|_| ErrorCode::InvalidInput)?;
                self.persist_update()?;
                self.drive_update().await?;
                if matches!(
                    self.update.state().outcome,
                    update::Outcome::RolledBack { .. }
                ) {
                    Ok(())
                } else {
                    Err(ErrorCode::OwnerRefused)
                }
            }
            Command::Reset { confirmation } => {
                if confirmation != RESET_CONFIRMATION || self.config.reset_stores.is_empty() {
                    return Err(ErrorCode::InvalidInput);
                }
                let now = self.now()?;
                self.reset
                    .as_mut()
                    .ok_or(ErrorCode::Unavailable)?
                    .begin(id, self.config.reset_stores.clone(), true, now)
                    .map_err(|_| ErrorCode::Storage)?;
                self.refresh_view().map_err(|_| ErrorCode::Storage)?;
                self.drive_reset().await
            }
            Command::ResumeReset {} => {
                let now = self.now()?;
                self.reset
                    .as_mut()
                    .ok_or(ErrorCode::Unavailable)?
                    .resume(now)
                    .map_err(|_| ErrorCode::InvalidInput)?;
                self.drive_reset().await
            }
            Command::JoinWifi { ssid, password } => {
                let credentials = network::WifiCredentials::new(
                    ssid.into_bytes(),
                    password.map_or(network::WifiSecurity::Open, |password| {
                        network::WifiSecurity::WpaPsk(password.0)
                    }),
                )
                .map_err(|_| ErrorCode::InvalidInput)?;
                self.join_wifi(id, credentials).await
            }
            Command::ProbeNetwork {} => self.probe_network(id).await,
        }
    }

    fn persist_update(&mut self) -> Result<(), ErrorCode> {
        atomic::write(
            &self.config.state_dir.join("update.json"),
            &serde_json::to_vec(self.update.state()).map_err(|_| ErrorCode::Storage)?,
            MAX_UPDATE_BYTES,
        )
        .map_err(|_| ErrorCode::Storage)?;
        self.refresh_view().map_err(|_| ErrorCode::Storage)
    }
    async fn recover_startup(&mut self) -> io::Result<()> {
        if self.recovered {
            return Ok(());
        }
        self.recovered = true;
        if !self.config.enabled || !self.update.state().active() {
            return Ok(());
        }
        if self
            .reset
            .as_ref()
            .and_then(|r| r.state().operation.as_ref())
            .is_some_and(|r| r.phase != reset::Phase::Complete)
        {
            return Err(io::Error::other("update and reset recovery conflict"));
        }
        self.shared.busy.store(true, Ordering::Release);
        let result = self.drive_update().await;
        self.shared.finished();
        if result == Err(ErrorCode::Storage) {
            return Err(io::Error::other("cannot persist update recovery"));
        }
        self.refresh_view()
    }
    async fn drive_update(&mut self) -> Result<(), ErrorCode> {
        for _ in 0..update::MAX_STEPS {
            let now = self.now()?;
            self.update.tick(now).map_err(|_| ErrorCode::Unavailable)?;
            self.persist_update()?;
            if !self.update.state().active() {
                return Ok(());
            }
            let action = self
                .update
                .committed(self.update.state().revision)
                .map_err(|_| ErrorCode::Storage)?
                .ok_or(ErrorCode::OwnerRefused)?;
            let id = format!("{}-{}", action.operation_id, action.step_id);
            let reply = self
                .call_owner(&id, owner::Command::Update(action.clone()))
                .await;
            let now = self.now()?;
            let mut receipt = match reply {
                Ok(owner::Reply::Update(receipt)) => receipt,
                _ => local_update_failure(&action, now),
            };
            if let update::Evidence::Failed { reason } = &mut receipt.evidence {
                *reason = "owner refused update".into();
            }
            if self.update.observe(now, receipt).is_err()
                && self
                    .update
                    .pending()
                    .is_some_and(|pending| pending.step_id == action.step_id)
            {
                self.update
                    .observe(now, local_update_failure(&action, now))
                    .map_err(|_| ErrorCode::OwnerRefused)?;
            }
            self.persist_update()?;
        }
        Err(ErrorCode::OwnerRefused)
    }
    async fn drive_reset(&mut self) -> Result<(), ErrorCode> {
        for _ in 0..16 {
            let now = self.now()?;
            let Some(action) = self
                .reset
                .as_mut()
                .ok_or(ErrorCode::Unavailable)?
                .next(now)
                .map_err(|_| ErrorCode::Storage)?
            else {
                self.refresh_view().map_err(|_| ErrorCode::Storage)?;
                return if self
                    .reset
                    .as_ref()
                    .and_then(|r| r.state().operation.as_ref())
                    .is_some_and(|op| op.phase == reset::Phase::Complete)
                {
                    Ok(())
                } else {
                    Err(ErrorCode::OwnerRefused)
                };
            };
            self.refresh_view().map_err(|_| ErrorCode::Storage)?;
            let reply = self
                .call_owner(&action.request_id, owner::Command::Reset(action.clone()))
                .await;
            let now = self.now()?;
            if now >= action.not_after_us {
                self.reset
                    .as_mut()
                    .unwrap()
                    .next(now)
                    .map_err(|_| ErrorCode::Storage)?;
                return Err(ErrorCode::OwnerRefused);
            }
            let receipt = match reply {
                Ok(owner::Reply::Reset(receipt)) => receipt,
                _ => local_reset_failure(&action, now),
            };
            if !self
                .reset
                .as_mut()
                .unwrap()
                .observe(now, receipt)
                .map_err(|_| ErrorCode::Storage)?
            {
                // A stale reply is no proof. Record only local unavailability
                // so the durable action can be explicitly resumed this boot.
                self.reset
                    .as_mut()
                    .unwrap()
                    .observe(now, local_reset_failure(&action, now))
                    .map_err(|_| ErrorCode::Storage)?;
                self.refresh_view().map_err(|_| ErrorCode::Storage)?;
                return Err(ErrorCode::OwnerRefused);
            }
            self.refresh_view().map_err(|_| ErrorCode::Storage)?;
        }
        Err(ErrorCode::OwnerRefused)
    }

    fn begin_setup(&mut self, id: &str, mode: setup::SetupMode) -> Result<(u64, u64), ErrorCode> {
        let now = self.now()?;
        let progress = self
            .setup
            .as_mut()
            .ok_or(ErrorCode::Unavailable)?
            .begin(id, now, mode)
            .map_err(|_| ErrorCode::Storage)?;
        let generation = progress.generation;
        self.refresh_view().map_err(|_| ErrorCode::Storage)?;
        Ok((
            generation,
            now.checked_add(setup::JOIN_TIMEOUT_US)
                .ok_or(ErrorCode::InvalidInput)?,
        ))
    }
    async fn network_call(
        &mut self,
        id: OperationId,
        command: NetworkCommand,
        deadline_us: u64,
    ) -> Result<network::Completion, ErrorCode> {
        // A cancelled idle probe remains owned by its adapter. Drain that
        // single read-only result before submitting a new command, with the
        // same bounded deadline; never confuse it with the new completion.
        if self.network_inflight {
            tokio::time::timeout(NETWORK_CALL, async {
                while self.network_inflight {
                    if self.now()? >= deadline_us {
                        return Err(ErrorCode::Unavailable);
                    }
                    if self.network.poll().is_some() {
                        self.network_inflight = false;
                    } else {
                        tokio::time::sleep(POLL).await;
                    }
                }
                Ok(())
            })
            .await
            .map_err(|_| ErrorCode::Unavailable)??;
        }
        self.network
            .submit(network::NetworkRequest {
                id: id.clone(),
                command,
            })
            .map_err(|_| ErrorCode::Unavailable)?;
        self.network_inflight = true;
        let waited = tokio::time::timeout(NETWORK_CALL, async {
            loop {
                let now = self.now()?;
                if now >= deadline_us {
                    return Err(ErrorCode::Unavailable);
                }
                if let Some(completion) = self.network.poll() {
                    self.network_inflight = false;
                    if completion.id != id {
                        return Err(ErrorCode::OwnerRefused);
                    }
                    if completion.result.is_ok()
                        && (completion.observed_us > now
                            || now - completion.observed_us > setup::RECEIPT_MAX_AGE_US)
                    {
                        return Err(ErrorCode::OwnerRefused);
                    }
                    return Ok(completion);
                }
                tokio::time::sleep(POLL).await;
            }
        })
        .await;
        waited.map_err(|_| ErrorCode::Unavailable)?
    }
    fn setup_receipt(
        &mut self,
        id: &OperationId,
        observed_us: u64,
        value: setup::ReceiptValue,
    ) -> Result<(), ErrorCode> {
        let now = self.now()?;
        let applied = self
            .setup
            .as_mut()
            .ok_or(ErrorCode::Unavailable)?
            .observe(
                now,
                setup::Receipt {
                    attempt_id: id.attempt_id.clone(),
                    generation: id.generation,
                    clock_id: self.clock_id.clone(),
                    sequence: id.sequence,
                    observed_us,
                    value,
                },
            )
            .map_err(|_| ErrorCode::Storage)?;
        self.refresh_view().map_err(|_| ErrorCode::Storage)?;
        if applied {
            Ok(())
        } else {
            Err(ErrorCode::OwnerRefused)
        }
    }
    async fn join_wifi(
        &mut self,
        id: &str,
        credentials: network::WifiCredentials,
    ) -> Result<(), ErrorCode> {
        let (generation, deadline) = self.begin_setup(id, setup::SetupMode::Wifi)?;
        let mut request = OperationId {
            attempt_id: id.into(),
            generation,
            sequence: 1,
        };
        let result = tokio::time::timeout(
            Duration::from_micros(setup::JOIN_TIMEOUT_US),
            self.join_steps(&mut request, deadline, credentials),
        )
        .await
        .map_err(|_| ErrorCode::Unavailable)
        .and_then(|result| result);
        if result == Err(ErrorCode::CheckStatus) {
            // A saved profile or destroyed checkpoint may already have taken
            // effect. Observe only; do not promise rollback erased the profile.
            request.sequence = request
                .sequence
                .checked_add(1)
                .ok_or(ErrorCode::Unavailable)?;
            let deadline = self
                .now()?
                .checked_add(10_000_000)
                .ok_or(ErrorCode::Unavailable)?;
            if let Ok(network::Completion {
                observed_us,
                result: Ok(NetworkResult::Observation(evidence)),
                ..
            }) = self
                .network_call(request.clone(), NetworkCommand::Probe, deadline)
                .await
            {
                self.setup_receipt(
                    &request,
                    observed_us,
                    setup::ReceiptValue::Network(evidence),
                )?;
                self.runtime_after_lan(&request).await?;
            }
            return result;
        }
        if result.is_err() {
            request.sequence = request.sequence.saturating_add(1);
            let rollback_deadline = self
                .now()?
                .checked_add(10_000_000)
                .ok_or(ErrorCode::Unavailable)?;
            // A still-pending adapter owns the checkpoint timeout; submitting
            // another mutation cannot cancel that blocking operation safely.
            if !self.network_inflight {
                let _ = self
                    .network_call(request.clone(), NetworkCommand::Rollback, rollback_deadline)
                    .await;
            }
            request.sequence = request.sequence.saturating_add(1);
            let now = self.now()?;
            self.setup
                .as_mut()
                .unwrap()
                .tick(now)
                .map_err(|_| ErrorCode::Storage)?;
            if self.setup.as_ref().unwrap().progress().phase != setup::Phase::Failed {
                let _ = self.setup_receipt(
                    &request,
                    now,
                    setup::ReceiptValue::Failure(setup::Failure::AdapterUnavailable),
                );
            }
            return result;
        }
        self.runtime_after_lan(&request).await?;
        Ok(())
    }
    async fn join_steps(
        &mut self,
        request: &mut OperationId,
        deadline: u64,
        credentials: network::WifiCredentials,
    ) -> Result<(), ErrorCode> {
        let accepted = self
            .network_call(
                request.clone(),
                NetworkCommand::JoinWifi(credentials),
                deadline,
            )
            .await?;
        if accepted.result != Ok(NetworkResult::Accepted) {
            return Err(ErrorCode::Unavailable);
        }
        // A 45-second wall timer also bounds a stalled injected clock.
        let joined = tokio::time::timeout(Duration::from_micros(setup::JOIN_TIMEOUT_US), async {
            for _ in 0..2250 {
                request.sequence += 1;
                let response = self
                    .network_call(request.clone(), NetworkCommand::Probe, deadline)
                    .await?;
                let Ok(NetworkResult::Observation(evidence)) = response.result else {
                    return Err(ErrorCode::Unavailable);
                };
                if evidence.kind != setup::LinkKind::Wifi {
                    return Err(ErrorCode::OwnerRefused);
                }
                let ready = evidence.lan_ready();
                self.setup_receipt(
                    request,
                    response.observed_us,
                    setup::ReceiptValue::Network(evidence),
                )?;
                if self.setup.as_ref().unwrap().progress().phase == setup::Phase::Failed {
                    return Err(ErrorCode::OwnerRefused);
                }
                if ready {
                    request.sequence += 1;
                    let committed = self
                        .network_call(request.clone(), NetworkCommand::Commit, deadline)
                        .await?;
                    if committed.result == Err(network::NetworkError::CommitUncertain) {
                        return Err(ErrorCode::CheckStatus);
                    }
                    return if committed.result == Ok(NetworkResult::Committed) {
                        Ok(())
                    } else {
                        Err(ErrorCode::OwnerRefused)
                    };
                }
                tokio::time::sleep(POLL).await;
            }
            Err(ErrorCode::Unavailable)
        })
        .await;
        joined.map_err(|_| ErrorCode::Unavailable)?
    }
    async fn probe_network(&mut self, id: &str) -> Result<(), ErrorCode> {
        let (generation, deadline) = self.begin_setup(id, self.config.network_mode)?;
        let request = OperationId {
            attempt_id: id.into(),
            generation,
            sequence: 1,
        };
        let response = self
            .network_call(request.clone(), NetworkCommand::Probe, deadline)
            .await?;
        let Ok(NetworkResult::Observation(evidence)) = response.result else {
            return Err(ErrorCode::Unavailable);
        };
        let ready = evidence.lan_ready();
        self.setup_receipt(
            &request,
            response.observed_us,
            setup::ReceiptValue::Network(evidence),
        )?;
        if ready {
            self.runtime_after_lan(&request).await?;
        }
        Ok(())
    }
    async fn runtime_after_lan(&mut self, network_id: &OperationId) -> Result<(), ErrorCode> {
        let mut id = network_id.clone();
        id.sequence = id.sequence.saturating_add(1);
        if let Ok(probe) = self
            .probe_owner(&format!("{}-runtime-{}", id.attempt_id, id.sequence))
            .await
        {
            let result = self.setup_receipt(
                &id,
                runtime_observed(&probe),
                setup::ReceiptValue::Runtime(probe.runtime),
            );
            match result {
                Err(ErrorCode::Storage) => return Err(ErrorCode::Storage),
                Err(_) => {
                    // A valid periodic Voice proof can predate this network
                    // attempt. Keep its honest global age, while withholding
                    // readiness for an attempt it cannot qualify.
                    let now = self.now()?;
                    self.setup
                        .as_mut()
                        .unwrap()
                        .withdraw_runtime(now)
                        .map_err(|_| ErrorCode::Storage)?;
                }
                Ok(()) => {}
            }
        } else {
            self.withdraw_runtime().map_err(|_| ErrorCode::Storage)?;
        }
        Ok(())
    }

    fn withdraw_runtime(&mut self) -> io::Result<()> {
        let now = self.now().map_err(|_| invalid())?;
        {
            let mut view = self.shared.view.write().map_err(|_| invalid())?;
            view.runtime = Some(setup::RuntimeState::Unknown);
            view.runtime_observed_us = None;
            view.voice_capability = None;
            if let Some(progress) = &mut view.setup {
                progress.runtime_fresh = false;
            }
        }
        if let Some(setup) = &mut self.setup {
            setup.withdraw_runtime(now)?;
        }
        Ok(())
    }

    /// Bounded read-only refresh; never joins, commits, starts owners or grants
    /// input permission. run() preempts this future for new admissions.
    pub async fn refresh_idle(&mut self) -> io::Result<()> {
        if !self.config.enabled || self.exclusive_conflict(&Command::ProbeNetwork {}) {
            return Ok(());
        }
        let now = self.now().map_err(|_| invalid())?;
        self.setup.as_mut().unwrap().tick(now)?;
        self.refresh_view()?;
        self.idle_sequence = self.idle_sequence.checked_add(1).ok_or_else(invalid)?;
        let probe_id = format!("idle-{}", self.idle_sequence);
        let progress = self.setup.as_ref().unwrap().progress().clone();
        if let Some(attempt_id) = progress.attempt_id.clone()
            && (matches!(
                progress.phase,
                setup::Phase::Connecting
                    | setup::Phase::LanConnected
                    | setup::Phase::PreparingRuntime
                    | setup::Phase::Ready
            ) || (progress.phase == setup::Phase::Failed
                && matches!(
                    progress.failure,
                    Some(setup::Failure::RuntimeFailed | setup::Failure::RuntimeTimeout)
                )))
        {
            let request = OperationId {
                attempt_id,
                generation: progress.generation,
                sequence: progress
                    .network_sequence
                    .max(progress.runtime_sequence)
                    .checked_add(1)
                    .ok_or_else(invalid)?,
            };
            let deadline = now.checked_add(10_000_000).ok_or_else(invalid)?;
            match self
                .network_call(request.clone(), NetworkCommand::Probe, deadline)
                .await
            {
                Ok(network::Completion {
                    observed_us,
                    result: Ok(NetworkResult::Observation(evidence)),
                    ..
                }) => {
                    match self.setup_receipt(
                        &request,
                        observed_us,
                        setup::ReceiptValue::Network(evidence),
                    ) {
                        Err(ErrorCode::Storage) => return Err(invalid()),
                        Err(_) => {
                            let now = self.now().map_err(|_| invalid())?;
                            self.setup.as_mut().unwrap().withdraw_network(now)?;
                        }
                        Ok(()) => {}
                    }
                }
                _ => {
                    let now = self.now().map_err(|_| invalid())?;
                    self.setup.as_mut().unwrap().withdraw_network(now)?;
                }
            }
            self.runtime_after_lan(&request)
                .await
                .map_err(|_| invalid())?;
        } else if self.probe_owner(&probe_id).await.is_err() {
            self.withdraw_runtime()?;
        }
        self.refresh_view()
    }
}

fn runtime_observed(probe: &owner::Probe) -> u64 {
    probe
        .startup
        .iter()
        .find(|r| r.component == startup::Component::Voice)
        .map_or(probe.observed_us, |r| r.observed_us.min(probe.observed_us))
}

fn local_update_failure(action: &update::Action, now: u64) -> update::Receipt {
    update::Receipt {
        operation_id: action.operation_id.clone(),
        step_id: action.step_id,
        clock_id: action.clock_id.clone(),
        coordinator_session: action.coordinator_session.clone(),
        observed_us: now,
        guard: action.guard.clone(),
        evidence: update::Evidence::Failed {
            reason: "owner transport or safety proof unavailable".into(),
        },
    }
}

fn local_reset_failure(action: &reset::Action, now: u64) -> reset::Receipt {
    reset::Receipt {
        operation_id: action.operation_id.clone(),
        coordinator_session: action.coordinator_session.clone(),
        clock_id: action.clock_id.clone(),
        request_id: action.request_id.clone(),
        owner_session: "unavailable".into(),
        observed_us: now,
        outcome: reset::Outcome::Failed {
            failure: reset::Failure::Unavailable,
        },
    }
}
