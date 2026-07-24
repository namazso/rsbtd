// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The API listener: a TCP socket or a unix domain socket.

use std::io;
use std::net::SocketAddr;

use axum::Router;

use crate::config::Listen;

/// A bound API listener, ready to serve.
pub enum Bound {
    Tcp {
        listener: tokio::net::TcpListener,
        addr: SocketAddr,
    },
    #[cfg(unix)]
    Unix {
        listener: tokio::net::UnixListener,
        path: std::path::PathBuf,
    },
}

/// Binds the configured listen address.
pub async fn bind(listen: &Listen) -> io::Result<Bound> {
    match listen {
        Listen::Tcp(addr) => {
            let listener = tokio::net::TcpListener::bind(addr).await?;
            let addr = listener.local_addr()?;
            Ok(Bound::Tcp { listener, addr })
        }
        #[cfg(unix)]
        Listen::Unix(path) => Ok(Bound::Unix {
            listener: bind_unix(path).await?,
            path: path.clone(),
        }),
        #[cfg(not(unix))]
        Listen::Unix(_) => Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "unix domain sockets are not supported on this platform",
        )),
    }
}

/// Binds a unix socket, replacing a stale socket file (one no process
/// accepts connections on) and restricting access to the daemon's user.
#[cfg(unix)]
async fn bind_unix(path: &std::path::Path) -> io::Result<tokio::net::UnixListener> {
    use std::os::unix::fs::FileTypeExt as _;

    match tokio::net::UnixStream::connect(path).await {
        Ok(_) => {
            return Err(io::Error::new(
                io::ErrorKind::AddrInUse,
                format!("{} is in use by another process", path.display()),
            ));
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) if e.kind() == io::ErrorKind::ConnectionRefused => {
            // Refused is the one error that establishes no process
            // accepts on the socket (stale after an unclean exit). Only
            // unlink an actual socket; never delete some other file that
            // happens to sit at the configured path.
            let file_type = std::fs::symlink_metadata(path)?.file_type();
            if !file_type.is_socket() {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    format!(
                        "{} exists and is not a socket ({file_type:?}); refusing to replace it",
                        path.display()
                    ),
                ));
            }
            tracing::info!("removing stale socket {}", path.display());
            std::fs::remove_file(path).or_else(|e| {
                // Lost a race against another daemon removing the same
                // stale socket; the subsequent bind arbitrates.
                if e.kind() == io::ErrorKind::NotFound {
                    Ok(())
                } else {
                    Err(e)
                }
            })?;
        }
        Err(e) => {
            // PermissionDenied and other inconclusive errors do not
            // prove the socket is dead; a live daemon may sit behind it.
            return Err(io::Error::new(
                e.kind(),
                format!(
                    "cannot probe {}: {e}; refusing to replace it",
                    path.display()
                ),
            ));
        }
    }
    let listener = tokio::net::UnixListener::bind(path)?;
    let perms = std::os::unix::fs::PermissionsExt::from_mode(0o600);
    std::fs::set_permissions(path, perms)?;
    Ok(listener)
}

impl Bound {
    /// The bound TCP address, when listening on TCP (lets tests discover
    /// an ephemeral port).
    pub fn tcp_addr(&self) -> Option<SocketAddr> {
        match self {
            Bound::Tcp { addr, .. } => Some(*addr),
            #[cfg(unix)]
            Bound::Unix { .. } => None,
        }
    }

    /// A human-readable endpoint description for logs.
    pub fn describe(&self) -> String {
        match self {
            Bound::Tcp { addr, .. } => format!("http://{addr}"),
            #[cfg(unix)]
            Bound::Unix { path, .. } => format!("unix:{}", path.display()),
        }
    }

    /// Releases the endpoint without serving (startup failed after
    /// binding). Removes the unix socket file.
    pub fn close(self) {
        match self {
            Bound::Tcp { .. } => {}
            #[cfg(unix)]
            Bound::Unix { path, .. } => {
                let _ = std::fs::remove_file(&path);
            }
        }
    }

    /// Serves `app` until `shutdown` resolves, then waits for in-flight
    /// connections. Removes the unix socket file when done.
    pub async fn serve(
        self,
        app: Router,
        shutdown: impl Future<Output = ()> + Send + 'static,
    ) -> io::Result<()> {
        match self {
            Bound::Tcp { listener, .. } => {
                axum::serve(listener, app)
                    .with_graceful_shutdown(shutdown)
                    .await
            }
            #[cfg(unix)]
            Bound::Unix { listener, path } => {
                let result = axum::serve(listener, app)
                    .with_graceful_shutdown(shutdown)
                    .await;
                let _ = std::fs::remove_file(&path);
                result
            }
        }
    }
}
