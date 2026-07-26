// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Session construction and lifecycle.

use std::future::Future;
use std::ptr::NonNull;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use libctorrent_sys as sys;
use tokio::sync::Notify;

use crate::alerts::Alerts;
use crate::alerts::requests::{AddTorrentToken, Registry};
use crate::error::{Result, with_error};
use crate::filter::{IpFilter, PortFilter};
use crate::settings::SettingsPack;

/// Selects one of libtorrent's built-in disk I/O backends.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
#[non_exhaustive]
pub enum DiskIo {
    /// Let libtorrent pick (mmap where available, POSIX otherwise).
    #[default]
    Default,
    /// Memory-mapped file I/O. Not available in every libtorrent build.
    Mmap,
    /// Single-threaded portable POSIX I/O.
    Posix,
    /// Multi-threaded `pread()`/`pwrite()` I/O.
    Pread,
    /// Discards all writes, reads zeros (for testing/benchmarks).
    Disabled,
}

impl DiskIo {
    fn to_ct(self) -> sys::ct_disk_io_backend {
        match self {
            DiskIo::Default => sys::CT_DISK_IO_DEFAULT,
            DiskIo::Mmap => sys::CT_DISK_IO_MMAP,
            DiskIo::Posix => sys::CT_DISK_IO_POSIX,
            DiskIo::Pread => sys::CT_DISK_IO_PREAD,
            DiskIo::Disabled => sys::CT_DISK_IO_DISABLED,
        }
    }
}

bitflags::bitflags! {
    /// Selects which parts of the session state [`Session::save_state`]
    /// serializes and [`SessionParams::load_state`] restores.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct SaveStateFlags: u32 {
        const SETTINGS = sys::CT_SAVE_SETTINGS;
        const DHT_STATE = sys::CT_SAVE_DHT_STATE;
        const EXTENSION_STATE = sys::CT_SAVE_EXTENSION_STATE;
        const IP_FILTER = sys::CT_SAVE_IP_FILTER;
    }
}

bitflags::bitflags! {
    /// Options for [`TorrentHandle::remove`](crate::TorrentHandle::remove).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub struct RemoveFlags: u32 {
        /// Delete the downloaded payload files from disk.
        const DELETE_FILES = sys::CT_REMOVE_DELETE_FILES;
        /// Delete the part file holding pieces of unselected files.
        const DELETE_PARTFILE = sys::CT_REMOVE_DELETE_PARTFILE;
    }
}

/// Construction parameters for a [`Session`] (a builder consumed by [`Session::new`]).
pub struct SessionParams {
    ptr: NonNull<sys::ct_session_params>,
}

// Plain data guarded by &mut for mutation.
unsafe impl Send for SessionParams {}
unsafe impl Sync for SessionParams {}

impl SessionParams {
    /// libtorrent defaults: default settings, the ut_metadata/ut_pex/
    /// smart_ban extensions enabled, default disk I/O.
    pub fn new() -> SessionParams {
        // SAFETY: constructor returns an owned pointer (null on OOM).
        let ptr = unsafe { sys::ct_session_params_new() };
        SessionParams {
            ptr: NonNull::new(ptr).expect("allocation failed"),
        }
    }

    /// Applies a settings delta on top of the defaults. Panics on
    /// allocation failure.
    pub fn settings(self, pack: &SettingsPack) -> Self {
        // SAFETY: both pointers valid; the pack is copied.
        with_error(|err| unsafe {
            sys::ct_session_params_set_settings(self.ptr.as_ptr(), pack.as_ptr(), err)
        })
        .expect("copying settings pack failed");
        self
    }

    /// Enables/disables the built-in extensions (all on by default).
    pub fn default_extensions(self, ut_metadata: bool, ut_pex: bool, smart_ban: bool) -> Self {
        // SAFETY: params pointer is valid.
        unsafe {
            sys::ct_session_params_set_default_extensions(
                self.ptr.as_ptr(),
                ut_metadata,
                ut_pex,
                smart_ban,
            );
        }
        self
    }

    /// Selects the disk I/O backend. Panics if this build of libtorrent
    /// lacks it (currently only possible for [`DiskIo::Mmap`]).
    pub fn disk_io(self, backend: DiskIo) -> Self {
        // SAFETY: params pointer is valid.
        let ok = unsafe {
            sys::ct_session_params_set_disk_io(self.ptr.as_ptr(), backend.to_ct() as i32)
        };
        assert!(
            ok,
            "disk I/O backend {backend:?} not available in this libtorrent build"
        );
        self
    }

