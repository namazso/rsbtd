// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The HTTP + GraphQL API layer.
//!
//! Routes: `POST /graphql` (bearer-authenticated queries/mutations),
//! `GET /graphql` (graphql-ws subscriptions, token via `connection_init`),
//! `GET /healthz` (unauthenticated liveness), and on `GET /` either
//! GraphiQL or a static web UI directory (`serve_root`), when enabled in
//! the config. The `cors` config option allows listed origins to call the
//! API from a web UI served elsewhere.

pub mod auth;
pub mod events;
pub mod listener;
pub mod mutation;
pub mod query;
pub mod scalars;
pub mod settings;
pub mod subscription;
pub mod types;

use std::sync::Arc;
use std::time::Duration;

use async_graphql::futures_util::{SinkExt as _, StreamExt as _};
use async_graphql::http::WebSocketProtocols;
use async_graphql::{Data, Schema};
use async_graphql_axum::{GraphQL, GraphQLProtocol, GraphQLWebSocket};
use axum::Router;
use axum::extract::ws::{CloseFrame, Message, close_code};
use axum::extract::{State, WebSocketUpgrade};
use axum::http::header::{AUTHORIZATION, CONTENT_TYPE, SEC_WEBSOCKET_PROTOCOL};
use axum::http::{HeaderMap, Method, StatusCode};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post_service};
use tokio::sync::{Semaphore, watch};
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;
use tower_http::cors::{AllowOrigin, CorsLayer};
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::services::ServeDir;

use crate::config::Config;
use crate::engine::Engine;
use auth::Auth;
use mutation::MutationRoot;
use query::QueryRoot;
use subscription::SubscriptionRoot;

/// Request body cap: base64 .torrent uploads can be tens of MiB.
const BODY_LIMIT: usize = 64 * 1024 * 1024;

/// How long an upgraded WebSocket may take to send `connection_init`
/// (which carries the token) before it is closed.
const CONNECTION_INIT_DEADLINE: Duration = Duration::from_secs(10);

/// Cap on upgraded connections that have not sent `connection_init`
/// yet; further upgrades are refused while this many are pending.
const MAX_PENDING_WS: usize = 64;

/// Cap on inbound WebSocket message and frame size. Clients only send
/// graphql-ws control payloads over the socket — the `connection_init`
/// token and subscription documents with their variables; queries and
/// mutations (including .torrent uploads) go over `POST /graphql`. The
/// transport default (64 MiB) would let unauthenticated connections
/// force large buffers before the token is ever checked.
const WS_MESSAGE_LIMIT: usize = 64 * 1024;

/// After a server-initiated close frame, how long to keep reading for the
/// client's close reply before dropping the socket. Dropping with inbound
/// data still unread aborts the connection (TCP RST), which can discard
/// the close frame before the client reads it (observed on Windows).
const WS_CLOSE_GRACE: Duration = Duration::from_secs(3);

pub type ApiSchema = Schema<QueryRoot, MutationRoot, SubscriptionRoot>;

/// Builds the GraphQL schema over a running engine.
pub fn build_schema(engine: Arc<Engine>) -> ApiSchema {
    Schema::build(QueryRoot, MutationRoot, SubscriptionRoot)
        .data(engine)
        .finish()
}

/// The schema SDL. No engine is needed: context data only matters at
/// execution time (used by the schema exporter).
pub fn sdl() -> String {
    Schema::build(QueryRoot, MutationRoot, SubscriptionRoot)
        .finish()
        .sdl()
}

/// Shared state of the WebSocket upgrade handler.
#[derive(Clone)]
struct WsState {
    schema: ApiSchema,
    auth: Arc<Auth>,
    /// Flips to true when the daemon stops: open sockets close with 1001.
    shutdown: watch::Receiver<bool>,
    /// Tracks the upgraded-connection futures so shutdown can await them.
    tasks: TaskTracker,
    /// Permits for connections awaiting `connection_init` (see
    /// [`MAX_PENDING_WS`]).
    pending: Arc<Semaphore>,
    /// Cancelled when graceful shutdown ran out of patience: tasks that
    /// ignored `shutdown` (e.g. a stalled peer) end at their next poll.
    kill: CancellationToken,
}

