//! Authenticated local operator page. The router performs no maintenance work:
//! a single bounded worker admits durable operations before acknowledging them.
use crate::{
    config::{ProviderDocument, Secret},
    configuration::Generation,
    setup,
};
use axum::{
    Json, Router,
    body::Body,
    extract::{DefaultBodyLimit, State},
    http::{HeaderMap, HeaderValue, Request, StatusCode},
    middleware::{self, Next},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
};
use serde::{Deserialize, Deserializer, Serialize};
use sha2::{Digest, Sha256};
use std::{
    io,
    sync::{
        Arc, Mutex, RwLock,
        atomic::{AtomicBool, Ordering},
    },
};
use subtle::ConstantTimeEq;
use tokio::sync::{mpsc, oneshot};
use url::Url;

pub const MAX_REQUEST_BYTES: usize = 32_768;
pub const SESSION_US: u64 = 900_000_000;
const ATTEMPT_WINDOW_US: u64 = 60_000_000;
pub type Clock = Arc<dyn Fn() -> u64 + Send + Sync>;

pub struct InputSecret(pub Secret);
impl<'de> Deserialize<'de> for InputSecret {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        Secret::new(value.into_bytes())
            .map(Self)
            .map_err(|_| serde::de::Error::custom("invalid secret"))
    }
}