    /// Starts the session paused.
    pub fn paused(self, paused: bool) -> Self {
        // SAFETY: params pointer is valid.
        unsafe { sys::ct_session_params_set_paused(self.ptr.as_ptr(), paused) };
        self
    }

    /// Restores state previously saved with [`Session::save_state`],
    /// replacing exactly the parts selected by `flags`; everything else is
    /// preserved. A selected part is replaced even if the saved blob lacks
    /// it (resetting it to defaults), so pass only the flags you saved
    /// with.
    ///
    /// # Safety
    /// With [`SaveStateFlags::SETTINGS`] selected, settings in the blob
    /// bypass the validated [`SettingsPack`] setters and reach libtorrent
    /// verbatim; out-of-domain values can trigger undefined behavior at
    /// use time. `bencoded` must therefore originate from
    /// [`Session::save_state`] of a session configured through this
    /// crate's validated API — never untrusted input.
    pub unsafe fn load_state(self, bencoded: &[u8], flags: SaveStateFlags) -> Result<Self> {
        // SAFETY: params valid; the span borrows `bencoded` for the call.
        with_error(|err| unsafe {
            sys::ct_session_params_load_state(
                self.ptr.as_ptr(),
                sys::ct_span {
                    ptr: bencoded.as_ptr(),
                    len: bencoded.len(),
                },
                flags.bits(),
                err,
            );
        })?;
        Ok(self)
    }
}

impl Default for SessionParams {
    fn default() -> Self {
        SessionParams::new()
    }
}

impl Drop for SessionParams {
    fn drop(&mut self) {
        // SAFETY: we own the params.
        unsafe { sys::ct_session_params_free(self.ptr.as_ptr()) }
    }
}

/// A running libtorrent session (network thread and all).
///
/// All methods are callable from any thread; methods documented as
/// *briefly blocking* wait for a round trip to libtorrent's network
/// thread.
///
/// Dropping a `Session` performs a **blocking** shutdown. Call
/// [`Session::close`] from async code instead.
///
/// # Lifecycle safety
///
/// Every object derived from a session ([`TorrentHandle`](crate::TorrentHandle),
/// [`Alerts`], the `add_torrent` future) borrows it, and teardown requires
/// ownership, so the borrow checker proves none of them exists (and no
/// handle call is in flight) when the session is closed or dropped.
pub struct Session {
    inner: SessionInner,
}

pub(crate) struct SessionInner {
    ptr: NonNull<sys::ct_session>,
    closed: AtomicBool,
    alerts_taken: AtomicBool,
    /// Signaled by libtorrent's alert-notify callback.
    pub(crate) notify: Arc<Notify>,
    /// Arc so add-torrent futures can unregister themselves on drop.
    pub(crate) registry: Arc<Registry>,
}

// lt::session is documented thread-safe.
unsafe impl Send for SessionInner {}
unsafe impl Sync for SessionInner {}

/// The alert-notify callback: runs on a libtorrent thread and must only
/// wake, never block.
extern "C" fn alert_notify_cb(userdata: *mut std::ffi::c_void) {
    // SAFETY: userdata is the Notify inside the owning SessionInner's Arc;
    // the callback is unregistered before teardown frees either.
    let notify = unsafe { &*(userdata as *const Notify) };
    notify.notify_one();
}

impl SessionInner {
    /// Unregister the notify callback, fail pending request futures, then
    /// abort -> free -> proxy-drop (blocks until the network thread exits).
    fn teardown(&self) {
        if self.closed.swap(true, Ordering::AcqRel) {
            return;
        }
        // SAFETY: ptr is valid and this runs exactly once (guarded by
        // `closed`). Unregistering synchronizes with the alert queue, so no
        // notify fires after this line; it must happen BEFORE
        // registry.close() to prevent spurious wakeups.
        unsafe {
            sys::ct_session_set_alert_notify(
                self.ptr.as_ptr(),
                None,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            );
        }
        self.registry.close();
        unsafe {
            let mut proxy = std::mem::MaybeUninit::<sys::ct_session_proxy>::uninit();
            sys::ct_session_abort(self.ptr.as_ptr(), proxy.as_mut_ptr());
            sys::ct_session_free(self.ptr.as_ptr());
            sys::ct_session_proxy_drop(proxy.as_mut_ptr());
        }
    }
}