/// Assembles the HTTP router from the API-related config (`graphiql`,
/// `serve_root`, `cors`). Upgraded WebSockets are detached from the HTTP
/// server: they watch `shutdown` and are tracked by `ws_tasks` so the
/// daemon can close and await them. `req_kill` cancels in-flight
/// `/graphql` requests (503) — hyper connection tasks are detached from
/// the serve coordinator, so this is the only way to end a long-running
/// handler that would otherwise hold engine references past shutdown.
pub fn router(
    schema: ApiSchema,
    auth: Arc<Auth>,
    config: &Config,
    shutdown: watch::Receiver<bool>,
    ws_tasks: TaskTracker,
    ws_kill: CancellationToken,
    req_kill: CancellationToken,
) -> Router {
    let ws = WsState {
        schema: schema.clone(),
        auth: Arc::clone(&auth),
        shutdown,
        tasks: ws_tasks,
        pending: Arc::new(Semaphore::new(MAX_PENDING_WS)),
        kill: ws_kill,
    };
    // route_layer wraps only the routes added before it (and unlike
    // `layer`, not the 404 fallback): /graphql is authenticated, /healthz
    // and GET / are not (GraphiQL and the web UI are static files; their
    // queries still need the token). WebSocket upgrades pass the header
    // middleware and authenticate in `connection_init` instead (browsers
    // cannot set headers on WebSockets).
    let mut router = Router::new()
        .route(
            "/graphql",
            post_service(GraphQL::new(schema))
                .get(graphql_ws)
                .with_state(ws),
        )
        .route_layer(axum::middleware::from_fn_with_state(
            auth,
            auth::require_bearer,
        ))
        // A real middleware-level cap: async-graphql's extractor consumes
        // the raw request body, which axum's DefaultBodyLimit (an
        // extractor-layer mechanism) does not restrict.
        .route_layer(RequestBodyLimitLayer::new(BODY_LIMIT))
        // Outermost on /graphql: dropping the inner future on cancel is
        // what releases the handler's engine/session references.
        .route_layer(axum::middleware::from_fn(
            move |req: axum::extract::Request, next: axum::middleware::Next| {
                let kill = req_kill.clone();
                async move {
                    tokio::select! {
                        biased;
                        () = kill.cancelled() => {
                            (StatusCode::SERVICE_UNAVAILABLE, "the daemon is shutting down")
                                .into_response()
                        }
                        resp = next.run(req) => resp,
                    }
                }
            },
        ))
        .route("/healthz", get(healthz));
    if let Some(layer) = cors_layer(&config.cors) {
        // Outermost on the routes above, so preflight OPTIONS requests are
        // answered before (unauthenticated) they would hit the bearer check.
        router = router.layer(layer);
    }
    if let Some(root) = &config.serve_root {
        // Static web UI: unknown paths 404 (the UI uses hash routing, so
        // no SPA fallback rewrite is needed).
        router = router.fallback_service(ServeDir::new(root));
    } else if config.graphiql {
        router = router.route("/", get(graphiql_page));
    }
    router
}

/// CORS for the listed `Origin` values (or any, on `"*"`); `None` when the
/// list is empty — same-origin use needs no CORS headers.
fn cors_layer(origins: &[String]) -> Option<CorsLayer> {
    if origins.is_empty() {
        return None;
    }
    let allow_origin = if origins.iter().any(|o| o == "*") {
        AllowOrigin::any()
    } else {
        AllowOrigin::list(origins.iter().map(|o| {
            o.parse()
                .expect("origins are validated at config parse time")
        }))
    };
    Some(
        CorsLayer::new()
            .allow_origin(allow_origin)
            .allow_methods([Method::GET, Method::POST])
            .allow_headers([AUTHORIZATION, CONTENT_TYPE])
            .max_age(std::time::Duration::from_secs(3600)),
    )
}

