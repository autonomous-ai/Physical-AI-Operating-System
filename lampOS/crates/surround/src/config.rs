//! Versioned Surround configuration and the exact, unversioned Voice payload.
//! Files are staged by the caller; activating a complete generation is a
//! separate operation. Credentials never belong in a document or status.
use crate::atomic;
use lamp_gemini::{Credential, SessionConfig, ThinkingLevel, Timeouts};
use serde::{Deserialize, Serialize};
use std::{fmt, io, path::Path, time::Duration};
use zeroize::Zeroizing;

pub const MAX_DOCUMENT_BYTES: usize = 16_384;
pub const MAX_SECRET_BYTES: usize = 4_096;

fn invalid() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidInput,
        "invalid provider configuration",
    )
}

/// Explicit exposure is needed only at a private file or authenticated local
/// transport boundary. No Debug, Clone or Serialize implementation is provided.
pub struct Secret(Zeroizing<Vec<u8>>);

impl Secret {
    pub fn new(bytes: Vec<u8>) -> io::Result<Self> {
        let bytes = Zeroizing::new(bytes);
        if bytes.is_empty()
            || bytes.len() > MAX_SECRET_BYTES
            || bytes.contains(&0)
            || bytes.contains(&b'\r')
            || bytes.contains(&b'\n')
            || std::str::from_utf8(&bytes).is_err()
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid secret",
            ));
        }
        Ok(Self(bytes))
    }

    pub fn expose(&self) -> &[u8] {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Authentication {
    #[default]
    ApiKey,
    Bearer,
    Ephemeral,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ReconnectContext {
    #[default]
    Retain,
    Off,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SilentResponse {
    #[default]
    Fail,
    AssumeOver,
}

/// Keep this wire schema aligned with Voice's deny_unknown_fields reader.
/// Validation delegates provider constraints to the existing Gemini crate.
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderPayload {
    pub endpoint: String,
    pub model: String,
    pub voice: String,
    #[serde(default)]
    pub thinking_level: Option<ThinkingLevel>,
    #[serde(default)]
    pub thinking_budget: Option<u32>,
    #[serde(default)]
    pub context_trigger_tokens: Option<u32>,
    #[serde(default)]
    pub context_target_tokens: Option<u32>,
    #[serde(default)]
    pub language_code: Option<String>,
    #[serde(default)]
    pub small_pictures: bool,
    #[serde(default)]
    pub search: bool,
    pub credential_file: String,
    #[serde(default)]
    pub authentication: Authentication,
    #[serde(default)]
    pub resumption: ReconnectContext,
    #[serde(default)]
    pub silent_response: SilentResponse,
    #[serde(default)]
    pub response_timeout_ms: Option<u32>,
    #[serde(default)]
    pub no_reply_after_ms: Option<u32>,
    #[serde(default)]
    pub instruction_file: Option<String>,
}

impl fmt::Debug for ProviderPayload {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ProviderPayload([PRIVATE])")
    }
}

impl ProviderPayload {
    pub fn validate(&self) -> io::Result<()> {
        if !private_path(&self.credential_file)
            || self
                .instruction_file
                .as_deref()
                .is_some_and(|p| !private_path(p))
            || self
                .response_timeout_ms
                .is_some_and(|ms| !(2_000..=60_000).contains(&ms))
            || self.no_reply_after_ms.is_some_and(|ms| {
                !(1_000..=30_000).contains(&ms) || ms >= self.response_timeout_ms.unwrap_or(30_000)
            })
            || self.context_trigger_tokens.is_some() != self.context_target_tokens.is_some()
        {
            return Err(invalid());
        }
        // This builds and validates data only. It does not open a connection or
        // read the credential path, which may refer to an unactivated generation.
        let credential =
            Credential::api_key("configuration-validation-only").map_err(|_| invalid())?;
        let mut session = SessionConfig::new(&self.endpoint, credential)
            .and_then(|s| s.model(&self.model))
            .and_then(|s| s.voice(&self.voice))
            .map_err(|_| invalid())?;
        if let Some(level) = self.thinking_level {
            session = session.thinking_level(level).map_err(|_| invalid())?;
        }
        if let Some(tokens) = self.thinking_budget {
            session = session.thinking_budget(tokens).map_err(|_| invalid())?;
        }
        if let (Some(trigger), Some(target)) =
            (self.context_trigger_tokens, self.context_target_tokens)
        {
            session = session
                .context_window(trigger, target)
                .map_err(|_| invalid())?;
        }
        if let Some(language) = &self.language_code {
            session = session.language_code(language).map_err(|_| invalid())?;
        }
        let defaults = Timeouts::default();
        session
            .timeouts(Timeouts {
                response: self
                    .response_timeout_ms
                    .map_or(defaults.response, |ms| Duration::from_millis(u64::from(ms))),
                unanswered: self
                    .no_reply_after_ms
                    .map(|ms| Duration::from_millis(u64::from(ms))),
                ..defaults
            })
            .map_err(|_| invalid())?;
        Ok(())
    }

    pub fn validate_credential(&self, secret: &Secret) -> io::Result<()> {
        let text = std::str::from_utf8(secret.expose()).map_err(|_| invalid())?;
        match self.authentication {
            Authentication::ApiKey => Credential::api_key(text),
            Authentication::Bearer => Credential::bearer(text),
            Authentication::Ephemeral => Credential::ephemeral(text),
        }
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid provider credential"))?;
        Ok(())
    }
}

fn private_path(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 4_096
        && !value.chars().any(char::is_control)
        && Path::new(value).is_absolute()
        && Path::new(value).components().all(|c| {
            matches!(
                c,
                std::path::Component::RootDir | std::path::Component::Normal(_)
            )
        })
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderDocument {
    pub schema_version: u32,
    pub provider: ProviderPayload,
}

impl ProviderDocument {
    pub fn new(provider: ProviderPayload) -> io::Result<Self> {
        provider.validate()?;
        Ok(Self {
            schema_version: 1,
            provider,
        })
    }

    pub fn validate(&self) -> io::Result<()> {
        if self.schema_version != 1 {
            return Err(invalid());
        }
        self.provider.validate()
    }

    pub fn parse(bytes: &[u8]) -> io::Result<Self> {
        if bytes.len() > MAX_DOCUMENT_BYTES {
            return Err(invalid());
        }
        let document: Self = serde_json::from_slice(bytes).map_err(|_| invalid())?;
        document.validate()?;
        Ok(document)
    }
}

pub fn read_document(path: &Path) -> io::Result<Option<ProviderDocument>> {
    atomic::read(path, MAX_DOCUMENT_BYTES)?
        .map(|bytes| ProviderDocument::parse(&bytes))
        .transpose()
}

pub fn write_document(path: &Path, document: &ProviderDocument) -> io::Result<()> {
    document.validate()?;
    let bytes = serde_json::to_vec(document).map_err(|_| invalid())?;
    atomic::write(path, &bytes, MAX_DOCUMENT_BYTES)
}

/// No Surround version or activation metadata is inserted into Voice's file.
pub fn write_voice_payload(path: &Path, payload: &ProviderPayload) -> io::Result<()> {
    payload.validate()?;
    let bytes = serde_json::to_vec(payload).map_err(|_| invalid())?;
    atomic::write(path, &bytes, MAX_DOCUMENT_BYTES)
}

pub fn write_secret(path: &Path, secret: &Secret) -> io::Result<()> {
    atomic::write(path, secret.expose(), MAX_SECRET_BYTES)
}
