// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Daemon lifecycle management for the tray app.
//!
//! The daemon (engine + API server) runs on a tokio runtime owned by a
//! background thread; the Win32 message loop stays on the main thread.
//! Applying settings restarts the daemon in-process with the new [`Config`].

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::{Config, Daemon, StopProfile};

/// Budget for a session-end checkpoint (WM_QUERYENDSESSION): flush what
/// can be flushed while Windows still lets the session linger.
const CHECKPOINT_BUDGET: Duration = Duration::from_secs(5);

/// Commands from the UI thread (senders are sync, usable from the message loop).
enum Cmd {
    /// Stop the daemon (if running) and start it with this config.
    Apply(Config),
    /// Flush resume data and session state without stopping (session end
    /// was announced; the real stop may follow within seconds).
    Checkpoint,
    /// Stop the daemon and end the supervisor thread.
    Quit,
    /// As `Quit`, but skip the transport graces and use a persistence
    /// deadline that fits inside the OS session-end budget.
    QuitFast,
}

/// What the tray shows. `token` rides along so "Open in browser" can build
/// the hash-param URL for exactly the config the daemon is serving.
#[derive(Clone)]
pub enum Status {
    Starting,
    Running {
        addr: SocketAddr,
        token: Option<String>,
    },
    Failed(String),
    Stopped,
}

pub struct Supervisor {
    cmd_tx: tokio::sync::mpsc::UnboundedSender<Cmd>,
    status: Arc<Mutex<Status>>,
    thread: Mutex<Option<std::thread::JoinHandle<()>>>,
}

impl Supervisor {
    /// Starts the supervisor thread. An `Err` initial config still brings
    /// the tray up, showing the error and waiting for corrected settings.
    /// `notify` is called (from the supervisor thread) after every status
    /// change; the tray uses it to poke its window via `PostMessageW`.
    pub fn spawn(
        initial: Result<Config, String>,
        notify: impl Fn() + Send + 'static,
    ) -> Supervisor {
        let (cmd_tx, cmd_rx) = tokio::sync::mpsc::unbounded_channel();
        let status = Arc::new(Mutex::new(Status::Starting));
        let thread = {
            let status = Arc::clone(&status);
            std::thread::Builder::new()
                .name("rsbtd-supervisor".into())
                .spawn(move || supervise(initial, cmd_rx, status, notify))
                .expect("cannot spawn the supervisor thread")
        };
        Supervisor {
            cmd_tx,
            status,
            thread: Mutex::new(Some(thread)),
        }
    }

    pub fn status(&self) -> Status {
        self.status.lock().unwrap().clone()
    }

    /// Restarts the daemon with `config` (or starts it after a failure).
    pub fn apply(&self, config: Config) {
        let _ = self.cmd_tx.send(Cmd::Apply(config));
    }

    /// Starts a nonblocking flush of resume data and session state.
    pub fn checkpoint(&self) {
        let _ = self.cmd_tx.send(Cmd::Checkpoint);
    }

    /// Stops the daemon gracefully and joins the supervisor thread.
    /// Blocking but bounded (HTTP and persistence deadlines). Idempotent.
    pub fn shutdown(&self) {
        self.stop_and_join(Cmd::Quit);
    }

    /// Stops the daemon inside the OS session-end budget (~5 s): no
    /// transport graces, short persistence deadline. Idempotent.
    pub fn shutdown_fast(&self) {
        self.stop_and_join(Cmd::QuitFast);
    }

    fn stop_and_join(&self, cmd: Cmd) {
        let handle = self.thread.lock().unwrap().take();
        if let Some(handle) = handle {
            let _ = self.cmd_tx.send(cmd);
            if let Err(e) = handle.join() {
                tracing::error!("supervisor thread panicked: {e:?}");
            }
        }
    }
}

