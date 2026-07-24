// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Minimal GraphQL-over-HTTP client: one HTTP/1.1 connection per request,
//! over TCP (plain or TLS) or a unix domain socket.

use std::fmt;
#[cfg(unix)]
use std::path::PathBuf;
use std::sync::Arc;

use http_body_util::{BodyExt, Full};
use hyper::body::Bytes;
use hyper::{Request, StatusCode};
use hyper_util::rt::TokioIo;
use serde_json::{Value, json};

/// Where the daemon listens.
pub enum Target {
    /// `host:port` of the daemon's TCP listener.
    Tcp { authority: String },
    /// `host:port` of a TLS reverse proxy in front of the daemon (the
    /// daemon itself never serves TLS). Certificates are validated
    /// against the system trust store.
    Tls {
        authority: String,
        server_name: String,
    },
    /// Path of the daemon's unix socket.
    #[cfg(unix)]
    Unix(PathBuf),
}

pub struct Client {
    target: Target,
    token: Option<String>,
}

#[derive(Debug)]
pub enum Error {
    /// Transport-level failure (connect, HTTP, non-200, bad JSON).
    Transport(String),
    /// The server executed the request and returned GraphQL errors.
    GraphQL(Vec<String>),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Transport(msg) => write!(f, "{msg}"),
            Error::GraphQL(messages) => write!(f, "{}", messages.join("; ")),
        }
    }
}

impl std::error::Error for Error {}

