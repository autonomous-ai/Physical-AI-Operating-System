//! One explicit file owns retention limits; unknown kinds cannot become paths.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{io, path::Path};

pub const DEFAULT_BYTES: u64 = 500_000_000;
pub const DEFAULT_AGE_SECONDS: u64 = 14 * 24 * 60 * 60;
pub const MAX_ROOTS: usize = 4;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub schema_version: u32,
    /// No default-on behavior, even if a timer has been installed.
    #[serde(default)]
    pub enabled: bool,
    pub bus_root: std::path::PathBuf,
    pub roots: Vec<RootConfig>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RootConfig {
    pub kind: Kind,
    pub path: std::path::PathBuf,
    pub unit: String,
    pub max_bytes: u64,
    pub max_age_seconds: u64,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    VoiceServiceMetadata,
}

pub fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message.into())
}

/// Reject ambiguous spellings rather than normalizing them after validation.
pub fn absolute_path(path: &Path) -> io::Result<()> {
    let text = path
        .to_str()
        .ok_or_else(|| invalid("paths must be UTF-8"))?;
    if !path.is_absolute()
        || text.len() > 4096
        || text.ends_with('/')
        || text.chars().any(char::is_control)
        || text
            .split('/')
            .skip(1)
            .any(|c| c.is_empty() || c == "." || c == "..")
        || text.split('/').count() > 65
    {
        return Err(invalid("paths must be bounded absolute normal paths"));
    }
    Ok(())
}

pub fn unit_name(unit: &str) -> io::Result<()> {
    if !unit.ends_with(".service")
        || unit.len() <= ".service".len()
        || unit.len() > 128
        || !unit
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.@".contains(&b))
        || !unit.as_bytes()[0].is_ascii_alphanumeric()
    {
        return Err(invalid("unit must be an explicit system .service name"));
    }
    Ok(())
}

impl Config {
    pub fn fingerprint(&self) -> String {
        // Struct field order is stable; no maps or secrets occur in this file.
        let bytes = serde_json::to_vec(self).expect("retention configuration is serializable");
        format!("{:x}", Sha256::digest(bytes))
    }

    pub fn customer_default() -> Self {
        Self {
            schema_version: 1,
            enabled: false,
            bus_root: "/run/lampos-bus".into(),
            roots: vec![RootConfig {
                kind: Kind::VoiceServiceMetadata,
                path: "/var/lib/lampOS/voice/sessions".into(),
                unit: "lamp-voice.service".into(),
                max_bytes: DEFAULT_BYTES,
                max_age_seconds: DEFAULT_AGE_SECONDS,
            }],
        }
    }

    pub fn validate(&self) -> io::Result<()> {
        if self.schema_version != 1 || self.roots.is_empty() || self.roots.len() > MAX_ROOTS {
            return Err(invalid("schema_version must be 1, with one to four roots"));
        }
        absolute_path(&self.bus_root)?;
        for (index, root) in self.roots.iter().enumerate() {
            absolute_path(&root.path)?;
            // This is an adapter for one writer's sessions, never a generic
            // cleanup of /var/lib, the journal, or a memory database directory.
            if root.path.file_name().is_none_or(|name| name != "sessions")
                || root.path.components().count() < 4
                || root.max_bytes == 0
                || root.max_bytes > 1_000_000_000_000
                || root.max_age_seconds == 0
                || root.max_age_seconds > 366 * 24 * 60 * 60
            {
                return Err(invalid(
                    "voice root must end in /sessions; limits must be positive and bounded",
                ));
            }
            unit_name(&root.unit)?;
            if self.bus_root.starts_with(&root.path) || root.path.starts_with(&self.bus_root) {
                return Err(invalid("bus and retention roots must be separate"));
            }
            for other in &self.roots[..index] {
                if root.path.starts_with(&other.path)
                    || other.path.starts_with(&root.path)
                    || root.unit == other.unit
                {
                    return Err(invalid("retention roots must not overlap or reuse a unit"));
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn customer_defaults_are_explicit_and_off() {
        let mut value = serde_json::to_value(Config::customer_default()).unwrap();
        value.as_object_mut().unwrap().remove("enabled");
        let config: Config = serde_json::from_value(value).unwrap();
        assert!(!config.enabled);
        assert_eq!(config.roots[0].max_bytes, 500_000_000);
        assert_eq!(config.roots[0].max_age_seconds, 1_209_600);
        config.validate().unwrap();
        let mut changed = config.clone();
        changed.enabled = true;
        assert_ne!(config.fingerprint(), changed.fingerprint());
        changed = config.clone();
        changed.roots[0].max_age_seconds += 1;
        assert_ne!(config.fingerprint(), changed.fingerprint());
    }

    /// The product's file, as install places it at `/etc/lampOS/records.json`
    /// (`deploy/product/settings/records.json`): valid, on, and bounding
    /// the voice's sessions at 2.0 GB and two weeks. Without it the timer
    /// runs nothing on a product lamp (Twelve on lamp-cb94, 17 Oct).
    #[test]
    fn the_products_config_is_valid_and_bounds_the_voices_sessions() {
        let config: Config = serde_json::from_str(include_str!(
            "../../../deploy/product/settings/records.json"
        ))
        .unwrap();
        config.validate().unwrap();
        assert!(config.enabled);
        assert_eq!(config.roots.len(), 1);
        let root = &config.roots[0];
        assert_eq!(
            root.path,
            std::path::Path::new("/var/lib/lampOS/voice/sessions")
        );
        assert_eq!(root.unit, "lamp-voice.service");
        assert_eq!(root.max_bytes, 2_000_000_000);
        assert_eq!(root.max_age_seconds, 1_209_600);
    }

    #[test]
    fn unknown_fields_and_kinds_do_not_enable_generic_cleanup() {
        let mut value = serde_json::to_value(Config::customer_default()).unwrap();
        value["roots"][0]["kind"] = "memory".into();
        assert!(serde_json::from_value::<Config>(value).is_err());
        let mut value = serde_json::to_value(Config::customer_default()).unwrap();
        value["delete_everything"] = true.into();
        assert!(serde_json::from_value::<Config>(value).is_err());
    }

    #[test]
    fn broad_ambiguous_and_overlapping_paths_are_rejected() {
        for path in [
            "/",
            "/var/lib",
            "/sessions",
            "sessions",
            "/a/../sessions",
            "/a/./sessions",
            "/a//sessions",
            "/a/sessions/",
        ] {
            let mut config = Config::customer_default();
            config.roots[0].path = path.into();
            assert!(config.validate().is_err(), "{path}");
        }
        let mut config = Config::customer_default();
        config.roots.push(config.roots[0].clone());
        assert!(config.validate().is_err());
        config.roots[1].unit = "different.service".into();
        config.roots[1].path = config.roots[0].path.join("a/sessions");
        assert!(config.validate().is_err());
    }

    #[test]
    fn malformed_service_and_limits_are_rejected() {
        for unit in [
            "*.service",
            "--all.service",
            ".service",
            "a\nb.service",
            "a.socket",
        ] {
            assert!(unit_name(unit).is_err());
        }
        for unit in ["lamp-voice.service", "voice@desk.service"] {
            unit_name(unit).unwrap();
        }
        let mut config = Config::customer_default();
        config.roots[0].max_bytes = 0;
        assert!(config.validate().is_err());
        config.roots[0].max_bytes = 1;
        config.roots[0].max_age_seconds = u64::MAX;
        assert!(config.validate().is_err());
    }
}
