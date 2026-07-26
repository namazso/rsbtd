// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The pending-request registry: matches request/response alerts to
//! futures. Resolution happens as a side effect of the alert stream being
//! polled (see the module docs of [`crate::alerts`]).

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

use libctorrent_sys as sys;
use tokio::sync::oneshot;

use crate::client_data::ClientData;
use crate::error::Error;
use crate::handle::RawHandle;

use super::{batch_len_for_registry, raw_alert_for_registry};

#[derive(Default)]
struct Inner {
    closed: bool,
    session_stats: VecDeque<oneshot::Sender<Result<Vec<i64>, Error>>>,
    // Session-less payload (RawHandle): the handle crosses the channel as
    // bytes and is re-paired inside `Session::add_torrent`'s future —
    // sound because this Registry is per-session.
    add_torrent: HashMap<u64, oneshot::Sender<Result<RawHandle, Error>>>,
    // Per-torrent client data, keyed by the same token for the torrent's
    // whole lifetime. Not a pending request: entries persist until the
    // torrent_removed sweep (or teardown), and survive close().
    client_data: HashMap<u64, Arc<dyn ClientData>>,
    // Live-torrent handles for `Session::find_torrent_by_token`, following
    // client_data's lifecycle exactly: inserted when an add survives,
    // swept with it, kept across close() (dropping a RawHandle after
    // teardown is safe — see `RawHandle`).
    handles: HashMap<u64, RawHandle>,
}

/// Shared between the `Session` (which enqueues requests) and the `Alerts`
/// receiver (which resolves them while popping).
#[derive(Default)]
pub(crate) struct Registry {
    inner: Mutex<Inner>,
    next_id: std::sync::atomic::AtomicU64,
}

impl Registry {
    fn id(&self) -> u64 {
        // Tokens start at 1: token 0 would become a null client_data_t in
        // the shim, indistinguishable from "userdata never set".
        self.next_id
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            + 1
    }

    /// The error every pending future resolves to when it can no longer be fulfilled.
    pub(crate) fn closed_error() -> Error {
        Error::binding("the session was closed before the response alert arrived")
    }

    /// The error for pending futures when an overflow may have dropped their response.
    fn dropped_error() -> Error {
        Error::binding(
            "the alert queue overflowed and the response alert was dropped; \
             retry the request (consider raising the alert_queue_size \
             setting)",
        )
    }

    /// Runs `post` (which must post the session-stats request) and enqueues
    /// the response slot under one lock, so concurrent requests observe
    /// responses in posting order (session_stats correlation is FIFO).
    /// Nothing is enqueued if `post` fails.
    pub(crate) fn enqueue_session_stats(
        &self,
        post: impl FnOnce() -> Result<(), Error>,
    ) -> Result<oneshot::Receiver<Result<Vec<i64>, Error>>, Error> {
        let mut inner = self.inner.lock().unwrap();
        if inner.closed {
            return Err(Self::closed_error());
        }
        post()?;
        let (tx, rx) = oneshot::channel();
        inner.session_stats.push_back(tx);
        Ok(rx)
    }

    pub(crate) fn enqueue_add_torrent(
        &self,
        data: Arc<dyn ClientData>,
    ) -> Result<(u64, oneshot::Receiver<Result<RawHandle, Error>>), Error> {
        let mut inner = self.inner.lock().unwrap();
        if inner.closed {
            return Err(Self::closed_error());
        }
        let token = self.id();
        let (tx, rx) = oneshot::channel();
        inner.add_torrent.insert(token, tx);
        // Same critical section as the token mint: the attach in libtorrent
        // is observed via the add_torrent alert, which `resolve_add_torrent`
        // only sees on a later `process` — the data is in place first.
        inner.client_data.insert(token, data);
        Ok((token, rx))
    }

    /// Drops the pending sender only (the drop-guard path): the post
    /// already happened and is not undone, so the torrent may attach
    /// regardless — its client data must stay reachable.
    pub(crate) fn cancel_add_torrent(&self, token: u64) {
        let mut inner = self.inner.lock().unwrap();
        inner.add_torrent.remove(&token);
    }

