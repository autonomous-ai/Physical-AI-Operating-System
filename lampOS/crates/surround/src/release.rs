//! TUF-authenticated application releases. This module stages files only: it
//! never starts/stops a service, changes a current pointer, or touches devices.
//!
//! Tough 0.24 uses the host UTC clock and writes its cache non-atomically. A
//! private cache is seeded from our strict atomic envelope. An incomplete-update
//! journal preserves verified progress across restarts; corrupt partial metadata
//! refuses new updates while already accepted binaries remain recoverable.
//! Expiry is never bypassed.
use crate::atomic::{self, DirectoryLock};
use futures_util::{StreamExt, stream};
use rustix::fs::{self, AtFlags, FileType, Mode, OFlags, RenameFlags};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    fs::File,
    io::{self, Read},
    path::{Component, Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tough::{
    Repository, RepositoryLoader, TargetName, Transport, TransportError, TransportErrorKind,
    TransportStream,
    schema::{RoleType, Root, Signed, Snapshot, Targets, Timestamp},
};
use url::Url;

pub const CORE_BINARIES: [&str; 3] = ["lamp-voice", "lamp-body", "lamp-eyes"];
const TRUST_BYTES: usize = 24 * 1024 * 1024;
const MAX_ROOT_CHAIN: usize = 64;
const CLOCK_SKEW_SECONDS: u64 = 30;

#[derive(Clone, Debug)]
pub struct ReleaseConfig {
    pub trusted_root: PathBuf,
    pub metadata_url: Url,
    pub targets_url: Url,
    pub state_dir: PathBuf,
    /// Parent of the binary SHA-256 directories, normally /opt/lampOS/releases.
    pub release_dir: PathBuf,
    pub compatibility: Compatibility,
    pub limits: ReleaseLimits,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Compatibility {
    pub board: String,
    pub architecture: String,
    pub protocol: u32,
    pub data_schema: u32,
}

#[derive(Clone, Copy, Debug)]
pub struct ReleaseLimits {
    pub metadata_bytes: usize,
    pub manifest_bytes: usize,
    pub binary_bytes: usize,
    pub total_binary_bytes: u64,
    pub target_count: usize,
    pub requests: usize,
    pub root_updates: u64,
    pub retained_sets: usize,
    pub binary_directories: usize,
    /// Bounds asynchronous repository waits. Bounded local reads, hashing and
    /// fsync cannot be preempted by a Tokio timer; this is not an end-to-end
    /// wall-clock guarantee for a stalled storage device.
    pub download_timeout: Duration,
}

impl Default for ReleaseLimits {
    fn default() -> Self {
        Self {
            metadata_bytes: 256 * 1024,
            manifest_bytes: 64 * 1024,
            binary_bytes: 128 * 1024 * 1024,
            total_binary_bytes: 384 * 1024 * 1024,
            target_count: 1024,
            requests: 64,
            root_updates: 16,
            retained_sets: 16,
            binary_directories: 128,
            download_timeout: Duration::from_secs(120),
        }
    }
}

/// An explicit assertion from the platform time owner, not inferred from TLS
/// working or a plausible date. Tough also checks its actual host UTC clock.
#[derive(Clone, Copy, Debug)]
pub struct TrustedTime {
    pub unix_seconds: u64,
    pub synchronized: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReleaseManifest {
    pub schema_version: u32,
    pub release_sequence: u64,
    pub board: String,
    pub architecture: String,
    pub protocol: u32,
    pub data_schema_min: u32,
    pub data_schema_max: u32,
    pub binaries: Vec<BinarySpec>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BinarySpec {
    pub name: String,
    pub sha256: String,
    pub length: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct VerifiedBinary {
    name: String,
    sha256: String,
    length: u64,
    path: PathBuf,
}
impl VerifiedBinary {
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn sha256(&self) -> &str {
        &self.sha256
    }
    pub fn length(&self) -> u64 {
        self.length
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// Constructed only after signature, compatibility and all file checks pass.
/// Consumers must call recover_verified again immediately before activation;
/// a descriptor is evidence of a completed check, not a permanent file lease.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct VerifiedRelease {
    set_sha256: String,
    manifest: ReleaseManifest,
    binaries: Vec<VerifiedBinary>,
}
impl VerifiedRelease {
    pub fn set_sha256(&self) -> &str {
        &self.set_sha256
    }
    pub fn manifest(&self) -> &ReleaseManifest {
        &self.manifest
    }
    pub fn binaries(&self) -> &[VerifiedBinary] {
        &self.binaries
    }
    pub fn binary(&self, name: &str) -> Option<&VerifiedBinary> {
        self.binaries.iter().find(|binary| binary.name == name)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReleaseError(pub String);
impl fmt::Display for ReleaseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for ReleaseError {}
impl From<io::Error> for ReleaseError {
    fn from(error: io::Error) -> Self {
        Self(error.to_string())
    }
}
impl From<serde_json::Error> for ReleaseError {
    fn from(error: serde_json::Error) -> Self {
        Self(error.to_string())
    }
}
fn fail(message: &str) -> ReleaseError {
    ReleaseError(message.into())
}
fn tuf(error: impl fmt::Display) -> ReleaseError {
    ReleaseError(format!("TUF verification: {error}"))
}
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn valid_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn plain(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
        && value != "."
        && value != ".."
}

impl ReleaseConfig {
    fn check(&self) -> Result<(), ReleaseError> {
        let limits = self.limits;
        if !plain(&self.compatibility.board)
            || !plain(&self.compatibility.architecture)
            || self.compatibility.protocol == 0
            || self.compatibility.data_schema == 0
            || !(1..=1024 * 1024).contains(&limits.metadata_bytes)
            || !(1..=64 * 1024).contains(&limits.manifest_bytes)
            || !(1..=512 * 1024 * 1024).contains(&limits.binary_bytes)
            || limits.total_binary_bytes == 0
            || limits.total_binary_bytes > 1536 * 1024 * 1024
            || !(4..=4096).contains(&limits.target_count)
            || !(8..=128).contains(&limits.requests)
            || !(1..=32).contains(&limits.root_updates)
            || !(2..=32).contains(&limits.retained_sets)
            || !(3..=512).contains(&limits.binary_directories)
            || limits.download_timeout.is_zero()
            || limits.download_timeout > Duration::from_secs(600)
        {
            return Err(fail("invalid release compatibility or resource bounds"));
        }
        for url in [&self.metadata_url, &self.targets_url] {
            if !matches!(url.scheme(), "file" | "https")
                || !url.username().is_empty()
                || url.password().is_some()
                || url.query().is_some()
                || url.fragment().is_some()
                || !url.path().ends_with('/')
            {
                return Err(fail(
                    "repository needs credential-free HTTPS or explicit file directory URLs",
                ));
            }
        }
        if self.metadata_url.scheme() != self.targets_url.scheme() {
            return Err(fail(
                "metadata and target transports must use the same scheme",
            ));
        }
        if within(&self.metadata_url, &self.targets_url)
            || within(&self.targets_url, &self.metadata_url)
        {
            return Err(fail(
                "metadata and target URL directory scopes must not overlap",
            ));
        }
        Ok(())
    }
}

impl ReleaseManifest {
    fn check(&self, config: &ReleaseConfig) -> Result<(), ReleaseError> {
        let expected = &config.compatibility;
        if self.schema_version != 1
            || self.release_sequence == 0
            || self.board != expected.board
            || self.architecture != expected.architecture
            || self.protocol != expected.protocol
            || self.data_schema_min == 0
            || self.data_schema_min > self.data_schema_max
            || !(self.data_schema_min..=self.data_schema_max).contains(&expected.data_schema)
            || self.binaries.len() != CORE_BINARIES.len()
        {
            return Err(fail(
                "release is incompatible with board, architecture, protocol or stored data",
            ));
        }
        self.check_structure(config)
    }

    fn check_structure(&self, config: &ReleaseConfig) -> Result<(), ReleaseError> {
        if self.schema_version != 1
            || self.release_sequence == 0
            || !plain(&self.board)
            || !plain(&self.architecture)
            || self.protocol == 0
            || self.data_schema_min == 0
            || self.data_schema_min > self.data_schema_max
            || self.binaries.len() != CORE_BINARIES.len()
        {
            return Err(fail("invalid accepted release structure"));
        }
        let mut names = BTreeSet::new();
        let mut total = 0u64;
        for binary in &self.binaries {
            if !CORE_BINARIES.contains(&binary.name.as_str())
                || !names.insert(&binary.name)
                || !valid_digest(&binary.sha256)
                || binary.length == 0
                || binary.length > config.limits.binary_bytes as u64
            {
                return Err(fail(
                    "release must contain exactly the three bounded, hash-addressed core binaries",
                ));
            }
            total = total
                .checked_add(binary.length)
                .ok_or_else(|| fail("release length overflow"))?;
        }
        if total > config.limits.total_binary_bytes {
            return Err(fail("release exceeds total binary byte bound"));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Accepted {
    manifest_target: String,
    manifest_json: String,
    targets: Value,
    root_version: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Trust {
    schema_version: u32,
    bootstrap_sha256: String,
    metadata_url: String,
    targets_url: String,
    roots: Vec<Value>,
    timestamp: Option<Value>,
    snapshot: Option<Value>,
    targets: Option<Value>,
    latest_unix_seconds: u64,
    highest_release_sequence: u64,
    highest_set_sha256: Option<String>,
    accepted: BTreeMap<String, Accepted>,
    /// Committed atomically with metadata floors. After RepositoryLoader has
    /// returned, read_target can change only its UTC cache, not TUF roles.
    #[serde(default)]
    cache_metadata_captured: bool,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    sha256: String,
    trust: Trust,
}

fn root_from(value: &Value) -> Result<Signed<Root>, ReleaseError> {
    serde_json::from_value(value.clone()).map_err(Into::into)
}

fn root_chain(bootstrap: &[u8], roots: &[Value]) -> Result<Vec<Signed<Root>>, ReleaseError> {
    if roots.is_empty() || roots.len() > MAX_ROOT_CHAIN {
        return Err(fail("invalid persistent root chain length"));
    }
    let pinned: Signed<Root> = serde_json::from_slice(bootstrap)?;
    pinned.signed.verify_role(&pinned).map_err(tuf)?;
    let mut verified = Vec::with_capacity(roots.len());
    for value in roots {
        let next = root_from(value)?;
        if next.signed.version.get() > u64::MAX - MAX_ROOT_CHAIN as u64 {
            return Err(fail("root version is too large for bounded safe rotation"));
        }
        if let Some(previous) = verified.last() {
            let previous: &Signed<Root> = previous;
            if previous.signed.version.get().checked_add(1) != Some(next.signed.version.get()) {
                return Err(fail("persistent root rotation is not consecutive"));
            }
            previous.signed.verify_role(&next).map_err(tuf)?;
        } else if next != pinned {
            return Err(fail(
                "persistent root chain differs from provisioned anchor",
            ));
        }
        next.signed.verify_role(&next).map_err(tuf)?;
        verified.push(next);
    }
    Ok(verified)
}

fn validate_trust(
    trust: &Trust,
    config: &ReleaseConfig,
    bootstrap: &[u8],
) -> Result<Vec<Signed<Root>>, ReleaseError> {
    if trust.schema_version != 1
        || trust.bootstrap_sha256 != digest(bootstrap)
        || trust.metadata_url != config.metadata_url.as_str()
        || trust.targets_url != config.targets_url.as_str()
        || trust.accepted.len() > config.limits.retained_sets
        || (trust.highest_release_sequence == 0) != trust.highest_set_sha256.is_none()
    {
        return Err(fail(
            "persistent trust identity, version or bounds are corrupt",
        ));
    }
    let roots = root_chain(bootstrap, &trust.roots)?;
    let root = &roots.last().unwrap().signed;
    if let Some(value) = &trust.timestamp {
        let role: Signed<Timestamp> = serde_json::from_value(value.clone())?;
        root.verify_role(&role).map_err(tuf)?;
    }
    if let Some(value) = &trust.snapshot {
        let role: Signed<Snapshot> = serde_json::from_value(value.clone())?;
        root.verify_role(&role).map_err(tuf)?;
    }
    if let Some(value) = &trust.targets {
        let role: Signed<Targets> = serde_json::from_value(value.clone())?;
        root.verify_role(&role).map_err(tuf)?;
    }
    for (set, accepted) in &trust.accepted {
        if !valid_digest(set) || digest(accepted.manifest_json.as_bytes()) != *set {
            return Err(fail("accepted release manifest is corrupt"));
        }
        authenticate_accepted(accepted, &roots, config)?;
    }
    if let Some(highest) = &trust.highest_set_sha256 {
        let accepted = trust
            .accepted
            .get(highest)
            .ok_or_else(|| fail("highest accepted release is missing"))?;
        let manifest: ReleaseManifest = serde_json::from_str(&accepted.manifest_json)?;
        if manifest.release_sequence != trust.highest_release_sequence {
            return Err(fail("release sequence watermark is corrupt"));
        }
    }
    Ok(roots)
}

fn save_trust(config: &ReleaseConfig, trust: &Trust) -> Result<(), ReleaseError> {
    let envelope = Envelope {
        sha256: digest(&serde_json::to_vec(trust)?),
        trust: trust.clone(),
    };
    atomic::write(
        &config.state_dir.join("trust.json"),
        &serde_json::to_vec(&envelope)?,
        TRUST_BYTES,
    )?;
    Ok(())
}

fn load_trust(config: &ReleaseConfig, create: bool) -> Result<(Trust, Vec<u8>), ReleaseError> {
    let bootstrap = atomic::read(&config.trusted_root, config.limits.metadata_bytes)?
        .ok_or_else(|| fail("no provisioned TUF trust root"))?;
    let marker = atomic::read(&config.state_dir.join("initialized"), 64)?;
    let stored = atomic::read(&config.state_dir.join("trust.json"), TRUST_BYTES)?;
    let trust = match (marker, stored) {
        (None, None) if create => {
            let root: Signed<Root> = serde_json::from_slice(&bootstrap)?;
            root.signed.verify_role(&root).map_err(tuf)?;
            let trust = Trust {
                schema_version: 1,
                bootstrap_sha256: digest(&bootstrap),
                metadata_url: config.metadata_url.to_string(),
                targets_url: config.targets_url.to_string(),
                roots: vec![serde_json::to_value(root)?],
                timestamp: None,
                snapshot: None,
                targets: None,
                latest_unix_seconds: 0,
                highest_release_sequence: 0,
                highest_set_sha256: None,
                accepted: BTreeMap::new(),
                cache_metadata_captured: false,
            };
            atomic::write(
                &config.state_dir.join("initialized"),
                trust.bootstrap_sha256.as_bytes(),
                64,
            )?;
            save_trust(config, &trust)?;
            trust
        }
        (Some(marker), Some(bytes)) => {
            if marker != digest(&bootstrap).as_bytes() {
                return Err(fail(
                    "provisioned root changed; explicit reprovisioning is required",
                ));
            }
            let envelope: Envelope = serde_json::from_slice(&bytes)?;
            if envelope.sha256 != digest(&serde_json::to_vec(&envelope.trust)?) {
                return Err(fail("persistent trust envelope checksum failed"));
            }
            envelope.trust
        }
        _ => {
            return Err(fail(
                "missing or interrupted persistent trust initialization; refuse silent reset",
            ));
        }
    };
    validate_trust(&trust, config, &bootstrap)?;
    Ok((trust, bootstrap))
}

fn check_time(time: TrustedTime, floor: u64) -> Result<u64, ReleaseError> {
    let actual = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| fail("host UTC predates Unix epoch"))?
        .as_secs();
    if !time.synchronized
        || time.unix_seconds < 1_577_836_800
        || time.unix_seconds < floor
        || actual < floor
        || actual.abs_diff(time.unix_seconds) > CLOCK_SKEW_SECONDS
    {
        return Err(fail(
            "trusted UTC is unknown, backwards, or disagrees with the TUF host clock",
        ));
    }
    Ok(actual.max(time.unix_seconds))
}

fn target_matches(targets: &Signed<Targets>, name: &str, bytes: &[u8]) -> Result<(), ReleaseError> {
    let name = TargetName::new(name).map_err(tuf)?;
    let target = targets
        .signed
        .targets
        .get(&name)
        .ok_or_else(|| fail("signed target is missing"))?;
    if target.length != bytes.len() as u64
        || target.hashes.sha256.as_ref() != Sha256::digest(bytes).as_slice()
    {
        return Err(fail("signed target length or SHA-256 differs"));
    }
    Ok(())
}

fn authenticate_accepted(
    accepted: &Accepted,
    roots: &[Signed<Root>],
    config: &ReleaseConfig,
) -> Result<ReleaseManifest, ReleaseError> {
    if !plain(&accepted.manifest_target)
        || accepted.manifest_json.len() > config.limits.manifest_bytes
    {
        return Err(fail("accepted manifest bounds are corrupt"));
    }
    let root = roots
        .iter()
        .find(|root| root.signed.version.get() == accepted.root_version)
        .ok_or_else(|| fail("accepted release signing root is missing"))?;
    let targets: Signed<Targets> = serde_json::from_value(accepted.targets.clone())?;
    if targets.signed.targets.len() > config.limits.target_count
        || targets.signed.delegations.is_some()
    {
        return Err(fail("unsupported delegated or excessive release targets"));
    }
    root.signed.verify_role(&targets).map_err(tuf)?;
    target_matches(
        &targets,
        &accepted.manifest_target,
        accepted.manifest_json.as_bytes(),
    )?;
    let manifest: ReleaseManifest = serde_json::from_str(&accepted.manifest_json)?;
    manifest.check_structure(config)?;
    // Compatibility with current persistent data is checked at recovery/use.
    for binary in &manifest.binaries {
        let name =
            TargetName::new(format!("binaries/{}/{}", binary.sha256, binary.name)).map_err(tuf)?;
        let entry = targets
            .signed
            .targets
            .get(&name)
            .ok_or_else(|| fail("manifest binary is absent from signed targets"))?;
        if entry.length != binary.length
            || format_digest(entry.hashes.sha256.as_ref()) != binary.sha256
        {
            return Err(fail("manifest binary differs from signed target"));
        }
    }
    Ok(manifest)
}

fn format_digest(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[derive(Debug, Default)]
struct TransferState {
    requests: usize,
    bytes: u64,
}

#[derive(Clone, Debug)]
struct BoundedTransport {
    metadata: Url,
    targets: Url,
    limits: ReleaseLimits,
    state: Arc<Mutex<TransferState>>,
    client: reqwest::Client,
    cache: PathBuf,
}

fn transport_error(url: &Url, message: impl fmt::Display) -> TransportError {
    TransportError::new_with_cause(
        TransportErrorKind::Other,
        url.as_str(),
        io::Error::other(message.to_string()),
    )
}

#[tough::async_trait]
impl Transport for BoundedTransport {
    async fn fetch(&self, url: Url) -> Result<TransportStream, TransportError> {
        let metadata = within(&url, &self.metadata);
        if !metadata && !within(&url, &self.targets) {
            return Err(transport_error(
                &url,
                "resource leaves configured repository",
            ));
        }
        if metadata && !metadata_name(url.path().strip_prefix(self.metadata.path()).unwrap_or("")) {
            return Err(transport_error(
                &url,
                "only top-level TUF metadata is supported",
            ));
        }
        let cap = if metadata {
            self.limits.metadata_bytes
        } else {
            self.limits.binary_bytes.max(self.limits.manifest_bytes)
        };
        {
            let mut state = self
                .state
                .lock()
                .map_err(|_| transport_error(&url, "transfer state poisoned"))?;
            state.requests += 1;
            if state.requests > self.limits.requests {
                return Err(transport_error(&url, "repository request bound exceeded"));
            }
        }
        let bytes = if url.scheme() == "file" {
            let path = url
                .to_file_path()
                .map_err(|_| transport_error(&url, "invalid local repository path"))?;
            match read_public_file(&path, cap) {
                Ok(bytes) => bytes,
                Err(error) if error.kind() == io::ErrorKind::NotFound => {
                    return Err(TransportError::new(
                        TransportErrorKind::FileNotFound,
                        url.as_str(),
                    ));
                }
                Err(error) => return Err(transport_error(&url, error)),
            }
        } else {
            let response = self
                .client
                .get(url.clone())
                .send()
                .await
                .map_err(|error| transport_error(&url, error.without_url()))?;
            if response.status() == reqwest::StatusCode::NOT_FOUND {
                return Err(TransportError::new(
                    TransportErrorKind::FileNotFound,
                    url.as_str(),
                ));
            }
            if !response.status().is_success() {
                return Err(transport_error(
                    &url,
                    "repository HTTP status is not successful",
                ));
            }
            if response
                .content_length()
                .is_some_and(|length| length > cap as u64)
            {
                return Err(transport_error(
                    &url,
                    "repository resource exceeds byte bound",
                ));
            }
            let mut chunks = response.bytes_stream();
            let mut bytes = Vec::new();
            while let Some(chunk) = chunks.next().await {
                let chunk = chunk.map_err(|error| transport_error(&url, error.without_url()))?;
                if bytes
                    .len()
                    .checked_add(chunk.len())
                    .is_none_or(|length| length > cap)
                {
                    return Err(transport_error(
                        &url,
                        "repository resource exceeds byte bound",
                    ));
                }
                bytes.extend_from_slice(&chunk);
            }
            bytes
        };
        {
            let mut state = self
                .state
                .lock()
                .map_err(|_| transport_error(&url, "transfer state poisoned"))?;
            state.bytes = state
                .bytes
                .checked_add(bytes.len() as u64)
                .ok_or_else(|| transport_error(&url, "transfer byte overflow"))?;
            let total = self.limits.total_binary_bytes
                + self.limits.metadata_bytes as u64 * self.limits.requests as u64;
            if state.bytes > total {
                return Err(transport_error(
                    &url,
                    "repository total byte bound exceeded",
                ));
            }
            if metadata
                && let Some(name) = url.path_segments().and_then(|mut parts| parts.next_back())
                && let Some(version) = name
                    .strip_suffix(".root.json")
                    .and_then(|name| name.parse::<u64>().ok())
            {
                atomic::write(
                    &self.cache.join(format!("proof-{version}.json")),
                    &bytes,
                    self.limits.metadata_bytes,
                )
                .map_err(|error| transport_error(&url, error))?;
            }
        }
        if metadata && url.path().ends_with("targets.json") {
            let targets: Signed<Targets> =
                serde_json::from_slice(&bytes).map_err(|error| transport_error(&url, error))?;
            if targets.signed.delegations.is_some()
                || targets.signed.targets.len() > self.limits.target_count
            {
                return Err(transport_error(
                    &url,
                    "delegated or excessive release targets are unsupported",
                ));
            }
        }
        Ok(Box::pin(stream::once(std::future::ready(Ok(
            tough::Bytes::from(bytes),
        )))))
    }
}

fn metadata_name(name: &str) -> bool {
    if name == "timestamp.json" || name == "snapshot.json" || name == "targets.json" {
        return true;
    }
    [".root.json", ".snapshot.json", ".targets.json"]
        .iter()
        .any(|suffix| {
            name.strip_suffix(suffix).is_some_and(|version| {
                !version.is_empty() && version.bytes().all(|byte| byte.is_ascii_digit())
            })
        })
}

fn within(url: &Url, base: &Url) -> bool {
    url.scheme() == base.scheme()
        && url.host_str() == base.host_str()
        && url.port_or_known_default() == base.port_or_known_default()
        && url.username().is_empty()
        && url.password().is_none()
        && url.query().is_none()
        && url.fragment().is_none()
        && url.path().starts_with(base.path())
}

fn public_directory(path: &Path) -> io::Result<File> {
    if !path.is_absolute() {
        return Err(io::Error::other("absolute path required"));
    }
    let mut directory = File::open("/")?;
    for part in path.components() {
        match part {
            Component::RootDir => {}
            Component::Normal(name) => {
                directory = fs::openat(
                    &directory,
                    name,
                    OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                    Mode::empty(),
                )?
                .into()
            }
            _ => return Err(io::Error::other("dot path component is forbidden")),
        }
    }
    Ok(directory)
}

fn read_public_file(path: &Path, cap: usize) -> io::Result<Vec<u8>> {
    let parent = public_directory(
        path.parent()
            .ok_or_else(|| io::Error::other("file has no parent"))?,
    )?;
    let file: File = fs::openat(
        &parent,
        path.file_name()
            .ok_or_else(|| io::Error::other("file has no name"))?,
        OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
        Mode::empty(),
    )?
    .into();
    let stat = fs::fstat(&file)?;
    if FileType::from_raw_mode(stat.st_mode) != FileType::RegularFile
        || stat.st_nlink != 1
        || stat.st_size < 0
        || stat.st_size as u64 > cap as u64
    {
        return Err(io::Error::other(
            "repository file is linked, special or oversized",
        ));
    }
    let mut bytes = Vec::new();
    file.take(cap as u64 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > cap {
        return Err(io::Error::other("repository file grew beyond its bound"));
    }
    Ok(bytes)
}

fn seed_cache(path: &Path, trust: &Trust) -> Result<(), ReleaseError> {
    for (name, value) in [
        ("timestamp.json", &trust.timestamp),
        ("snapshot.json", &trust.snapshot),
        ("targets.json", &trust.targets),
    ] {
        if let Some(value) = value {
            atomic::write(&path.join(name), &serde_json::to_vec(value)?, TRUST_BYTES)?;
        }
    }
    if trust.latest_unix_seconds > 0 {
        let timestamp = jiff::Timestamp::from_second(
            i64::try_from(trust.latest_unix_seconds)
                .map_err(|_| fail("trusted UTC is outside representable range"))?,
        )
        .map_err(tuf)?;
        atomic::write(
            &path.join("latest_known_time.json"),
            &serde_json::to_vec(&timestamp)?,
            128,
        )?;
    }
    Ok(())
}

fn cache_bytes(path: &Path, cap: usize) -> Result<Option<Vec<u8>>, ReleaseError> {
    match read_public_file(path, cap) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn clear_cache(path: &Path) -> Result<(), ReleaseError> {
    let directory = atomic::directory(path)?;
    let mut names = Vec::new();
    for entry in fs::Dir::read_from(&directory).map_err(io::Error::from)? {
        let entry = entry.map_err(io::Error::from)?;
        let name = entry
            .file_name()
            .to_str()
            .map_err(|_| fail("invalid cache entry"))?;
        if matches!(name, "." | "..") {
            continue;
        }
        let base = name
            .strip_prefix('.')
            .and_then(|name| name.strip_suffix(".pending"))
            .unwrap_or(name);
        let allowed = matches!(
            base,
            "root.json"
                | "timestamp.json"
                | "snapshot.json"
                | "targets.json"
                | "latest_known_time.json"
        ) || base
            .strip_prefix("proof-")
            .and_then(|name| name.strip_suffix(".json"))
            .is_some_and(|version| {
                !version.is_empty() && version.bytes().all(|byte| byte.is_ascii_digit())
            });
        if !allowed || names.len() >= MAX_ROOT_CHAIN * 2 + 10 {
            return Err(fail("unexpected or excessive private TUF cache entries"));
        }
        // Check every entry before removing any. Never follow a link or remove
        // a directory even when it occupies an expected cache filename.
        read_public_file(&path.join(name), TRUST_BYTES)?;
        names.push(name.to_owned());
    }
    for name in names {
        fs::unlinkat(&directory, name, AtFlags::empty()).map_err(io::Error::from)?;
    }
    directory.sync_all()?;
    Ok(())
}

/// Persist complete metadata and verified consecutive root proofs. A partial
/// cached role blocks fresh acceptance instead of discarding a rollback floor.
fn capture_progress(
    config: &ReleaseConfig,
    bootstrap: &[u8],
    trust: &mut Trust,
    cache: &Path,
    metadata_complete: bool,
) -> Result<(), ReleaseError> {
    capture_clock(trust, cache)?;
    let cached_root = cache_bytes(&cache.join("root.json"), config.limits.metadata_bytes)?
        .map(|bytes| {
            serde_json::from_slice::<Signed<Root>>(&bytes)
                .map_err(|_| fail("interrupted or corrupt root cache; refuse trust rollback"))
        })
        .transpose()?;
    let original = root_from(trust.roots.last().unwrap())?;
    let mut current = original.clone();
    let mut chain = trust.roots.clone();
    // Root proofs are atomic before delivery to Tough. Rechecking both Tough
    // thresholds also recovers a crash between its remove(root) and write(root).
    // An invalid downloaded proof is untrusted input, never a rotation.
    for _ in 0..config.limits.root_updates {
        let version = current
            .signed
            .version
            .get()
            .checked_add(1)
            .ok_or_else(|| fail("root version overflow"))?;
        let Some(proof) = atomic::read(
            &cache.join(format!("proof-{version}.json")),
            config.limits.metadata_bytes,
        )?
        else {
            break;
        };
        let Ok(next) = serde_json::from_slice::<Signed<Root>>(&proof) else {
            break;
        };
        if next.signed.version.get() != version
            || current.signed.verify_role(&next).is_err()
            || next.signed.verify_role(&next).is_err()
        {
            break;
        }
        chain.push(serde_json::to_value(&next)?);
        current = next;
    }
    let verified = root_chain(bootstrap, &chain)?;
    if let Some(root) = cached_root
        && (root.signed.version < original.signed.version || !verified.contains(&root))
    {
        return Err(fail("cache root differs from verified rotation chain"));
    }
    trust.roots = chain;
    let root = root_from(trust.roots.last().unwrap())?;
    // Match Tough's TUF fast-forward recovery: changing timestamp or snapshot
    // key sets resets their version floors, even if an old key remains valid.
    // The snapshot floor also owns the targets version floor. Seeded old cache
    // values must not silently reinstate those floors after a root-only crash.
    let keys_changed = original
        .signed
        .keys(RoleType::Timestamp)
        .ne(root.signed.keys(RoleType::Timestamp))
        || original
            .signed
            .keys(RoleType::Snapshot)
            .ne(root.signed.keys(RoleType::Snapshot));
    for (name, previous) in [
        ("timestamp.json", &mut trust.timestamp),
        ("snapshot.json", &mut trust.snapshot),
        ("targets.json", &mut trust.targets),
    ] {
        let superseded = if keys_changed { previous.take() } else { None };
        let candidate = cache_bytes(&cache.join(name), config.limits.metadata_bytes)?
            .map(|bytes| {
                serde_json::from_slice::<Value>(&bytes).map_err(|_| {
                    fail("interrupted or corrupt metadata cache; refuse trust rollback")
                })
            })
            .transpose()?
            .filter(|candidate| superseded.as_ref() != Some(candidate));
        let valid = |value: &Value| -> bool {
            match name {
                "timestamp.json" => serde_json::from_value::<Signed<Timestamp>>(value.clone())
                    .is_ok_and(|signed| root.signed.verify_role(&signed).is_ok()),
                "snapshot.json" => serde_json::from_value::<Signed<Snapshot>>(value.clone())
                    .is_ok_and(|signed| root.signed.verify_role(&signed).is_ok()),
                _ => serde_json::from_value::<Signed<Targets>>(value.clone())
                    .is_ok_and(|signed| root.signed.verify_role(&signed).is_ok()),
            }
        };
        if let Some(candidate) = candidate {
            if valid(&candidate) {
                if previous
                    .as_ref()
                    .filter(|previous| valid(previous))
                    .is_some_and(|previous| {
                        previous["signed"]["version"].as_u64()
                            > candidate["signed"]["version"].as_u64()
                    })
                {
                    return Err(fail("cached metadata rolls back persistent trust"));
                }
                *previous = Some(candidate);
            } else if previous.as_ref() == Some(&candidate) {
                *previous = None;
            } else {
                return Err(fail("cached metadata signature is corrupt"));
            }
        } else if previous.as_ref().is_some_and(|previous| !valid(previous)) {
            *previous = None;
        }
    }
    trust.cache_metadata_captured = metadata_complete;
    validate_trust(trust, config, bootstrap)?;
    save_trust(config, trust)
}

fn capture_clock(trust: &mut Trust, cache: &Path) -> Result<(), ReleaseError> {
    if let Some(bytes) = cache_bytes(&cache.join("latest_known_time.json"), 128)? {
        let timestamp: jiff::Timestamp = serde_json::from_slice(&bytes)
            .map_err(|_| fail("interrupted or corrupt cached UTC; refuse clock rollback"))?;
        let seconds =
            u64::try_from(timestamp.as_second()).map_err(|_| fail("cached UTC predates epoch"))?;
        trust.latest_unix_seconds = trust.latest_unix_seconds.max(seconds);
    }
    Ok(())
}

fn capture_pending(
    config: &ReleaseConfig,
    bootstrap: &[u8],
    trust: &mut Trust,
    cache: &Path,
) -> Result<(), ReleaseError> {
    if trust.cache_metadata_captured {
        capture_clock(trust, cache)?;
        save_trust(config, trust)
    } else {
        capture_progress(config, bootstrap, trust, cache, false)
    }
}

async fn target_bytes(
    repository: &Repository,
    name: &str,
    cap: usize,
) -> Result<Vec<u8>, ReleaseError> {
    let target = TargetName::new(name).map_err(tuf)?;
    let entry = repository
        .targets()
        .signed
        .targets
        .get(&target)
        .ok_or_else(|| fail("target absent from signed top-level metadata"))?;
    if entry.length > cap as u64 {
        return Err(fail("target exceeds configured bound"));
    }
    let stream = repository
        .read_target(&target)
        .await
        .map_err(tuf)?
        .ok_or_else(|| fail("signed target missing"))?;
    futures_util::pin_mut!(stream);
    let mut bytes = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(tuf)?;
        if bytes
            .len()
            .checked_add(chunk.len())
            .is_none_or(|size| size > cap)
        {
            return Err(fail("target exceeds configured bound"));
        }
        bytes.extend_from_slice(&chunk);
    }
    // A read_target stream is unverified until its final item has been read.
    target_matches(repository.targets(), name, &bytes)?;
    Ok(bytes)
}

fn binary_bytes(
    config: &ReleaseConfig,
    binary: &BinarySpec,
) -> Result<Option<Vec<u8>>, ReleaseError> {
    let path = config.release_dir.join(&binary.sha256).join(&binary.name);
    let parent_path = path.parent().unwrap();
    match atomic::directory(parent_path) {
        Ok(_) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    }
    let Some(bytes) = atomic::read(&path, config.limits.binary_bytes)? else {
        return Ok(None);
    };
    if bytes.len() as u64 != binary.length || digest(&bytes) != binary.sha256 {
        return Err(fail("immutable installed binary digest or length failed"));
    }
    let parent = atomic::directory(parent_path)?;
    let file = fs::openat(
        &parent,
        binary.name.as_str(),
        OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .map_err(io::Error::from)?;
    let mode = fs::fstat(file).map_err(io::Error::from)?.st_mode;
    if mode & 0o222 != 0 || mode & 0o100 == 0 {
        return Err(fail(
            "immutable binary must be owner-executable and nonwritable",
        ));
    }
    Ok(Some(bytes))
}

fn stage_binary(
    config: &ReleaseConfig,
    binary: &BinarySpec,
    bytes: &[u8],
) -> Result<(), ReleaseError> {
    if bytes.len() as u64 != binary.length || digest(bytes) != binary.sha256 {
        return Err(fail("binary does not match authenticated manifest"));
    }
    if binary_bytes(config, binary)?.is_some() {
        return Ok(());
    }
    let root = atomic::directory(&config.release_dir)?;
    let entries = fs::Dir::read_from(&root).map_err(io::Error::from)?;
    let mut count = 0usize;
    for entry in entries {
        let entry = entry.map_err(io::Error::from)?;
        if !matches!(entry.file_name().to_bytes(), b"." | b"..") {
            count += 1;
        }
        if count > config.limits.binary_directories {
            return Err(fail(
                "immutable release storage directory bound exceeded; explicit collection required",
            ));
        }
    }
    let directory_path = config.release_dir.join(&binary.sha256);
    if atomic::directory(&directory_path).is_err() && count >= config.limits.binary_directories {
        return Err(fail("immutable release storage is full"));
    }
    atomic::ensure_private_dir(&directory_path)?;
    let partial = directory_path.join(format!("{}.partial", binary.name));
    // A previous partial file is uncommitted. It may be replaced, never the
    // immutable final file or any previously accepted descriptor.
    atomic::write(&partial, bytes, config.limits.binary_bytes)?;
    let directory = atomic::directory(&directory_path)?;
    let file = fs::openat(
        &directory,
        partial.file_name().unwrap(),
        OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .map_err(io::Error::from)?;
    fs::fchmod(&file, Mode::from_bits_retain(0o500)).map_err(io::Error::from)?;
    fs::fsync(&file).map_err(io::Error::from)?;
    fs::renameat_with(
        &directory,
        partial.file_name().unwrap(),
        &directory,
        binary.name.as_str(),
        RenameFlags::NOREPLACE,
    )
    .map_err(io::Error::from)?;
    directory.sync_all()?;
    binary_bytes(config, binary)?.ok_or_else(|| fail("staged binary vanished"))?;
    Ok(())
}

fn descriptor(
    config: &ReleaseConfig,
    set: String,
    manifest: ReleaseManifest,
) -> Result<VerifiedRelease, ReleaseError> {
    manifest.check(config)?;
    let mut binaries = Vec::new();
    for binary in &manifest.binaries {
        binary_bytes(config, binary)?.ok_or_else(|| fail("accepted binary is missing"))?;
        binaries.push(VerifiedBinary {
            name: binary.name.clone(),
            sha256: binary.sha256.clone(),
            length: binary.length,
            path: config.release_dir.join(&binary.sha256).join(&binary.name),
        });
    }
    Ok(VerifiedRelease {
        set_sha256: set,
        manifest,
        binaries,
    })
}

pub async fn verify_and_stage(
    config: &ReleaseConfig,
    time: TrustedTime,
    manifest_target: &str,
) -> Result<VerifiedRelease, ReleaseError> {
    config.check()?;
    if !plain(manifest_target) {
        return Err(fail("release manifest target must be one plain filename"));
    }
    check_time(time, 0)?;
    atomic::ensure_private_dir(&config.state_dir)?;
    let _lock = DirectoryLock::acquire(&config.state_dir)?;
    atomic::ensure_private_dir(&config.release_dir)?;
    let (mut trust, bootstrap) = load_trust(config, true)?;
    trust.latest_unix_seconds = check_time(time, trust.latest_unix_seconds)?;
    save_trust(config, &trust)?;
    let working = config.state_dir.join("verification");
    atomic::ensure_private_dir(&working)?;
    let pending = config.state_dir.join("verification-pending");
    if let Some(marker) = atomic::read(&pending, 1)? {
        if marker != b"1" {
            return Err(fail("invalid incomplete-verification journal"));
        }
        capture_pending(config, &bootstrap, &mut trust, &working)?;
        atomic::remove(&pending)?;
    }
    clear_cache(&working)?;
    seed_cache(&working, &trust)?;
    trust.cache_metadata_captured = false;
    save_trust(config, &trust)?;
    atomic::write(&pending, b"1", 1)?;
    let root = serde_json::to_vec(trust.roots.last().unwrap())?;
    let transfer = Arc::new(Mutex::new(TransferState::default()));
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let verifier = rustls_platform_verifier::Verifier::new(provider.clone())
        .map_err(|error| fail(&format!("HTTPS certificate verifier: {error}")))?;
    let tls = rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|error| fail(&format!("HTTPS protocol configuration: {error}")))?
        .dangerous()
        // This is the maintained platform certificate and hostname verifier,
        // not a custom acceptance rule or a certificate-verification bypass.
        .with_custom_certificate_verifier(Arc::new(verifier))
        .with_no_client_auth();
    let client = reqwest::Client::builder()
        .tls_backend_preconfigured(tls)
        .https_only(true)
        .redirect(reqwest::redirect::Policy::none())
        .timeout(config.limits.download_timeout)
        .connect_timeout(Duration::from_secs(10).min(config.limits.download_timeout))
        .build()
        .map_err(|error| fail(&format!("HTTPS client: {error}")))?;
    let transport = BoundedTransport {
        metadata: config.metadata_url.clone(),
        targets: config.targets_url.clone(),
        limits: config.limits,
        state: transfer.clone(),
        client,
        cache: working.clone(),
    };
    let limits = tough::Limits {
        max_root_size: config.limits.metadata_bytes as u64,
        max_targets_size: config.limits.metadata_bytes as u64,
        max_timestamp_size: config.limits.metadata_bytes as u64,
        max_snapshot_size: config.limits.metadata_bytes as u64,
        max_root_updates: config.limits.root_updates,
    };
    let operation = async {
        let loaded = RepositoryLoader::new(
            &root,
            config.metadata_url.clone(),
            config.targets_url.clone(),
        )
        .transport(transport)
        .limits(limits)
        .datastore(&working)
        .load()
        .await;
        capture_progress(config, &bootstrap, &mut trust, &working, true)?;
        let repository = loaded.map_err(tuf)?;
        if repository.targets().signed.targets.len() > config.limits.target_count
            || repository.targets().signed.delegations.is_some()
        {
            return Err(fail(
                "release repository has too many targets or unsupported delegations",
            ));
        }
        let manifest_bytes =
            target_bytes(&repository, manifest_target, config.limits.manifest_bytes).await?;
        let manifest: ReleaseManifest = serde_json::from_slice(&manifest_bytes)?;
        manifest.check(config)?;
        let set = digest(&manifest_bytes);
        if manifest.release_sequence < trust.highest_release_sequence
            || (manifest.release_sequence == trust.highest_release_sequence
                && trust.highest_set_sha256.as_deref() != Some(&set))
        {
            return Err(fail(
                "release sequence rollback or same-sequence substitution",
            ));
        }
        if !trust.accepted.contains_key(&set) && trust.accepted.len() == config.limits.retained_sets
        {
            return Err(fail(
                "accepted release bound reached; explicit collection required",
            ));
        }
        let accepted = Accepted {
            manifest_target: manifest_target.into(),
            manifest_json: String::from_utf8(manifest_bytes)
                .map_err(|_| fail("manifest is not UTF-8"))?,
            targets: serde_json::to_value(repository.targets())?,
            root_version: repository.root().signed.version.get(),
        };
        let roots = root_chain(&bootstrap, &trust.roots)?;
        authenticate_accepted(&accepted, &roots, config)?;
        for binary in &manifest.binaries {
            if binary_bytes(config, binary)?.is_none() {
                let name = format!("binaries/{}/{}", binary.sha256, binary.name);
                let bytes = target_bytes(&repository, &name, config.limits.binary_bytes).await?;
                stage_binary(config, binary, &bytes)?;
            }
        }
        let verified = descriptor(config, set.clone(), manifest)?;
        trust.highest_release_sequence = verified.manifest.release_sequence;
        trust.highest_set_sha256 = Some(set.clone());
        trust.accepted.insert(set, accepted);
        save_trust(config, &trust)?;
        Ok(verified)
    };
    let result = tokio::time::timeout(config.limits.download_timeout, operation).await;
    // A timeout can occur after a verified root/timestamp was cached. Preserve
    // that progress; an uncommitted release remains unavailable to recovery.
    capture_pending(config, &bootstrap, &mut trust, &working)?;
    if result.is_ok() {
        atomic::remove(&pending)?;
    }
    result.map_err(|_| fail("release download deadline exceeded"))?
}

/// Recover only an already accepted set. No network, expiry bypass, new trust,
/// or service action occurs; all three immutable files are rehashed.
pub fn recover_verified(
    config: &ReleaseConfig,
    set_sha256: &str,
) -> Result<VerifiedRelease, ReleaseError> {
    config.check()?;
    if !valid_digest(set_sha256) {
        return Err(fail("invalid release set SHA-256"));
    }
    let _lock = DirectoryLock::acquire(&config.state_dir)?;
    let (trust, bootstrap) = load_trust(config, false)?;
    let accepted = trust
        .accepted
        .get(set_sha256)
        .ok_or_else(|| fail("release set has never been accepted"))?;
    let roots = root_chain(&bootstrap, &trust.roots)?;
    let manifest = authenticate_accepted(accepted, &roots, config)?;
    descriptor(config, set_sha256.into(), manifest)
}

/// Selection must remain inspectable when the active binary is damaged, so
/// rollback can still choose a healthy accepted set. This proves metadata only
/// and deliberately cannot construct a VerifiedRelease or expose file paths.
pub(crate) fn accepted_manifest(
    config: &ReleaseConfig,
    set_sha256: &str,
) -> Result<ReleaseManifest, ReleaseError> {
    config.check()?;
    if !valid_digest(set_sha256) {
        return Err(fail("invalid release set SHA-256"));
    }
    let _lock = DirectoryLock::acquire(&config.state_dir)?;
    let (trust, bootstrap) = load_trust(config, false)?;
    let accepted = trust
        .accepted
        .get(set_sha256)
        .ok_or_else(|| fail("release set has never been accepted"))?;
    authenticate_accepted(accepted, &root_chain(&bootstrap, &trust.roots)?, config)
}

/// Collection is exposed through Selector, which holds the selection lock and
/// always protects the complete current/previous sets. Trust floors and root
/// proofs are lifetime records and are never collected.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct CollectionReport {
    pub removed_sets: usize,
    pub removed_directories: usize,
    pub removed_files: usize,
}

pub(crate) fn collect_unselected(
    config: &ReleaseConfig,
    protected: &[String],
) -> Result<CollectionReport, ReleaseError> {
    config.check()?;
    if protected.len() > config.limits.retained_sets
        || protected.iter().any(|set| !valid_digest(set))
    {
        return Err(fail("invalid protected release set list"));
    }
    let _lock = DirectoryLock::acquire(&config.state_dir)?;
    if atomic::read(&config.state_dir.join("initialized"), 64)?.is_none() {
        if !protected.is_empty()
            || atomic::read(&config.state_dir.join("trust.json"), TRUST_BYTES)?.is_some()
        {
            return Err(fail("collection requires initialized trust"));
        }
        return Ok(CollectionReport::default());
    }
    let (mut trust, bootstrap) = load_trust(config, false)?;
    let roots = root_chain(&bootstrap, &trust.roots)?;
    let mut keep: BTreeSet<String> = protected.iter().cloned().collect();
    if let Some(highest) = &trust.highest_set_sha256 {
        keep.insert(highest.clone());
    }
    let mut hashes = BTreeSet::new();
    for set in &keep {
        let accepted = trust
            .accepted
            .get(set)
            .ok_or_else(|| fail("protected release is not accepted"))?;
        for binary in authenticate_accepted(accepted, &roots, config)?.binaries {
            hashes.insert(binary.sha256);
        }
    }
    let root = atomic::directory(&config.release_dir)?;
    let mut plan = Vec::<(String, Vec<String>)>::new();
    let mut directories = 0;
    for entry in fs::Dir::read_from(&root).map_err(io::Error::from)? {
        let entry = entry.map_err(io::Error::from)?;
        let bytes = entry.file_name().to_bytes();
        if matches!(bytes, b"." | b"..") {
            continue;
        }
        directories += 1;
        let hash = std::str::from_utf8(bytes).map_err(|_| fail("unknown release directory"))?;
        if directories > config.limits.binary_directories || !valid_digest(hash) {
            return Err(fail("unknown or excessive release directories"));
        }
        let directory = atomic::directory(&config.release_dir.join(hash))?;
        let mut names = Vec::new();
        for file in fs::Dir::read_from(&directory).map_err(io::Error::from)? {
            let file = file.map_err(io::Error::from)?;
            let bytes = file.file_name().to_bytes();
            if matches!(bytes, b"." | b"..") {
                continue;
            }
            let name = std::str::from_utf8(bytes).map_err(|_| fail("unknown release file"))?;
            if names.len() >= CORE_BINARIES.len() * 3
                || !CORE_BINARIES.iter().any(|binary| {
                    name == *binary
                        || name == format!("{binary}.partial")
                        || name == format!(".{binary}.partial.pending")
                })
            {
                return Err(fail("foreign file blocks release collection"));
            }
            let descriptor = fs::openat(
                &directory,
                name,
                OFlags::RDONLY | OFlags::NONBLOCK | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                Mode::empty(),
            )
            .map_err(io::Error::from)?;
            let stat = fs::fstat(&descriptor).map_err(io::Error::from)?;
            if FileType::from_raw_mode(stat.st_mode) != FileType::RegularFile
                || stat.st_mode & 0o077 != 0
                || stat.st_nlink != 1
                || stat.st_uid != rustix::process::geteuid().as_raw()
                || stat.st_size < 0
                || stat.st_size as u64 > config.limits.binary_bytes as u64
            {
                return Err(fail("unsafe release file blocks collection"));
            }
            names.push(name.to_owned());
        }
        if !hashes.contains(hash) {
            plan.push((hash.to_owned(), names));
        }
    }
    let mut report = CollectionReport {
        removed_sets: trust.accepted.len() - keep.len(),
        ..CollectionReport::default()
    };
    trust.accepted.retain(|set, _| keep.contains(set));
    // Commit logical removal first. A crash can leave unreachable owned files,
    // but can never leave accepted metadata referring to a deleted binary.
    save_trust(config, &trust)?;
    for (hash, names) in plan {
        let directory = atomic::directory(&config.release_dir.join(&hash))?;
        for name in names {
            atomic::remove(&config.release_dir.join(&hash).join(name))?;
            report.removed_files += 1;
        }
        directory.sync_all()?;
        fs::unlinkat(&root, hash.as_str(), AtFlags::REMOVEDIR).map_err(io::Error::from)?;
        root.sync_all()?;
        report.removed_directories += 1;
    }
    Ok(report)
}
