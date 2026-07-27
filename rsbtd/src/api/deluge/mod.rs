// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! A best-effort Deluge-compatible JSON API on `POST /json`.
//!
//! Speaks the deluge-web JSON protocol (a JSON-RPC 1.0 dialect):
//! request `{"method", "params", "id"}`, response
//! `{"result", "error", "id"}`, always HTTP 200, numeric error codes.
//! An `application/json` content type is required, and that check is
//! load-bearing: it forces cross-origin browser calls through a CORS
//! preflight, so a CORS-safelisted POST cannot mutate anything with the
//! session cookie attached (CSRF).
//!
//! The deluge-web and daemon method surfaces are merged: the endpoint
//! acts as a web UI permanently connected to the one fake host
//! [`HOST_ID`], rsbtd itself. Modules are cut by subject rather than
//! namespace, so a `web.*` method sits with the machinery it wraps.
//! A torrent's Deluge id is its rsbtd uuid string, so v2-only torrents
//! need no info-hash bridging.

pub mod auth;
pub mod config;
pub mod daemon;
pub mod filetree;
pub mod plugins;
pub mod proto;
pub mod registry;
pub mod session;
pub mod status;
pub mod torrents;
pub mod values;
pub mod webui;

use std::sync::Arc;

use axum::Router;
use axum::extract::{Request, State};
use axum::http::{HeaderMap, HeaderValue, header};
use axum::response::{IntoResponse, Json, Response};
use serde_json::Value;
use tokio_util::sync::CancellationToken;

use crate::engine::Engine;
use proto::{Envelope, RpcError, RpcRequest};
use registry::{Access, Ctx, HandlerResult, Registry};

/// Fixed, so `web.get_hosts` → `web.connect` flows survive restarts.
pub const HOST_ID: &str = "0d5190f4991f4d5eb2ae9e34e0c11d63";

/// Body cap for a request without a session. The public methods take no
/// more than a password, and the full [`BODY_LIMIT`](super::BODY_LIMIT)
/// would let a stranger make the daemon buffer and parse tens of MiB of
/// JSON before the session check rejects the call.
const PUBLIC_BODY_LIMIT: usize = 64 * 1024;

const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Deliberately stateless beyond the registry table: every call is
/// answered from live engine state.
pub struct DelugeState {
    pub engine: Arc<Engine>,
    pub auth: Arc<crate::api::auth::Auth>,
    pub registry: Registry,
}

fn build_registry() -> Registry {
    let mut r = Registry::default();
    auth::register(&mut r);
    config::register(&mut r);
    daemon::register(&mut r);
    filetree::register(&mut r);
    plugins::register(&mut r);
    session::register(&mut r);
    status::register(&mut r);
    torrents::register(&mut r);
    webui::register(&mut r);
    r
}

/// The Deluge endpoint sub-router (`POST /json`). Bearer auth does not
/// apply: the protocol authenticates per call via the session cookie.
pub fn router(
    engine: Arc<Engine>,
    auth: Arc<crate::api::auth::Auth>,
    req_kill: CancellationToken,
) -> Router {
    let state = Arc::new(DelugeState {
        engine,
        auth,
        registry: build_registry(),
    });
    Router::new()
        .route("/json", axum::routing::post(handle_json))
        .with_state(state)
        .route_layer(axum::middleware::from_fn_with_state(
            req_kill,
            super::shutdown_503,
        ))
}

/// The body is read (and capped) here rather than by an extractor, so
/// that an oversized one is answered in the envelope like every other
/// bad request instead of with a bare 413. The session decides the cap,
/// so it is checked before anything is read.
async fn handle_json(State(state): State<Arc<DelugeState>>, request: Request) -> Response {
    let (parts, body) = request.into_parts();
    let headers = parts.headers;
    // See the module docs for why this check must not be relaxed.
    let content_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("");
    let media_type = content_type.split(';').next().unwrap_or("").trim();
    if !media_type.eq_ignore_ascii_case("application/json") {
        let error = RpcError::malformed(format!("invalid request content-type: {content_type}"));
        return respond(Envelope::err(error, Value::Null), None);
    }
    let authed = auth::session_ok(&state.auth, &headers);
    let limit = if authed {
        super::BODY_LIMIT
    } else {
        PUBLIC_BODY_LIMIT
    };
    let Ok(body) = axum::body::to_bytes(body, limit).await else {
        let error = RpcError::malformed(format!(
            "request body is unreadable or larger than {limit} bytes"
        ));
        return respond(Envelope::err(error, Value::Null), None);
    };
    let request = match serde_json::from_slice::<RpcRequest>(&body) {
        Ok(request) => request,
        Err(e) => {
            let error = RpcError::malformed(format!("invalid request: {e}"));
            return respond(Envelope::err(error, Value::Null), None);
        }
    };
    let RpcRequest { method, params, id } = request;
    tracing::debug!("deluge json call: {method}");
    match dispatch(&state, &headers, authed, &method, params).await {
        Ok(reply) => respond(Envelope::ok(reply.result, id), reply.set_cookie),
        Err(error) => respond(Envelope::err(error, id), None),
    }
}

/// Method lookup (unknown → code 2, checked before the session so
/// absent methods do not need auth), session check (code 1), handler.
async fn dispatch(
    state: &Arc<DelugeState>,
    headers: &HeaderMap,
    authed: bool,
    method: &str,
    params: Vec<Value>,
) -> HandlerResult {
    let Some(entry) = state.registry.get(method) else {
        return Err(RpcError::unknown_method());
    };
    if entry.access == Access::Normal && !authed {
        return Err(RpcError::not_authenticated());
    }
    let ctx = Ctx {
        state: Arc::clone(state),
        params,
        authed,
        cross_site: proto::cross_site(headers),
    };
    entry.call(ctx).await
}

/// Session cookies are base64url plus fixed attributes, so building the
/// header value cannot fail.
fn respond(envelope: Envelope, set_cookie: Option<String>) -> Response {
    let mut response = Json(envelope).into_response();
    if let Some(value) = set_cookie
        .as_deref()
        .and_then(|cookie| HeaderValue::from_str(cookie).ok())
    {
        response.headers_mut().append(header::SET_COOKIE, value);
    }
    response
}