    /// Rolls back a refused post: the shim rejected the request, so the
    /// torrent can never attach — reclaim the client data too.
    pub(crate) fn abort_add_torrent(&self, token: u64) {
        let data = {
            let mut inner = self.inner.lock().unwrap();
            inner.add_torrent.remove(&token);
            inner.client_data.remove(&token)
        };
        // Dropped outside the lock: a user Drop may re-enter the registry.
        drop(data);
    }

    pub(crate) fn client_data(&self, token: u64) -> Option<Arc<dyn ClientData>> {
        self.inner.lock().unwrap().client_data.get(&token).cloned()
    }

    /// The live handle of the torrent added under `token`, if any.
    pub(crate) fn torrent_handle(&self, token: u64) -> Option<RawHandle> {
        self.inner
            .lock()
            .unwrap()
            .handles
            .get(&token)
            .map(RawHandle::clone_raw)
    }

    /// Replaces a live torrent's data. Replace-only by design: inserting
    /// under an unknown token would resurrect an entry the torrent_removed
    /// sweep already reclaimed, leaking it for the session's lifetime.
    pub(crate) fn set_client_data(
        &self,
        token: u64,
        data: Arc<dyn ClientData>,
    ) -> Result<(), Error> {
        let old = {
            let mut inner = self.inner.lock().unwrap();
            match inner.client_data.get_mut(&token) {
                Some(slot) => std::mem::replace(slot, data),
                None => return Err(Error::binding("the torrent was removed")),
            }
        };
        // Dropped outside the lock (see abort_add_torrent).
        drop(old);
        Ok(())
    }
    /// Scans a freshly popped batch and resolves matching requests.
    pub(crate) fn process(&self, batch: *mut sys::ct_alert_batch) {
        let len = batch_len_for_registry(batch);
        for i in 0..len {
            let alert = raw_alert_for_registry(batch, i);
            // SAFETY: alert is valid within the current batch.
            let ty = unsafe { sys::ct_alert_type(alert) };
            if ty == sys::CT_ALERT_TYPE_SESSION_STATS as i32 {
                self.resolve_session_stats(alert);
            } else if ty == sys::CT_ALERT_TYPE_ADD_TORRENT as i32 {
                self.resolve_add_torrent(alert);
            } else if ty == sys::CT_ALERT_TYPE_TORRENT_REMOVED as i32 {
                self.resolve_torrent_removed(alert);
            } else if ty == sys::CT_ALERT_TYPE_ALERTS_DROPPED as i32 {
                self.handle_alerts_dropped(alert);
            }
        }
    }

    /// The alert queue overflowed. Response alerts are *not* exempt from
    /// dropping, and a dropped session_stats response would desync the
    /// FIFO correlation permanently, so fail everything pending of the
    /// affected type: a spuriously failed request can be retried, a
    /// misaligned one is silent corruption. Alerts earlier in this batch
    /// have already been resolved (`process` walks in order); later
    /// response alerts find an empty queue and are ignored.
    fn handle_alerts_dropped(&self, alert: *const sys::ct_alert) {
        // SAFETY: alert is valid within the current batch.
        let view = unsafe {
            let mut view = sys::ct_alerts_dropped_view::default();
            if !sys::ct_alert_as_alerts_dropped(alert, &mut view) {
                return;
            }
            view
        };
        let bit = |ty: u32| {
            let ty = ty as usize;
            ty < 128 && view.dropped[ty / 8] & (1 << (ty % 8)) != 0
        };
        // Drain under the lock, send after releasing it: a oneshot
        // send/drop invokes the receiver's waker, which may re-enter the
        // registry and deadlock on this mutex.
        let mut stats = VecDeque::new();
        let mut adds = HashMap::new();
        {
            let mut inner = self.inner.lock().unwrap();
            if bit(sys::CT_ALERT_TYPE_SESSION_STATS) {
                stats = std::mem::take(&mut inner.session_stats);
            }
            if bit(sys::CT_ALERT_TYPE_ADD_TORRENT) {
                adds = std::mem::take(&mut inner.add_torrent);
            }
        }
        for sender in stats {
            let _ = sender.send(Err(Self::dropped_error()));
        }
        for (_, sender) in adds {
            let _ = sender.send(Err(Self::dropped_error()));
        }
    }

