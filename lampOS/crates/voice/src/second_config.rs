//! The second voice route's private configuration: an LLM, a speech-to-text
//! and a text-to-speech service, with V1's field names. Off unless
//! `LAMP_VOICE_SECOND_CONFIG` names the file. Keys are never written in it:
//! each is a private credential file read like `provider.json`'s.
use crate::config::{read_credential, read_private};
use serde::Deserialize;
use std::{ffi::OsString, fmt, io, path::Path};

pub const SECOND_CONFIG_ENV: &str = "LAMP_VOICE_SECOND_CONFIG";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Raw {
    llm_base_url: String,
    llm_model: String,
    llm_credential_file: String,
    #[serde(default)]
    stt_base_url: Option<String>,
    #[serde(default)]
    stt_credential_file: Option<String>,
    #[serde(default)]
    stt_language: Option<String>,
    #[serde(default)]
    deepgram_credential_file: Option<String>,
    #[serde(default)]
    tts_base_url: Option<String>,
    #[serde(default)]
    tts_credential_file: Option<String>,
    #[serde(default)]
    tts_provider: Option<String>,
    #[serde(default)]
    tts_voice: Option<String>,
}

/// A key read from a credential file. Never shown by Debug.
#[derive(Clone, PartialEq, Eq)]
pub struct Key(String);
impl Key {
    pub fn expose(&self) -> &str {
        &self.0
    }
}
impl fmt::Debug for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Key([REDACTED])")
    }
}

/// Which speech-to-text service the route uses: Deepgram when its
/// credential file is given, as in V1, otherwise the Autonomous one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stt {
    Deepgram { key: Key },
    Autonomous { base_url: String, key: Key },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TtsProvider {
    OpenAi,
    ElevenLabs,
}

/// The route with V1's fallbacks applied: an empty or absent STT or TTS
/// URL or credential file is the LLM's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecondConfig {
    pub llm_base_url: String,
    pub llm_model: String,
    pub llm_key: Key,
    pub stt: Stt,
    pub stt_language: Option<String>,
    pub tts_base_url: String,
    pub tts_key: Key,
    pub tts_provider: TtsProvider,
    pub tts_voice: Option<String>,
}

/// `None` when the variable is unset or empty: the second route is off.
pub fn load_from_env() -> io::Result<Option<SecondConfig>> {
    from_setting(std::env::var_os(SECOND_CONFIG_ENV))
}

fn from_setting(value: Option<OsString>) -> io::Result<Option<SecondConfig>> {
    match value {
        Some(path) if !path.is_empty() => load_from_path(Path::new(&path)).map(Some),
        _ => Ok(None),
    }
}

pub fn load_from_path(path: &Path) -> io::Result<SecondConfig> {
    let data = read_private(path, 16_384)?;
    let raw: Raw = serde_json::from_slice(&data).map_err(|_| {
        invalid(
            io::ErrorKind::InvalidData,
            "invalid second route configuration",
        )
    })?;
    let given = |value: Option<String>| value.filter(|v| !v.is_empty());
    let tts_provider = match given(raw.tts_provider).as_deref() {
        None | Some("openai") => TtsProvider::OpenAi,
        Some("elevenlabs") => TtsProvider::ElevenLabs,
        Some(_) => {
            return Err(invalid(
                io::ErrorKind::InvalidInput,
                "tts_provider must be openai or elevenlabs",
            ));
        }
    };
    if raw.llm_model.is_empty() {
        return Err(invalid(
            io::ErrorKind::InvalidInput,
            "llm_model is required",
        ));
    }
    let llm_base_url = checked_url(raw.llm_base_url)?;
    let stt_base_url = given(raw.stt_base_url).unwrap_or_else(|| llm_base_url.clone());
    let tts_base_url = given(raw.tts_base_url).unwrap_or_else(|| llm_base_url.clone());
    let stt_file =
        given(raw.stt_credential_file).unwrap_or_else(|| raw.llm_credential_file.clone());
    let tts_file =
        given(raw.tts_credential_file).unwrap_or_else(|| raw.llm_credential_file.clone());
    let stt = match given(raw.deepgram_credential_file) {
        Some(file) => Stt::Deepgram { key: key(&file)? },
        None => Stt::Autonomous {
            base_url: checked_url(stt_base_url)?,
            key: key(&stt_file)?,
        },
    };
    Ok(SecondConfig {
        llm_key: key(&raw.llm_credential_file)?,
        llm_base_url,
        llm_model: raw.llm_model,
        stt,
        stt_language: given(raw.stt_language),
        tts_base_url: checked_url(tts_base_url)?,
        tts_key: key(&tts_file)?,
        tts_provider,
        tts_voice: given(raw.tts_voice),
    })
}

