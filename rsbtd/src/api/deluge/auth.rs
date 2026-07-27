// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Sessions against the rsbtd API token. There is no session store —
//! `auth.login` proves the client knows the token and echoes it back
//! base64url-encoded as the `_session_id` cookie, so a valid cookie
//! *is* the token, and nothing can revoke one. The encoding is what
//! keeps a token holding cookie delimiters (a `;`, say) usable.

use axum::http::HeaderMap;
use base64::Engine as _;
use serde_json::{Value, json};

use super::proto::session_cookie;
use super::registry::{Access, Ctx, HandlerResult, Registry, Reply, Scope, ok, positional};
use crate::api::auth::Auth;

const SESSION_ENCODING: base64::engine::general_purpose::GeneralPurpose =
    base64::engine::general_purpose::URL_SAFE_NO_PAD;

/// A cookie matching the API token, or no cookie while the API is open
/// — mirroring bearer auth, where a missing header passes only when no
/// token is configured.
pub(super) fn session_ok(auth: &Auth, headers: &HeaderMap) -> bool {
    match session_cookie(headers) {
        // An undecodable cookie is just a wrong token, which an open
        // API still accepts.
        Some(cookie) => auth.check_token(&decode_session(cookie).unwrap_or_default()),
        None => auth.check_authorization(None),
    }
}

fn decode_session(cookie: &str) -> Option<String> {
    String::from_utf8(SESSION_ENCODING.decode(cookie).ok()?).ok()
}

/// The `Set-Cookie` value carrying `session`, with the attributes this
/// caller can store it under (see
/// [`cross_site`](super::proto::cross_site)).
fn cookie_header(session: &str, cross_site: bool) -> String {
    let attributes = if cross_site {
        "; SameSite=None; Secure"
    } else {
        ""
    };
    format!("_session_id={session}; Path=/; HttpOnly{attributes}")
}

pub(super) fn register(r: &mut Registry) {
    r.add("auth.login", Scope::WebLocal, Access::Public, login);
    r.add(
        "auth.check_session",
        Scope::WebLocal,
        Access::Public,
        check_session,
    );
    r.add(
        "auth.delete_session",
        Scope::WebLocal,
        Access::Normal,
        delete_session,
    );
    r.add(
        "auth.change_password",
        Scope::WebLocal,
        Access::Normal,
        change_password,
    );
}

/// With no token configured, any password (and so any later cookie) is
/// accepted.
async fn login(ctx: Ctx) -> HandlerResult {
    let ([password], []) = positional("login", ctx.params, ["password"], [])?;
    // A non-string password is simply a wrong one.
    let Some(password) = password.as_str() else {
        return ok(json!(false));
    };
    if !ctx.state.auth.check_token(password) {
        return ok(json!(false));
    }
    Ok(Reply {
        result: json!(true),
        set_cookie: Some(cookie_header(
            &SESSION_ENCODING.encode(password),
            ctx.cross_site,
        )),
    })
}

/// The optional `session_id` parameter is ignored; the cookie counts.
async fn check_session(ctx: Ctx) -> HandlerResult {
    let ([], [_session_id]) = positional(
        "check_session",
        ctx.params,
        [],
        [("session_id", Value::Null)],
    )?;
    ok(json!(ctx.authed))
}

/// Expires the caller's cookie, which logs out a well-behaved client.
async fn delete_session(ctx: Ctx) -> HandlerResult {
    positional("delete_session", ctx.params, [], [])?;
    Ok(Reply {
        result: json!(true),
        set_cookie: Some(format!(
            "{}; Max-Age=0; Expires=Thu, 01 Jan 1970 00:00:00 GMT",
            cookie_header("", ctx.cross_site)
        )),
    })
}

/// The "Web UI password" is the API token, unchangeable at runtime.
async fn change_password(ctx: Ctx) -> HandlerResult {
    positional(
        "change_password",
        ctx.params,
        ["old_password", "new_password"],
        [],
    )?;
    ok(json!(false))
}
