// Copyright 2019 Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: MIT OR Apache-2.0

use crate::{error, transport::TransportStream, TransportError};
use aws_lc_rs::digest::{Context, SHA256};
use futures::StreamExt;
use futures_core::Stream;
use std::{convert::TryInto, path::Path, task::Poll};
use tokio::fs;
use url::Url;

pub(crate) struct DigestAdapter {
    url: Url,
    stream: TransportStream,
    hash: Vec<u8>,
    digest: Context,
}

impl DigestAdapter {
    pub(crate) fn sha256(stream: TransportStream, hash: &[u8], url: Url) -> TransportStream {
        Box::pin(Self {
            url,
            stream,
            hash: hash.to_owned(),
            digest: Context::new(&SHA256),
        })
    }
}

impl Stream for DigestAdapter {
    type Item = <TransportStream as Stream>::Item;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        let poll = self.stream.as_mut().poll_next(cx);
        match &poll {
            Poll::Ready(Some(Ok(bytes))) => {
                self.digest.update(bytes);
            }
            Poll::Ready(None) => {
                let result = &self.digest.clone().finish();
                if result.as_ref() != self.hash.as_slice() {
                    let mismatch_err = error::HashMismatchSnafu {
                        context: self.url.to_string(),
                        calculated: hex::encode(result),
                        expected: hex::encode(&self.hash),
                    }
                    .build();
                    return Poll::Ready(Some(Err(TransportError::new_with_cause(
                        crate::TransportErrorKind::Other,
                        self.url.clone(),
                        mismatch_err,
                    ))));
                }
            }
            Poll::Ready(Some(Err(_))) | Poll::Pending => (),
        }

        poll
    }
}

/// Create a new stream from `stream`. The new stream returns an error for the item that exceeds the
/// total byte count of `max_size`.
/// * `stream` - The original stream.
/// * `max_size` - Size limit in bytes.
/// * `specifier` - Error message to use.
pub(crate) fn max_size_adapter(
    stream: TransportStream,
    url: Url,
    max_size: u64,
    specifier: &'static str,
) -> TransportStream {
    let mut size: u64 = 0;
    let stream = stream.map(move |chunk| {
        if let Ok(bytes) = &chunk {
            size = size.saturating_add(bytes.len().try_into().unwrap_or(u64::MAX));
        }
        if size > max_size {
            let size_err = error::MaxSizeExceededSnafu {
                max_size,
                specifier,
            }
            .build();
            return Err(TransportError::new_with_cause(
                crate::TransportErrorKind::Other,
                url.clone(),
                size_err,
            ));
        }
        chunk
    });

    Box::pin(stream)
}

/// Async analogue of `std::path::Path::is_file`
pub async fn is_file(path: impl AsRef<Path>) -> bool {
    fs::metadata(path).await.is_ok_and(|m| m.is_file())
}

/// Async analogue of `std::path::Path::is_dir`
pub async fn is_dir(path: impl AsRef<Path>) -> bool {
    fs::metadata(path).await.is_ok_and(|m| m.is_dir())
}

#[cfg(test)]
mod tests {
    use crate::{
        io::{max_size_adapter, DigestAdapter},
        transport::IntoVec,
    };
    use bytes::Bytes;
    use futures::stream;
    use hex_literal::hex;
    use url::Url;

    #[tokio::test]
    async fn test_max_size_adapter() {
        let url = Url::parse("file:///").unwrap();

        let stream = stream::iter("hello".as_bytes().chunks(2).map(Bytes::from).map(Ok));
        let stream = max_size_adapter(Box::pin(stream), url.clone(), 5, "test");
        let buf = stream.into_vec().await.expect("consuming entire stream");
        assert_eq!(buf, b"hello");

        let stream = stream::iter("hello".as_bytes().chunks(2).map(Bytes::from).map(Ok));
        let stream = max_size_adapter(Box::pin(stream), url, 4, "test");
        assert!(stream.into_vec().await.is_err());
    }

    #[tokio::test]
    async fn test_digest_adapter() {
        let stream = stream::iter("hello".as_bytes().chunks(2).map(Bytes::from).map(Ok));
        let stream = DigestAdapter::sha256(
            Box::pin(stream),
            &hex!("2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"),
            Url::parse("file:///").unwrap(),
        );
        let buf = stream.into_vec().await.expect("consuming entire stream");
        assert_eq!(buf, b"hello");

        let stream = stream::iter("hello".as_bytes().chunks(2).map(Bytes::from).map(Ok));
        let stream = DigestAdapter::sha256(
            Box::pin(stream),
            &hex!("0ebdc3317b75839f643387d783535adc360ca01f33c75f7c1e7373adcd675c0b"),
            Url::parse("file:///").unwrap(),
        );
        assert!(stream.into_vec().await.is_err());
    }
}
