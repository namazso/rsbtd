// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Registry-backed daemon configuration.
//!
//! The Windows build has no config file; the equivalent of the TOML
//! settings lives under `HKCU\Software\rsbtd` (see the value table in
//! [`crate::windows`]). The app seeds `Token` itself on its first start
//! (see [`initialize`]); everything else falls back to defaults, so a
//! bare `rsbtd.exe` works without an installer.

use std::net::SocketAddr;
use std::path::PathBuf;

use crate::config::{Config, Listen, validate_origin};

/// The config key, relative to HKCU.
pub const REGISTRY_KEY: &str = r"Software\rsbtd";

pub const DEFAULT_LISTEN: &str = "127.0.0.1:3928";
pub const DEFAULT_SHUTDOWN_GRACE_SECS: u32 = 15;

/// The subset of settings editable in the tray's Settings dialog.
#[derive(Clone, Debug)]
pub struct Editable {
    pub listen: String,
    /// `None` disables authentication (a deliberate, confirmed choice).
    pub token: Option<String>,
    pub state_dir: PathBuf,
    pub shutdown_grace_secs: u32,
}

/// Converts a registry read into an optional value: `Ok(None)` only when
/// the value (or key) does not exist. Any other failure — wrong type,
/// access denied — is a hard error: silently defaulting a
/// present-but-malformed `Token` would disable authentication.
fn optional<T>(result: windows_registry::Result<T>, what: &str) -> Result<Option<T>, String> {
    // HRESULT-encoded ERROR_FILE_NOT_FOUND: a missing value or key.
    const FILE_NOT_FOUND: i32 = 0x8007_0002_u32 as i32;
    match result {
        Ok(value) => Ok(Some(value)),
        Err(e) if e.code().0 == FILE_NOT_FOUND => Ok(None),
        Err(e) => Err(format!(
            "registry value HKCU\\{REGISTRY_KEY}\\{what} is invalid or unreadable: {e}"
        )),
    }
}

fn open() -> Result<Option<windows_registry::Key>, String> {
    optional(windows_registry::CURRENT_USER.open(REGISTRY_KEY), "")
}

/// Reads an optional string value, treating an empty `REG_SZ` as absent.
/// For `Token` this is how unauthenticated access is configured: the
/// Settings dialog stores an empty string, and `None` disables auth.
fn get_string(key: Option<&windows_registry::Key>, name: &str) -> Result<Option<String>, String> {
    let Some(key) = key else { return Ok(None) };
    Ok(optional(key.get_string(name), name)?.filter(|value| !value.is_empty()))
}

/// First-start initialization: a config key without any `Token` value
/// means rsbtd has never run for this user (an empty value is the
/// deliberate "no authentication" choice, and a malformed one is a hard
/// error rather than something to overwrite). Generates and persists a
/// random API token, and reports whether this was the first start so
/// the caller can run one-time setup like the autostart prompt.
pub fn initialize() -> Result<bool, String> {
    if let Some(key) = open()?.as_ref()
        && optional(key.get_string("Token"), "Token")?.is_some()
    {
        return Ok(false);
    }
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(|e| format!("cannot generate an API token: {e}"))?;
    let token = hex::encode(bytes);
    let key = windows_registry::CURRENT_USER
        .create(REGISTRY_KEY)
        .map_err(|e| format!("cannot open HKCU\\{REGISTRY_KEY}: {e}"))?;
    key.set_string("Token", &token)
        .map_err(|e| format!("cannot write the generated Token: {e}"))?;
    Ok(true)
}

/// Directory of the running executable, for deriving the web UI path.
fn exe_dir() -> Option<PathBuf> {
    Some(std::env::current_exe().ok()?.parent()?.to_path_buf())
}

fn default_state_dir() -> Result<PathBuf, String> {
    let base = std::env::var_os("LOCALAPPDATA")
        .ok_or_else(|| "LOCALAPPDATA is not set; cannot derive a state directory".to_string())?;
    Ok(PathBuf::from(base).join("rsbtd").join("state"))
}

