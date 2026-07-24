// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Daemon bootstrap configuration.
//!
//! Only daemon-level concerns live here (where to listen, the API token,
//! where state is kept). libtorrent settings are managed through the API
//! and persisted in the state directory as session state.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use serde::Deserialize;

/// Where the API server listens. Exactly one of TCP or unix socket.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Listen {
    /// TCP socket address, e.g. `127.0.0.1:3928`.
    Tcp(SocketAddr),
    Unix(PathBuf),
}

/// Daemon configuration, loaded from a TOML file.
///
/// ```toml
/// state_dir = "/var/lib/rsbtd"
///
/// [api]
/// listen = "127.0.0.1:3928"    # or a unix socket: "unix:/run/rsbtd/api.sock"
/// token = "changeme"           # optional; omit to disable auth
/// graphiql = false             # serve GraphiQL on GET /
/// serve_root = "/srv/webui"    # serve static files (a built web UI) on /
/// cors = ["https://ui.example.com"]  # origins allowed to call the API
/// ```
#[derive(Clone, Debug)]
pub struct Config {
    /// Directory for resume data, session state, and other daemon state.
    pub state_dir: PathBuf,
    /// Where the API listens.
    pub listen: Listen,
    /// Static bearer token required on API requests; `None` disables auth.
    pub token: Option<String>,
    /// Whether to serve the GraphiQL IDE on `GET /`.
    pub graphiql: bool,
    /// Directory of static files (a built web UI) served on `/`;
    /// mutually exclusive with `graphiql`.
    pub serve_root: Option<PathBuf>,
    /// Origins allowed to call the API cross-origin (`Origin` header
    /// values, e.g. `https://ui.example.com`, or `"*"` for any). Empty
    /// means no CORS headers: browsers only allow same-origin use.
    pub cors: Vec<String>,
    /// Seconds allotted to the shutdown persistence phase.
    pub shutdown_grace_secs: u64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawConfig {
    state_dir: PathBuf,
    api: RawApi,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawApi {
    listen: String,
    token: Option<String>,
    #[serde(default)]
    graphiql: bool,
    serve_root: Option<PathBuf>,
    #[serde(default)]
    cors: Vec<String>,
    #[serde(default = "default_shutdown_grace")]
    shutdown_grace_secs: u64,
}

fn default_shutdown_grace() -> u64 {
    15
}

#[derive(Debug)]
pub enum ConfigError {
    Io(std::io::Error),
    /// The TOML failed to parse or had unknown/missing fields.
    Parse(toml::de::Error),
    /// A field value was rejected.
    Invalid(String),
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::Io(e) => write!(f, "cannot read config: {e}"),
            ConfigError::Parse(e) => write!(f, "cannot parse config: {e}"),
            ConfigError::Invalid(msg) => write!(f, "invalid config: {msg}"),
        }
    }
}

impl std::error::Error for ConfigError {}

impl Config {
    /// Loads and validates a TOML config file.
    pub fn load(path: &Path) -> Result<Config, ConfigError> {
        let text = std::fs::read_to_string(path).map_err(ConfigError::Io)?;
        Config::parse(&text)
    }

    /// Parses and validates TOML config text.
    pub fn parse(text: &str) -> Result<Config, ConfigError> {
        let raw: RawConfig = toml::from_str(text).map_err(ConfigError::Parse)?;
        let listen = parse_listen(&raw.api.listen)?;
        if let Some(token) = &raw.api.token
            && token.is_empty()
        {
            return Err(ConfigError::Invalid(
                "api.token must be non-empty (omit it to disable auth)".into(),
            ));
        }
        if raw.api.graphiql && raw.api.serve_root.is_some() {
            return Err(ConfigError::Invalid(
                "api.graphiql and api.serve_root both claim GET /; enable only one".into(),
            ));
        }
        for origin in &raw.api.cors {
            validate_origin(origin)?;
        }
        Ok(Config {
            state_dir: raw.state_dir,
            listen,
            token: raw.api.token,
            graphiql: raw.api.graphiql,
            serve_root: raw.api.serve_root,
            cors: raw.api.cors,
            shutdown_grace_secs: raw.api.shutdown_grace_secs,
        })
    }
}

/// An allowed CORS origin: `*`, or scheme://host[:port] exactly as browsers
/// send it in the `Origin` header (no path, no trailing slash).
/// `pub(crate)`: every config source must validate origins before
/// constructing a [`Config`]; `api::cors_layer` trusts them to parse.
pub(crate) fn validate_origin(origin: &str) -> Result<(), ConfigError> {
    if origin == "*" {
        return Ok(());
    }
    let rest = origin
        .strip_prefix("http://")
        .or_else(|| origin.strip_prefix("https://"));
    let ok = match rest {
        Some(host) => {
            !host.is_empty()
                && host
                    .bytes()
                    .all(|b| (0x21..=0x7e).contains(&b) && b != b'/')
        }
        None => false,
    };
    if ok {
        Ok(())
    } else {
        Err(ConfigError::Invalid(format!(
            "api.cors entry {origin:?} is not \"*\" or an origin \
             (scheme://host[:port], no path)"
        )))
    }
}

