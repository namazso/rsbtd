// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The web-UI shell: what deluge-web answers out of its own process
//! rather than from a daemon — the connection manager, the `web.conf`
//! settings, the event long-poll, and the `webutils.*` frontend assets.
//!
//! The connection manager pretends a single always-connected host exists
//! at 127.0.0.1:58846, rsbtd being its own daemon, and every operation
//! on the host list fails softly. rsbtd ships no Deluge frontend, so
//! there are no translations or themes to offer.

use std::time::Duration;

use serde_json::{Value, json};

use super::proto::RpcError;
use super::registry::{Access, Ctx, HandlerResult, Registry, Scope, ok, positional};
use super::{HOST_ID, VERSION};

pub(super) fn register(r: &mut Registry) {
    use Access::Normal;
    use Scope::WebLocal;
    r.add("web.connected", WebLocal, Normal, connected);
    r.add("web.connect", WebLocal, Normal, connect);
    r.add("web.disconnect", WebLocal, Normal, disconnect);
    r.add("web.get_hosts", WebLocal, Normal, get_hosts);
    r.add("web.get_host_status", WebLocal, Normal, get_host_status);
    r.add("web.add_host", WebLocal, Normal, add_host);
    r.add("web.edit_host", WebLocal, Normal, edit_host);
    r.add("web.remove_host", WebLocal, Normal, remove_host);
    r.add("web.start_daemon", WebLocal, Normal, start_daemon);
    r.add("web.stop_daemon", WebLocal, Normal, stop_daemon);
    r.add("web.get_config", WebLocal, Normal, get_config);
    r.add("web.set_config", WebLocal, Normal, set_config);
    r.add(
        "web.register_event_listener",
        WebLocal,
        Normal,
        register_event_listener,
    );
    r.add(
        "web.deregister_event_listener",
        WebLocal,
        Normal,
        deregister_event_listener,
    );
    r.add("web.get_events", WebLocal, Normal, get_events);
    r.add("web.set_theme", WebLocal, Normal, set_theme);
    r.add("webutils.get_languages", WebLocal, Normal, get_languages);
    r.add("webutils.get_themes", WebLocal, Normal, get_themes);
}

async fn connected(ctx: Ctx) -> HandlerResult {
    positional("connected", ctx.params, [], [])?;
    ok(json!(true))
}

/// The one known host yields the daemon-scope method list; any other
/// resolves to null, the way a failed connect does.
async fn connect(ctx: Ctx) -> HandlerResult {
    let ([host_id], []) = positional("connect", ctx.params, ["host_id"], [])?;
    if host_id.as_str() != Some(HOST_ID) {
        return ok(Value::Null);
    }
    let methods: Vec<_> = ctx.state.registry.names(Some(Scope::Daemon)).collect();
    ok(json!(methods))
}

/// No-op: `web.connected` stays true.
async fn disconnect(ctx: Ctx) -> HandlerResult {
    positional("disconnect", ctx.params, [], [])?;
    ok(json!("Connection was closed cleanly."))
}

async fn get_hosts(ctx: Ctx) -> HandlerResult {
    positional("get_hosts", ctx.params, [], [])?;
    ok(json!([[HOST_ID, "127.0.0.1", 58846, ""]]))
}

/// The one known host is always "Connected", this daemon being it;
/// anything else is "Offline".
async fn get_host_status(ctx: Ctx) -> HandlerResult {
    let ([host_id], []) = positional("get_host_status", ctx.params, ["host_id"], [])?;
    if host_id.as_str() == Some(HOST_ID) {
        ok(json!([HOST_ID, "Connected", VERSION]))
    } else {
        ok(json!([host_id, "Offline", ""]))
    }
}

/// The host list is fixed; answer with the duplicate-host failure.
async fn add_host(ctx: Ctx) -> HandlerResult {
    positional(
        "add_host",
        ctx.params,
        ["host", "port"],
        [("username", json!("")), ("password", json!(""))],
    )?;
    ok(json!([false, "Host details already in hostlist"]))
}

async fn edit_host(ctx: Ctx) -> HandlerResult {
    positional(
        "edit_host",
        ctx.params,
        ["host_id", "host", "port"],
        [("username", json!("")), ("password", json!(""))],
    )?;
    ok(json!(false))
}

async fn remove_host(ctx: Ctx) -> HandlerResult {
    positional("remove_host", ctx.params, ["host_id"], [])?;
    ok(json!(false))
}

/// No-op: rsbtd is the daemon and it is already running.
async fn start_daemon(ctx: Ctx) -> HandlerResult {
    positional("start_daemon", ctx.params, ["port"], [])?;
    ok(Value::Null)
}

async fn stop_daemon(ctx: Ctx) -> HandlerResult {
    positional("stop_daemon", ctx.params, ["host_id"], [])?;
    ok(Value::Null)
}

/// rsbtd stores no `web.conf`, so: the defaults, `default_daemon`
/// pointing at the fake host so clients auto-connect, wizard done.
async fn get_config(ctx: Ctx) -> HandlerResult {
    positional("get_config", ctx.params, [], [])?;
    ok(json!({
        "base": "/",
        "cert": "ssl/daemon.cert",
        "default_daemon": HOST_ID,
        "enabled_plugins": [],
        "first_login": false,
        "https": false,
        "interface": "0.0.0.0",
        "language": "",
        "pkey": "ssl/daemon.pkey",
        "port": 8112,
        "session_timeout": 3600,
        "show_session_speed": false,
        "show_sidebar": true,
        "sidebar_multiple_filters": true,
        "sidebar_show_zero": false,
        "theme": "gray",
    }))
}

/// Accepted and dropped: there is no web.conf to write to.
async fn set_config(ctx: Ctx) -> HandlerResult {
    let ([config], []) = positional("set_config", ctx.params, ["config"], [])?;
    if !config.is_object() {
        return Err(RpcError::call_error("config must be an object"));
    }
    ok(Value::Null)
}

async fn register_event_listener(ctx: Ctx) -> HandlerResult {
    positional("register_event_listener", ctx.params, ["event"], [])?;
    ok(Value::Null)
}

async fn deregister_event_listener(ctx: Ctx) -> HandlerResult {
    positional("deregister_event_listener", ctx.params, ["event"], [])?;
    ok(Value::Null)
}

/// Nothing ever arrives, but the ~5 s long-poll pause is kept so a
/// polling client paces itself on the server block instead of spinning.
async fn get_events(ctx: Ctx) -> HandlerResult {
    positional("get_events", ctx.params, [], [])?;
    tokio::time::sleep(Duration::from_secs(5)).await;
    ok(Value::Null)
}

/// No themes are served, and an unknown one quietly falls back.
async fn set_theme(ctx: Ctx) -> HandlerResult {
    positional("set_theme", ctx.params, ["theme"], [])?;
    ok(Value::Null)
}

async fn get_languages(ctx: Ctx) -> HandlerResult {
    positional("get_languages", ctx.params, [], [])?;
    ok(json!([]))
}

async fn get_themes(ctx: Ctx) -> HandlerResult {
    positional("get_themes", ctx.params, [], [])?;
    ok(json!([]))
}