/// The current editable settings (registry values or their defaults),
/// as shown in the Settings dialog.
pub fn editable() -> Result<Editable, String> {
    let key = open()?;
    let key = key.as_ref();
    let state_dir = match get_string(key, "StateDir")? {
        Some(dir) => PathBuf::from(dir),
        None => default_state_dir()?,
    };
    let shutdown_grace_secs = match key {
        Some(k) => optional(k.get_u32("ShutdownGraceSecs"), "ShutdownGraceSecs")?,
        None => None,
    };
    Ok(Editable {
        listen: get_string(key, "Listen")?.unwrap_or_else(|| DEFAULT_LISTEN.to_string()),
        token: get_string(key, "Token")?,
        state_dir,
        shutdown_grace_secs: shutdown_grace_secs.unwrap_or(DEFAULT_SHUTDOWN_GRACE_SECS),
    })
}

/// Persists the Settings dialog values. `token: None` stores an empty
/// string (disabling authentication): a completely missing value means
/// "never ran" and would make [`initialize`] generate a fresh token on
/// the next start.
pub fn save(editable: &Editable) -> Result<(), String> {
    let key = windows_registry::CURRENT_USER
        .create(REGISTRY_KEY)
        .map_err(|e| format!("cannot open HKCU\\{REGISTRY_KEY}: {e}"))?;
    let put = |name: &str, value: &str| {
        key.set_string(name, value)
            .map_err(|e| format!("cannot write {name}: {e}"))
    };
    put("Listen", &editable.listen)?;
    put("Token", editable.token.as_deref().unwrap_or(""))?;
    put("StateDir", &editable.state_dir.to_string_lossy())?;
    key.set_u32("ShutdownGraceSecs", editable.shutdown_grace_secs)
        .map_err(|e| format!("cannot write ShutdownGraceSecs: {e}"))?;
    Ok(())
}

/// The tracing filter (registry `LogFilter`, default `info`). Read before
/// the full config, and the one value where any failure falls back to the
/// default: logging must come up so a config error can be reported.
pub fn log_filter() -> String {
    open()
        .ok()
        .flatten()
        .and_then(|key| key.get_string("LogFilter").ok())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "info".to_string())
}

/// Builds the daemon [`Config`] from the registry (plus derived values),
/// validating exactly like the TOML path does.
pub fn load() -> Result<Config, String> {
    let editable = editable()?;
    let key = open()?;
    let key = key.as_ref();

    // The web UI ships next to the exe; an explicit ServeRoot overrides.
    // Only a directory that exists is passed on: Daemon::start rejects a
    // missing serve_root, and a bare cargo-built exe has no webui dir.
    let serve_root = match get_string(key, "ServeRoot")? {
        Some(root) => Some(PathBuf::from(root)),
        None => exe_dir()
            .map(|dir| dir.join("webui"))
            .filter(|p| p.is_dir()),
    };

    let cors: Vec<String> = match key {
        Some(k) => optional(k.get_multi_string("Cors"), "Cors")?,
        None => None,
    }
    .unwrap_or_default()
    .into_iter()
    .filter(|origin| !origin.is_empty())
    .collect();
    for origin in &cors {
        validate_origin(origin).map_err(|e| e.to_string())?;
    }

    Ok(Config {
        state_dir: editable.state_dir,
        listen: Listen::Tcp(parse_listen(&editable.listen)?),
        token: editable.token,
        graphiql: false,
        serve_root,
        cors,
        shutdown_grace_secs: editable.shutdown_grace_secs.into(),
    })
}

/// Parses the `Listen` registry value (TCP only on Windows).
pub fn parse_listen(listen: &str) -> Result<SocketAddr, String> {
    listen
        .parse()
        .map_err(|e| format!("Listen {listen:?} is not a host:port address: {e}"))
}
