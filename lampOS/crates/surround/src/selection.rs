//! Atomic selection of a complete, previously verified release set.
//!
//! This helper cannot establish quiescence. The privileged supervisor must
//! hold its owner fence, stop old owners safely, and keep that fence through
//! selection and restart. No service, device, or network operation occurs here.
use crate::{
    atomic::{self, DirectoryLock},
    release::{
        BinarySpec, ReleaseConfig, ReleaseError, VerifiedRelease, accepted_manifest,
        recover_verified,
    },
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::PathBuf;

const RECORD_BYTES: usize = 8192;

/// A checked observation for compare-and-select. Callers cannot deserialize or
/// construct one and bypass the persistent-record checks.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Selection {
    generation: u64,
    current_set: String,
    previous_set: Option<String>,
}

impl Selection {
    pub fn generation(&self) -> u64 {
        self.generation
    }
    pub fn current_set(&self) -> &str {
        &self.current_set
    }
    pub fn previous_set(&self) -> Option<&str> {
        self.previous_set.as_deref()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Pointer {
    schema_version: u32,
    generation: u64,
    current_set: Option<String>,
    previous_set: Option<String>,
    /// All three hashes are committed in the same record. They must also match
    /// the authenticated manifest selected by current_set.
    binaries: Vec<BinarySpec>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    sha256: String,
    pointer: Pointer,
}

pub struct Selector {
    release: ReleaseConfig,
    state_dir: PathBuf,
    enabled: bool,
}

fn fail(message: &str) -> ReleaseError {
    ReleaseError(message.into())
}
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn valid_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

impl Selector {
    /// state_dir must be a distinct, private directory from the release trust
    /// store. Both locks remain independent; enabled=false permits inspection
    /// and first initialization, but always refuses a selection change.
    pub fn open(
        release: ReleaseConfig,
        state_dir: PathBuf,
        enabled: bool,
    ) -> Result<Self, ReleaseError> {
        if state_dir == release.state_dir || state_dir == release.release_dir {
            return Err(fail("selection needs its own private state directory"));
        }
        atomic::ensure_private_dir(&state_dir)?;
        let selector = Self {
            release,
            state_dir,
            enabled,
        };
        let _lock = DirectoryLock::acquire(&selector.state_dir)?;
        let marker = atomic::read(&selector.state_dir.join("initialized"), 1)?;
        let record = atomic::read(&selector.state_dir.join("current.json"), RECORD_BYTES)?;
        match (marker, record) {
            (None, None) => {
                // Never silently recreate a pointer if initialization was
                // interrupted or an existing record was lost.
                atomic::write(&selector.state_dir.join("initialized"), b"1", 1)?;
                selector.write(&Pointer {
                    schema_version: 1,
                    generation: 0,
                    current_set: None,
                    previous_set: None,
                    binaries: Vec::new(),
                })?;
            }
            (Some(marker), Some(_)) if marker == b"1" => {
                selector.read()?;
            }
            _ => return Err(fail("selection initialization is incomplete or corrupt")),
        }
        Ok(selector)
    }

    pub fn current(&self) -> Result<Option<Selection>, ReleaseError> {
        let _lock = DirectoryLock::acquire(&self.state_dir)?;
        let pointer = self.read()?;
        Ok(selection(&pointer))
    }

    /// Resolve current paths only through fresh verification of its complete
    /// accepted manifest and files. The returned descriptor is not a file lease.
    pub fn current_release(&self) -> Result<Option<VerifiedRelease>, ReleaseError> {
        let _lock = DirectoryLock::acquire(&self.state_dir)?;
        let pointer = self.read()?;
        pointer
            .current_set
            .as_deref()
            .map(|set| recover_verified(&self.release, set))
            .transpose()
    }

    /// Remove only unselected accepted sets and orphaned owned staging files.
    /// The caller must additionally list every staged/in-progress set it still
    /// needs, and serialize its update coordinator. This lock prevents a
    /// concurrent selection change while current and previous are protected.
    pub fn collect(
        &self,
        additional_protected: &[String],
    ) -> Result<crate::release::CollectionReport, ReleaseError> {
        if !self.enabled {
            return Err(fail("release collection is disabled"));
        }
        let _lock = DirectoryLock::acquire(&self.state_dir)?;
        let pointer = self.read()?;
        let mut protected = additional_protected.to_vec();
        protected.extend(pointer.current_set);
        protected.extend(pointer.previous_set);
        protected.sort();
        protected.dedup();
        crate::release::collect_unselected(&self.release, &protected)
    }

    /// Compare the exact prior generation, then rehash and atomically select
    /// the complete set. A same-current retry leaves its generation and prior
    /// rollback set unchanged. No caller-supplied path is written.
    pub fn select(
        &self,
        expected: Option<&Selection>,
        candidate: &VerifiedRelease,
    ) -> Result<Selection, ReleaseError> {
        if !self.enabled {
            return Err(fail("release selection is disabled"));
        }
        let _lock = DirectoryLock::acquire(&self.state_dir)?;
        let pointer = self.read()?;
        let observed = selection(&pointer);
        if observed.as_ref() != expected {
            return Err(fail("release selection changed; stale expected generation"));
        }
        let verified = recover_verified(&self.release, candidate.set_sha256())?;
        if verified != *candidate {
            return Err(fail(
                "candidate no longer matches its verified release descriptor",
            ));
        }
        if observed
            .as_ref()
            .is_some_and(|current| current.current_set == verified.set_sha256())
        {
            return Ok(observed.unwrap());
        }
        let generation = pointer
            .generation
            .checked_add(1)
            .ok_or_else(|| fail("selection generation overflow"))?;
        let next = Pointer {
            schema_version: 1,
            generation,
            current_set: Some(verified.set_sha256().into()),
            previous_set: pointer.current_set,
            binaries: verified.manifest().binaries.clone(),
        };
        self.write(&next)?;
        Ok(selection(&next).unwrap())
    }

    fn write(&self, pointer: &Pointer) -> Result<(), ReleaseError> {
        let envelope = Envelope {
            sha256: digest(&serde_json::to_vec(pointer)?),
            pointer: pointer.clone(),
        };
        atomic::write(
            &self.state_dir.join("current.json"),
            &serde_json::to_vec(&envelope)?,
            RECORD_BYTES,
        )?;
        Ok(())
    }

    fn read(&self) -> Result<Pointer, ReleaseError> {
        if atomic::read(&self.state_dir.join("initialized"), 1)?.as_deref() != Some(b"1") {
            return Err(fail("selection marker is missing or corrupt"));
        }
        let bytes = atomic::read(&self.state_dir.join("current.json"), RECORD_BYTES)?
            .ok_or_else(|| fail("selection record is missing"))?;
        let envelope: Envelope = serde_json::from_slice(&bytes)?;
        let pointer = envelope.pointer;
        if envelope.sha256 != digest(&serde_json::to_vec(&pointer)?) || pointer.schema_version != 1
        {
            return Err(fail("selection checksum or schema is corrupt"));
        }
        let Some(current) = pointer.current_set.as_deref() else {
            if pointer.generation != 0
                || pointer.previous_set.is_some()
                || !pointer.binaries.is_empty()
            {
                return Err(fail("invalid empty release selection"));
            }
            return Ok(pointer);
        };
        if pointer.generation == 0
            || !valid_digest(current)
            || pointer
                .previous_set
                .as_ref()
                .is_some_and(|previous| !valid_digest(previous) || previous == current)
        {
            return Err(fail("invalid release selection identity"));
        }
        let manifest = accepted_manifest(&self.release, current)?;
        if pointer.binaries != manifest.binaries {
            return Err(fail(
                "selection mixes binary hashes outside its authenticated set",
            ));
        }
        if let Some(previous) = pointer.previous_set.as_deref() {
            accepted_manifest(&self.release, previous)?;
        }
        Ok(pointer)
    }
}

fn selection(pointer: &Pointer) -> Option<Selection> {
    pointer.current_set.as_ref().map(|current| Selection {
        generation: pointer.generation,
        current_set: current.clone(),
        previous_set: pointer.previous_set.clone(),
    })
}
