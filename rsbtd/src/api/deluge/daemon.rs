// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The methods that describe the daemon and its own method table:
//! `daemon.*` plus `system.listMethods` over everything registered.

use serde_json::{Value, json};

use super::VERSION;
use super::registry::{Access, Ctx, HandlerResult, Registry, Scope, ok, positional};

pub(super) fn register(r: &mut Registry) {
    r.add(
        "system.listMethods",
        Scope::WebLocal,
        Access::Public,
        list_methods,
    );
    r.add(
        "daemon.authorized_call",
        Scope::Daemon,
        Access::Normal,
        authorized_call,
    );
    r.add(
        "daemon.get_method_list",
        Scope::Daemon,
        Access::Normal,
        get_method_list,
    );
    r.add(
        "daemon.get_version",
        Scope::Daemon,
        Access::Normal,
        get_version,
    );
    r.add("daemon.shutdown", Scope::Daemon, Access::Normal, shutdown);
}

/// Both scopes: the fake daemon is always connected.
async fn list_methods(ctx: Ctx) -> HandlerResult {
    positional("listMethods", ctx.params, [], [])?;
    let methods: Vec<_> = ctx.state.registry.names(None).collect();
    ok(json!(methods))
}

/// True exactly for registered daemon-scope methods — sessions hold the
/// single NORMAL level. Unknown names are false, never an error.
async fn authorized_call(ctx: Ctx) -> HandlerResult {
    let ([rpc], []) = positional("authorized_call", ctx.params, ["rpc"], [])?;
    let authorized = rpc
        .as_str()
        .and_then(|name| ctx.state.registry.get(name))
        .is_some_and(|method| method.scope == Scope::Daemon);
    ok(json!(authorized))
}

async fn get_method_list(ctx: Ctx) -> HandlerResult {
    positional("get_method_list", ctx.params, [], [])?;
    let methods: Vec<_> = ctx.state.registry.names(Some(Scope::Daemon)).collect();
    ok(json!(methods))
}

async fn get_version(ctx: Ctx) -> HandlerResult {
    positional("get_version", ctx.params, [], [])?;
    ok(json!(VERSION))
}

/// No-op: remote shutdown is not something rsbtd offers.
async fn shutdown(ctx: Ctx) -> HandlerResult {
    positional("shutdown", ctx.params, [], [])?;
    ok(Value::Null)
}
