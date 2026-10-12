//! Explicit private configuration. Neither the HAL nor os-server configuration
//! is read by this runtime. Provisioning may migrate values once outside source.
use lamp_gemini::{Credential, Resumption, SessionConfig, ThinkingLevel};
use serde::Deserialize;
use std::{
    fs::OpenOptions,
    io::{self, Read},
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::Path,
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderConfig {
    endpoint: String,
    model: String,
    voice: String,
    #[serde(default)]
    thinking_level: Option<ThinkingLevel>,
    #[serde(default)]
    language_code: Option<String>,
    credential_file: String,
    #[serde(default)]
    authentication: Authentication,
    #[serde(default)]
    resumption: ReconnectContext,
}
#[derive(Default, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Authentication {
    #[default]
    ApiKey,
    Bearer,
    Ephemeral,
}
/// What a reconnect after an outage presents to the service.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
enum ReconnectContext {
    /// The handle the service last volunteered, so that the new session
    /// continues the old conversation.
    #[default]
    Retain,
    /// Nothing: every reconnect is a new conversation. For a path on which a
    /// resumed session accepts audio and never answers. On lamp-4ace through
    /// its relay, after the first failed exchange seven requests in a row
    /// went unanswered, each on a session reported as resumed, while every
    /// fresh session answered.
    Off,
}
impl From<ReconnectContext> for Resumption {
    fn from(context: ReconnectContext) -> Self {
        match context {
            ReconnectContext::Retain => Self::Retain,
            ReconnectContext::Off => Self::Off,
        }
    }
}

pub const CHAT_INSTRUCTION: &str = "You are Lamp, a considerate desk companion having a live conversation with the person beside you. Always speak English, whatever language you hear, and respond to their meaning. Give short answers when appropriate; expand when asked. Do not use spoken waiting fillers or repetitive acknowledgments. Do not pretend to see, remember, browse, control devices or finish tasks you cannot access. This session supports voice chat only. If something is unclear, ask a brief relevant question. A human may pause mid-thought; do not finish their thought for them.";

/// Availability is not sight: only an attached frame supplies visual evidence.
pub const CAMERA_CHAT_INSTRUCTION: &str = "You are Lamp, a considerate desk companion having a live conversation with the person beside you. Always speak English, whatever language you hear, and respond to their meaning. Give short answers when appropriate; expand when asked. Do not use spoken waiting fillers or repetitive acknowledgments. A fresh camera image may accompany a question. Describe only what an attached image supports; if there is no image or details are unclear, say so. Do not pretend to remember, browse, control devices or finish tasks you cannot access. A human may pause mid-thought; do not finish their thought for them.";

impl ProviderConfig {
    pub fn load(path: &Path) -> io::Result<Self> {
        let data = read_private(path, 16_384)?;
        let config: Self = serde_json::from_slice(&data).map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid private provider configuration",
            )
        })?;
        if !Path::new(&config.credential_file).is_absolute() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "credential path must be absolute",
            ));
        }
        Ok(config)
    }
    pub fn into_session(self) -> Result<SessionConfig, Box<dyn std::error::Error + Send + Sync>> {
        self.into_session_with_camera(false)
    }
    pub fn into_session_with_camera(
        self,
        camera: bool,
    ) -> Result<SessionConfig, Box<dyn std::error::Error + Send + Sync>> {
        let bytes = read_private(Path::new(&self.credential_file), 4098)?;
        let value = std::str::from_utf8(&bytes)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "credential must be UTF-8"))?
            .trim_end_matches(['\r', '\n']);
        let credential = match self.authentication {
            Authentication::ApiKey => Credential::api_key(value)?,
            Authentication::Bearer => Credential::bearer(value)?,
            Authentication::Ephemeral => Credential::ephemeral(value)?,
        };
        let mut session = SessionConfig::new(&self.endpoint, credential)?
            .model(&self.model)?
            .voice(&self.voice)?
            .instruction(if camera {
                CAMERA_CHAT_INSTRUCTION
            } else {
                CHAT_INSTRUCTION
            })?
            .resumption(self.resumption.into());
        if let Some(level) = self.thinking_level {
            session = session.thinking_level(level)?;
        }
        if let Some(language) = self.language_code {
            session = session.language_code(&language)?;
        }
        Ok(session)
    }
}

fn read_private(path: &Path, max: usize) -> io::Result<Vec<u8>> {
    let flags =
        rustix::fs::OFlags::NOFOLLOW | rustix::fs::OFlags::NONBLOCK | rustix::fs::OFlags::CLOEXEC;
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(flags.bits() as i32)
        .open(path)?;
    let info = file.metadata()?;
    if !info.is_file()
        || info.mode() & 0o077 != 0
        || info.uid() != rustix::process::geteuid().as_raw()
        || info.len() > max as u64
    {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "configuration must be a bounded private regular file owned by this process user",
        ));
    }
    let mut bytes = Vec::with_capacity(info.len() as usize);
    (&mut file).take(max as u64 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > max {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "private file exceeds bound",
        ));
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed(extra: &str) -> Result<ProviderConfig, serde_json::Error> {
        serde_json::from_str(&format!(
            r#"{{"endpoint":"wss://example.invalid/live","model":"m","voice":"v","credential_file":"/nonexistent"{extra}}}"#
        ))
    }

    #[test]
    fn a_reconnect_keeps_the_conversation_unless_the_configuration_says_otherwise() {
        assert_eq!(parsed("").unwrap().resumption, ReconnectContext::Retain);
        assert_eq!(
            parsed(r#","resumption":"retain""#).unwrap().resumption,
            ReconnectContext::Retain
        );
        assert_eq!(
            parsed(r#","resumption":"off""#).unwrap().resumption,
            ReconnectContext::Off
        );
        assert_eq!(
            Resumption::from(ReconnectContext::Retain),
            Resumption::Retain
        );
        assert_eq!(Resumption::from(ReconnectContext::Off), Resumption::Off);
    }

    #[test]
    fn an_unknown_reconnect_choice_or_field_is_refused() {
        assert!(parsed(r#","resumption":"request""#).is_err());
        assert!(parsed(r#","resumption":true"#).is_err());
        assert!(parsed(r#","resume":"off""#).is_err());
    }

    #[test]
    fn both_instructions_fix_the_reply_language() {
        for instruction in [CHAT_INSTRUCTION, CAMERA_CHAT_INSTRUCTION] {
            assert!(instruction.contains("Always speak English, whatever language you hear"));
        }
    }
}
