// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Request/response correlation over the event bus.
//!
//! Most libtorrent operations are fire-and-forget: the result arrives later
//! as an alert. [`request`] gives them call semantics: subscribe to the
//! event bus *first*, post the operation, then await the matching event —
//! the response cannot slip by between posting and listening.

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::EngineError;
use super::events::{Event, EventKind};

/// Default time to wait for a response alert.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);
/// `move_storage` copies data across filesystems; give it much longer.
pub const MOVE_STORAGE_TIMEOUT: Duration = Duration::from_secs(600);

/// Posts an operation and awaits its response event.
///
/// `matcher` returns `Some(result)` for the event that answers this
/// request (match on the torrent uuid *and* the operation-specific key).
/// The `shutdown` token aborts the wait promptly (and refuses to post
/// once cancelled): waiters hold session references that the engine
/// needs released before it can close the session gracefully.
pub async fn request<T>(
    events: &broadcast::Sender<Arc<Event>>,
    shutdown: &CancellationToken,
    post: impl FnOnce() -> Result<(), EngineError>,
    mut matcher: impl FnMut(&Event) -> Option<Result<T, EngineError>>,
    timeout: Duration,
) -> Result<T, EngineError> {
    if shutdown.is_cancelled() {
        return Err(EngineError::ShuttingDown);
    }
    let mut rx = events.subscribe();
    post()?;
    let wait = async {
        loop {
            match rx.recv().await {
                Ok(event) => {
                    if let Some(result) = matcher(&event) {
                        return result;
                    }
                }
                // Missed events; the response may be among them. Keep
                // scanning — the timeout is the backstop.
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!("event bus lagged by {n} while awaiting a response");
                }
                Err(broadcast::error::RecvError::Closed) => {
                    return Err(EngineError::ShuttingDown);
                }
            }
        }
    };
    tokio::select! {
        () = shutdown.cancelled() => Err(EngineError::ShuttingDown),
        result = tokio::time::timeout(timeout, wait) => match result {
            Ok(result) => result,
            Err(_) => Err(EngineError::Timeout),
        },
    }
}

/// As [`request`], for operations serialized by a per-(torrent, class)
/// lock because their response alerts carry no request key. The lock is
/// held until the operation truly concludes: timing out the *wait* does
/// not stop the operation inside libtorrent, and its late reply would
/// otherwise satisfy the next serialized request on the same torrent.
/// On timeout, a background drainer inherits the guard and this call's
/// receiver (a fresh subscription could miss a reply landing in the
/// handover gap) and releases the lock once the stale reply arrives,
/// the torrent goes away, or the engine shuts down.
pub async fn request_serialized<T: Send + 'static>(
    events: &broadcast::Sender<Arc<Event>>,
    shutdown: &CancellationToken,
    guard: tokio::sync::OwnedMutexGuard<()>,
    torrent: Uuid,
    post: impl FnOnce() -> Result<(), EngineError>,
    mut matcher: impl FnMut(&Event) -> Option<Result<T, EngineError>> + Send + 'static,
    timeout: Duration,
) -> Result<T, EngineError> {
    if shutdown.is_cancelled() {
        return Err(EngineError::ShuttingDown);
    }
    let mut rx = events.subscribe();
    post()?;
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        let recv = tokio::select! {
            () = shutdown.cancelled() => return Err(EngineError::ShuttingDown),
            recv = tokio::time::timeout_at(deadline, rx.recv()) => recv,
        };
        let event = match recv {
            Ok(event) => event,
            Err(_elapsed) => {
                tokio::spawn(drain(rx, matcher, guard, torrent, shutdown.clone()));
                return Err(EngineError::Timeout);
            }
        };
        match event {
            Ok(event) => {
                if let Some(result) = matcher(&event) {
                    return result;
                }
            }
            // Missed events; the response may be among them. Keep
            // scanning — the timeout is the backstop.
            Err(broadcast::error::RecvError::Lagged(n)) => {
                tracing::warn!("event bus lagged by {n} while awaiting a response");
            }
            Err(broadcast::error::RecvError::Closed) => {
                return Err(EngineError::ShuttingDown);
            }
        }
    }
}

/// Holds a timed-out serialized operation's lock until its reply (or a
/// terminal condition) arrives, then releases it by dropping the guard.
async fn drain<T>(
    mut rx: broadcast::Receiver<Arc<Event>>,
    mut matcher: impl FnMut(&Event) -> Option<Result<T, EngineError>>,
    guard: tokio::sync::OwnedMutexGuard<()>,
    torrent: Uuid,
    shutdown: CancellationToken,
) {
    let _guard = guard;
    loop {
        let recv = tokio::select! {
            () = shutdown.cancelled() => return,
            recv = rx.recv() => recv,
        };
        match recv {
            Ok(event) => {
                if matcher(&event).is_some() {
                    return;
                }
                // A removed torrent's operation may never answer.
                if is_torrent(&event, torrent) && matches!(event.kind, EventKind::TorrentRemoved) {
                    return;
                }
            }
            // The reply may be among the missed events; freeing the lock
            // on lag beats holding it for a reply that already passed.
            Err(broadcast::error::RecvError::Lagged(_)) => return,
            Err(broadcast::error::RecvError::Closed) => return,
        }
    }
}