    fn resolve_session_stats(&self, alert: *const sys::ct_alert) {
        let sender = {
            let mut inner = self.inner.lock().unwrap();
            inner.session_stats.pop_front()
        };
        let Some(sender) = sender else { return };
        // SAFETY: alert is valid; the view's counters live until the next
        // pop - we copy them out immediately.
        let counters = unsafe {
            let mut view = sys::ct_session_stats_view::default();
            if !sys::ct_alert_as_session_stats(alert, &mut view) || view.counters.is_null() {
                Vec::new()
            } else {
                std::slice::from_raw_parts(view.counters, view.len).to_vec()
            }
        };
        let _ = sender.send(Ok(counters));
    }

    fn resolve_add_torrent(&self, alert: *const sys::ct_alert) {
        // SAFETY: alert is valid; the view borrows the batch.
        let (token, stored, result) = unsafe {
            let mut view = sys::ct_add_torrent_view::default();
            if !sys::ct_alert_as_add_torrent(alert, &mut view) {
                return;
            }
            let token = view.userdata as u64;
            let result = if let Some(err) = Error::from_ct(&view.error) {
                Err(err)
            } else if view.handle.is_null() {
                Err(Error::binding("add_torrent_alert has null handle"))
            } else {
                Ok(RawHandle::from_ptr(view.handle))
            };
            // The client data stays only if the torrent attached under this
            // token: a failed add never attached, and a duplicate add
            // resolves to the pre-existing torrent, which kept its original
            // userdata (the new attempt's data can never be reached again).
            let surviving = result.is_ok() && sys::ct_torrent_handle_userdata(view.handle) == token;
            // A surviving add also registers the torrent's live handle.
            let stored = surviving.then(|| RawHandle::from_ptr(view.handle));
            (token, stored, result)
        };
        // Regardless of sender presence: the future may be long cancelled
        // (drop-guard) while the data entry still needs error cleanup.
        let (sender, stale_data) = {
            let mut inner = self.inner.lock().unwrap();
            let stale_data = match stored {
                Some(handle) => {
                    inner.handles.insert(token, handle);
                    None
                }
                None => inner.client_data.remove(&token),
            };
            (inner.add_torrent.remove(&token), stale_data)
        };
        // Dropped outside the lock: a user Drop may re-enter the registry.
        drop(stale_data);
        if let Some(sender) = sender {
            let _ = sender.send(result);
        }
    }

    /// Reclaims the removed torrent's client data and handle. Best-effort:
    /// libtorrent can drop the alert on extreme queue overflow
    /// (alerts_dropped carries no token), leaking the entries until
    /// teardown.
    fn resolve_torrent_removed(&self, alert: *const sys::ct_alert) {
        // SAFETY: alert is valid within the current batch.
        let token = unsafe {
            let mut view = sys::ct_torrent_removed_view::default();
            if !sys::ct_alert_as_torrent_removed(alert, &mut view) {
                return;
            }
            view.userdata as u64
        };
        if token == 0 {
            return;
        }
        let (data, handle) = {
            let mut inner = self.inner.lock().unwrap();
            (
                inner.client_data.remove(&token),
                inner.handles.remove(&token),
            )
        };
        // Dropped outside the lock: a user Drop may re-enter the registry.
        drop(data);
        drop(handle);
    }

    /// Fails every pending request; called on session teardown. Client
    /// data and torrent handles are deliberately untouched: they belong to
    /// torrents, not pending requests, and handles remain usable until the
    /// session drops (both maps die with the `Arc<Registry>`).
    pub(crate) fn close(&self) {
        // Dropping a sender wakes its receiver; move them out of the lock
        // first (see handle_alerts_dropped).
        let (stats, adds) = {
            let mut inner = self.inner.lock().unwrap();
            inner.closed = true;
            (
                std::mem::take(&mut inner.session_stats),
                std::mem::take(&mut inner.add_torrent),
            )
        };
        drop(stats);
        drop(adds);
    }
}

/// Owned by `Session::add_torrent`'s future: unregisters the pending
/// entry when the future is dropped, so a cancelled future does not leave
/// its slot behind. No-op after normal resolution.
pub(crate) struct AddTorrentToken {
    registry: Arc<Registry>,
    token: u64,
}

impl AddTorrentToken {
    pub(crate) fn new(registry: Arc<Registry>, token: u64) -> AddTorrentToken {
        AddTorrentToken { registry, token }
    }
}

impl Drop for AddTorrentToken {
    fn drop(&mut self) {
        self.registry.cancel_add_torrent(self.token);
    }
}
