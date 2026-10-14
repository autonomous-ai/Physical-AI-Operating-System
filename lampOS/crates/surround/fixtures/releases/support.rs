use ed25519_dalek::{Signer, SigningKey};
use lamp_surround::{atomic, release::*};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tempfile::TempDir;
use tough::schema::{Role, Root, Snapshot, Targets, Timestamp, key::Key};
use url::Url;

pub const FUTURE: &str = "2099-01-01T00:00:00Z";
pub const EXPIRED: &str = "2001-01-01T00:00:00Z";

pub fn sha(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}
pub fn time() -> TrustedTime {
    TrustedTime {
        unix_seconds: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs(),
        synchronized: true,
    }
}
pub fn key(seed: u8) -> SigningKey {
    SigningKey::from_bytes(&[seed; 32])
}
pub fn key_value(key: &SigningKey) -> Value {
    json!({"keytype":"ed25519", "scheme":"ed25519", "keyval":{"public":hex::encode(key.verifying_key().as_bytes())}})
}
pub fn key_id(key: &SigningKey) -> String {
    let value: Key = serde_json::from_value(key_value(key)).unwrap();
    hex::encode(value.key_id().unwrap().as_ref())
}
pub fn signed<T: Role + DeserializeOwned + Serialize>(value: Value, keys: &[&SigningKey]) -> Value {
    let role: T = serde_json::from_value(value).unwrap();
    let canonical = role.canonical_form().unwrap();
    json!({"signed":serde_json::to_value(role).unwrap(), "signatures":keys.iter().map(|key| {
        json!({"keyid":key_id(key), "sig":hex::encode(key.sign(&canonical).to_bytes())})
    }).collect::<Vec<_>>()})
}
pub fn root(version: u64, role_key: &SigningKey, signers: &[&SigningKey], expires: &str) -> Value {
    let id = key_id(role_key);
    let role = json!({"keyids":[id], "threshold":1});
    signed::<Root>(
        json!({"_type":"root", "spec_version":"1.0.0", "version":version, "expires":expires,
        "consistent_snapshot":false, "keys":{id:key_value(role_key)},
        "roles":{"root":role,"timestamp":role,"snapshot":role,"targets":role}}),
        signers,
    )
}
pub fn write(path: &Path, bytes: &[u8]) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
}
pub fn write_json(path: &Path, value: &Value) -> Vec<u8> {
    let bytes = serde_json::to_vec(value).unwrap();
    write(path, &bytes);
    bytes
}

pub struct Fixture {
    _dir: TempDir,
    pub config: ReleaseConfig,
    pub metadata: PathBuf,
    pub targets: PathBuf,
}
impl Default for Fixture {
    fn default() -> Self {
        Self::new()
    }
}
impl Fixture {
    pub fn new() -> Self {
        let temp = fs::canonicalize(std::env::temp_dir()).unwrap();
        let dir = tempfile::Builder::new()
            .permissions(fs::Permissions::from_mode(0o700))
            .tempdir_in(temp)
            .unwrap();
        let path = fs::canonicalize(dir.path()).unwrap();
        for name in ["anchor", "state", "releases"] {
            atomic::ensure_private_dir(&path.join(name)).unwrap();
        }
        let metadata = path.join("metadata");
        let targets = path.join("targets");
        fs::create_dir(&metadata).unwrap();
        fs::create_dir(&targets).unwrap();
        let trusted_root = path.join("anchor/root.json");
        atomic::write(
            &trusted_root,
            &serde_json::to_vec(&root(1, &key(1), &[&key(1)], FUTURE)).unwrap(),
            32_768,
        )
        .unwrap();
        let config = ReleaseConfig {
            trusted_root,
            metadata_url: Url::from_directory_path(&metadata).unwrap(),
            targets_url: Url::from_directory_path(&targets).unwrap(),
            state_dir: path.join("state"),
            release_dir: path.join("releases"),
            compatibility: Compatibility {
                board: "orangepi5".into(),
                architecture: "aarch64".into(),
                protocol: 1,
                data_schema: 2,
            },
            limits: ReleaseLimits {
                metadata_bytes: 32_768,
                manifest_bytes: 8_192,
                binary_bytes: 8_192,
                total_binary_bytes: 24_576,
                target_count: 16,
                requests: 16,
                root_updates: 4,
                retained_sets: 4,
                binary_directories: 24,
                download_timeout: Duration::from_secs(10),
            },
        };
        Self {
            _dir: dir,
            config,
            metadata,
            targets,
        }
    }
    pub fn manifest(&self, sequence: u64) -> ReleaseManifest {
        ReleaseManifest {
            schema_version: 1,
            release_sequence: sequence,
            board: "orangepi5".into(),
            architecture: "aarch64".into(),
            protocol: 1,
            data_schema_min: 1,
            data_schema_max: 3,
            binaries: CORE_BINARIES
                .iter()
                .map(|name| {
                    let bytes = format!("test-only {name} revision {sequence}\n").into_bytes();
                    let sha256 = sha(&bytes);
                    write(
                        &self.targets.join(format!("binaries/{sha256}/{name}")),
                        &bytes,
                    );
                    BinarySpec {
                        name: (*name).into(),
                        sha256,
                        length: bytes.len() as u64,
                    }
                })
                .collect(),
        }
    }
    pub fn publish(
        &self,
        manifest: &ReleaseManifest,
        version: u64,
        signing: &SigningKey,
        expires: &str,
    ) {
        let bytes = serde_json::to_vec(manifest).unwrap();
        write(&self.targets.join("release.json"), &bytes);
        let mut targets = serde_json::Map::new();
        targets.insert(
            "release.json".into(),
            json!({"length":bytes.len(),"hashes":{"sha256":sha(&bytes)}}),
        );
        for binary in &manifest.binaries {
            targets.insert(
                format!("binaries/{}/{}", binary.sha256, binary.name),
                json!({"length":binary.length,"hashes":{"sha256":binary.sha256}}),
            );
        }
        self.publish_targets(json!({"_type":"targets","spec_version":"1.0.0","version":version,"expires":expires,"targets":targets}),version,signing,expires);
    }
    pub fn publish_targets(
        &self,
        targets: Value,
        version: u64,
        signing: &SigningKey,
        expires: &str,
    ) {
        let targets = write_json(
            &self.metadata.join("targets.json"),
            &signed::<Targets>(targets, &[signing]),
        );
        let snapshot = signed::<Snapshot>(
            json!({"_type":"snapshot","spec_version":"1.0.0","version":version,"expires":expires,
            "meta":{"targets.json":{"version":version,"length":targets.len(),"hashes":{"sha256":sha(&targets)}}}}),
            &[signing],
        );
        let snapshot = write_json(&self.metadata.join("snapshot.json"), &snapshot);
        let timestamp = signed::<Timestamp>(
            json!({"_type":"timestamp","spec_version":"1.0.0","version":version,"expires":expires,
            "meta":{"snapshot.json":{"version":version,"length":snapshot.len(),"hashes":{"sha256":sha(&snapshot)}}}}),
            &[signing],
        );
        write_json(&self.metadata.join("timestamp.json"), &timestamp);
    }
    pub async fn stage(&self) -> Result<VerifiedRelease, ReleaseError> {
        verify_and_stage(&self.config, time(), "release.json").await
    }
    pub fn target(&self, binary: &BinarySpec) -> PathBuf {
        self.targets
            .join(format!("binaries/{}/{}", binary.sha256, binary.name))
    }
}