fn supervise(
    initial: Result<Config, String>,
    mut cmd_rx: tokio::sync::mpsc::UnboundedReceiver<Cmd>,
    status: Arc<Mutex<Status>>,
    notify: impl Fn(),
) {
    let set_status = |s: Status| {
        *status.lock().unwrap() = s;
        notify();
    };
    let runtime = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(e) => {
            set_status(Status::Failed(format!("cannot start the runtime: {e}")));
            return;
        }
    };
    runtime.block_on(async {
        let mut config = match initial {
            Ok(config) => Some(config),
            Err(e) => {
                tracing::error!("invalid configuration: {e}");
                set_status(Status::Failed(e));
                None
            }
        };
        'main: loop {
            // No valid config yet: wait for one (or for quit).
            let Some(current) = config.take() else {
                match cmd_rx.recv().await {
                    Some(Cmd::Apply(new)) => {
                        config = Some(new);
                        continue;
                    }
                    // Nothing runs, so there is nothing to flush.
                    Some(Cmd::Checkpoint) => continue,
                    Some(Cmd::Quit | Cmd::QuitFast) | None => break,
                }
            };
            set_status(Status::Starting);
            match start_with_retry(&current).await {
                Ok(daemon) => {
                    // Report the bound address (a port-0 config picks a real one).
                    let addr = daemon
                        .tcp_addr()
                        .expect("windows config always listens on TCP");
                    tracing::info!("daemon running on {addr}");
                    set_status(Status::Running {
                        addr,
                        token: current.token.clone(),
                    });
                    enum Next {
                        Apply(Config),
                        Quit,
                        QuitFast,
                    }
                    let next = loop {
                        match cmd_rx.recv().await {
                            Some(Cmd::Checkpoint) => {
                                let engine = Arc::clone(daemon.engine());
                                tokio::spawn(async move {
                                    engine
                                        .checkpoint(tokio::time::Instant::now() + CHECKPOINT_BUDGET)
                                        .await;
                                });
                            }
                            Some(Cmd::Apply(new)) => break Next::Apply(new),
                            Some(Cmd::Quit) | None => break Next::Quit,
                            Some(Cmd::QuitFast) => break Next::QuitFast,
                        }
                    };
                    match next {
                        Next::Apply(new) => {
                            tracing::info!("settings changed; restarting the daemon");
                            daemon.stop().await;
                            config = Some(new);
                        }
                        Next::Quit => {
                            daemon.stop().await;
                            break;
                        }
                        Next::QuitFast => {
                            daemon.stop_with(StopProfile::fast()).await;
                            break;
                        }
                    }
                }
                Err(e) => {
                    tracing::error!("daemon start failed: {e}");
                    set_status(Status::Failed(e.to_string()));
                    loop {
                        match cmd_rx.recv().await {
                            Some(Cmd::Apply(new)) => {
                                config = Some(new);
                                break;
                            }
                            // Nothing runs, so there is nothing to flush.
                            Some(Cmd::Checkpoint) => {}
                            Some(Cmd::Quit | Cmd::QuitFast) | None => break 'main,
                        }
                    }
                }
            }
        }
    });
    set_status(Status::Stopped);
}

/// Starts the daemon, absorbing the small window where the previous
/// instance's listener is still closing after a settings restart.
async fn start_with_retry(config: &Config) -> Result<Daemon, crate::BoxError> {
    let mut attempts = 0;
    loop {
        match Daemon::start(config.clone(), None).await {
            Err(e) if attempts < 3 && is_addr_in_use(e.as_ref()) => {
                attempts += 1;
                tokio::time::sleep(Duration::from_millis(250)).await;
            }
            result => return result,
        }
    }
}

fn is_addr_in_use(e: &(dyn std::error::Error + 'static)) -> bool {
    let mut source = Some(e);
    while let Some(err) = source {
        if let Some(io) = err.downcast_ref::<std::io::Error>() {
            return io.kind() == std::io::ErrorKind::AddrInUse;
        }
        source = err.source();
    }
    false
}
