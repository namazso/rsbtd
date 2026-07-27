// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The Deluge `core.conf` mapping over the session settings —
//! `core.get_config`, its per-key variants, `core.set_config`, and the
//! `core.get_proxy` shortcut.
//!
//! Only keys with engine backing appear (`CONFIG_KEYS`); clients
//! enumerate the config at runtime and tolerate absent keys.
//! `download_location` is one of those — rsbtd has no daemon-level
//! default, which is also why `core.add_torrent_*` demands one per add.

use std::num::NonZeroU16;

use rbtorrent::settings::{EncLevel, EncPolicy, setting_by_name};
use rbtorrent::{Credentials, ProxyConfig, ProxyProtocol, SettingsPack};
use serde_json::{Map, Value, json};

use super::proto::RpcError;
use super::registry::{Access, Ctx, HandlerResult, Registry, Scope, ok, positional};
use super::values::{bool_of, f64_of, i32_of, rate_out, session_rate_of};
use crate::engine::Engine;

const CONFIG_KEYS: [&str; 22] = [
    "auto_manage_prefer_seeds",
    "dht",
    "dont_count_slow_torrents",
    "enc_in_policy",
    "enc_level",
    "enc_out_policy",
    "listen_ports",
    "lsd",
    "max_active_downloading",
    "max_active_limit",
    "max_active_seeding",
    "max_connections_global",
    "max_download_speed",
    "max_upload_slots_global",
    "max_upload_speed",
    "natpmp",
    "proxy",
    "rate_limit_ip_overhead",
    "seed_time_limit",
    "seed_time_ratio_limit",
    "share_ratio_limit",
    "upnp",
];

pub(super) fn register(r: &mut Registry) {
    use Access::Normal;
    use Scope::Daemon;
    r.add("core.get_config", Daemon, Normal, get_config);
    r.add("core.get_config_value", Daemon, Normal, get_config_value);
    r.add("core.get_config_values", Daemon, Normal, get_config_values);
    r.add("core.set_config", Daemon, Normal, set_config);
    r.add("core.get_proxy", Daemon, Normal, get_proxy);
}

fn raw_int(pack: &SettingsPack, name: &str) -> i64 {
    setting_by_name(name)
        .and_then(|key| pack.get_int(key))
        .map_or(0, i64::from)
}

fn raw_str(pack: &SettingsPack, name: &str) -> String {
    setting_by_name(name)
        .and_then(|key| pack.get_str(key))
        .unwrap_or_default()
}

fn raw_bool(pack: &SettingsPack, name: &str) -> bool {
    setting_by_name(name)
        .and_then(|key| pack.get_bool(key))
        .unwrap_or(false)
}

pub(super) fn config_value(engine: &Engine, pack: &SettingsPack, key: &str) -> Option<Value> {
    Some(match key {
        "dht" => json!(raw_bool(pack, "enable_dht")),
        "upnp" => json!(raw_bool(pack, "enable_upnp")),
        "natpmp" => json!(raw_bool(pack, "enable_natpmp")),
        "lsd" => json!(raw_bool(pack, "enable_lsd")),
        "max_connections_global" => json!(raw_int(pack, "connections_limit")),
        "max_upload_slots_global" => json!(raw_int(pack, "unchoke_slots_limit")),
        "max_upload_speed" => json!(rate_out(raw_int(pack, "upload_rate_limit"))),
        "max_download_speed" => json!(rate_out(raw_int(pack, "download_rate_limit"))),
        "max_active_downloading" => json!(raw_int(pack, "active_downloads")),
        "max_active_seeding" => json!(raw_int(pack, "active_seeds")),
        "max_active_limit" => json!(raw_int(pack, "active_limit")),
        "dont_count_slow_torrents" => json!(raw_bool(pack, "dont_count_slow_torrents")),
        "auto_manage_prefer_seeds" => json!(raw_bool(pack, "auto_manage_prefer_seeds")),
        "rate_limit_ip_overhead" => json!(raw_bool(pack, "rate_limit_ip_overhead")),
        // libtorrent's ratios are percent, Deluge's plain floats.
        "share_ratio_limit" => json!(raw_int(pack, "share_ratio_limit") as f64 / 100.0),
        "seed_time_ratio_limit" => json!(raw_int(pack, "seed_time_ratio_limit") as f64 / 100.0),
        // Seconds in libtorrent, minutes in Deluge.
        "seed_time_limit" => json!(raw_int(pack, "seed_time_limit") / 60),
        // The policy ints happen to coincide.
        "enc_in_policy" => json!(raw_int(pack, "in_enc_policy")),
        "enc_out_policy" => json!(raw_int(pack, "out_enc_policy")),
        "enc_level" => json!(match pack.get_allowed_enc_level() {
            Some(EncLevel::PePlaintext) => 0,
            Some(EncLevel::PeRc4) => 1,
            _ => 2,
        }),
        "listen_ports" => {
            let port = engine.listen_port().ok()?;
            json!([port, port])
        }
        "proxy" => proxy_value(pack),
        _ => return None,
    })
}