/// graphql-ws upgrade handler. A connection is accepted when the upgrade
/// request carried a valid `Authorization` header, or the
/// `connection_init` payload contains a valid `{"token": "..."}`.
/// `connection_init` must arrive within [`CONNECTION_INIT_DEADLINE`],
/// and at most [`MAX_PENDING_WS`] connections may be waiting for it.
async fn graphql_ws(
    State(state): State<WsState>,
    protocol: GraphQLProtocol,
    upgrade: WebSocketUpgrade,
    headers: HeaderMap,
) -> Response {
    let header_ok = state
        .auth
        .check_authorization(headers.get(AUTHORIZATION).and_then(|v| v.to_str().ok()));
    let WsState {
        schema,
        auth,
        shutdown,
        tasks,
        pending,
        kill,
    } = state;
    let Ok(pending) = pending.try_acquire_owned() else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            "too many connections awaiting connection_init",
        )
            .into_response();
    };
    // Mirror the protocol the `GraphQLProtocol` extractor selected (the
    // first client-offered one; its choice is not otherwise readable)
    // and accept exactly that subprotocol, so the 101 response cannot
    // disagree with what the graphql-ws state machine speaks.
    let Some(selected) = headers
        .get(SEC_WEBSOCKET_PROTOCOL)
        .and_then(|v| v.to_str().ok())
        .and_then(|list| {
            list.split(',')
                .find_map(|p| <WebSocketProtocols as std::str::FromStr>::from_str(p.trim()).ok())
        })
    else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    upgrade
        .max_message_size(WS_MESSAGE_LIMIT)
        .max_frame_size(WS_MESSAGE_LIMIT)
        .protocols([selected.sec_websocket_protocol()])
        .on_upgrade(move |socket| {
            tasks.clone().track_future(async move {
                let work = async {
                    let (mut sink, stream) = socket.split();
                // End the inbound stream at shutdown so serve() winds down.
                let mut stopping = shutdown.clone();
                let mut stream = stream.take_until(Box::pin(async move {
                    let _ = stopping.wait_for(|&stop| stop).await;
                }));
                let (init_tx, init_rx) = tokio::sync::oneshot::channel::<()>();
                // The block scopes `serve` (and its borrows of the sink and
                // stream) so the close handshake below can use them directly.
                let timed_out = {
                    let serve =
                        GraphQLWebSocket::new_with_pair(&mut sink, &mut stream, schema, protocol)
                            .on_connection_init(move |payload| {
                                // Initialized: stop counting against the
                                // pending cap and disarm the deadline.
                                drop(pending);
                                let _ = init_tx.send(());
                                async move {
                                    let payload_ok = payload
                                        .get("token")
                                        .and_then(|t| t.as_str())
                                        .is_some_and(|t| auth.check_token(t));
                                    if header_ok || payload_ok {
                                        Ok(Data::default())
                                    } else {
                                        Err(async_graphql::Error::new(
                                            "unauthorized: send {\"token\": \"...\"} in connection_init",
                                        ))
                                    }
                                }
                            })
                            .serve();
                    let mut serve = std::pin::pin!(serve);
                    tokio::select! {
                        () = &mut serve => false,
                        init = tokio::time::timeout(CONNECTION_INIT_DEADLINE, init_rx) => {
                            if init.is_ok() {
                                serve.await;
                                false
                            } else {
                                true
                            }
                        }
                    }
                };
                    let close_sent = if timed_out {
                        sink.send(Message::Close(Some(CloseFrame {
                            code: close_code::POLICY,
                            reason: "connection_init was not received in time".into(),
                        })))
                        .await
                        .is_ok()
                    } else if *shutdown.borrow() {
                        sink.send(Message::Close(Some(CloseFrame {
                            code: close_code::AWAY,
                            reason: "the daemon is shutting down".into(),
                        })))
                        .await
                        .is_ok()
                    } else {
                        false
                    };
                    if close_sent {
                        // Wait (briefly) for the client's close reply before
                        // dropping the socket, so the close frame is not lost
                        // to an abortive close. Read the raw stream: after
                        // shutdown the take_until wrapper only yields None.
                        let mut stream = stream.into_inner();
                        let drained = async {
                            while let Some(msg) = stream.next().await {
                                if matches!(msg, Ok(Message::Close(_)) | Err(_)) {
                                    break;
                                }
                            }
                        };
                        let _ = tokio::time::timeout(WS_CLOSE_GRACE, drained).await;
                    }
                };
                tokio::select! {
                    () = work => {}
                    () = kill.cancelled() => {}
                }
            })
        })
}

async fn healthz() -> &'static str {
    "ok"
}

async fn graphiql_page() -> impl IntoResponse {
    Html(
        async_graphql::http::GraphiQLSource::build()
            .endpoint("/graphql")
            .subscription_endpoint("/graphql")
            .finish(),
    )
}
