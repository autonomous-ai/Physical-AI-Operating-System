//! Whole provider generations. Publication never changes a running Voice;
//! its supervisor selects one generation while Voice is safely quiescent.
use crate::{
    atomic,
    config::{self, ProviderDocument, Secret},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    io,
    path::{Path, PathBuf},
};

pub const MAX_GENERATIONS: usize = 16;
pub const MAX_INSTRUCTIONS: usize = 16_384;
const MAX_MANIFEST: usize = 4_096;
const FILES: [&str; 4] = [
    "document.json",
    "provider.json",
    "provider.key",
    "instructions.txt",
];

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Generation {
    pub id: String,
    pub document_sha256: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    schema_version: u32,
    generation: Generation,
    files: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Selection {
    pub current: Option<Generation>,
    pub previous: Option<Generation>,
}

pub struct Configurations {
    root: PathBuf,
}

fn invalid() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        "invalid or incomplete configuration generation",
    )
}
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn valid_id(id: &str) -> bool {
    id.len() == 32
        && id
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}
fn valid_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}

impl Configurations {
    /// The caller holds this root's DirectoryLock for the writer's lifetime.
    pub fn open(root: &Path) -> io::Result<Self> {
        atomic::ensure_private_dir(root)?;
        atomic::ensure_private_dir(&root.join("generations"))?;
        Ok(Self { root: root.into() })
    }

    pub fn provider_path(&self, generation: &Generation) -> io::Result<PathBuf> {
        self.verify(generation)?;
        Ok(self
            .root
            .join("generations")
            .join(&generation.id)
            .join("provider.json"))
    }

    /// Validates and writes every private file before publishing candidate.json.
    /// A crash during staging leaves an unselected, bounded orphan, never a
    /// mixed credential/configuration pair. IDs must never be reused.
    pub fn stage(
        &self,
        id: &str,
        mut document: ProviderDocument,
        credential: Secret,
        instructions: Option<&str>,
    ) -> io::Result<Generation> {
        if !valid_id(id)
            || instructions.is_some_and(|s| {
                s.trim().is_empty() || s.len() > MAX_INSTRUCTIONS || s.contains('\0')
            })
        {
            return Err(invalid());
        }
        let parent_path = self.root.join("generations");
        let parent = atomic::directory(&parent_path)?;
        let dir = parent_path.join(id);
        document.provider.credential_file = dir
            .join("provider.key")
            .to_str()
            .ok_or_else(invalid)?
            .into();
        document.provider.instruction_file =
            instructions.map(|_| dir.join("instructions.txt").to_string_lossy().into_owned());
        document.validate()?;
        document.provider.validate_credential(&credential)?;
        match std::fs::symlink_metadata(&dir) {
            Ok(_) => {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    "configuration identity already exists",
                ));
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        self.collect_superseded()?;
        // mkdirat must fail on an existing identity; ensure_private_dir would
        // instead admit it and allow a retry to mutate an accepted generation.
        rustix::fs::mkdirat(&parent, id, rustix::fs::Mode::from_bits_retain(0o700))?;
        parent.sync_all()?;
        config::write_document(&dir.join("document.json"), &document)?;
        config::write_voice_payload(&dir.join("provider.json"), &document.provider)?;
        config::write_secret(&dir.join("provider.key"), &credential)?;
        if let Some(text) = instructions {
            atomic::write(
                &dir.join("instructions.txt"),
                text.as_bytes(),
                MAX_INSTRUCTIONS,
            )?;
        }
        let mut files = BTreeMap::new();
        for file in FILES {
            if file == "instructions.txt" && instructions.is_none() {
                continue;
            }
            let bytes = atomic::read(&dir.join(file), MAX_INSTRUCTIONS)?.ok_or_else(invalid)?;
            files.insert(file.into(), digest(&bytes));
        }
        let generation = Generation {
            id: id.into(),
            document_sha256: files["document.json"].clone(),
        };
        let manifest = Manifest {
            schema_version: 1,
            generation: generation.clone(),
            files,
        };
        atomic::write(
            &dir.join("manifest.json"),
            &serde_json::to_vec(&manifest)?,
            MAX_MANIFEST,
        )?;
        self.verify(&generation)?;
        atomic::write(
            &self.root.join("candidate.json"),
            &serde_json::to_vec(&generation)?,
            MAX_MANIFEST,
        )?;
        Ok(generation)
    }

