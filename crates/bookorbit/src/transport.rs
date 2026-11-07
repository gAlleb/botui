use crate::Result;
use reqwest::{StatusCode, Url, blocking::Response};
use serde::de::DeserializeOwned;
use std::{
    error::Error,
    fmt,
    io::{self, Read, Write},
    net::IpAddr,
};

pub(crate) fn api_root(server: &str) -> Result<Url> {
    let mut url = Url::parse(server).map_err(|_| "Invalid BookOrbit server address.")?;
    if !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err("Use the BookOrbit root URL without credentials or query parameters.".into());
    }
    let host = url
        .host_str()
        .ok_or("Enter the BookOrbit server address.")?;
    match url.scheme() {
        "https" => {}
        "http" => {
            if host == "localhost" {
                // Resolve localhost here so plaintext credentials cannot be
                // redirected by /etc/hosts, DNS, or a proxy to another machine.
                url.set_host(Some("127.0.0.1"))?;
            } else if !host
                .trim_matches(['[', ']'])
                .parse::<IpAddr>()
                .is_ok_and(|ip| ip.is_loopback())
            {
                return Err("Remote BookOrbit requires HTTPS.".into());
            }
        }
        _ => return Err("Use HTTPS or local HTTP.".into()),
    }
    url.path_segments_mut()
        .map_err(|_| "Invalid server path.")?
        .pop_if_empty()
        .extend(["api", "v1", ""]);
    Ok(url)
}

#[derive(Debug)]
pub(crate) struct HttpFailure {
    pub status: StatusCode,
    pub description: Option<&'static str>,
}

impl fmt::Display for HttpFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "BookOrbit returned HTTP {}.", self.status.as_u16())?;
        if let Some(reason) = self.description {
            write!(f, " {reason}")?;
        }
        Ok(())
    }
}

impl Error for HttpFailure {}

pub(crate) fn checked(response: Response) -> Result<Response> {
    let status = response.status();
    if status.is_success() {
        Ok(response)
    } else {
        Err(HttpFailure {
            status,
            description: None,
        }
        .into())
    }
}

pub(crate) fn raw_bounded(input: impl Read, max: u64) -> Result<Vec<u8>> {
    let mut buf = Vec::new();
    copy_bounded(input, &mut buf, max)?;
    Ok(buf)
}

pub(crate) fn copy_bounded(
    mut input: impl Read,
    mut writer: impl Write,
    limit: u64,
) -> Result<u64> {
    let count = io::copy(&mut input.by_ref().take(limit), &mut writer)?;
    if count == limit && input.read(&mut [0; 1])? != 0 {
        return Err("BookOrbit response exceeds the size limit.".into());
    }
    Ok(count)
}

pub(crate) fn decode<T: DeserializeOwned>(input: impl Read) -> Result<T> {
    let bytes = raw_bounded(input, 8 * 1024 * 1024)?;
    // Upstream JSON or proxy error pages can contain arbitrary text, including credentials.
    serde_json::from_slice(&bytes).map_err(|_| "Invalid JSON in the BookOrbit response.".into())
}