/// No Debug or Serialize route exists for request bodies containing secrets.
#[derive(Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    SaveProvider {
        document: ProviderDocument,
        credential: InputSecret,
        instructions: Option<String>,
    },
    ApplyConfiguration {
        target: Generation,
    },
    JoinWifi {
        ssid: String,
        password: Option<InputSecret>,
    },
    ProbeNetwork {},
    StageRelease {
        manifest_target: String,
    },
    ApplyRelease {
        set_sha256: String,
        rollback: bool,
    },
    RetryUpdate {},
    Reset {
        confirmation: String,
    },
    ResumeReset {},
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    SaveProvider,
    ApplyConfiguration,
    JoinWifi,
    ProbeNetwork,
    StageRelease,
    ApplyRelease,
    RetryUpdate,
    Reset,
    ResumeReset,
}
impl Command {
    pub fn kind(&self) -> Kind {
        match self {
            Self::SaveProvider { .. } => Kind::SaveProvider,
            Self::ApplyConfiguration { .. } => Kind::ApplyConfiguration,
            Self::JoinWifi { .. } => Kind::JoinWifi,
            Self::ProbeNetwork {} => Kind::ProbeNetwork,
            Self::StageRelease { .. } => Kind::StageRelease,
            Self::ApplyRelease { .. } => Kind::ApplyRelease,
            Self::RetryUpdate {} => Kind::RetryUpdate,
            Self::Reset { .. } => Kind::Reset,
            Self::ResumeReset {} => Kind::ResumeReset,
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Working,
    Complete,
    Failed,
    Interrupted,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    Disabled,
    Busy,
    InvalidInput,
    Unavailable,
    Storage,
    Verification,
    OwnerRefused,
    Interrupted,
    CheckStatus,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Operation {
    pub schema_version: u32,
    pub id: String,
    pub kind: Kind,
    pub phase: Phase,
    pub updated_us: u64,
    pub error: Option<ErrorCode>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct View {
    pub enabled: bool,
    pub operation: Option<Operation>,
    pub setup: Option<setup::Progress>,
    pub candidate_configuration: Option<Generation>,
    pub current_configuration: Option<Generation>,
    pub staged_release: Option<String>,
    pub update_outcome: Option<crate::update::Outcome>,
    pub reset_phase: Option<crate::reset::Phase>,
    pub runtime: Option<setup::RuntimeState>,
    pub runtime_observed_us: Option<u64>,
    pub voice_capability: Option<crate::startup::Capability>,
}

impl View {
    /// Historical progress remains visible; live readiness expires while a
    /// worker is downloading, unavailable or otherwise unable to refresh it.
    pub fn at(&self, now_us: u64) -> Self {
        let mut view = self.clone();
        let fresh = |observed: Option<u64>, bound: u64| {
            observed.is_some_and(|at| at <= now_us && now_us - at < bound)
        };
        if !fresh(view.runtime_observed_us, crate::startup::REPORT_MAX_AGE_US) {
            view.runtime = Some(setup::RuntimeState::Unknown);
            view.voice_capability = None;
        }
        if let Some(progress) = &mut view.setup {
            progress.network_fresh &=
                fresh(progress.network_observed_us, setup::RECEIPT_MAX_AGE_US);
            progress.runtime_fresh &=
                fresh(progress.runtime_observed_us, setup::RECEIPT_MAX_AGE_US);
        }
        view
    }
}

pub struct Submission {
    pub id: String,
    pub command: Command,
    /// Send Ok only after an intent is durably stored. A failed send does not
    /// cancel accepted work: the caller can find the stable ID in status.
    pub admitted: oneshot::Sender<Result<(), ErrorCode>>,
}

#[derive(Clone)]
pub struct Shared {
    pub view: Arc<RwLock<View>>,
    pub busy: Arc<AtomicBool>,
}
impl Shared {
    pub fn new(enabled: bool) -> Self {
        Self {
            view: Arc::new(RwLock::new(View {
                enabled,
                ..View::default()
            })),
            busy: Arc::new(AtomicBool::new(false)),
        }
    }
    pub fn finished(&self) {
        self.busy.store(false, Ordering::Release);
    }
}

struct Session {
    hash: [u8; 32],
    expires_us: u64,
}
struct Auth {
    sessions: Vec<Session>,
    attempts: u8,
    window_us: u64,
    last_us: u64,
}
struct App {
    secret_hash: [u8; 32],
    origin: String,
    host: String,
    secure: bool,
    auth: Mutex<Auth>,
    clock: Clock,
    shared: Shared,
    tx: mpsc::Sender<Submission>,
}

fn hash(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}
pub fn random_id() -> io::Result<String> {
    let mut random = [0u8; 16];
    getrandom::fill(&mut random).map_err(io::Error::other)?;
    Ok(random.iter().map(|b| format!("{b:02x}")).collect())
}

pub fn router(
    secret: Secret,
    origin: &str,
    shared: Shared,
    tx: mpsc::Sender<Submission>,
    clock: Clock,
) -> io::Result<Router> {
    let url = Url::parse(origin).map_err(|_| io::Error::other("invalid operator origin"))?;
    if !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.path() != "/"
        || url.host_str().is_none()
        || (url.scheme() != "https"
            && !(url.scheme() == "http"
                && matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"))))
        || secret.expose().len() != 64
        || !secret.expose().iter().all(u8::is_ascii_hexdigit)
    {
        return Err(io::Error::other(
            "operator needs an exact HTTPS origin and a private 32-byte hex login secret",
        ));
    }
    let host = &url[url::Position::BeforeHost..url::Position::AfterPort];
    let canonical = url.origin().ascii_serialization();
    if canonical != origin {
        return Err(io::Error::other(
            "operator origin must be canonical, with no trailing slash",
        ));
    }
    let app = Arc::new(App {
        secret_hash: hash(secret.expose()),
        origin: canonical,
        host: host.into(),
        secure: url.scheme() == "https",
        auth: Mutex::new(Auth {
            sessions: vec![],
            attempts: 0,
            window_us: 0,
            last_us: 0,
        }),
        clock,
        shared,
        tx,
    });
    Ok(Router::new()
        .route("/", get(page))
        .route("/app.js", get(script))
        .route("/app.css", get(style))
        .route("/api/login", post(login))
        .route("/api/logout", post(logout))
        .route("/api/status", get(status))
        .route("/api/operation", post(operation))
        .fallback(|| async { response(StatusCode::NOT_FOUND, None::<()>, "not found") })
        .layer(DefaultBodyLimit::max(MAX_REQUEST_BYTES))
        .layer(middleware::from_fn_with_state(
            crate::http_limit::RequestLimits::default(),
            crate::http_limit::limit_requests,
        ))
        .layer(middleware::from_fn_with_state(app.clone(), boundary))
        .with_state(app))
}

async fn boundary(State(app): State<Arc<App>>, request: Request<Body>, next: Next) -> Response {
    let host = request.headers().get("host").and_then(|h| h.to_str().ok());
    let origin = request
        .headers()
        .get("origin")
        .and_then(|h| h.to_str().ok());
    let write = request.method() != axum::http::Method::GET;
    let allowed = request.uri().query().is_none()
        && request.headers().get_all("host").iter().count() == 1
        && request.headers().get_all("origin").iter().count() <= 1
        && host == Some(&app.host)
        && origin.is_none_or(|o| o == app.origin)
        && (!write || origin == Some(&app.origin));
    let mut reply = if allowed {
        next.run(request).await
    } else {
        response(StatusCode::FORBIDDEN, None::<()>, "request origin refused")
    };
    for (key, value) in [
        ("cache-control", "no-store"),
        ("x-content-type-options", "nosniff"),
        ("referrer-policy", "no-referrer"),
        (
            "content-security-policy",
            "default-src 'none'; script-src 'self'; style-src 'self'; connect-src 'self'; form-action 'self'; frame-ancestors 'none'; base-uri 'none'",
        ),
    ] {
        reply
            .headers_mut()
            .insert(key, HeaderValue::from_static(value));
    }
    reply
}
fn response<T: Serialize>(code: StatusCode, data: Option<T>, message: &str) -> Response {
    (code, Json(serde_json::json!({"status":u8::from(code.is_success()),"data":data,"message":if message.is_empty(){None}else{Some(message)}}))).into_response()
}
fn authenticated(app: &App, headers: &HeaderMap) -> bool {
    if headers.get_all("cookie").iter().count() != 1 {
        return false;
    }
    let Some(cookie) = headers
        .get("cookie")
        .and_then(|v| v.to_str().ok())
        .filter(|v| v.len() <= 512)
    else {
        return false;
    };
    let mut tokens = cookie
        .split(';')
        .filter_map(|v| v.trim().strip_prefix("lamp_session="));
    let Some(token) = tokens
        .next()
        .filter(|s| s.len() == 64 && s.bytes().all(|c| c.is_ascii_hexdigit()))
    else {
        return false;
    };
    if tokens.next().is_some() {
        return false;
    }
    let now = (app.clock)();
    let Ok(mut auth) = app.auth.lock() else {
        return false;
    };
    if now < auth.last_us {
        auth.sessions.clear();
        return false;
    }
    auth.last_us = now;
    auth.sessions.retain(|s| s.expires_us > now);
    let candidate = hash(token.as_bytes());
    auth.sessions
        .iter()
        .any(|s| bool::from(s.hash.ct_eq(&candidate)))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Login {
    secret: InputSecret,
}
async fn login(
    State(app): State<Arc<App>>,
    body: Result<Json<Login>, axum::extract::rejection::JsonRejection>,
) -> Response {
    let Ok(Json(body)) = body else {
        return response(StatusCode::BAD_REQUEST, None::<()>, "invalid login");
    };
    let now = (app.clock)();
    let Ok(mut auth) = app.auth.lock() else {
        return response(
            StatusCode::SERVICE_UNAVAILABLE,
            None::<()>,
            "login unavailable",
        );
    };
    if now < auth.last_us {
        auth.sessions.clear();
        return response(
            StatusCode::SERVICE_UNAVAILABLE,
            None::<()>,
            "clock unavailable",
        );
    }
    auth.last_us = now;
    if now.saturating_sub(auth.window_us) >= ATTEMPT_WINDOW_US {
        auth.window_us = now;
        auth.attempts = 0;
    }
    if auth.attempts >= 10 {
        return response(StatusCode::TOO_MANY_REQUESTS, None::<()>, "try again later");
    }
    auth.attempts += 1;
    if !bool::from(hash(body.secret.0.expose()).ct_eq(&app.secret_hash)) {
        return response(StatusCode::UNAUTHORIZED, None::<()>, "login refused");
    }
    let (Ok(left), Ok(right)) = (random_id(), random_id()) else {
        return response(
            StatusCode::SERVICE_UNAVAILABLE,
            None::<()>,
            "login unavailable",
        );
    };
    let token = format!("{left}{right}");
    auth.sessions.retain(|s| s.expires_us > now);
    if auth.sessions.len() >= 4 {
        auth.sessions.remove(0);
    }
    let Some(expires_us) = now.checked_add(SESSION_US) else {
        return response(
            StatusCode::SERVICE_UNAVAILABLE,
            None::<()>,
            "clock unavailable",
        );
    };
    auth.sessions.push(Session {
        hash: hash(token.as_bytes()),
        expires_us,
    });
    let secure = if app.secure { "; Secure" } else { "" };
    let mut reply = response(
        StatusCode::OK,
        Some(serde_json::json!({"expires_in_seconds":900})),
        "",
    );
    reply.headers_mut().insert(
        "set-cookie",
        HeaderValue::from_str(&format!(
            "lamp_session={token}; HttpOnly; SameSite=Strict; Path=/; Max-Age=900{secure}"
        ))
        .unwrap(),
    );
    reply
}
async fn logout(State(app): State<Arc<App>>, headers: HeaderMap) -> Response {
    if !authenticated(&app, &headers) {
        return response(StatusCode::UNAUTHORIZED, None::<()>, "login required");
    }
    // Logout invalidates all four bounded local sessions so a shared browser
    // cannot leave a second operator session quietly active.
    if let Ok(mut auth) = app.auth.lock() {
        auth.sessions.clear();
    }
    let mut reply = response(StatusCode::OK, Some(()), "");
    reply.headers_mut().insert(
        "set-cookie",
        HeaderValue::from_static(
            "lamp_session=; HttpOnly; SameSite=Strict; Path=/; Max-Age=0; Secure",
        ),
    );
    reply
}
async fn status(State(app): State<Arc<App>>, headers: HeaderMap) -> Response {
    if !authenticated(&app, &headers) {
        return response(StatusCode::UNAUTHORIZED, None::<()>, "login required");
    }
    match app.shared.view.read() {
        Ok(view) => response(StatusCode::OK, Some(view.at((app.clock)())), ""),
        Err(_) => response(
            StatusCode::SERVICE_UNAVAILABLE,
            None::<()>,
            "status unavailable",
        ),
    }
}
async fn operation(
    State(app): State<Arc<App>>,
    headers: HeaderMap,
    body: Result<Json<Command>, axum::extract::rejection::JsonRejection>,
) -> Response {
    if !authenticated(&app, &headers) {
        return response(StatusCode::UNAUTHORIZED, None::<()>, "login required");
    }
    if !app.shared.view.read().is_ok_and(|v| v.enabled) {
        return response(
            StatusCode::FORBIDDEN,
            Some(ErrorCode::Disabled),
            "maintenance disabled",
        );
    }
    let Ok(Json(command)) = body else {
        return response(
            StatusCode::BAD_REQUEST,
            Some(ErrorCode::InvalidInput),
            "invalid request",
        );
    };
    if app
        .shared
        .busy
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return response(
            StatusCode::CONFLICT,
            Some(ErrorCode::Busy),
            "an operation is already running",
        );
    }
    let Ok(id) = random_id() else {
        app.shared.finished();
        return response(
            StatusCode::SERVICE_UNAVAILABLE,
            None::<()>,
            "request unavailable",
        );
    };
    let (admitted, rx) = oneshot::channel();
    if app
        .tx
        .try_send(Submission {
            id: id.clone(),
            command,
            admitted,
        })
        .is_err()
    {
        app.shared.finished();
        return response(
            StatusCode::SERVICE_UNAVAILABLE,
            None::<()>,
            "worker unavailable",
        );
    }
    match tokio::time::timeout(std::time::Duration::from_secs(2), rx).await {
        Ok(Ok(Ok(()))) => response(
            StatusCode::ACCEPTED,
            Some(serde_json::json!({"id":id})),
            "request saved",
        ),
        Ok(Ok(Err(code))) => response(
            StatusCode::SERVICE_UNAVAILABLE,
            Some(serde_json::json!({"id":id,"error":code})),
            "request refused",
        ),
        _ => response(
            StatusCode::SERVICE_UNAVAILABLE,
            Some(serde_json::json!({"id":id,"error":ErrorCode::CheckStatus})),
            "check status before trying again",
        ),
    }
}
async fn page() -> Html<&'static str> {
    Html(include_str!("../web/index.html"))
}
async fn script() -> impl IntoResponse {
    (
        [("content-type", "text/javascript; charset=utf-8")],
        include_str!("../web/app.js"),
    )
}
async fn style() -> impl IntoResponse {
    (
        [("content-type", "text/css; charset=utf-8")],
        include_str!("../web/app.css"),
    )
}