impl Drop for SessionInner {
    fn drop(&mut self) {
        self.teardown();
    }
}

impl Session {
    /// [`Session::reopen_network_sockets`] option: also remap UPnP/NAT-PMP mappings.
    pub const REOPEN_MAP_PORTS: u32 = sys::CT_REOPEN_MAP_PORTS;

    /// Spawns the session. `params` is consumed.
    pub fn new(params: SessionParams) -> Result<Session> {
        let ptr = with_error(|err|
            // SAFETY: params is valid; on success we own the session.
            unsafe { sys::ct_session_new(params.ptr.as_ptr(), err) })?;
        let session = Session {
            inner: SessionInner {
                ptr: NonNull::new(ptr).expect("ct_session_new returned null without error"),
                closed: AtomicBool::new(false),
                alerts_taken: AtomicBool::new(false),
                notify: Arc::new(Notify::new()),
                registry: Arc::new(Registry::default()),
            },
        };
        // SAFETY: the callback borrows the Notify owned by SessionInner and
        // is unregistered in teardown. On error the session drops via Drop.
        with_error(|err| unsafe {
            sys::ct_session_set_alert_notify(
                session.inner.ptr.as_ptr(),
                Some(alert_notify_cb),
                Arc::as_ptr(&session.inner.notify) as *mut std::ffi::c_void,
                err,
            );
        })?;
        Ok(session)
    }