/// Parses `--url http(s)://host[:port]` into a [`Target`]. `https`
/// connects through a TLS reverse proxy in front of the daemon.
pub fn parse_url(url: &str) -> Result<Target, String> {
    let (rest, tls) = if let Some(rest) = url.strip_prefix("http://") {
        (rest, false)
    } else if let Some(rest) = url.strip_prefix("https://") {
        (rest, true)
    } else {
        return Err(format!("unsupported url {url}: use http:// or https://"));
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    if authority.is_empty() {
        return Err(format!("no host in url {url}"));
    }
    let (server_name, authority) = host_with_port(authority, if tls { 443 } else { 80 })
        .map_err(|e| format!("bad url {url}: {e}"))?;
    if tls {
        Ok(Target::Tls {
            authority,
            server_name,
        })
    } else {
        Ok(Target::Tcp { authority })
    }
}

/// Splits `host[:port]` (handling bracketed IPv6 literals) and applies
/// `default_port` when no port is given.
fn host_with_port(authority: &str, default_port: u16) -> Result<(String, String), String> {
    if let Some(rest) = authority.strip_prefix('[') {
        let Some((host, after)) = rest.split_once(']') else {
            return Err(format!("unclosed `[` in {authority}"));
        };
        let port = if after.is_empty() {
            default_port
        } else {
            let Some(port) = after.strip_prefix(':') else {
                return Err(format!("unexpected {after:?} after the IPv6 literal"));
            };
            port.parse::<u16>()
                .map_err(|_| format!("invalid port {port:?}"))?
        };
        return Ok((host.to_owned(), format!("[{host}]:{port}")));
    }
    match authority.split_once(':') {
        Some((_, rest)) if rest.contains(':') => {
            Err(format!("IPv6 must be bracketed, like [{authority}]"))
        }
        Some((host, port)) => {
            let port: u16 = port.parse().map_err(|_| format!("invalid port {port:?}"))?;
            Ok((host.to_owned(), format!("{host}:{port}")))
        }
        None => Ok((authority.to_owned(), format!("{authority}:{default_port}"))),
    }
}

/// A TLS connector trusting the system certificate store.
fn tls_connector() -> Result<tokio_rustls::TlsConnector, Error> {
    use tokio_rustls::rustls;

    let mut roots = rustls::RootCertStore::empty();
    for cert in rustls_native_certs::load_native_certs().certs {
        let _ = roots.add(cert);
    }
    if roots.is_empty() {
        return Err(Error::Transport(
            "no trusted root certificates found in the system store".to_owned(),
        ));
    }
    let config = rustls::ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    Ok(tokio_rustls::TlsConnector::from(Arc::new(config)))
}

impl Client {
    pub fn new(target: Target, token: Option<String>) -> Client {
        Client { target, token }
    }

    /// Executes one GraphQL request and returns its `data` field.
    pub async fn graphql(&self, query: &str, variables: Value) -> Result<Value, Error> {
        let payload = json!({ "query": query, "variables": variables }).to_string();
        // Send the real authority so name-based reverse proxies route the
        // request; the unix transport has no meaningful authority.
        let host = match &self.target {
            Target::Tcp { authority } | Target::Tls { authority, .. } => authority.as_str(),
            #[cfg(unix)]
            Target::Unix(_) => "rsbtd",
        };
        let mut builder = Request::builder()
            .method("POST")
            .uri("/graphql")
            .header("host", host)
            .header("content-type", "application/json");
        if let Some(token) = &self.token {
            builder = builder.header("authorization", format!("Bearer {token}"));
        }
        let request = builder
            .body(Full::new(Bytes::from(payload)))
            .map_err(|e| Error::Transport(e.to_string()))?;

        let (status, body) = match &self.target {
            Target::Tcp { authority } => {
                let stream = tokio::net::TcpStream::connect(authority)
                    .await
                    .map_err(|e| Error::Transport(format!("cannot connect to {authority}: {e}")))?;
                send(stream, request).await?
            }
            Target::Tls {
                authority,
                server_name,
            } => {
                let stream = tokio::net::TcpStream::connect(authority)
                    .await
                    .map_err(|e| Error::Transport(format!("cannot connect to {authority}: {e}")))?;
                let name =
                    tokio_rustls::rustls::pki_types::ServerName::try_from(server_name.clone())
                        .map_err(|e| {
                            Error::Transport(format!("invalid server name {server_name}: {e}"))
                        })?;
                let stream = tls_connector()?.connect(name, stream).await.map_err(|e| {
                    Error::Transport(format!("TLS handshake with {server_name} failed: {e}"))
                })?;
                send(stream, request).await?
            }
            #[cfg(unix)]
            Target::Unix(path) => {
                let stream = tokio::net::UnixStream::connect(path).await.map_err(|e| {
                    Error::Transport(format!("cannot connect to {}: {e}", path.display()))
                })?;
                send(stream, request).await?
            }
        };

        if status == StatusCode::UNAUTHORIZED {
            return Err(Error::Transport(
                "unauthorized: pass --token or set RSBTCTL_TOKEN".to_owned(),
            ));
        }
        if status != StatusCode::OK {
            return Err(Error::Transport(format!(
                "daemon returned {status}: {body}"
            )));
        }
        let mut response: Value = serde_json::from_str(&body)
            .map_err(|e| Error::Transport(format!("bad response JSON: {e}")))?;
        if let Some(errors) = response.get("errors").and_then(Value::as_array) {
            let messages = errors
                .iter()
                .map(|e| {
                    e.get("message")
                        .and_then(Value::as_str)
                        .unwrap_or("unknown error")
                        .to_owned()
                })
                .collect();
            return Err(Error::GraphQL(messages));
        }
        Ok(response["data"].take())
    }
}

async fn send<S>(stream: S, request: Request<Full<Bytes>>) -> Result<(StatusCode, String), Error>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    let (mut sender, connection) = hyper::client::conn::http1::handshake(TokioIo::new(stream))
        .await
        .map_err(|e| Error::Transport(format!("handshake failed: {e}")))?;
    tokio::spawn(connection);
    let response = sender
        .send_request(request)
        .await
        .map_err(|e| Error::Transport(format!("request failed: {e}")))?;
    let status = response.status();
    let body = response
        .into_body()
        .collect()
        .await
        .map_err(|e| Error::Transport(format!("cannot read response: {e}")))?
        .to_bytes();
    Ok((status, String::from_utf8_lossy(&body).into_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tcp(url: &str) -> String {
        match parse_url(url).unwrap() {
            Target::Tcp { authority } => authority,
            _ => panic!("{url} should parse as plain TCP"),
        }
    }

    #[test]
    fn applies_scheme_default_ports() {
        assert_eq!(tcp("http://example.com"), "example.com:80");
        assert_eq!(tcp("http://example.com/"), "example.com:80");
        assert_eq!(tcp("http://example.com/graphql?x#y"), "example.com:80");
        assert_eq!(tcp("http://example.com:3928"), "example.com:3928");
        assert_eq!(tcp("http://[::1]"), "[::1]:80");
        assert_eq!(tcp("http://[::1]:3928"), "[::1]:3928");
        match parse_url("https://example.com").unwrap() {
            Target::Tls {
                authority,
                server_name,
            } => {
                assert_eq!(authority, "example.com:443");
                assert_eq!(server_name, "example.com");
            }
            _ => panic!("https should parse as TLS"),
        }
    }

    #[test]
    fn rejects_malformed_urls() {
        for url in [
            "ftp://example.com",
            "http://",
            "http:///graphql",
            "http://example.com:",
            "http://example.com:port",
            "http://example.com:99999",
            "http://::1",
            "http://[::1",
            "http://[::1]x",
        ] {
            assert!(parse_url(url).is_err(), "{url} should be rejected");
        }
    }
}