    /// Reclaim this store's superseded generations before staging. Current,
    /// previous and last candidate identities survive even when their files
    /// are damaged. The caller holds the same DirectoryLock used by selection;
    /// no live owner can change its selected generation during collection.
    pub fn collect_superseded(&self) -> io::Result<Vec<String>> {
        let selection = self.selection()?;
        let mut keep: BTreeSet<String> = [selection.current, selection.previous]
            .into_iter()
            .flatten()
            .map(|g| g.id)
            .collect();
        if let Some(bytes) = atomic::read(&self.root.join("candidate.json"), MAX_MANIFEST)? {
            let candidate: Generation = serde_json::from_slice(&bytes).map_err(|_| invalid())?;
            if !valid_id(&candidate.id) || !valid_digest(&candidate.document_sha256) {
                return Err(invalid());
            }
            keep.insert(candidate.id);
        }
        let parent_path = self.root.join("generations");
        let parent = atomic::directory(&parent_path)?;
        let mut garbage = Vec::new();
        // Preflight the complete bounded tree before removing any file. A
        // crash during later deletion can leave only unselected partial data.
        for (count, item) in std::fs::read_dir(&parent_path)?.enumerate() {
            if count >= MAX_GENERATIONS {
                return Err(io::Error::other("too many configuration directories"));
            }
            let item = item?;
            let id = item.file_name().into_string().map_err(|_| invalid())?;
            if !item.file_type()?.is_dir() || !valid_id(&id) {
                return Err(invalid());
            }
            atomic::directory(&item.path())?;
            if keep.contains(&id) {
                continue;
            }
            let mut files = Vec::new();
            for (count, entry) in std::fs::read_dir(item.path())?.enumerate() {
                if count >= 10 {
                    return Err(invalid());
                }
                let entry = entry?;
                let file = entry.file_name().into_string().map_err(|_| invalid())?;
                let base = file
                    .strip_prefix('.')
                    .and_then(|s| s.strip_suffix(".pending"))
                    .unwrap_or(&file);
                if !FILES.contains(&base) && base != "manifest.json" {
                    return Err(invalid());
                }
                atomic::read(&entry.path(), MAX_INSTRUCTIONS)?.ok_or_else(invalid)?;
                files.push(entry.path());
            }
            garbage.push((id, files));
        }
        garbage.sort_by(|a, b| a.0.cmp(&b.0));
        for (id, files) in &garbage {
            for path in files {
                atomic::remove(path)?;
            }
            rustix::fs::unlinkat(&parent, id.as_str(), rustix::fs::AtFlags::REMOVEDIR)?;
            parent.sync_all()?;
        }
        Ok(garbage.into_iter().map(|(id, _)| id).collect())
    }

    pub fn verify(&self, generation: &Generation) -> io::Result<()> {
        if !valid_id(&generation.id) || !valid_digest(&generation.document_sha256) {
            return Err(invalid());
        }
        let dir = self.root.join("generations").join(&generation.id);
        let bytes = atomic::read(&dir.join("manifest.json"), MAX_MANIFEST)?.ok_or_else(invalid)?;
        let manifest: Manifest = serde_json::from_slice(&bytes).map_err(|_| invalid())?;
        if manifest.schema_version != 1
            || manifest.generation != *generation
            || !(3..=4).contains(&manifest.files.len())
            || manifest.files.keys().any(|s| !FILES.contains(&s.as_str()))
            || manifest.files.get("document.json") != Some(&generation.document_sha256)
        {
            return Err(invalid());
        }
        for file in &FILES[..3] {
            if !manifest.files.contains_key(*file) {
                return Err(invalid());
            }
        }
        for (file, expected) in &manifest.files {
            let bytes = atomic::read(&dir.join(file), MAX_INSTRUCTIONS)?.ok_or_else(invalid)?;
            if digest(&bytes) != *expected {
                return Err(invalid());
            }
        }
        let document = config::read_document(&dir.join("document.json"))?.ok_or_else(invalid)?;
        let payload = atomic::read(&dir.join("provider.json"), config::MAX_DOCUMENT_BYTES)?
            .ok_or_else(invalid)?;
        if payload != serde_json::to_vec(&document.provider)?
            || Path::new(&document.provider.credential_file) != dir.join("provider.key")
            || document.provider.instruction_file.as_deref().map(Path::new)
                != manifest
                    .files
                    .contains_key("instructions.txt")
                    .then(|| dir.join("instructions.txt"))
                    .as_deref()
        {
            return Err(invalid());
        }
        let secret = Secret::new(
            atomic::read(&dir.join("provider.key"), config::MAX_SECRET_BYTES)?
                .ok_or_else(invalid)?,
        )?;
        document.provider.validate_credential(&secret)
    }

    pub fn candidate(&self) -> io::Result<Option<Generation>> {
        atomic::read(&self.root.join("candidate.json"), MAX_MANIFEST)?
            .map(|bytes| {
                let generation: Generation =
                    serde_json::from_slice(&bytes).map_err(|_| invalid())?;
                self.verify(&generation)?;
                Ok(generation)
            })
            .transpose()
    }

    pub fn selection(&self) -> io::Result<Selection> {
        let selected: Selection = match atomic::read(&self.root.join("current.json"), MAX_MANIFEST)?
        {
            Some(bytes) => serde_json::from_slice(&bytes).map_err(|_| invalid())?,
            None => Selection::default(),
        };
        // This is a durable identity/CAS record, not a readiness proof. A
        // damaged selected file must not prevent rollback to a healthy target.
        for generation in [&selected.current, &selected.previous]
            .into_iter()
            .flatten()
        {
            if !valid_id(&generation.id) || !valid_digest(&generation.document_sha256) {
                return Err(invalid());
            }
        }
        if (selected.current.is_none() && selected.previous.is_some())
            || (selected.current.is_some() && selected.current == selected.previous)
        {
            return Err(invalid());
        }
        Ok(selected)
    }

    /// Supervisor-only primitive, not an HTTP operation. Its caller holds the
    /// exclusive process fence, has rechecked 45s quiet and closed Voice's old
    /// workers, and keeps them closed through this compare-and-select. The
    /// filesystem cannot prove those physical ownership conditions.
    pub fn select_quiescent(
        &self,
        enabled: bool,
        expected: &Option<Generation>,
        target: &Generation,
    ) -> io::Result<Selection> {
        if !enabled {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "configuration activation disabled",
            ));
        }
        let current = self.selection()?;
        if &current.current != expected {
            return Err(io::Error::other("configuration selection changed"));
        }
        self.verify(target)?;
        if current.current.as_ref() == Some(target) {
            return Ok(current);
        }
        let selected = Selection {
            current: Some(target.clone()),
            previous: current.current,
        };
        atomic::write(
            &self.root.join("current.json"),
            &serde_json::to_vec(&selected)?,
            MAX_MANIFEST,
        )?;
        Ok(selected)
    }
}
