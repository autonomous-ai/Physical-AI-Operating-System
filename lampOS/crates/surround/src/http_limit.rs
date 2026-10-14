//! Bound local HTTP work before authentication/body extraction. Connection
//! lifetime also closes peers that never finish sending an HTTP header.
use axum::{
    Json,
    body::Body,
    extract::State,
    http::{Method, Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    serve::Listener,
};
use std::{
    future::Future,
    io,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
    time::Duration,
};
use tokio::{
    io::{AsyncRead, AsyncWrite, ReadBuf},
    sync::{OwnedSemaphorePermit, Semaphore},
    time::Sleep,
};

pub const MAX_CONNECTIONS: usize = 32;
pub const CONNECTION_LIFETIME: Duration = Duration::from_secs(10);
pub const MAX_READ_REQUESTS: usize = 8;
pub const MAX_WRITE_REQUESTS: usize = 2;
pub const REQUEST_DEADLINE: Duration = Duration::from_secs(5);

fn invalid() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, "invalid HTTP resource bounds")
}

#[derive(Clone)]
pub struct RequestLimits {
    reads: Arc<Semaphore>,
    writes: Arc<Semaphore>,
    deadline: Duration,
}
impl Default for RequestLimits {
    fn default() -> Self {
        Self::with_limits(MAX_READ_REQUESTS, MAX_WRITE_REQUESTS, REQUEST_DEADLINE)
            .expect("constant HTTP limits are valid")
    }
}
impl RequestLimits {
    /// A deployment/test may lower a bound, never silently lift the defaults.
    pub fn with_limits(reads: usize, writes: usize, deadline: Duration) -> io::Result<Self> {
        if !(1..=MAX_READ_REQUESTS).contains(&reads)
            || !(1..=MAX_WRITE_REQUESTS).contains(&writes)
            || deadline.is_zero()
            || deadline > REQUEST_DEADLINE
        {
            return Err(invalid());
        }
        Ok(Self {
            reads: Arc::new(Semaphore::new(reads)),
            writes: Arc::new(Semaphore::new(writes)),
            deadline,
        })
    }
}

fn refused(code: StatusCode, message: &'static str) -> Response {
    (
        code,
        Json(serde_json::json!({"status":0,"data":null,"message":message})),
    )
        .into_response()
}

/// Install inside the origin/security middleware so refusals receive its
/// headers. GET/HEAD status has a pool independent of slow mutation bodies.
/// The permit covers extraction and handling; this app serves bounded fixed
/// response bodies. LimitedListener bounds their transport lifetime as well.
pub async fn limit_requests(
    State(limits): State<RequestLimits>,
    request: Request<Body>,
    next: Next,
) -> Response {
    let pool = if matches!(*request.method(), Method::GET | Method::HEAD) {
        &limits.reads
    } else {
        &limits.writes
    };
    let Ok(_permit) = pool.clone().try_acquire_owned() else {
        return refused(StatusCode::TOO_MANY_REQUESTS, "request capacity reached");
    };
    match tokio::time::timeout(limits.deadline, next.run(request)).await {
        Ok(response) => response,
        Err(_) => refused(
            StatusCode::SERVICE_UNAVAILABLE,
            "request timed out; check status before trying again",
        ),
    }
}

pub struct LimitedListener<L> {
    inner: L,
    permits: Arc<Semaphore>,
    lifetime: Duration,
}
impl<L> LimitedListener<L> {
    pub fn new(inner: L) -> Self {
        Self {
            inner,
            permits: Arc::new(Semaphore::new(MAX_CONNECTIONS)),
            lifetime: CONNECTION_LIFETIME,
        }
    }

    /// Limits may be reduced for a constrained deployment or fast fixtures.
    pub fn with_limits(inner: L, connections: usize, lifetime: Duration) -> io::Result<Self> {
        if !(1..=MAX_CONNECTIONS).contains(&connections)
            || lifetime.is_zero()
            || lifetime > CONNECTION_LIFETIME
        {
            return Err(invalid());
        }
        Ok(Self {
            inner,
            permits: Arc::new(Semaphore::new(connections)),
            lifetime,
        })
    }
}
impl<L: Listener> Listener for LimitedListener<L> {
    type Io = LimitedIo<L::Io>;
    type Addr = L::Addr;

    async fn accept(&mut self) -> (Self::Io, Self::Addr) {
        loop {
            let (io, address) = self.inner.accept().await;
            if let Ok(permit) = self.permits.clone().try_acquire_owned() {
                return (
                    LimitedIo {
                        inner: io,
                        _permit: permit,
                        deadline: Box::pin(tokio::time::sleep(self.lifetime)),
                    },
                    address,
                );
            }
            // Never spawn a task or queue a body for an excess connection.
            // Yield even if the underlying listener remains continuously ready.
            drop(io);
            tokio::task::yield_now().await;
        }
    }

    fn local_addr(&self) -> io::Result<Self::Addr> {
        self.inner.local_addr()
    }
}

/// Holds the capacity permit until the transport is dropped. The absolute
/// deadline is not renewed by trickled headers, body bytes or keepalive.
pub struct LimitedIo<T> {
    inner: T,
    _permit: OwnedSemaphorePermit,
    deadline: Pin<Box<Sleep>>,
}
impl<T> LimitedIo<T> {
    fn check_deadline(&mut self, cx: &mut Context<'_>) -> io::Result<()> {
        if self.deadline.as_mut().poll(cx).is_ready() {
            Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "HTTP connection lifetime exceeded",
            ))
        } else {
            Ok(())
        }
    }
}
impl<T: AsyncRead + Unpin> AsyncRead for LimitedIo<T> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        this.check_deadline(cx)?;
        Pin::new(&mut this.inner).poll_read(cx, buf)
    }
}
impl<T: AsyncWrite + Unpin> AsyncWrite for LimitedIo<T> {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<io::Result<usize>> {
        let this = self.get_mut();
        this.check_deadline(cx)?;
        Pin::new(&mut this.inner).poll_write(cx, bytes)
    }

    fn poll_write_vectored(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bytes: &[io::IoSlice<'_>],
    ) -> Poll<io::Result<usize>> {
        let this = self.get_mut();
        this.check_deadline(cx)?;
        Pin::new(&mut this.inner).poll_write_vectored(cx, bytes)
    }

    fn is_write_vectored(&self) -> bool {
        self.inner.is_write_vectored()
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        this.check_deadline(cx)?;
        Pin::new(&mut this.inner).poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        this.check_deadline(cx)?;
        Pin::new(&mut this.inner).poll_shutdown(cx)
    }
}
