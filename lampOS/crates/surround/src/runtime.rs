//! Standalone service configuration and private listener ownership. This file
//! never guesses board paths, calibration or network-manager authority.
use crate::{atomic, network, operator, release, reset, setup, startup};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    fs, io,
    os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt},
    path::{Component, Path, PathBuf},
    sync::Arc,
};
use tokio::net::UnixListener;

pub const MAX_CONFIG_BYTES: usize = 32_768;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReleaseSource {
    pub trusted_root: PathBuf,
    pub metadata_url: String,
    pub targets_url: String,
    pub state_dir: PathBuf,
    pub release_dir: PathBuf,
    pub selection_dir: PathBuf,
    pub compatibility: release::Compatibility,
}
impl ReleaseSource {
    pub fn config(&self) -> io::Result<release::ReleaseConfig> {
        Ok(release::ReleaseConfig {
            trusted_root: self.trusted_root.clone(),
            metadata_url: self.metadata_url.parse().map_err(|_| invalid())?,
            targets_url: self.targets_url.parse().map_err(|_| invalid())?,
            state_dir: self.state_dir.clone(),
            release_dir: self.release_dir.clone(),
            compatibility: self.compatibility.clone(),
            limits: release::ReleaseLimits::default(),
        })
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct NetworkSelection {
    #[serde(default)]
    pub enabled: bool,
    pub interface: String,
    pub kind: setup::LinkKind,
    pub recovery_profile_uuid: Option<String>,
    pub station_profile_uuid: Option<String>,
}
impl NetworkSelection {
    pub fn selection(&self) -> network::Selection {
        network::Selection {
            enabled: self.enabled,
            interface: self.interface.clone(),
            kind: self.kind,
            recovery_profile_uuid: self.recovery_profile_uuid.clone(),
            station_profile_uuid: self.station_profile_uuid.clone(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub schema_version: u32,
    #[serde(default)]
    pub enabled: bool,
    pub state_dir: PathBuf,
    pub configuration_dir: PathBuf,
    pub socket: PathBuf,
    pub operator_origin: String,
    pub operator_code_file: PathBuf,
    pub owner_socket: Option<PathBuf>,
    pub release: Option<ReleaseSource>,
    pub network: Option<NetworkSelection>,
    pub reset_stores: Vec<reset::Store>,
    #[serde(default)]
    pub startup: startup::Config,
    pub bus_root: Option<PathBuf>,
}

fn invalid() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidInput,
        "invalid surround configuration",
    )
}
pub fn absolute(path: &Path) -> bool {
    path.is_absolute()
        && path.as_os_str().len() <= 4_096
        && path
            .components()
            .all(|c| matches!(c, Component::RootDir | Component::Normal(_)))
        && !path
            .as_os_str()
            .to_string_lossy()
            .chars()
            .any(char::is_control)
}
impl Config {
    pub fn parse(bytes: &[u8]) -> io::Result<Self> {
        if bytes.len() > MAX_CONFIG_BYTES {
            return Err(invalid());
        }
        let config: Self = serde_json::from_slice(bytes).map_err(|_| invalid())?;
        config.validate()?;
        Ok(config)
    }
    pub fn read(path: &Path) -> io::Result<Self> {
        Self::parse(&atomic::read(path, MAX_CONFIG_BYTES)?.ok_or_else(invalid)?)
    }
    pub fn validate(&self) -> io::Result<()> {
        if self.schema_version != 1
            || ![
                &self.state_dir,
                &self.configuration_dir,
                &self.socket,
                &self.operator_code_file,
            ]
            .into_iter()
            .all(|p| absolute(p))
            || self.socket.as_os_str().len() > 103
            || self.socket.file_name().is_none()
            || self
                .owner_socket
                .as_ref()
                .is_some_and(|p| !absolute(p) || p.as_os_str().len() > 103 || *p == self.socket)
            || self.bus_root.as_ref().is_some_and(|p| !absolute(p))
            || self.reset_stores.is_empty()
            || self.reset_stores.len() > 7
            || self.reset_stores[0] != reset::Store::Voice
            || self.reset_stores.iter().collect::<BTreeSet<_>>().len() != self.reset_stores.len()
        {
            return Err(invalid());
        }
        self.startup.validate().map_err(|_| invalid())?;
        let mut roots = vec![&self.state_dir, &self.configuration_dir];
        if let Some(source) = &self.release {
            for path in [
                &source.trusted_root,
                &source.state_dir,
                &source.release_dir,
                &source.selection_dir,
            ] {
                if !absolute(path) {
                    return Err(invalid());
                }
            }
            roots.extend([
                &source.state_dir,
                &source.release_dir,
                &source.selection_dir,
            ]);
            let release = source.config()?;
            for url in [&release.metadata_url, &release.targets_url] {
                if url.scheme() != "https"
                    || url.host_str().is_none()
                    || !url.username().is_empty()
                    || url.password().is_some()
                    || url.query().is_some()
                    || url.fragment().is_some()
                    || !url.path().ends_with('/')
                {
                    return Err(invalid());
                }
            }
            if source.compatibility.board.is_empty()
                || source.compatibility.architecture.is_empty()
                || source.compatibility.protocol == 0
                || source.compatibility.data_schema == 0
            {
                return Err(invalid());
            }
        }
        if roots.iter().collect::<BTreeSet<_>>().len() != roots.len()
            || roots.iter().any(|p| p.as_path() == Path::new("/"))
            || roots.iter().enumerate().any(|(i, left)| {
                roots
                    .iter()
                    .enumerate()
                    .any(|(j, right)| i != j && left.starts_with(right))
            })
            || roots.iter().any(|root| {
                self.socket
                    .parent()
                    .is_some_and(|parent| parent.starts_with(root) || root.starts_with(parent))
            })
            || self.bus_root.as_ref().is_some_and(|bus| {
                roots
                    .iter()
                    .any(|root| bus.starts_with(root) || root.starts_with(bus))
            })
        {
            return Err(invalid());
        }
        // Validate the origin using the exact same router constructor. The
        // fixed value is a validation-only fixture, never a service credential.
        let (tx, _) = tokio::sync::mpsc::channel(1);
        let _ = operator::router(
            crate::config::Secret::new(vec![b'0'; 64])?,
            &self.operator_origin,
            operator::Shared::new(false),
            tx,
            Arc::new(|| 0),
        )?;
        if let Some(network) = &self.network {
            let mut selection = network.selection();
            selection.enabled = true;
            selection.validate().map_err(|_| invalid())?;
        }
        Ok(())
    }

    /// No worker or OS connection is created by validation/doctor.
    pub fn network_adapter(
        &self,
        clock: operator::Clock,
    ) -> io::Result<Box<dyn network::NetworkAdapter + Send>> {
        if self.enabled
            && let Some(selection) = &self.network
            && selection.enabled
        {
            #[cfg(feature = "network-manager")]
            {
                return network::nm::NmAdapter::spawn(selection.selection(), clock)
                    .map(|adapter| Box::new(adapter) as Box<dyn network::NetworkAdapter + Send>)
                    .map_err(io::Error::other);
            }
            #[cfg(not(feature = "network-manager"))]
            {
                let _ = clock;
                return Err(io::Error::new(
                    io::ErrorKind::Unsupported,
                    "network-manager support was not built",
                ));
            }
        }
        let _ = clock;
        Ok(Box::<network::UnsupportedAdapter>::default())
    }
}

/// Linux peers share the kernel boot identity. A development host must provide
/// an explicit identity; it is not claimed to identify the lamp's boot.
pub fn clock_id(given: Option<&str>) -> io::Result<String> {
    let id = match given {
        Some(id) => id.to_owned(),
        None if cfg!(target_os = "linux") => fs::read_to_string("/proc/sys/kernel/random/boot_id")?
            .trim()
            .into(),
        None => {
            return Err(io::Error::other(
                "supply --clock-id for this development host",
            ));
        }
    };
    if !crate::setup::valid_id(&id) {
        return Err(invalid());
    }
    Ok(id)
}

pub struct ListenerGuard {
    path: PathBuf,
    device: u64,
    inode: u64,
    _lock: atomic::DirectoryLock,
}
impl Drop for ListenerGuard {
    fn drop(&mut self) {
        if fs::symlink_metadata(&self.path).is_ok_and(|metadata| {
            metadata.file_type().is_socket()
                && metadata.dev() == self.device
                && metadata.ino() == self.inode
        }) {
            let _ = fs::remove_file(&self.path);
        }
    }
}

/// No public TCP listener. First provisions an independent TLS proxy to this
/// private socket, retaining the configured Host and Origin. A process lock and
/// refused connection are both needed before removing a stale socket.
pub async fn listen(path: &Path) -> io::Result<(UnixListener, ListenerGuard)> {
    if !absolute(path) || path.as_os_str().len() > 103 {
        return Err(invalid());
    }
    let parent = path.parent().ok_or_else(invalid)?;
    let lock = atomic::DirectoryLock::acquire(parent)?;
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if !metadata.file_type().is_socket()
                || metadata.uid() != rustix::process::geteuid().as_raw()
                || metadata.mode() & 0o077 != 0
            {
                return Err(invalid());
            }
            match tokio::time::timeout(
                std::time::Duration::from_secs(1),
                tokio::net::UnixStream::connect(path),
            )
            .await
            .map_err(|_| {
                io::Error::new(
                    io::ErrorKind::TimedOut,
                    "existing operator socket did not answer",
                )
            })? {
                Ok(_) => {
                    return Err(io::Error::new(
                        io::ErrorKind::AddrInUse,
                        "operator socket is active",
                    ));
                }
                Err(error) if error.kind() == io::ErrorKind::ConnectionRefused => {
                    let again = fs::symlink_metadata(path)?;
                    if again.ino() != metadata.ino() || again.dev() != metadata.dev() {
                        return Err(invalid());
                    }
                    fs::remove_file(path)?;
                }
                Err(error) => return Err(error),
            }
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    let listener = UnixListener::bind(path)?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    let metadata = fs::symlink_metadata(path)?;
    let guard = ListenerGuard {
        path: path.into(),
        device: metadata.dev(),
        inode: metadata.ino(),
        _lock: lock,
    };
    Ok((listener, guard))
}

pub fn diagnostic(view: &operator::View, now_us: u64) -> lamp_msgs::Diagnostic {
    use lamp_msgs::{Diagnostic, Level};
    let view = view.at(now_us);
    let (level, message) = if !view.enabled {
        (Level::Warn, "maintenance disabled")
    } else if view.operation.as_ref().is_some_and(|op| {
        matches!(
            op.phase,
            operator::Phase::Failed | operator::Phase::Interrupted
        )
    }) {
        (Level::Warn, "maintenance operation needs attention")
    } else {
        (
            Level::Ok,
            "operator service available; owner readiness is separate",
        )
    };
    Diagnostic::new(level, "surround", message)
        .with("enabled", view.enabled)
        .with(
            "operation",
            view.operation
                .as_ref()
                .map_or("none".into(), |op| format!("{:?}", op.phase)),
        )
        .with(
            "runtime",
            format!("{:?}", view.runtime.unwrap_or(setup::RuntimeState::Unknown)),
        )
}
