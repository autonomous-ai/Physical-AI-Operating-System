use crate::SafeUrlPath;
#[cfg(feature = "http")]
use crate::{HttpTransport, HttpTransportBuilder};
use async_trait::async_trait;
use bytes::Bytes;
use dyn_clone::DynClone;
use futures::TryStreamExt;
use futures_core::Stream;
use std::error::Error;
use std::fmt::{Debug, Display, Formatter};
use std::io::{self, ErrorKind};
use std::path::Path;
use std::pin::Pin;
use tokio_util::io::ReaderStream;
use url::Url;

/// Type alias for the stream returned by transports
pub type TransportStream = Pin<Box<dyn Stream<Item = Result<Bytes, TransportError>> + Send + Sync>>;

/// Fallible byte streams that collect into a `Vec<u8>`.
#[async_trait]
pub trait IntoVec<E> {
    /// Try to collect into `Vec<u8>`.
    async fn into_vec(self) -> Result<Vec<u8>, E>;
}

#[async_trait]
impl<S: Stream<Item = Result<Bytes, E>> + Send, E: Send> IntoVec<E> for S {
    async fn into_vec(self) -> Result<Vec<u8>, E> {
        self.try_fold(Vec::new(), |mut acc, bytes| {
            acc.extend(bytes.as_ref());
            std::future::ready(Ok(acc))
        })
        .await
    }
}

/// A trait to abstract over the method/protocol by which files are obtained.
///
/// The trait hides the underlying types involved by returning the `Read` object as a
/// `Box<dyn Read + Send>` and by requiring concrete type [`TransportError`] as the error type.
///
/// Inclusion of the `DynClone` trait means that you will need to implement `Clone` when
/// implementing a `Transport`.
#[async_trait]
pub trait Transport: Debug + DynClone + Send + Sync {
    /// Opens a `Read` object for the file specified by `url`.
    async fn fetch(&self, url: Url) -> Result<TransportStream, TransportError>;
}

// Implements `Clone` for `Transport` trait objects (i.e. on `Box::<dyn Clone>`). To facilitate
// this, `Clone` needs to be implemented for any `Transport`s. The compiler will enforce this.
dyn_clone::clone_trait_object!(Transport);

// =^..^=   =^..^=   =^..^=   =^..^=   =^..^=   =^..^=   =^..^=   =^..^=   =^..^=   =^..^=   =^..^=

/// The kind of error that the transport object experienced during `fetch`.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
#[non_exhaustive]
pub enum TransportErrorKind {
    /// The [`Transport`] does not handle the URL scheme. e.g. `file://` or `http://`.
    UnsupportedUrlScheme,
    /// The file cannot be found.
    ///
    /// Some TUF operations could benefit from knowing whether a [`Transport`] failure is a result
    /// of a file not existing. In particular:
    /// > TUF v1.0.16 5.2.2. Try downloading version N+1 of the root metadata file `[...]` If this
    /// > file is not available `[...]` then go to step 5.1.9.
    ///
    /// We want to distinguish cases when a specific file probably doesn't exist from cases where
    /// the failure to fetch it is due to some other problem (i.e. some fault in the [`Transport`]
    /// or the machine hosting the file).
    ///
    /// For some transports, the distinction is obvious. For example, a local file transport should
    /// return `FileNotFound` for `std::error::ErrorKind::NotFound` and nothing else. For other
    /// transports it might be less obvious, but the intent of `FileNotFound` is to indicate that
    /// the file probably doesn't exist.
    FileNotFound,
    /// The transport failed for any other reason, e.g. IO error, HTTP broken pipe, etc.
    Other,
}

impl Display for TransportErrorKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                TransportErrorKind::UnsupportedUrlScheme => "unsupported URL scheme",
                TransportErrorKind::FileNotFound => "file not found",
                TransportErrorKind::Other => "other",
            }
        )
    }
}

/// The error type that [`Transport::fetch`] returns.
#[derive(Debug)]
pub struct TransportError {
    /// The kind of error that occurred.
    kind: TransportErrorKind,
    /// The URL that the transport was trying to fetch.
    url: String,
    /// The underlying error that occurred (if any).
    source: Option<Box<dyn Error + Send + Sync>>,
}

impl TransportError {
    /// Creates a new [`TransportError`]. Use this when there is no underlying error to wrap.
    pub fn new<S>(kind: TransportErrorKind, url: S) -> Self
    where
        S: AsRef<str>,
    {
        Self {
            kind,
            url: url.as_ref().into(),
            source: None,
        }
    }

    /// Creates a new [`TransportError`]. Use this to preserve an underlying error.
    pub fn new_with_cause<S, E>(kind: TransportErrorKind, url: S, source: E) -> Self
    where
        E: Into<Box<dyn Error + Send + Sync>>,
        S: AsRef<str>,
    {
        Self {
            kind,
            url: url.as_ref().into(),
            source: Some(source.into()),
        }
    }

