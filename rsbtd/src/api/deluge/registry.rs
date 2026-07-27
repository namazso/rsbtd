// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The method registry — name, scope, and access metadata over plain
//! `async fn` handlers — plus the positional-parameter splitter.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use serde_json::Value;

use super::DelugeState;
use super::proto::RpcError;

/// `daemon.*` and `core.*` are daemon-scope, the rest deluge-web's own.
/// Both are served, but `daemon.get_method_list` and
/// `daemon.authorized_call` report daemon scope only.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Scope {
    Daemon,
    WebLocal,
}

/// Who may call a method: everyone, or only a valid session. Sessions
/// have the single NORMAL level — rsbtd has no user accounts.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Access {
    Public,
    Normal,
}

/// What a handler receives. `authed` is already enforced for
/// [`Access::Normal`] methods; the public `auth.check_session` reads it.
pub struct Ctx {
    pub state: Arc<DelugeState>,
    pub params: Vec<Value>,
    pub authed: bool,
    /// [`cross_site`](super::proto::cross_site) for this request: the
    /// session cookie needs its own attributes for such a caller.
    pub cross_site: bool,
}

/// The result value plus an optional `Set-Cookie` header value.
pub struct Reply {
    pub result: Value,
    pub set_cookie: Option<String>,
}

impl From<Value> for Reply {
    fn from(result: Value) -> Reply {
        Reply {
            result,
            set_cookie: None,
        }
    }
}

pub type HandlerResult = Result<Reply, RpcError>;

pub fn ok(result: Value) -> HandlerResult {
    Ok(result.into())
}

type BoxedFuture = Pin<Box<dyn Future<Output = HandlerResult> + Send>>;
type Handler = Box<dyn Fn(Ctx) -> BoxedFuture + Send + Sync>;

pub struct Method {
    pub name: &'static str,
    pub scope: Scope,
    pub access: Access,
    handler: Handler,
}

impl Method {
    pub fn call(&self, ctx: Ctx) -> BoxedFuture {
        (self.handler)(ctx)
    }
}

/// Vec-backed: the surface is small and listed about as often as called.
#[derive(Default)]
pub struct Registry {
    methods: Vec<Method>,
}

impl Registry {
    /// Panics on a duplicate name, at daemon startup.
    pub fn add<F, Fut>(&mut self, name: &'static str, scope: Scope, access: Access, handler: F)
    where
        F: Fn(Ctx) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = HandlerResult> + Send + 'static,
    {
        assert!(self.get(name).is_none(), "duplicate deluge method {name}");
        self.methods.push(Method {
            name,
            scope,
            access,
            handler: Box::new(move |ctx| Box::pin(handler(ctx))),
        });
    }

    pub fn get(&self, name: &str) -> Option<&Method> {
        self.methods.iter().find(|m| m.name == name)
    }

    pub fn names(&self, scope: Option<Scope>) -> impl Iterator<Item = &'static str> + '_ {
        self.methods
            .iter()
            .filter(move |m| scope.is_none_or(|s| m.scope == s))
            .map(|m| m.name)
    }
}

/// Splits positional `params` into required and optional values, filling
/// absent optionals from their defaults; wrong arity is a code-3 error
/// naming `leaf`. Parameter names only document the signature at the
/// call site, and values stay untyped — handlers coerce them.
pub fn positional<const R: usize, const O: usize>(
    leaf: &str,
    params: Vec<Value>,
    required: [&'static str; R],
    optional: [(&'static str, Value); O],
) -> Result<([Value; R], [Value; O]), RpcError> {
    if params.len() < R || params.len() > R + O {
        let expected = if O == 0 {
            format!("{R}")
        } else {
            format!("{R} to {}", R + O)
        };
        return Err(RpcError::call_error(format!(
            "{leaf}() takes {expected} argument(s) ({} given)",
            params.len()
        )));
    }
    let mut params = params.into_iter();
    let required = required.map(|_name| params.next().expect("length checked above"));
    let optional = optional.map(|(_name, default)| params.next().unwrap_or(default));
    Ok((required, optional))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    async fn dummy(_ctx: Ctx) -> HandlerResult {
        ok(Value::Null)
    }

    fn sample() -> Registry {
        let mut r = Registry::default();
        r.add("daemon.a", Scope::Daemon, Access::Normal, dummy);
        r.add("web.b", Scope::WebLocal, Access::Normal, dummy);
        r.add("daemon.c", Scope::Daemon, Access::Public, dummy);
        r
    }

    #[test]
    fn lookup_and_scoped_names() {
        let r = sample();
        assert_eq!(r.get("web.b").unwrap().access, Access::Normal);
        assert!(r.get("web.nope").is_none());
        let daemon: Vec<_> = r.names(Some(Scope::Daemon)).collect();
        assert_eq!(daemon, ["daemon.a", "daemon.c"]);
        let all: Vec<_> = r.names(None).collect();
        assert_eq!(all, ["daemon.a", "web.b", "daemon.c"]);
    }

    #[test]
    #[should_panic(expected = "duplicate deluge method")]
    fn duplicate_names_panic() {
        let mut r = sample();
        r.add("daemon.a", Scope::Daemon, Access::Normal, dummy);
    }

    #[test]
    fn positional_splits_and_defaults() {
        let ([a], [b, c]) = positional(
            "m",
            vec![json!(1), json!(2)],
            ["a"],
            [("b", json!("x")), ("c", json!("y"))],
        )
        .unwrap();
        assert_eq!((a, b, c), (json!(1), json!(2), json!("y")));

        let ([], []) = positional("m", vec![], [], []).unwrap();
    }

    #[test]
    fn positional_rejects_wrong_arity() {
        let err = positional("login", vec![], ["password"], []).unwrap_err();
        assert_eq!(err.code, 3);
        assert_eq!(err.message, "login() takes 1 argument(s) (0 given)");

        let err = positional("m", vec![json!(1), json!(2)], [], [("o", json!(0))]).unwrap_err();
        assert_eq!(err.code, 3);
        assert_eq!(err.message, "m() takes 0 to 1 argument(s) (2 given)");
    }
}