fn parse_listen(s: &str) -> Result<Listen, ConfigError> {
    if let Some(path) = s.strip_prefix("unix:") {
        if path.is_empty() {
            return Err(ConfigError::Invalid("empty unix socket path".into()));
        }
        return Ok(Listen::Unix(PathBuf::from(path)));
    }
    s.parse()
        .map(Listen::Tcp)
        .map_err(|e| ConfigError::Invalid(format!("api.listen {s:?}: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_tcp_listen() {
        let cfg = Config::parse(
            r#"
            state_dir = "/var/lib/rsbtd"
            [api]
            listen = "127.0.0.1:3928"
            token = "secret"
            "#,
        )
        .unwrap();
        assert_eq!(cfg.listen, Listen::Tcp("127.0.0.1:3928".parse().unwrap()));
        assert_eq!(cfg.token.as_deref(), Some("secret"));
        assert!(!cfg.graphiql);
        assert_eq!(cfg.serve_root, None);
        assert!(cfg.cors.is_empty());
        assert_eq!(cfg.shutdown_grace_secs, 15);
    }

    #[test]
    fn parses_serve_root_and_cors() {
        let cfg = Config::parse(
            r#"
            state_dir = "/var/lib/rsbtd"
            [api]
            listen = "127.0.0.1:3928"
            serve_root = "/srv/webui"
            cors = ["https://ui.example.com", "http://localhost:5173"]
            "#,
        )
        .unwrap();
        assert_eq!(cfg.serve_root.as_deref(), Some(Path::new("/srv/webui")));
        assert_eq!(
            cfg.cors,
            vec!["https://ui.example.com", "http://localhost:5173"]
        );
    }

    #[test]
    fn accepts_wildcard_cors() {
        let cfg =
            Config::parse("state_dir = \"/x\"\n[api]\nlisten = \"127.0.0.1:1\"\ncors = [\"*\"]")
                .unwrap();
        assert_eq!(cfg.cors, vec!["*"]);
    }

    #[test]
    fn rejects_bad_cors_origins() {
        for bad in [
            "ui.example.com",           // missing scheme
            "https://ui.example.com/",  // trailing slash
            "https://ui.example.com/x", // path
            "",
        ] {
            let text =
                format!("state_dir = \"/x\"\n[api]\nlisten = \"127.0.0.1:1\"\ncors = [{bad:?}]");
            assert!(
                matches!(Config::parse(&text), Err(ConfigError::Invalid(_))),
                "accepted {bad:?}"
            );
        }
    }

    #[test]
    fn rejects_graphiql_with_serve_root() {
        assert!(matches!(
            Config::parse(
                "state_dir = \"/x\"\n[api]\nlisten = \"127.0.0.1:1\"\ngraphiql = true\nserve_root = \"/srv\"",
            ),
            Err(ConfigError::Invalid(_))
        ));
    }

    #[test]
    fn parses_unix_listen() {
        let cfg = Config::parse(
            r#"
            state_dir = "/tmp/rsbtd"
            [api]
            listen = "unix:/run/rsbtd/api.sock"
            graphiql = true
            "#,
        )
        .unwrap();
        assert_eq!(cfg.listen, Listen::Unix("/run/rsbtd/api.sock".into()));
        assert_eq!(cfg.token, None);
        assert!(cfg.graphiql);
    }

    #[test]
    fn rejects_bad_listen_and_empty_token() {
        assert!(matches!(
            Config::parse("state_dir = \"/x\"\n[api]\nlisten = \"nonsense\""),
            Err(ConfigError::Invalid(_))
        ));
        assert!(matches!(
            Config::parse("state_dir = \"/x\"\n[api]\nlisten = \"unix:\""),
            Err(ConfigError::Invalid(_))
        ));
        assert!(matches!(
            Config::parse("state_dir = \"/x\"\n[api]\nlisten = \"127.0.0.1:1\"\ntoken = \"\""),
            Err(ConfigError::Invalid(_))
        ));
    }

    #[test]
    fn rejects_unknown_fields() {
        assert!(matches!(
            Config::parse("state_dir = \"/x\"\ntypo = 1\n[api]\nlisten = \"127.0.0.1:1\""),
            Err(ConfigError::Parse(_))
        ));
    }
}