    /// The type of [`Transport`] error that occurred.
    pub fn kind(&self) -> TransportErrorKind {
        self.kind
    }

    /// The URL that the [`Transport`] was trying to fetch when the error occurred.
    pub fn url(&self) -> &str {
        self.url.as_str()
    }
}

impl Display for TransportError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        if let Some(e) = self.source.as_ref() {
            write!(
                f,
                "Transport '{}' error fetching '{}': {e}",
                self.kind, self.url
            )
        } else {
            write!(f, "Transport '{}' error fetching '{}'", self.kind, self.url)
        }
    }
}

impl Error for TransportError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.source.as_ref().map(|e| e.as_ref() as &dyn Error)
    }
}

// =^..^=   =^..^=   =^..^=   =^..^=   =^..^=   =^..^=   =^..^=   =^..^=   =^..^=   =^..^=   =^..^=

/// Provides a [`Transport`] for local files.
#[derive(Debug, Clone, Copy)]
pub struct FilesystemTransport;

impl FilesystemTransport {
    async fn open(
        file_path: impl AsRef<Path>,
    ) -> Result<impl Stream<Item = Result<Bytes, io::Error>> + Send, io::Error> {
        // Open the file
        let f = tokio::fs::File::open(file_path).await?;

        // And convert to stream
        let reader = tokio::io::BufReader::new(f);
        let stream = ReaderStream::new(reader);

        Ok(stream)
    }
}

#[async_trait]
impl Transport for FilesystemTransport {
    async fn fetch(&self, url: Url) -> Result<TransportStream, TransportError> {
        // If the scheme isn't "file://", reject
        if url.scheme() != "file" {
            return Err(TransportError::new(
                TransportErrorKind::UnsupportedUrlScheme,
                url,
            ));
        }

        let file_path = url.safe_url_filepath();

        // Open the file
        let stream = Self::open(file_path).await;

        // And map to `TransportError`
        let map_io_err = move |e: io::Error| -> TransportError {
            let kind = match e.kind() {
                ErrorKind::NotFound => TransportErrorKind::FileNotFound,
                _ => TransportErrorKind::Other,
            };
            TransportError::new_with_cause(kind, url.clone(), e)
        };
        Ok(Box::pin(
            stream.map_err(map_io_err.clone())?.map_err(map_io_err),
        ))
    }
}

// =^..^=   =^..^=   =^..^=   =^..^=   =^..^=   =^..^=   =^..^=   =^..^=   =^..^=   =^..^=   =^..^=

/// A Transport that provides support for both local files and, if the `http` feature is enabled,
/// HTTP-transported files.
// `Copy` cannot be implemented when the `http` feature is enabled because `HttpTransport`
// contains an `Arc<CryptoProvider>` which is not `Copy`.
#[derive(Debug, Clone)]
#[allow(missing_copy_implementations)]
pub struct DefaultTransport {
    file: FilesystemTransport,
    #[cfg(feature = "http")]
    http: HttpTransport,
}

impl Default for DefaultTransport {
    fn default() -> Self {
        Self {
            file: FilesystemTransport,
            #[cfg(feature = "http")]
            http: HttpTransport::default(),
        }
    }
}

impl DefaultTransport {
    /// Creates a new `DefaultTransport`. Same as `default()`.
    pub fn new() -> Self {
        Self::default()
    }
}

#[cfg(feature = "http")]
impl DefaultTransport {
    /// Create a new `DefaultTransport` with potentially customized settings.
    pub fn new_with_http_settings(builder: HttpTransportBuilder) -> Self {
        Self {
            file: FilesystemTransport,
            http: builder.build(),
        }
    }
}

#[async_trait]
impl Transport for DefaultTransport {
    async fn fetch(&self, url: Url) -> Result<TransportStream, TransportError> {
        match url.scheme() {
            "file" => self.file.fetch(url).await,
            "http" | "https" => self.handle_http(url).await,
            _ => Err(TransportError::new(
                TransportErrorKind::UnsupportedUrlScheme,
                url,
            )),
        }
    }
}

impl DefaultTransport {
    #[cfg(not(feature = "http"))]
    #[allow(clippy::trivially_copy_pass_by_ref, clippy::unused_self)]
    async fn handle_http(&self, url: Url) -> Result<TransportStream, TransportError> {
        Err(TransportError::new_with_cause(
            TransportErrorKind::UnsupportedUrlScheme,
            url,
            "The library was not compiled with the http feature enabled.",
        ))
    }

    #[cfg(feature = "http")]
    async fn handle_http(&self, url: Url) -> Result<TransportStream, TransportError> {
        self.http.fetch(url).await
    }
}