/// Matcher helper: whether `event` belongs to torrent `uuid`.
pub fn is_torrent(event: &Event, uuid: Uuid) -> bool {
    event.torrent.map(|t| t.uuid) == Some(uuid)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::events::{EventKind, TorrentRef};

    fn uuid(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }

    fn event(n: u128, kind: EventKind) -> Arc<Event> {
        Arc::new(Event {
            torrent: Some(TorrentRef { uuid: uuid(n) }),
            kind,
        })
    }

    #[tokio::test]
    async fn resolves_on_matching_event() {
        let (tx, _keep) = broadcast::channel(16);
        let tx2 = tx.clone();
        let token = CancellationToken::new();
        let fut = request(
            &tx,
            &token,
            || Ok(()),
            |e| {
                (is_torrent(e, uuid(7))
                    && matches!(e.kind, EventKind::FileRenamed { index: 3, .. }))
                .then_some(Ok(()))
            },
            Duration::from_secs(5),
        );
        let publish = async {
            // Wrong torrent, wrong index, then the match.
            let _ = tx2.send(event(
                9,
                EventKind::FileRenamed {
                    index: 3,
                    new_name: "x".into(),
                },
            ));
            let _ = tx2.send(event(
                7,
                EventKind::FileRenamed {
                    index: 1,
                    new_name: "x".into(),
                },
            ));
            let _ = tx2.send(event(
                7,
                EventKind::FileRenamed {
                    index: 3,
                    new_name: "x".into(),
                },
            ));
        };
        let (result, ()) = tokio::join!(fut, publish);
        result.unwrap();
    }

    #[tokio::test(start_paused = true)]
    async fn times_out() {
        let (tx, _keep) = broadcast::channel(16);
        let result = request(
            &tx,
            &CancellationToken::new(),
            || Ok(()),
            |_| None::<Result<(), EngineError>>,
            Duration::from_millis(50),
        )
        .await;
        assert!(matches!(result, Err(EngineError::Timeout)));
    }

    #[tokio::test(start_paused = true)]
    async fn shutdown_token_ends_the_wait() {
        let (tx, _keep) = broadcast::channel(16);
        let token = CancellationToken::new();
        let canceller = token.clone();
        let (result, ()) = tokio::join!(
            request(
                &tx,
                &token,
                || Ok(()),
                |_| None::<Result<(), EngineError>>,
                Duration::from_secs(600),
            ),
            async move { canceller.cancel() }
        );
        assert!(matches!(result, Err(EngineError::ShuttingDown)));
    }

    async fn locked_request_times_out(
        tx: &broadcast::Sender<Arc<Event>>,
        token: &CancellationToken,
        lock: &Arc<tokio::sync::Mutex<()>>,
    ) {
        let guard = Arc::clone(lock).lock_owned().await;
        let result = request_serialized(
            tx,
            token,
            guard,
            uuid(7),
            || Ok(()),
            |e| {
                (is_torrent(e, uuid(7)) && matches!(e.kind, EventKind::StorageMoved { .. }))
                    .then_some(Ok(()))
            },
            Duration::from_millis(50),
        )
        .await;
        assert!(matches!(result, Err(EngineError::Timeout)));
        // The drainer inherited the guard: the lock stays held.
        assert!(Arc::clone(lock).try_lock_owned().is_err());
    }

    async fn assert_released(lock: Arc<tokio::sync::Mutex<()>>) {
        tokio::time::timeout(Duration::from_secs(5), lock.lock_owned())
            .await
            .expect("drainer must release the lock");
    }

    #[tokio::test(start_paused = true)]
    async fn timed_out_lock_is_released_by_the_stale_reply() {
        let (tx, _keep) = broadcast::channel(16);
        let token = CancellationToken::new();
        let lock = Arc::new(tokio::sync::Mutex::new(()));
        locked_request_times_out(&tx, &token, &lock).await;
        let _ = tx.send(event(7, EventKind::StorageMoved { path: "x".into() }));
        assert_released(lock).await;
    }

    #[tokio::test(start_paused = true)]
    async fn timed_out_lock_is_released_by_torrent_removal() {
        let (tx, _keep) = broadcast::channel(16);
        let token = CancellationToken::new();
        let lock = Arc::new(tokio::sync::Mutex::new(()));
        locked_request_times_out(&tx, &token, &lock).await;
        // An unrelated torrent's removal keeps the lock held.
        let _ = tx.send(event(9, EventKind::TorrentRemoved));
        let _ = tx.send(event(7, EventKind::TorrentRemoved));
        assert_released(lock).await;
    }

    #[tokio::test(start_paused = true)]
    async fn timed_out_lock_is_released_by_shutdown() {
        let (tx, _keep) = broadcast::channel(16);
        let token = CancellationToken::new();
        let lock = Arc::new(tokio::sync::Mutex::new(()));
        locked_request_times_out(&tx, &token, &lock).await;
        token.cancel();
        assert_released(lock).await;
    }

    #[tokio::test]
    async fn cancelled_token_refuses_to_post() {
        let (tx, _keep) = broadcast::channel(16);
        let token = CancellationToken::new();
        token.cancel();
        let result = request(
            &tx,
            &token,
            || panic!("must not post after shutdown"),
            |_| None::<Result<(), EngineError>>,
            Duration::from_secs(5),
        )
        .await;
        assert!(matches!(result, Err(EngineError::ShuttingDown)));
    }

    #[tokio::test]
    async fn post_failure_short_circuits() {
        let (tx, _keep) = broadcast::channel(16);
        let result = request(
            &tx,
            &CancellationToken::new(),
            || Err(EngineError::NotFound),
            |_| None::<Result<(), EngineError>>,
            Duration::from_secs(5),
        )
        .await;
        assert!(matches!(result, Err(EngineError::NotFound)));
    }
}