/// The type ints coincide with libtorrent's (0 none … 5 HTTP with auth).
fn proxy_value(pack: &SettingsPack) -> Value {
    json!({
        "type": raw_int(pack, "proxy_type"),
        "hostname": raw_str(pack, "proxy_hostname"),
        "port": raw_int(pack, "proxy_port"),
        "username": raw_str(pack, "proxy_username"),
        "password": raw_str(pack, "proxy_password"),
        "proxy_hostnames": raw_bool(pack, "proxy_hostnames"),
        "proxy_peer_connections": raw_bool(pack, "proxy_peer_connections"),
        "proxy_tracker_connections": raw_bool(pack, "proxy_tracker_connections"),
        "anonymous_mode": raw_bool(pack, "anonymous_mode"),
    })
}

/// Returns whether the key was staged; unsupported ones are skipped.
fn stage_config(
    engine: &Engine,
    pack: &mut SettingsPack,
    key: &str,
    value: &Value,
) -> Result<bool, RpcError> {
    match key {
        "dht" => {
            pack.enable_dht(bool_of(key, value)?);
        }
        "upnp" => {
            pack.enable_upnp(bool_of(key, value)?);
        }
        "natpmp" => {
            pack.enable_natpmp(bool_of(key, value)?);
        }
        "lsd" => {
            pack.enable_lsd(bool_of(key, value)?);
        }
        "max_connections_global" => {
            pack.connections_limit(i32_of(key, value)?);
        }
        "max_upload_slots_global" => {
            pack.unchoke_slots_limit(i32_of(key, value)?);
        }
        "max_upload_speed" => {
            pack.upload_rate_limit(session_rate_of(key, value)?);
        }
        "max_download_speed" => {
            pack.download_rate_limit(session_rate_of(key, value)?);
        }
        "max_active_downloading" => {
            pack.active_downloads(i32_of(key, value)?);
        }
        "max_active_seeding" => {
            pack.active_seeds(i32_of(key, value)?);
        }
        "max_active_limit" => {
            pack.active_limit(i32_of(key, value)?);
        }
        "dont_count_slow_torrents" => {
            pack.dont_count_slow_torrents(bool_of(key, value)?);
        }
        "auto_manage_prefer_seeds" => {
            pack.auto_manage_prefer_seeds(bool_of(key, value)?);
        }
        "rate_limit_ip_overhead" => {
            pack.rate_limit_ip_overhead(bool_of(key, value)?);
        }
        "share_ratio_limit" => {
            pack.share_ratio_limit(ratio_in(key, value)?);
        }
        "seed_time_ratio_limit" => {
            pack.seed_time_ratio_limit(ratio_in(key, value)?);
        }
        "seed_time_limit" => {
            // A value whose conversion to seconds overflows fails
            // rather than silently saturating.
            let seconds = i32_of(key, value)?
                .checked_mul(60)
                .ok_or_else(|| RpcError::call_error(format!("{key} is out of range in seconds")))?;
            pack.seed_time_limit(seconds);
        }
        // Upstream applies the encryption keys as one group whose
        // reapply always sets prefer_rc4 true; the unstaged members
        // keep their session values, which is the same reapply.
        "enc_in_policy" => {
            pack.in_enc_policy(enc_policy_in(key, value)?);
            pack.prefer_rc4(true);
        }
        "enc_out_policy" => {
            pack.out_enc_policy(enc_policy_in(key, value)?);
            pack.prefer_rc4(true);
        }
        "enc_level" => {
            pack.allowed_enc_level(match i32_of(key, value)? {
                0 => EncLevel::PePlaintext,
                1 => EncLevel::PeRc4,
                _ => EncLevel::PeBoth,
            });
            pack.prefer_rc4(true);
        }
        "listen_ports" => {
            // Deluge's [from, to] range: libtorrent listens on a single
            // configured port, so the range's first lands on every
            // non-SSL endpoint of the effective listen interfaces,
            // keeping the configured bind addresses (and any SSL
            // endpoint's own port) intact.
            let port = value
                .get(0)
                .and_then(Value::as_u64)
                .and_then(|p| u16::try_from(p).ok())
                .ok_or_else(|| {
                    RpcError::call_error("listen_ports must be a [from, to] list of ports")
                })?;
            let mut endpoints = engine
                .settings()?
                .get_listen_interfaces_parsed()
                .unwrap_or_default();
            if endpoints.is_empty() {
                endpoints.push(rbtorrent::ListenEndpoint::new("0.0.0.0", port));
                endpoints.push(rbtorrent::ListenEndpoint::new("[::]", port));
            }
            for endpoint in endpoints.iter_mut().filter(|e| !e.ssl) {
                endpoint.port = port;
            }
            pack.listen_interfaces(&endpoints)
                .map_err(|e| RpcError::call_error(e.to_string()))?;
        }
        "proxy" => stage_proxy(pack, value)?,
        _ => return Ok(false),
    }
    Ok(true)
}

fn ratio_in(key: &str, value: &Value) -> Result<i32, RpcError> {
    Ok((f64_of(key, value)? * 100.0) as i32)
}