    /// Takes the alert stream. Panics if it was already taken and not
    /// dropped; drop the previous [`Alerts`] first.
    pub fn alerts(&self) -> Alerts<'_> {
        assert!(
            !self.inner.alerts_taken.swap(true, Ordering::AcqRel),
            "Session::alerts() called while another Alerts receiver exists"
        );
        Alerts::new(self)
    }

    /// Requests a snapshot of the session-wide counters.
    ///
    /// The returned future resolves **only while the alert stream is being
    /// polled** (see [`crate::alerts`]); the counter indices come from
    /// [`session_stats_metrics`](crate::stats::session_stats_metrics) /
    /// [`find_metric_index`](crate::stats::find_metric_index).
    pub fn session_stats(&self) -> impl Future<Output = Result<Vec<i64>>> + Send + 'static {
        let queued = self.inner.registry.enqueue_session_stats(|| {
            // SAFETY: session pointer is valid.
            with_error(|err| unsafe { sys::ct_session_post_session_stats(self.ptr(), err) })
        });
        async move {
            match queued {
                Ok(rx) => rx.await.map_err(|_| Registry::closed_error())?,
                Err(e) => Err(e),
            }
        }
    }

    /// Serializes the parts of the session state selected by `flags` for
    /// [`SessionParams::load_state`]. Briefly blocking.
    pub fn save_state(&self, flags: SaveStateFlags) -> Result<Vec<u8>> {
        // SAFETY: session valid; on success we own the buffer and must free it.
        let mut buf =
            with_error(|err| unsafe { sys::ct_session_get_state(self.ptr(), flags.bits(), err) })?;
        let bytes = if buf.ptr.is_null() {
            Vec::new()
        } else {
            // SAFETY: ptr/len describe the owned buffer.
            unsafe { std::slice::from_raw_parts(buf.ptr, buf.len) }.to_vec()
        };
        // SAFETY: frees the buffer returned above.
        unsafe { sys::ct_buf_free(&mut buf) };
        Ok(bytes)
    }

    pub(crate) fn inner(&self) -> &SessionInner {
        &self.inner
    }

    pub(crate) fn release_alerts(&self) {
        self.inner.alerts_taken.store(false, Ordering::Release);
    }

    /// Shuts the session down without blocking the async runtime; resolves
    /// once the network thread has fully torn down. Cancellation only
    /// stops the waiting: the teardown task keeps running if this future
    /// is dropped.
    pub async fn close(self) {
        // The session lives inside the closure until teardown completes; if
        // the runtime drops the closure without running it (shutdown),
        // SessionInner::drop still tears down.
        match tokio::task::spawn_blocking(move || self.inner.teardown()).await {
            Ok(()) => {}
            Err(e) if e.is_cancelled() => {}
            Err(e) => std::panic::resume_unwind(e.into_panic()),
        }
    }

    /// Blocking equivalent of [`Session::close`].
    pub fn close_blocking(self) {
        self.inner.teardown();
    }

    /// Applies a settings delta to the running session.
    pub fn apply_settings(&self, pack: &SettingsPack) -> Result<()> {
        // SAFETY: both pointers valid; pack is copied.
        with_error(|err| unsafe { sys::ct_session_apply_settings(self.ptr(), pack.as_ptr(), err) })
    }

    /// The session's full effective settings (every setting present). Briefly blocking.
    pub fn settings(&self) -> Result<SettingsPack> {
        // SAFETY: session valid; on success we own the returned pack.
        let ptr = with_error(|err| unsafe { sys::ct_session_get_settings(self.ptr(), err) })?;
        Ok(unsafe { SettingsPack::from_owned_ptr(ptr) })
    }

    /// The port the session listens on. Briefly blocking.
    pub fn listen_port(&self) -> Result<u16> {
        // SAFETY: session valid.
        with_error(|err| unsafe { sys::ct_session_listen_port(self.ptr(), err) })
    }

    /// The SSL listen port, or 0. Briefly blocking.
    pub fn ssl_listen_port(&self) -> Result<u16> {
        // SAFETY: session valid.
        with_error(|err| unsafe { sys::ct_session_ssl_listen_port(self.ptr(), err) })
    }

    /// Whether a listen socket is open. Briefly blocking.
    pub fn is_listening(&self) -> Result<bool> {
        // SAFETY: session valid.
        with_error(|err| unsafe { sys::ct_session_is_listening(self.ptr(), err) })
    }

    /// Pauses all torrents in the session.
    pub fn pause(&self) -> Result<()> {
        // SAFETY: session valid.
        with_error(|err| unsafe { sys::ct_session_pause(self.ptr(), err) })
    }

    /// Resumes the session after [`Session::pause`].
    pub fn resume(&self) -> Result<()> {
        // SAFETY: session valid.
        with_error(|err| unsafe { sys::ct_session_resume(self.ptr(), err) })
    }

    /// Whether the session is paused. Briefly blocking.
    pub fn is_paused(&self) -> Result<bool> {
        // SAFETY: session valid.
        with_error(|err| unsafe { sys::ct_session_is_paused(self.ptr(), err) })
    }

    /// Whether the DHT is running. Briefly blocking.
    pub fn is_dht_running(&self) -> Result<bool> {
        // SAFETY: session valid.
        with_error(|err| unsafe { sys::ct_session_is_dht_running(self.ptr(), err) })
    }

    /// Set the IP filter for the session (copied). Blocked addresses are
    /// neither connected to nor accepted as incoming connections.
    pub fn set_ip_filter(&self, filter: &IpFilter) -> Result<()> {
        // SAFETY: both pointers valid; filter is copied.
        with_error(|err| unsafe { sys::ct_session_set_ip_filter(self.ptr(), filter.as_ptr(), err) })
    }

    /// Get a copy of the current IP filter.
    pub fn get_ip_filter(&self) -> Result<IpFilter> {
        // SAFETY: session valid; on success we own the returned filter.
        let ptr = with_error(|err| unsafe { sys::ct_session_get_ip_filter(self.ptr(), err) })?;
        Ok(IpFilter { ptr })
    }

    /// Set the port filter for the session (copied). Blocked destination
    /// ports are not connected to.
    pub fn set_port_filter(&self, filter: &PortFilter) -> Result<()> {
        // SAFETY: both pointers valid; filter is copied.
        with_error(|err| unsafe {
            sys::ct_session_set_port_filter(self.ptr(), filter.as_ptr(), err)
        })
    }

    /// Closes and reopens all listen and outgoing sockets, e.g. after the
    /// machine's network configuration changed. `options` is 0 or
    /// [`Session::REOPEN_MAP_PORTS`].
    pub fn reopen_network_sockets(&self, options: u32) -> Result<()> {
        // SAFETY: session valid.
        with_error(|err| unsafe {
            sys::ct_session_reopen_network_sockets(self.ptr(), options, err)
        })
    }

    /// Requests an [`Alert::StateUpdate`](crate::Alert::StateUpdate) with
    /// status snapshots of every torrent that changed since the last update
    /// (fire-and-forget). `flags` selects the optional status fields
    /// (`TorrentHandle::QUERY_*`; 0 = all non-optional fields).
    pub fn post_torrent_updates(&self, flags: u32) -> Result<()> {
        // SAFETY: session valid.
        with_error(|err| unsafe { sys::ct_session_post_torrent_updates(self.ptr(), flags, err) })
    }

    /// Asynchronously adds a torrent to the session.
    ///
    /// The returned future resolves **only while the alert stream is being
    /// polled** (see [`crate::alerts`]) — to the torrent handle on
    /// success, or the libtorrent error (e.g. duplicate hash, bad
    /// metadata). The `add_torrent_alert` it correlates on is exempt from
    /// the `alert_mask` setting, so a restrictive mask cannot starve it.
    ///
    /// Dropping the future does **not** undo the addition (it is posted
    /// before this function returns): the torrent still joins the session.
    /// The future (and the handle it resolves to) borrows the session.
    ///
    /// `data` is the torrent's [`ClientData`](crate::ClientData), kept for
    /// the torrent's lifetime: retrieve it via
    /// [`TorrentHandle::client_data`](crate::TorrentHandle::client_data),
    /// persist it through the handle-aware resume-data writers. Pass
    /// `Arc::new(())` to attach nothing. When the add resolves to an
    /// already-present torrent (duplicate hash without
    /// [`TorrentFlags::DUPLICATE_IS_ERROR`](crate::TorrentFlags::DUPLICATE_IS_ERROR)),
    /// `data` is discarded — the existing torrent keeps the data it was
    /// added with.
    pub fn add_torrent<'s>(
        &'s self,
        params: &crate::params::AddTorrentParams,
        data: Arc<dyn crate::ClientData>,
    ) -> impl Future<Output = Result<crate::handle::TorrentHandle<'s>>> + Send + use<'s> {
        let queued = self
            .inner
            .registry
            .enqueue_add_torrent(data)
            .and_then(|(token, rx)| {
                // SAFETY: session and params valid; params are copied.
                match with_error(|err| unsafe {
                    sys::ct_session_async_add_torrent(self.ptr(), params.as_ptr(), token, err)
                }) {
                    Ok(()) => Ok((
                        rx,
                        AddTorrentToken::new(Arc::clone(&self.inner.registry), token),
                    )),
                    Err(e) => {
                        self.inner.registry.abort_add_torrent(token);
                        Err(e)
                    }
                }
            });
        async move {
            match queued {
                // _guard unregisters the token if dropped unresolved.
                Ok((rx, _guard)) => {
                    let raw = rx.await.map_err(|_| Registry::closed_error())??;
                    Ok(crate::handle::TorrentHandle::from_raw(raw, self))
                }
                Err(e) => Err(e),
            }
        }
    }

    /// Looks a torrent up by its client-data token
    /// ([`TorrentHandle::client_data_token`](crate::TorrentHandle::client_data_token),
    /// minted by [`Session::add_torrent`]). Returns `None` for unknown
    /// tokens and for torrents whose removal alert has already been
    /// processed.
    pub fn find_torrent_by_token(&self, token: u64) -> Option<crate::handle::TorrentHandle<'_>> {
        let raw = self.inner.registry.torrent_handle(token)?;
        Some(crate::handle::TorrentHandle::from_raw(raw, self))
    }

    /// Looks a torrent up by its v1 (SHA-1) info-hash; hybrid torrents are
    /// found by either hash form. Returns `None` when no torrent matches.
    /// Briefly blocking (round-trips to the network thread), unlike
    /// [`find_torrent_by_token`](Session::find_torrent_by_token).
    pub fn find_torrent_v1(
        &self,
        hash: crate::types::Sha1Hash,
    ) -> Option<crate::handle::TorrentHandle<'_>> {
        let ct = sys::ct_sha1 { data: hash.0 };
        let mut out = std::mem::MaybeUninit::<sys::ct_torrent_handle>::uninit();
        // SAFETY: session valid; on `true` the shim placement-constructed
        // an owned handle into `out`, whose ownership we take.
        unsafe {
            sys::ct_session_find_torrent_v1(self.ptr(), &ct, out.as_mut_ptr())
                .then(|| crate::handle::TorrentHandle::from_owned(out.assume_init(), self))
        }
    }

    /// Looks a torrent up by its v2 (SHA-256) info-hash; the v2 twin of
    /// [`find_torrent_v1`](Session::find_torrent_v1).
    pub fn find_torrent_v2(
        &self,
        hash: crate::types::Sha256Hash,
    ) -> Option<crate::handle::TorrentHandle<'_>> {
        let ct = sys::ct_sha256 { data: hash.0 };
        let mut out = std::mem::MaybeUninit::<sys::ct_torrent_handle>::uninit();
        // SAFETY: as in `find_torrent_v1`.
        unsafe {
            sys::ct_session_find_torrent_v2(self.ptr(), &ct, out.as_mut_ptr())
                .then(|| crate::handle::TorrentHandle::from_owned(out.assume_init(), self))
        }
    }

    /// Serializes an [`AddTorrentParams`](crate::params::AddTorrentParams) to a
    /// bencoded buffer for resume data persistence.
    pub fn write_resume_data(params: &crate::params::AddTorrentParams) -> Result<Vec<u8>> {
        // SAFETY: params valid; on success we own the buffer and must free it.
        let mut buf =
            with_error(|err| unsafe { sys::ct_write_resume_data_buf(params.as_ptr(), err) })?;
        let bytes = if buf.ptr.is_null() {
            Vec::new()
        } else {
            // SAFETY: ptr/len describe the owned buffer.
            unsafe { std::slice::from_raw_parts(buf.ptr, buf.len) }.to_vec()
        };
        // SAFETY: frees the buffer returned above.
        unsafe { sys::ct_buf_free(&mut buf) };
        Ok(bytes)
    }

    /// Serializes `params` like
    /// [`write_resume_data`](Session::write_resume_data), additionally
    /// embedding `data` under the resume data's `"rbt-data"` key — without
    /// needing a live torrent (e.g. rewriting resume records offline).
    /// Data serializing to nothing writes no key.
    pub fn write_resume_data_with(
        params: &crate::params::AddTorrentParams,
        data: &dyn crate::ClientData,
    ) -> Result<Vec<u8>> {
        let blob = data.to_bencode();
        // SAFETY: params and blob are valid; on success we own the buffer
        // and must free it. An empty blob writes no "rbt-data" key.
        let mut buf = with_error(|err| unsafe {
            sys::ct_write_resume_data_buf_ex(
                params.as_ptr(),
                sys::ct_span {
                    ptr: blob.as_ptr(),
                    len: blob.len(),
                },
                err,
            )
        })?;
        let bytes = if buf.ptr.is_null() {
            Vec::new()
        } else {
            // SAFETY: ptr/len describe the owned buffer.
            unsafe { std::slice::from_raw_parts(buf.ptr, buf.len) }.to_vec()
        };
        // SAFETY: frees the buffer returned above.
        unsafe { sys::ct_buf_free(&mut buf) };
        Ok(bytes)
    }

    /// Reads an [`AddTorrentParams`](crate::params::AddTorrentParams) from
    /// bencoded resume data, together with the raw
    /// [`ClientData`](crate::ClientData) bytes stored under the resume
    /// data's `"rbt-data"` key (`None` when absent, e.g. for resume data
    /// written without client data). See
    /// [`read_resume_data_with`](Session::read_resume_data_with) for the
    /// typed form.
    pub fn read_resume_data(
        bencoded: &[u8],
        limits: Option<crate::params::LoadTorrentLimits>,
    ) -> Result<(crate::params::AddTorrentParams, Option<Vec<u8>>)> {
        let limits_ct = limits.map(|l| l.to_ct());
        let limits_ptr = limits_ct
            .as_ref()
            .map(|l| l as *const _)
            .unwrap_or(std::ptr::null());
        let mut extra = sys::ct_buf::default();
        // SAFETY: span borrows `bencoded`; on success we own the returned
        // params and the extra buffer.
        let ptr = with_error(|err| unsafe {
            sys::ct_read_resume_data_ex(
                sys::ct_span {
                    ptr: bencoded.as_ptr(),
                    len: bencoded.len(),
                },
                limits_ptr,
                &mut extra,
                err,
            )
        })?;
        let bytes = if extra.ptr.is_null() || extra.len == 0 {
            None
        } else {
            // SAFETY: ptr/len describe the owned buffer.
            Some(unsafe { std::slice::from_raw_parts(extra.ptr, extra.len) }.to_vec())
        };
        // SAFETY: frees the buffer filled above (no-op when zeroed).
        unsafe { sys::ct_buf_free(&mut extra) };
        Ok((
            // SAFETY: on success we own the returned params.
            unsafe { crate::params::AddTorrentParams::from_owned_ptr(ptr) },
            bytes,
        ))
    }

    /// Reads resume data and decodes its client-data blob into a concrete
    /// [`ClientData`](crate::ClientData) type; an absent `"rbt-data"` key
    /// decodes as `T::from_bencode(None)` (the type's defaults), which is
    /// what makes resume data written before `T` existed load cleanly.
    pub fn read_resume_data_with<T: crate::ClientData>(
        bencoded: &[u8],
        limits: Option<crate::params::LoadTorrentLimits>,
    ) -> Result<(crate::params::AddTorrentParams, T)> {
        let (params, extra) = Self::read_resume_data(bencoded, limits)?;
        Ok((params, T::from_bencode(extra.as_deref())?))
    }

    #[inline]
    pub(crate) fn ptr(&self) -> *mut sys::ct_session {
        self.inner.ptr.as_ptr()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_params() -> SessionParams {
        let mut settings = SettingsPack::new();
        settings
            .listen_interfaces(&[crate::ListenEndpoint::new("127.0.0.1", 0)])
            .unwrap()
            .enable_dht(false)
            .enable_lsd(false)
            .enable_upnp(false)
            .enable_natpmp(false)
            .alert_mask(i32::MAX);
        SessionParams::new().settings(&settings)
    }

    #[tokio::test]
    async fn session_lifecycle_and_listen_alert() {
        let session = Session::new(test_params()).expect("session");

        // raw alert pump until the listen_succeeded alert shows up
        // SAFETY: raw shim usage mirroring the future Alerts API; the batch
        // outlives every pointer read from it and is popped serially.
        let saw_listen = unsafe {
            use libctorrent_sys as sys;
            let batch = sys::ct_alert_batch_new();
            assert!(!batch.is_null());
            let mut found = false;
            'outer: for _ in 0..50 {
                let _ = with_error(|err| sys::ct_session_wait_for_alert(session.ptr(), 100, err));
                with_error(|err| sys::ct_session_pop_alerts(session.ptr(), batch, err))
                    .expect("pop");
                for i in 0..sys::ct_alert_batch_len(batch) {
                    let alert = sys::ct_alert_batch_get(batch, i);
                    let ty = sys::ct_alert_type(alert);
                    assert!(sys::ct_alert_timestamp_us(alert) > 0);
                    let what = sys::ct_alert_what(alert);
                    assert!(!what.ptr.is_null() && what.len > 0);
                    if ty == sys::CT_ALERT_TYPE_LISTEN_SUCCEEDED as i32 {
                        let mut msg = sys::ct_str::default();
                        sys::ct_alert_message(alert, &mut msg);
                        let text = std::str::from_utf8(std::slice::from_raw_parts(
                            msg.ptr.cast(),
                            msg.len,
                        ))
                        .unwrap()
                        .to_owned();
                        sys::ct_str_free(&mut msg);
                        assert!(text.contains("127.0.0.1"), "{text}");
                        found = true;
                        break 'outer;
                    }
                }
            }
            sys::ct_alert_batch_free(batch);
            found
        };
        assert!(saw_listen, "no listen_succeeded alert within timeout");

        assert!(session.is_listening().unwrap());
        assert!(session.listen_port().unwrap() > 0);
        assert!(!session.is_dht_running().unwrap());
        session.close().await;
    }

    #[tokio::test]
    async fn settings_roundtrip_through_session() {
        let session = Session::new(test_params()).expect("session");

        let mut delta = SettingsPack::new();
        delta.upload_rate_limit(123_456).user_agent("rbtorrent/0.1");
        session.apply_settings(&delta).unwrap();

        let effective = session.settings().unwrap();
        assert_eq!(effective.get_upload_rate_limit(), Some(123_456));
        assert_eq!(effective.get_user_agent().as_deref(), Some("rbtorrent/0.1"));
        // effective settings contain every known setting
        assert_eq!(effective.get_enable_dht(), Some(false));

        session.close().await;
    }

    #[tokio::test]
    async fn pause_resume() {
        let session = Session::new(test_params()).expect("session");
        assert!(!session.is_paused().unwrap());
        session.pause().unwrap();
        assert!(session.is_paused().unwrap());
        session.resume().unwrap();
        assert!(!session.is_paused().unwrap());
        session.close().await;
    }
}
