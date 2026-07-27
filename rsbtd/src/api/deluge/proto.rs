// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Wire types: the request and response envelopes, the numbered error
//! object, and session-cookie extraction.

use axum::http::HeaderMap;
use axum::http::header::{COOKIE, HOST, ORIGIN};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Anything that does not fit this shape (undecodable JSON, missing key,
/// wrong key type) is answered with a code-5 [`RpcError`].
#[derive(Deserialize)]
pub struct RpcRequest {
    pub method: String,
    pub params: Vec<Value>,
    /// Echoed verbatim in the response; may be any JSON value.
    pub id: Value,
}

/// The numeric codes are Deluge's — clients branch on them (1 triggers
/// a re-login) — while the message texts are ours.
#[derive(Debug, Serialize)]
pub struct RpcError {
    pub message: String,
    pub code: u8,
}

impl RpcError {
    pub fn not_authenticated() -> RpcError {
        RpcError {
            message: "Not authenticated".into(),
            code: 1,
        }
    }

    pub fn unknown_method() -> RpcError {
        RpcError {
            message: "Unknown method".into(),
            code: 2,
        }
    }

    /// Code 3: the call itself failed (wrong arity, handler error).
    pub fn call_error(message: impl Into<String>) -> RpcError {
        RpcError {
            message: message.into(),
            code: 3,
        }
    }

    /// Code 5: undecodable envelope, answered with a null `id`.
    pub fn malformed(message: impl Into<String>) -> RpcError {
        RpcError {
            message: message.into(),
            code: 5,
        }
    }
}

impl From<crate::engine::EngineError> for RpcError {
    fn from(e: crate::engine::EngineError) -> RpcError {
        RpcError::call_error(e.to_string())
    }
}

/// Exactly one of `result`/`error` is non-null, and all three keys are
/// always present.
#[derive(Serialize)]
pub struct Envelope {
    pub result: Value,
    pub error: Option<RpcError>,
    pub id: Value,
}

impl Envelope {
    pub fn ok(result: Value, id: Value) -> Envelope {
        Envelope {
            result,
            error: None,
            id,
        }
    }

    pub fn err(error: RpcError, id: Value) -> Envelope {
        Envelope {
            result: Value::Null,
            error: Some(error),
            id,
        }
    }
}

pub fn session_cookie(headers: &HeaderMap) -> Option<&str> {
    headers
        .get_all(COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(';'))
        .find_map(|pair| pair.trim().strip_prefix("_session_id="))
}

/// Whether the request came from an `https` origin other than the host
/// it was addressed to — the credentialed-CORS case, where a browser
/// keeps the session cookie only if it is `SameSite=None`, which in
/// turn only holds with `Secure`. A plain-http origin is not treated as
/// one: it could not store such a cookie at all.
pub fn cross_site(headers: &HeaderMap) -> bool {
    let Some(origin) = headers.get(ORIGIN).and_then(|value| value.to_str().ok()) else {
        return false;
    };
    let Some(authority) = origin.strip_prefix("https://") else {
        return false;
    };
    headers.get(HOST).and_then(|value| value.to_str().ok()) != Some(authority)
}

#[cfg(test)]
mod tests {
    use axum::http::HeaderValue;
    use serde_json::json;

    use super::*;

    #[test]
    fn envelopes_serialize_with_all_keys() {
        let ok = Envelope::ok(json!(true), json!(7));
        assert_eq!(
            serde_json::to_string(&ok).unwrap(),
            r#"{"result":true,"error":null,"id":7}"#
        );
        let err = Envelope::err(RpcError::unknown_method(), json!({"a": 1}));
        assert_eq!(
            serde_json::to_string(&err).unwrap(),
            r#"{"result":null,"error":{"message":"Unknown method","code":2},"id":{"a":1}}"#
        );
    }

    #[test]
    fn requests_parse_or_reject() {
        let req: RpcRequest =
            serde_json::from_str(r#"{"method":"auth.login","params":["x"],"id":null}"#).unwrap();
        assert_eq!(req.method, "auth.login");
        assert_eq!(req.params, vec![json!("x")]);
        assert_eq!(req.id, Value::Null);
        // Unknown extra keys are ignored.
        serde_json::from_str::<RpcRequest>(r#"{"method":"m","params":[],"id":1,"extra":2}"#)
            .unwrap();
        for bad in [
            "not json",
            r#"{"params":[],"id":1}"#,              // no method
            r#"{"method":"m","id":1}"#,             // no params
            r#"{"method":"m","params":[]}"#,        // no id
            r#"{"method":1,"params":[],"id":1}"#,   // method not a string
            r#"{"method":"m","params":{},"id":1}"#, // params not an array
            "[1,2,3]",
        ] {
            assert!(
                serde_json::from_str::<RpcRequest>(bad).is_err(),
                "accepted {bad:?}"
            );
        }
    }

    #[test]
    fn session_cookie_is_found_among_cookies() {
        let mut headers = HeaderMap::new();
        assert_eq!(session_cookie(&headers), None);
        headers.insert(COOKIE, HeaderValue::from_static("other=1"));
        assert_eq!(session_cookie(&headers), None);
        // A prefixed name must not match.
        headers.insert(COOKIE, HeaderValue::from_static("x_session_id=nope"));
        assert_eq!(session_cookie(&headers), None);
        headers.insert(
            COOKIE,
            HeaderValue::from_static("other=1;  _session_id=tok ;more=2"),
        );
        assert_eq!(session_cookie(&headers), Some("tok"));
        // Multiple Cookie headers are all searched.
        let mut headers = HeaderMap::new();
        headers.append(COOKIE, HeaderValue::from_static("a=b"));
        headers.append(COOKIE, HeaderValue::from_static("_session_id=tok2"));
        assert_eq!(session_cookie(&headers), Some("tok2"));
    }

    #[test]
    fn cross_site_needs_another_https_origin() {
        let mut headers = HeaderMap::new();
        headers.insert(HOST, HeaderValue::from_static("api.example.net"));
        assert!(!cross_site(&headers));
        for (origin, want) in [
            ("https://ui.example.com", true),
            ("https://api.example.net", false),
            ("http://ui.example.com", false),
        ] {
            headers.insert(ORIGIN, HeaderValue::from_str(origin).unwrap());
            assert_eq!(cross_site(&headers), want, "{origin}");
        }
    }
}