fn enc_policy_in(key: &str, value: &Value) -> Result<EncPolicy, RpcError> {
    Ok(match i32_of(key, value)? {
        0 => EncPolicy::PeForced,
        1 => EncPolicy::PeEnabled,
        _ => EncPolicy::PeDisabled,
    })
}

fn stage_proxy(pack: &mut SettingsPack, value: &Value) -> Result<(), RpcError> {
    let dict = value
        .as_object()
        .ok_or_else(|| RpcError::call_error("proxy must be an object"))?;
    let get_str = |key: &str| {
        dict.get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned()
    };
    let get_bool =
        |key: &str, default: bool| dict.get(key).and_then(Value::as_bool).unwrap_or(default);
    // `anonymous_mode` rides in the proxy dict but applies even with
    // proxy type none.
    if let Some(value) = dict.get("anonymous_mode") {
        pack.anonymous_mode(bool_of("anonymous_mode", value)?);
    }
    let proxy_type = dict.get("type").and_then(Value::as_i64).unwrap_or(0);
    if proxy_type == 0 {
        pack.proxy(None)
            .map_err(|e| RpcError::call_error(e.to_string()))?;
        return Ok(());
    }
    let auth = || {
        Some(Credentials {
            username: get_str("username"),
            password: get_str("password"),
        })
    };
    let resolve_hostnames = get_bool("proxy_hostnames", true);
    let protocol = match proxy_type {
        1 => ProxyProtocol::Socks4 {
            username: get_str("username"),
        },
        2 | 3 => ProxyProtocol::Socks5 {
            auth: if proxy_type == 3 { auth() } else { None },
            resolve_hostnames,
            udp_send_local_endpoint: false,
        },
        4 | 5 => ProxyProtocol::Http {
            auth: if proxy_type == 5 { auth() } else { None },
            resolve_hostnames,
            send_hostname_in_connect: false,
        },
        other => {
            return Err(RpcError::call_error(format!(
                "unsupported proxy type {other}"
            )));
        }
    };
    let port = dict
        .get("port")
        .and_then(Value::as_u64)
        .and_then(|p| u16::try_from(p).ok())
        .and_then(NonZeroU16::new)
        .ok_or_else(|| RpcError::call_error("proxy.port must be an integer in 1..=65535"))?;
    let config = ProxyConfig {
        protocol,
        host: get_str("hostname"),
        port,
        peer_connections: get_bool("proxy_peer_connections", true),
        tracker_connections: get_bool("proxy_tracker_connections", true),
    };
    pack.proxy(Some(&config))
        .map_err(|e| RpcError::call_error(e.to_string()))?;
    Ok(())
}

async fn get_config(ctx: Ctx) -> HandlerResult {
    positional("get_config", ctx.params, [], [])?;
    let pack = ctx.state.engine.settings()?;
    let mut config = Map::new();
    for key in CONFIG_KEYS {
        if let Some(value) = config_value(&ctx.state.engine, &pack, key) {
            config.insert(key.to_owned(), value);
        }
    }
    ok(Value::Object(config))
}

/// Missing keys read as null.
async fn get_config_value(ctx: Ctx) -> HandlerResult {
    let ([key], []) = positional("get_config_value", ctx.params, ["key"], [])?;
    let key = key
        .as_str()
        .ok_or_else(|| RpcError::call_error("key must be a string"))?;
    let pack = ctx.state.engine.settings()?;
    ok(config_value(&ctx.state.engine, &pack, key).unwrap_or(Value::Null))
}

async fn get_config_values(ctx: Ctx) -> HandlerResult {
    let ([keys], []) = positional("get_config_values", ctx.params, ["keys"], [])?;
    let keys = keys
        .as_array()
        .ok_or_else(|| RpcError::call_error("keys must be a list of strings"))?;
    let pack = ctx.state.engine.settings()?;
    let mut result = Map::new();
    for key in keys {
        let key = key
            .as_str()
            .ok_or_else(|| RpcError::call_error("keys must be a list of strings"))?;
        result.insert(
            key.to_owned(),
            config_value(&ctx.state.engine, &pack, key).unwrap_or(Value::Null),
        );
    }
    ok(Value::Object(result))
}

/// One delta, applied atomically; unsupported keys are skipped.
async fn set_config(ctx: Ctx) -> HandlerResult {
    let ([config], []) = positional("set_config", ctx.params, ["config"], [])?;
    let dict = config
        .as_object()
        .ok_or_else(|| RpcError::call_error("config must be an object"))?;
    let mut pack = SettingsPack::new();
    let mut changed = false;
    for (key, value) in dict {
        changed |= stage_config(&ctx.state.engine, &mut pack, key, value)?;
    }
    if changed {
        ctx.state.engine.apply_settings(&mut pack).await?;
    }
    ok(Value::Null)
}

async fn get_proxy(ctx: Ctx) -> HandlerResult {
    positional("get_proxy", ctx.params, [], [])?;
    let pack = ctx.state.engine.settings()?;
    ok(proxy_value(&pack))
}
