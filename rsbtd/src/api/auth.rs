// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Static bearer-token authentication.
//!
//! The daemon has exactly one credential: an optional static token from the
//! config file. Comparison is constant-time over SHA-256 digests, so
//! neither the comparison time nor its length depends on the supplied
//! token.

use std::sync::Arc;

use axum::extract::{Request, State};
use axum::http::{HeaderValue, Method, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

/// The API's authentication configuration.
pub struct Auth {
    /// SHA-256 of the configured token; `None` means the API is open.
    token_hash: Option<[u8; 32]>,
}

impl Auth {
    pub fn new(token: Option<&str>) -> Auth {
        Auth {
            token_hash: token.map(|t| Sha256::digest(t).into()),
        }
    }

    /// Whether `token` matches the configured token (or none is required).
    pub fn check_token(&self, token: &str) -> bool {
        match &self.token_hash {
            None => true,
            Some(expected) => {
                let supplied: [u8; 32] = Sha256::digest(token).into();
                supplied.ct_eq(expected).into()
            }
        }
    }

    /// Whether an `Authorization` header value satisfies the config.
    pub fn check_authorization(&self, header: Option<&str>) -> bool {
        if self.token_hash.is_none() {
            return true;
        }
        let Some(value) = header else { return false };
        let Some(token) = value.strip_prefix("Bearer ") else {
            return false;
        };
        self.check_token(token)
    }
}

/// Axum middleware rejecting unauthorized requests with 401.
///
/// GET WebSocket upgrades pass through: browsers cannot set headers on
/// WebSockets, so graphql-ws connections authenticate via their
/// `connection_init` payload instead (see `api::graphql_ws`).
pub async fn require_bearer(
    State(auth): State<Arc<Auth>>,
    request: Request,
    next: Next,
) -> Response {
    let is_ws_upgrade = request.method() == Method::GET
        && request
            .headers()
            .get(header::UPGRADE)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v.eq_ignore_ascii_case("websocket"));
    let header = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok());
    if is_ws_upgrade || auth.check_authorization(header) {
        next.run(request).await
    } else {
        (
            StatusCode::UNAUTHORIZED,
            [(header::WWW_AUTHENTICATE, HeaderValue::from_static("Bearer"))],
        )
            .into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_token_accepts_everything() {
        let auth = Auth::new(None);
        assert!(auth.check_authorization(None));
        assert!(auth.check_authorization(Some("Bearer whatever")));
        assert!(auth.check_token("anything"));
    }

    #[test]
    fn token_is_enforced() {
        let auth = Auth::new(Some("s3cret"));
        assert!(auth.check_authorization(Some("Bearer s3cret")));
        assert!(!auth.check_authorization(Some("Bearer wrong")));
        assert!(!auth.check_authorization(Some("Bearer s3cret ")));
        assert!(!auth.check_authorization(Some("Bearer s3cretbutlonger")));
        assert!(!auth.check_authorization(Some("s3cret")));
        assert!(!auth.check_authorization(Some("Basic s3cret")));
        assert!(!auth.check_authorization(Some("bearer s3cret")));
        assert!(!auth.check_authorization(None));
    }
}