fn invalid(kind: io::ErrorKind, message: &'static str) -> io::Error {
    io::Error::new(kind, message)
}

fn key(file: &str) -> io::Result<Key> {
    if !Path::new(file).is_absolute() {
        return Err(invalid(
            io::ErrorKind::InvalidInput,
            "credential path must be absolute",
        ));
    }
    let text = read_credential(Path::new(file))?;
    if text.trim().is_empty() {
        return Err(invalid(
            io::ErrorKind::InvalidData,
            "credential file is empty",
        ));
    }
    Ok(Key(text))
}

/// An explicit https URL. Authentication belongs only in the key; userinfo,
/// queries and fragments are refused, as for the live endpoint.
fn checked_url(url: String) -> io::Result<String> {
    let host = url
        .strip_prefix("https://")
        .map(|rest| rest.split('/').next().unwrap_or(""));
    if url.len() > 2048 || url.contains(['?', '#', '@']) || host.is_none_or(str::is_empty) {
        return Err(invalid(
            io::ErrorKind::InvalidInput,
            "service URL must be https with a host",
        ));
    }
    Ok(url)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, os::unix::fs::PermissionsExt, path::PathBuf};

    fn directory(name: &str) -> PathBuf {
        let directory = std::env::temp_dir().join(format!(
            "lamp-voice-second-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        fs::create_dir_all(&directory).unwrap();
        directory
    }

    fn private(path: &Path, text: &str) {
        fs::write(path, text).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
    }

    /// The configuration with `extra` fields, beside an LLM key file.
    fn written(name: &str, extra: &str) -> (PathBuf, PathBuf) {
        let directory = directory(name);
        let llm = directory.join("llm.key");
        private(&llm, "llm-secret\n");
        let config = directory.join("second.json");
        private(
            &config,
            &format!(
                r#"{{"llm_base_url":"https://llm.example.invalid/v1","llm_model":"m","llm_credential_file":"{}"{extra}}}"#,
                llm.display()
            ),
        );
        (directory, config)
    }

    #[test]
    fn an_unset_or_empty_variable_leaves_the_route_off() {
        assert!(from_setting(None).unwrap().is_none());
        assert!(from_setting(Some(OsString::new())).unwrap().is_none());
    }

    #[test]
    fn absent_stt_and_tts_settings_are_the_llms() {
        let (_, config) = written("fallback", r#","stt_base_url":"","tts_voice":"alloy""#);
        let route = load_from_path(&config).unwrap();
        assert_eq!(route.llm_key.expose(), "llm-secret");
        assert_eq!(
            route.stt,
            Stt::Autonomous {
                base_url: "https://llm.example.invalid/v1".into(),
                key: route.llm_key.clone(),
            }
        );
        assert_eq!(route.tts_base_url, route.llm_base_url);
        assert_eq!(route.tts_key, route.llm_key);
        assert_eq!(route.tts_provider, TtsProvider::OpenAi);
        assert_eq!(route.tts_voice.as_deref(), Some("alloy"));
        assert!(!format!("{route:?}").contains("llm-secret"));
    }

    #[test]
    fn a_deepgram_credential_selects_deepgram() {
        let directory = directory("deepgram");
        let deepgram = directory.join("deepgram.key");
        private(&deepgram, "dg-secret");
        let (_, config) = written(
            "deepgram",
            &format!(
                r#","deepgram_credential_file":"{}","tts_provider":"elevenlabs","tts_base_url":"https://tts.example.invalid""#,
                deepgram.display()
            ),
        );
        let route = load_from_path(&config).unwrap();
        assert_eq!(
            route.stt,
            Stt::Deepgram {
                key: Key("dg-secret".into())
            }
        );
        assert_eq!(route.tts_provider, TtsProvider::ElevenLabs);
        assert_eq!(route.tts_base_url, "https://tts.example.invalid");
    }

    #[test]
    fn a_key_written_in_the_configuration_is_refused() {
        let (_, config) = written("inline", r#","llm_api_key":"sk-inline""#);
        assert!(load_from_path(&config).is_err());
    }

    #[test]
    fn an_unknown_tts_provider_is_refused() {
        let (_, config) = written("provider", r#","tts_provider":"polly""#);
        let error = load_from_path(&config).unwrap_err();
        assert!(error.to_string().contains("openai or elevenlabs"));
    }

    #[test]
    fn a_credential_others_can_read_is_refused() {
        let (directory, config) = written("mode", "");
        fs::set_permissions(directory.join("llm.key"), fs::Permissions::from_mode(0o644)).unwrap();
        let error = load_from_path(&config).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
    }

    #[test]
    fn a_plain_http_url_is_refused() {
        let (_, config) = written("http", r#","tts_base_url":"http://tts.example.invalid""#);
        assert!(load_from_path(&config).is_err());
    }
}
