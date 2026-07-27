// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The plugin methods of both halves, `core.*` and `web.*`: rsbtd has
//! no plugin system, so the lists are empty and every plugin looks like
//! one that cannot be found. Enabling or uploading reports failure as
//! `false` rather than an error; disabling an already-disabled plugin
//! succeeds.

use serde_json::{Value, json};

use super::registry::{Access, Ctx, HandlerResult, Registry, Scope, ok, positional};

pub(super) fn register(r: &mut Registry) {
    use Access::Normal;
    use Scope::{Daemon, WebLocal};
    r.add(
        "core.get_available_plugins",
        Daemon,
        Normal,
        get_available_plugins,
    );
    r.add(
        "core.get_enabled_plugins",
        Daemon,
        Normal,
        get_enabled_plugins,
    );
    r.add("core.enable_plugin", Daemon, Normal, enable_plugin);
    r.add("core.disable_plugin", Daemon, Normal, disable_plugin);
    r.add("core.rescan_plugins", Daemon, Normal, rescan_plugins);
    r.add("core.upload_plugin", Daemon, Normal, core_upload_plugin);
    r.add("web.get_plugins", WebLocal, Normal, get_plugins);
    r.add("web.get_plugin_info", WebLocal, Normal, get_plugin_info);
    r.add(
        "web.get_plugin_resources",
        WebLocal,
        Normal,
        get_plugin_resources,
    );
    r.add("web.upload_plugin", WebLocal, Normal, web_upload_plugin);
}

async fn get_available_plugins(ctx: Ctx) -> HandlerResult {
    positional("get_available_plugins", ctx.params, [], [])?;
    ok(json!([]))
}

async fn get_enabled_plugins(ctx: Ctx) -> HandlerResult {
    positional("get_enabled_plugins", ctx.params, [], [])?;
    ok(json!([]))
}

async fn enable_plugin(ctx: Ctx) -> HandlerResult {
    positional("enable_plugin", ctx.params, ["plugin"], [])?;
    ok(json!(false))
}

async fn disable_plugin(ctx: Ctx) -> HandlerResult {
    positional("disable_plugin", ctx.params, ["plugin"], [])?;
    ok(json!(true))
}

async fn rescan_plugins(ctx: Ctx) -> HandlerResult {
    positional("rescan_plugins", ctx.params, [], [])?;
    ok(Value::Null)
}

/// The daemon-side upload returns nothing, the web-side a success flag.
async fn core_upload_plugin(ctx: Ctx) -> HandlerResult {
    positional("upload_plugin", ctx.params, ["filename", "filedump"], [])?;
    ok(Value::Null)
}

async fn get_plugins(ctx: Ctx) -> HandlerResult {
    positional("get_plugins", ctx.params, [], [])?;
    ok(json!({
        "available_plugins": [],
        "enabled_plugins": [],
    }))
}

async fn get_plugin_info(ctx: Ctx) -> HandlerResult {
    positional("get_plugin_info", ctx.params, ["name"], [])?;
    ok(json!({
        "Name": "not available",
        "Version": "not available",
        "Author": "",
        "Author-email": "",
        "Description": "",
        "Home-page": "",
        "License": "",
        "Platform": "",
        "Summary": "",
    }))
}

/// Null is the answer for a plugin without a web UI.
async fn get_plugin_resources(ctx: Ctx) -> HandlerResult {
    positional("get_plugin_resources", ctx.params, ["name"], [])?;
    ok(Value::Null)
}

async fn web_upload_plugin(ctx: Ctx) -> HandlerResult {
    positional("upload_plugin", ctx.params, ["filename", "path"], [])?;
    ok(json!(false))
}
