// Copyright 2019 Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: MIT OR Apache-2.0

use crate::error::{self, Result};
use crate::io::{max_size_adapter, DigestAdapter};
use crate::transport::{Transport, TransportStream};
use snafu::ResultExt;
use url::Url;

pub(crate) async fn fetch_max_size(
    transport: &dyn Transport,
    url: Url,
    max_size: u64,
    specifier: &'static str,
) -> Result<TransportStream> {
    let stream = transport
        .fetch(url.clone())
        .await
        .with_context(|_| error::TransportSnafu { url: url.clone() })?;

    let stream = max_size_adapter(stream, url, max_size, specifier);
    Ok(stream)
}

pub(crate) async fn fetch_sha256(
    transport: &dyn Transport,
    url: Url,
    size: u64,
    specifier: &'static str,
    sha256: &[u8],
) -> Result<TransportStream> {
    let stream = fetch_max_size(transport, url.clone(), size, specifier).await?;
    Ok(DigestAdapter::sha256(stream, sha256, url))
}
