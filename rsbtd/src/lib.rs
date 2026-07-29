// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! rsbtd — a torrent client daemon with a GraphQL API, built on
//! [`rbtorrent`]. The library crate exists so integration tests can run
//! the daemon in-process; the `rsbtd` binary thinly wraps [`run`].

pub mod api;
pub mod config;
pub mod engine;
#[cfg(windows)]
pub mod windows;

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

pub use config::Config;

use engine::Engine;

/// Boxed error for daemon startup/shutdown failures surfaced to `main`.
pub type BoxError = Box<dyn std::error::Error + Send + Sync>;

/// Deadlines for one [`Daemon::stop_with`] run.
pub struct StopProfile {
    /// Wait for open HTTP connections to finish before cancelling their
    /// in-flight requests.
    pub http_grace: Duration,
    /// After cancelling requests, wait for the (now unblocked)
    /// connections to close before aborting the server task outright.
    pub req_kill_grace: Duration,
    /// Wait for upgraded WebSockets to close before cancelling them.
    pub ws_grace: Duration,
    /// Overrides the engine's configured persistence grace.
    pub engine_grace: Option<Duration>,
}

impl StopProfile {
    /// The regular profile: generous transport drains, the configured
    /// engine grace.
    pub fn normal() -> StopProfile {
        StopProfile {
            http_grace: Duration::from_secs(10),
            req_kill_grace: Duration::from_secs(2),
            ws_grace: Duration::from_secs(10),
            engine_grace: None,
        }
    }

    /// For OS session end: cancel transports immediately and flush
    /// within the few seconds Windows allows before killing the process.
    pub fn fast() -> StopProfile {
        StopProfile {
            http_grace: Duration::ZERO,
            req_kill_grace: Duration::from_millis(500),
            ws_grace: Duration::ZERO,
            engine_grace: Some(Duration::from_secs(3)),
        }
    }
}

/// A running daemon: the engine plus the API server.
pub struct Daemon {
    engine: Arc<Engine>,
    server: tokio::task::JoinHandle<std::io::Result<()>>,
    stop_tx: tokio::sync::watch::Sender<bool>,
    ws_tasks: tokio_util::task::TaskTracker,
    ws_kill: tokio_util::sync::CancellationToken,
    req_kill: tokio_util::sync::CancellationToken,
    tcp_addr: Option<SocketAddr>,
}

impl Daemon {
    /// Binds the API endpoint, then starts the engine (restoring
    /// persisted torrents), then serves — the daemon owns its endpoint
    /// before touching persisted state, and clients never see a partially
    /// restored session. `initial_settings` seeds a *fresh* session only
    /// (tests); a persisted session state takes precedence.
    pub async fn start(
        config: Config,
        initial_settings: Option<rbtorrent::SettingsPack>,
    ) -> Result<Daemon, BoxError> {
        if let Some(root) = &config.serve_root
            && !root.is_dir()
        {
            return Err(format!(
                "api.serve_root {} is not a readable directory",
                root.display()
            )
            .into());
        }
        let bound = api::listener::bind(&config.listen).await?;
        let engine = match Engine::start(&config, initial_settings).await {
            Ok(engine) => engine,
            Err(e) => {
                bound.close();
                return Err(e.into());
            }
        };
        if config.token.is_none() {
            tracing::warn!("no api.token configured; the API is unauthenticated");
        }
        let auth = Arc::new(api::auth::Auth::new(config.token.as_deref()));
        let schema = api::build_schema(Arc::clone(&engine));
        let (stop_tx, mut stop_rx) = tokio::sync::watch::channel(false);
        let ws_tasks = tokio_util::task::TaskTracker::new();
        let ws_kill = tokio_util::sync::CancellationToken::new();
        let req_kill = tokio_util::sync::CancellationToken::new();
        let app = api::router(
            schema,
            auth,
            Arc::clone(&engine),
            &config,
            stop_tx.subscribe(),
            ws_tasks.clone(),
            ws_kill.clone(),
            req_kill.clone(),
        );
        let tcp_addr = bound.tcp_addr();
        tracing::info!("API listening on {}", bound.describe());
        let server = tokio::spawn(bound.serve(app, async move {
            let _ = stop_rx.wait_for(|&stop| stop).await;
        }));
        Ok(Daemon {
            engine,
            server,
            stop_tx,
            ws_tasks,
            ws_kill,
            req_kill,
            tcp_addr,
        })
    }

    /// The engine, for in-process embedding (tests).
    pub fn engine(&self) -> &Arc<Engine> {
        &self.engine
    }

    /// The bound TCP address, when listening on TCP.
    pub fn tcp_addr(&self) -> Option<SocketAddr> {
        self.tcp_addr
    }

    /// Stops the API server (bounded wait for open connections and
    /// upgraded WebSockets), then shuts the engine down gracefully.
    pub async fn stop(self) {
        self.stop_with(StopProfile::normal()).await;
    }

    /// [`Daemon::stop`] with explicit deadlines.
    pub async fn stop_with(mut self, profile: StopProfile) {
        let _ = self.stop_tx.send(true);
        match tokio::time::timeout(profile.http_grace, &mut self.server).await {
            Ok(Ok(Ok(()))) => {}
            Ok(Ok(Err(e))) => tracing::warn!("API server failed: {e}"),
            Ok(Err(e)) => tracing::warn!("API server task failed: {e}"),
            Err(_) => {
                // Connection tasks are detached from the serve
                // coordinator, so aborting it leaves handlers (and their
                // engine references) running. Cancel the requests
                // themselves, then give the drained connections a moment
                // to close; the abort is a last resort.
                tracing::warn!("open connections did not finish in time; cancelling requests");
                self.req_kill.cancel();
                if tokio::time::timeout(profile.req_kill_grace, &mut self.server)
                    .await
                    .is_err()
                {
                    tracing::warn!("connections still open; aborting the API server");
                    self.server.abort();
                }
            }
        }
        // WebSockets outlive the HTTP server; they saw the stop signal and
        // close with 1001. Await them so nothing retains the engine.
        self.ws_tasks.close();
        if tokio::time::timeout(profile.ws_grace, self.ws_tasks.wait())
            .await
            .is_err()
        {
            tracing::warn!("open websockets did not close in time; cancelling them");
            self.ws_kill.cancel();
            self.ws_tasks.wait().await;
        }
        match profile.engine_grace {
            Some(grace) => self.engine.shutdown_with(grace).await,
            None => self.engine.shutdown().await,
        }
    }
}

/// Runs the daemon until `shutdown` resolves, then shuts down gracefully.
///
/// `shutdown` is polled during startup too: an early SIGTERM during a
/// long state restore stops the daemon as soon as startup completes.
pub async fn run(
    config: Config,
    shutdown: impl std::future::Future<Output = ()> + Send + 'static,
) -> Result<(), BoxError> {
    let mut shutdown = std::pin::pin!(shutdown);
    let mut start = std::pin::pin!(Daemon::start(config, None));
    tokio::select! {
        result = &mut start => {
            let daemon = result?;
            shutdown.await;
            daemon.stop().await;
        }
        () = &mut shutdown => {
            tracing::info!("shutting down as soon as startup completes");
            start.await?.stop().await;
        }
    }
    Ok(())
}
