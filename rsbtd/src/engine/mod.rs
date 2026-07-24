// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The engine: owns the libtorrent session, the alert pump, the torrent
//! registry, and persistence.
//!
//! No long-lived [`Session`] references leave the engine: API layers get
//! lifetime-scoped handles via [`Engine::with_handle`], so shutdown ends
//! with exclusive ownership (`Session::close` consumes the session by
//! value). Fire-and-forget operations are matched to their response
//! alerts via [`correlate`].

pub mod correlate;
pub mod events;
pub mod jobs;
pub mod persist;
mod pump;
pub mod registry;

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rbtorrent::{
    AddTorrentParams, AlertCategory, InfoHash, RemoveFlags, SaveStateFlags, Session, SessionParams,
    SettingsPack, TorrentFlags, TorrentHandle,
};
use tokio::sync::{Notify, broadcast, mpsc, watch};
use tokio::task::JoinHandle;

use crate::config::Config;
use events::{Event, EventKind, TorrentRef, TrackerInfo};
use persist::{PersistOp, StatePaths};
use registry::{Registry, TorrentEntry};

/// The exact alert mask the engine's events are generated from. Not
/// user-configurable (request/response alerts are posted regardless).
pub const ALERT_MASK: AlertCategory = AlertCategory::from_bits(
    AlertCategory::ERROR.bits()        // TorrentError, SessionError, MetadataFailed, various failures
        | AlertCategory::STATUS.bits()    // TorrentAdded, TorrentRemoved, TorrentFinished, StateChanged, TorrentDeleted
        | AlertCategory::STORAGE.bits()   // SaveResumeData, FileRenamed, StorageMoved, ReadPiece
        | AlertCategory::TRACKER.bits()   // TrackerList, ScrapeReply, ScrapeFailed
        | AlertCategory::PEER.bits()      // PeerInfo
        | AlertCategory::FILE_PROGRESS.bits(), // FileProgress
);

/// Alert queue size, pinned effectively unlimited (a drop is fatal; see
/// the pump). i32::MAX is overflow-safe: the vendored emplace_alert
/// divides the queue length by (1 + priority), never multiplying the limit.
pub const ALERT_QUEUE_SIZE: i32 = i32::MAX;

const EVENT_BUS_CAPACITY: usize = 4096;
const PERSIST_QUEUE_CAPACITY: usize = 1024;
const SWEEP_INTERVAL: Duration = Duration::from_secs(180);
const STATE_UPDATE_INTERVAL: Duration = Duration::from_secs(1);

fn session_state_flags() -> SaveStateFlags {
    SaveStateFlags::SETTINGS | SaveStateFlags::DHT_STATE | SaveStateFlags::IP_FILTER
}

/// Logs a fatal engine invariant violation and aborts the process.
/// `abort`, not `panic!`: a panic inside the alert pump unwinds silently
/// (its JoinHandle is only inspected at shutdown), leaving a zombie
/// daemon. Stderr + a short grace outruns Windows' non-blocking appender.
fn fatal(msg: impl std::fmt::Display) -> ! {
    tracing::error!("fatal: {msg}");
    eprintln!("rsbtd: fatal: {msg}");
    std::thread::sleep(Duration::from_millis(200));
    std::process::abort()
}

/// Engine-level errors, surfaced to the API as GraphQL errors.
#[derive(Debug)]
pub enum EngineError {
    Torrent(rbtorrent::Error),
    /// The torrent is not in the session.
    NotFound,
    /// The response alert did not arrive in time.
    Timeout,
    ShuttingDown,
    Invalid(String),
    /// A state-directory I/O failure.
    Io(std::io::Error),
}

impl std::fmt::Display for EngineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EngineError::Torrent(e) => write!(f, "libtorrent error: {e}"),
            EngineError::NotFound => write!(f, "torrent not found"),
            EngineError::Timeout => write!(f, "timed out waiting for the operation to complete"),
            EngineError::ShuttingDown => write!(f, "the daemon is shutting down"),
            EngineError::Invalid(msg) => write!(f, "invalid request: {msg}"),
            EngineError::Io(e) => write!(f, "state directory I/O error: {e}"),
        }
    }
}

impl std::error::Error for EngineError {}

impl From<rbtorrent::Error> for EngineError {
    fn from(e: rbtorrent::Error) -> Self {
        EngineError::Torrent(e)
    }
}

/// Counts in-flight resume-data saves so shutdown can wait for the flush.
///
/// Save alerts carry no request identity, so the accounting only works if
/// every `save_resume_data` post goes through an accounted engine path:
/// [`inc`](Inflight::inc) before the post, [`dec`](Inflight::dec) when the
/// post is refused or its response has been fully handled. A save posted
/// outside these paths steals a token from whatever tracked save is
/// concurrently outstanding (letting shutdown stop before its own flush
/// lands), or trips the underflow check.
#[derive(Default)]
pub struct Inflight {
    count: AtomicUsize,
    notify: Notify,
}

impl Inflight {
    pub fn inc(&self) {
        self.count.fetch_add(1, Ordering::SeqCst);
    }

    /// Strict decrement: consumes one token when a save response has been
    /// fully handled (persisted or failed), or undoes an
    /// [`inc`](Inflight::inc) after a refused post. An underflow means a
    /// save was posted outside the accounted paths, and it is fatal.
    pub fn dec(&self) {
        match self
            .count
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |c| c.checked_sub(1))
        {
            Ok(prev) => {
                if prev <= 1 {
                    self.notify.notify_waiters();
                }
            }
            Err(_) => fatal("in-flight resume-save counter underflow"),
        }
    }

    pub async fn wait_zero(&self) {
        loop {
            let notified = self.notify.notified();
            if self.count.load(Ordering::SeqCst) == 0 {
                return;
            }
            notified.await;
        }
    }
}

/// Torrent ids whose durable resume state is uncertain after a failed
/// resume write. The pump re-requests these saves, shutdown includes them
/// in the final flush, and the persister clears an id once a write lands.
#[derive(Default)]
pub struct DirtyResume(Mutex<HashSet<u32>>);

impl DirtyResume {
    pub fn insert(&self, id: u32) {
        self.0.lock().unwrap().insert(id);
    }

    pub fn remove(&self, id: u32) {
        self.0.lock().unwrap().remove(&id);
    }

    pub fn contains(&self, id: u32) -> bool {
        self.0.lock().unwrap().contains(&id)
    }

    pub fn is_empty(&self) -> bool {
        self.0.lock().unwrap().is_empty()
    }

    pub fn snapshot(&self) -> Vec<u32> {
        self.0.lock().unwrap().iter().copied().collect()
    }

    pub fn extend(&self, ids: impl IntoIterator<Item = u32>) {
        self.0.lock().unwrap().extend(ids);
    }
}

/// The daemon engine: [`Engine::start`] creates, [`Engine::shutdown`] stops.
pub struct Engine {
    session: Mutex<Option<Arc<Session>>>,
    registry: Arc<Registry>,
    events: broadcast::Sender<Arc<Event>>,
    persist_tx: Mutex<Option<mpsc::Sender<PersistOp>>>,
    inflight: Arc<Inflight>,
    dirty: Arc<DirtyResume>,
    paths: StatePaths,
    /// Stops the auxiliary tasks (sweep, ticker) at shutdown start.
    aux_shutdown: watch::Sender<bool>,
    /// Set at shutdown start: the pump stops initiating resume saves.
    quiesce: Arc<AtomicBool>,
    /// Stops the pump after the final resume flush.
    pump_shutdown: watch::Sender<bool>,
    pump_task: Mutex<Option<JoinHandle<()>>>,
    persister_task: Mutex<Option<JoinHandle<()>>>,
    sweep_task: Mutex<Option<JoinHandle<()>>>,
    ticker_task: Mutex<Option<JoinHandle<()>>>,
    /// Live `torrentChanged` subscriptions; the ticker posts only while non-zero.
    state_interest: Arc<AtomicUsize>,
    jobs: jobs::JobManager,
    grace: Duration,
    /// Serializes correlated operations whose response alerts carry no
    /// request key (see [`OpClass`]): otherwise two concurrent moves on
    /// one torrent would both resolve from the first `StorageMoved` alert.
    op_locks: OpLocks,
    /// Serializes session-state persistence (mutate → snapshot → durable
    /// ack) so a concurrent writer cannot persist an older snapshot last.
    session_state_lock: tokio::sync::Mutex<()>,
    /// Cancelled at shutdown start: correlated waits end promptly and
    /// release their session references.
    shutdown_token: tokio_util::sync::CancellationToken,
}

/// Operation classes that must run one-at-a-time per torrent because
/// their response alerts are indistinguishable between requests.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum OpClass {
    MoveStorage,
    RenameFile,
    Scrape,
}

/// Per-(torrent, class) locks; weak so a lock dies with its last holder.
type OpLocks = Mutex<HashMap<(u32, OpClass), std::sync::Weak<tokio::sync::Mutex<()>>>>;

/// Keeps the state-update ticker running while a subscriber exists
/// (obtained from [`Engine::state_interest`]; drop to release).
pub struct StateInterest(Arc<AtomicUsize>);

impl Drop for StateInterest {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

impl Engine {
    /// Boots a session from the state directory (creating it if needed),
    /// starts the pump/persister tasks, and re-adds all persisted torrents.
    ///
    /// `initial_settings` seeds a *fresh* session only (typically tests);
    /// an existing session state takes precedence (settings are API-managed).
    pub async fn start(
        config: &Config,
        initial_settings: Option<SettingsPack>,
    ) -> Result<Arc<Engine>, EngineError> {
        let paths = StatePaths::new(config.state_dir.clone());
        paths.create_dirs().map_err(EngineError::Io)?;

        let mut defaults = initial_settings.unwrap_or_default();
        if defaults.get_user_agent().is_none() {
            defaults.user_agent(concat!("rsbtd/", env!("CARGO_PKG_VERSION")));
        }
        // Daemon-owned alert settings: exact mask, unlimited queue.
        defaults.alert_mask(ALERT_MASK.bits_i32());
        defaults
            .alert_queue_size(ALERT_QUEUE_SIZE)
            .expect("engine queue size is in domain");

        let mut params = SessionParams::new().settings(&defaults);
        let state_path = paths.session_state();
        match tokio::fs::read(&state_path).await {
            // SAFETY: session.state lives in the owner-only state dir and
            // is only written by persist_session_state (validated settings).
            Ok(bytes) => match unsafe {
                SessionParams::new()
                    .settings(&defaults)
                    .load_state(&bytes, session_state_flags())
            } {
                Ok(loaded) => params = loaded,
                Err(e) => {
                    tracing::warn!("ignoring corrupt session state: {e}");
                    quarantine(&state_path).await;
                }
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(EngineError::Io(e)),
        }

        let session = Arc::new(Session::new(params)?);

        // The restored state replaces settings wholesale; re-pin the
        // daemon-owned alert settings it may have overridden.
        let effective = session.settings()?;
        let mut fix = SettingsPack::new();
        let mut need_fix = false;
        if effective.get_alert_mask() != Some(ALERT_MASK.bits_i32()) {
            fix.alert_mask(ALERT_MASK.bits_i32());
            need_fix = true;
        }
        if effective.get_alert_queue_size() != Some(ALERT_QUEUE_SIZE) {
            fix.alert_queue_size(ALERT_QUEUE_SIZE)
                .expect("engine queue size is in domain");
            need_fix = true;
        }
        if need_fix {
            session.apply_settings(&fix)?;
        }

        let registry = Arc::new(Registry::new());
        let (events, _) = broadcast::channel(EVENT_BUS_CAPACITY);
        let (persist_tx, persist_rx) = mpsc::channel(PERSIST_QUEUE_CAPACITY);
        let inflight = Arc::new(Inflight::default());
        let dirty = Arc::new(DirtyResume::default());
        let quiesce = Arc::new(AtomicBool::new(false));
        let (aux_shutdown, _) = watch::channel(false);
        let (pump_shutdown, pump_shutdown_rx) = watch::channel(false);

        let persister_task = tokio::spawn(persist::run_persister(
            paths.clone(),
            persist_rx,
            events.clone(),
            Arc::clone(&inflight),
            Arc::clone(&dirty),
        ));

        let pump_task = tokio::spawn(pump::run(pump::PumpCtx {
            session: Arc::clone(&session),
            registry: Arc::clone(&registry),
            events: events.clone(),
            persist: persist_tx.clone(),
            inflight: Arc::clone(&inflight),
            dirty: Arc::clone(&dirty),
            quiesce: Arc::clone(&quiesce),
            shutdown: pump_shutdown_rx,
        }));

        let sweep_task = tokio::spawn(run_sweep(
            Arc::clone(&session),
            Arc::clone(&registry),
            Arc::clone(&inflight),
            aux_shutdown.subscribe(),
        ));

        let state_interest = Arc::new(AtomicUsize::new(0));
        let ticker_task = tokio::spawn(run_state_ticker(
            Arc::clone(&session),
            Arc::clone(&state_interest),
            aux_shutdown.subscribe(),
        ));

        let engine = Arc::new(Engine {
            session: Mutex::new(Some(session)),
            registry,
            events,
            persist_tx: Mutex::new(Some(persist_tx)),
            inflight,
            dirty,
            paths,
            aux_shutdown,
            quiesce,
            pump_shutdown,
            pump_task: Mutex::new(Some(pump_task)),
            persister_task: Mutex::new(Some(persister_task)),
            sweep_task: Mutex::new(Some(sweep_task)),
            ticker_task: Mutex::new(Some(ticker_task)),
            state_interest,
            jobs: jobs::JobManager::new(),
            grace: Duration::from_secs(config.shutdown_grace_secs),
            op_locks: Mutex::new(HashMap::new()),
            session_state_lock: tokio::sync::Mutex::new(()),
            shutdown_token: tokio_util::sync::CancellationToken::new(),
        });

        engine.restore().await?;
        Ok(engine)
    }

    /// Re-adds every torrent persisted in the state directory.
    ///
    /// A resume file's key can drift from its stem: a hybrid added via a
    /// v2-only magnet is keyed v2, but once its v1 hash is persisted,
    /// [`registry::resume_key`] is v1. Rename to the canonical key
    /// *before* the add — otherwise removal would miss the stale v2 file,
    /// which would resurrect the torrent on the next restart.
    async fn restore(&self) -> Result<(), EngineError> {
        let session = self.session()?;
        let mut adds = Vec::new();
        let mut dir = tokio::fs::read_dir(self.paths.torrents_dir())
            .await
            .map_err(EngineError::Io)?;
        while let Some(dirent) = dir.next_entry().await.map_err(EngineError::Io)? {
            let path = dirent.path();
            if path.extension().and_then(|e| e.to_str()) != Some(persist::RESUME_EXT) {
                continue;
            }
            let bytes = match tokio::fs::read(&path).await {
                Ok(bytes) => bytes,
                Err(e) => {
                    tracing::warn!("cannot read {}: {e}", path.display());
                    continue;
                }
            };
            match Session::read_resume_data(&bytes, None) {
                Ok(atp) => {
                    let canonical = self
                        .paths
                        .resume_file(&registry::resume_key(&atp.info_hashes()));
                    if path != canonical
                        && let Err(e) = tokio::fs::rename(&path, &canonical).await
                    {
                        tracing::warn!(
                            "cannot canonicalize {} to {}: {e}; skipping it",
                            path.display(),
                            canonical.display()
                        );
                        continue;
                    }
                    adds.push(session.add_torrent(&atp));
                }
                Err(e) => {
                    tracing::warn!("corrupt resume data {}: {e}", path.display());
                    quarantine(&path).await;
                }
            }
        }
        // All adds were already posted; sequential draining loses no concurrency.
        let mut restored = 0usize;
        for add in adds {
            match add.await {
                Ok(handle) => {
                    self.registry.upsert(handle.id(), handle.info_hashes());
                    restored += 1;
                }
                Err(e) => tracing::warn!("cannot restore torrent: {e}"),
            }
        }
        if restored > 0 {
            tracing::info!("restored {restored} torrent(s)");
        }
        Ok(())
    }

    /// The session, while the engine is running.
    fn session(&self) -> Result<Arc<Session>, EngineError> {
        self.session
            .lock()
            .unwrap()
            .clone()
            .ok_or(EngineError::ShuttingDown)
    }

    /// Runs `f` with a live handle for `entry`'s torrent: `NotFound` when
    /// the torrent left the session, `ShuttingDown` when the session is
    /// gone. The handle borrows a call-local session reference, so it
    /// cannot escape `f` or outlive shutdown sequencing.
    ///
    /// `f` must not post resume saves on the handle — an unaccounted save
    /// corrupts the [`Inflight`] flush accounting. Use
    /// [`Engine::save_resume_data`] instead.
    pub fn with_handle<T>(
        &self,
        entry: &TorrentEntry,
        f: impl FnOnce(&TorrentHandle<'_>) -> T,
    ) -> Result<T, EngineError> {
        let session = self.session()?;
        let handle = session
            .find_torrent(entry.info_hash)
            .ok_or(EngineError::NotFound)?;
        Ok(f(&handle))
    }

    pub fn registry(&self) -> &Registry {
        &self.registry
    }

    pub fn subscribe_events(&self) -> broadcast::Receiver<Arc<Event>> {
        self.events.subscribe()
    }

    /// Enqueues a persist op the caller was promised durability for:
    /// waits for queue space, errors when persistence cannot be scheduled.
    async fn enqueue_persist_durable(&self, op: PersistOp) -> Result<(), EngineError> {
        let tx = self
            .persist_tx
            .lock()
            .unwrap()
            .clone()
            .ok_or(EngineError::ShuttingDown)?;
        tx.send(op).await.map_err(|_| EngineError::ShuttingDown)
    }

    /// Persists the session state and waits until the write is durably
    /// on disk: success means the acknowledged state survives a crash.
    async fn persist_session_state(&self, bytes: Vec<u8>) -> Result<(), EngineError> {
        let (ack_tx, ack_rx) = tokio::sync::oneshot::channel();
        self.enqueue_persist_durable(PersistOp::WriteSessionState {
            bytes,
            ack: Some(ack_tx),
        })
        .await?;
        match ack_rx.await {
            Ok(Ok(())) => Ok(()),
            Ok(Err(e)) => Err(EngineError::Io(e)),
            // The persister dropped the op without processing it.
            Err(_) => Err(EngineError::ShuttingDown),
        }
    }

    // ---- torrent lifecycle -------------------------------------------------

    /// Adds a torrent. Duplicates are an error.
    pub async fn add_torrent(
        &self,
        atp: &mut AddTorrentParams,
    ) -> Result<Arc<TorrentEntry>, EngineError> {
        atp.set_flags(atp.flags() | TorrentFlags::DUPLICATE_IS_ERROR);
        let session = self.session()?;
        let handle = session.add_torrent(atp).await?;
        let entry = self.registry.upsert(handle.id(), handle.info_hashes());
        // The initial resume record comes from the original params: the
        // add alert's copy is libtorrent's selected-field snapshot, which
        // would downgrade the record (no trackers, web seeds, limits,
        // priorities, …). Restores never reach this path, so their files
        // stay untouched. Success is reported only once the record is
        // durably on disk; otherwise the add is unwound, so an
        // acknowledged torrent is never silently volatile.
        let bytes = match Session::write_resume_data(atp) {
            Ok(bytes) => bytes,
            Err(e) => {
                self.unwind_failed_add(&entry);
                return Err(e.into());
            }
        };
        let (ack_tx, ack_rx) = tokio::sync::oneshot::channel();
        self.inflight.inc();
        if self
            .enqueue_persist_durable(PersistOp::WriteResume {
                resume_key: entry.resume_key.clone(),
                torrent: TorrentRef {
                    id: entry.id,
                    info_hash: entry.info_hash,
                },
                bytes,
                ack: Some(ack_tx),
            })
            .await
            .is_err()
        {
            self.inflight.dec();
            self.unwind_failed_add(&entry);
            return Err(EngineError::ShuttingDown);
        }
        match ack_rx.await {
            Ok(Ok(())) => {}
            Ok(Err(e)) => {
                self.unwind_failed_add(&entry);
                return Err(EngineError::Io(e));
            }
            // The persister dropped the op without processing it.
            Err(_) => {
                self.unwind_failed_add(&entry);
                return Err(EngineError::ShuttingDown);
            }
        }
        // A concurrent remove (racing via the TorrentAdded event, which
        // the pump publishes before this continuation resumes) may have
        // enqueued its DeleteResume ahead of our write. Sends are FIFO,
        // so a delete enqueued later orders behind our write; this
        // compensates the other interleaving.
        if self.registry.get(entry.id).is_none() {
            let _ = self
                .enqueue_persist_durable(PersistOp::DeleteResume {
                    resume_key: entry.resume_key.clone(),
                })
                .await;
        }
        Ok(entry)
    }

    /// Best-effort removal of a torrent whose add could not be completed
    /// (its initial resume record never became durable): the caller was
    /// told the add failed, so neither the session nor the registry may
    /// keep the torrent.
    fn unwind_failed_add(&self, entry: &TorrentEntry) {
        self.dirty.remove(entry.id);
        self.registry.remove_by_id(entry.id);
        if let Ok(session) = self.session()
            && let Some(handle) = session.find_torrent(entry.info_hash)
        {
            handle.remove(RemoveFlags::empty());
        }
    }

    /// Removes a torrent and waits until the session has dropped it. With
    /// `delete_files`, success also means the files are actually gone.
    pub async fn remove_torrent(
        &self,
        info_hash: &InfoHash,
        delete_files: bool,
    ) -> Result<(), EngineError> {
        let entry = self.registry.find(info_hash).ok_or(EngineError::NotFound)?;
        let session = self.session()?;
        let flags = if delete_files {
            RemoveFlags::DELETE_FILES | RemoveFlags::DELETE_PARTFILE
        } else {
            RemoveFlags::empty()
        };
        let id = entry.id;
        // TorrentDeleted/TorrentDeleteFailed arrive after the registry
        // entry is gone and carry id 0, so match by info-hash.
        let hash = entry.info_hash;
        let handle = session
            .find_torrent(entry.info_hash)
            .ok_or(EngineError::NotFound)?;
        correlate::request(
            &self.events,
            &self.shutdown_token,
            move || {
                handle.remove(flags);
                Ok(())
            },
            move |e| {
                // Deletion alerts arrive after the registry entry is gone
                // (id 0) and carry libtorrent's hash set, which may be
                // wider than the one captured at insertion — overlap, not
                // equality, identifies the torrent.
                if !correlate::is_torrent(e, id)
                    && !e.torrent.is_some_and(|t| t.info_hash.overlaps(&hash))
                {
                    return None;
                }
                match &e.kind {
                    EventKind::TorrentRemoved if !delete_files => Some(Ok(())),
                    EventKind::TorrentDeleted if delete_files => Some(Ok(())),
                    EventKind::TorrentDeleteFailed { error } if delete_files => {
                        Some(Err(EngineError::Invalid(format!(
                            "torrent removed, but deleting its files failed: {}",
                            error
                                .as_ref()
                                .map_or_else(|| "unknown error".to_owned(), ToString::to_string)
                        ))))
                    }
                    _ => None,
                }
            },
            correlate::DEFAULT_TIMEOUT,
        )
        .await
    }

    /// Generates and persists resume data for one torrent now.
    pub async fn save_resume_data(&self, entry: &TorrentEntry) -> Result<(), EngineError> {
        let id = entry.id;
        let session = self.session()?;
        let handle = session
            .find_torrent(entry.info_hash)
            .ok_or(EngineError::NotFound)?;
        let inflight = Arc::clone(&self.inflight);
        correlate::request(
            &self.events,
            &self.shutdown_token,
            move || {
                // Inc before posting (the alert could otherwise be consumed
                // first); undo a refused post: an expired handle posts no
                // alert, and counting it would leak an in-flight save forever.
                inflight.inc();
                if !handle.save_resume_data(TorrentHandle::RESUME_SAVE_INFO_DICT) {
                    inflight.dec();
                    return Err(EngineError::NotFound);
                }
                Ok(())
            },
            |e| {
                if !correlate::is_torrent(e, id) {
                    return None;
                }
                match &e.kind {
                    EventKind::ResumeDataSaved => Some(Ok(())),
                    EventKind::ResumeDataFailed { message } => {
                        Some(Err(EngineError::Invalid(message.clone())))
                    }
                    _ => None,
                }
            },
            correlate::DEFAULT_TIMEOUT,
        )
        .await
    }

    // ---- session-wide operations -------------------------------------------

    /// Applies a settings delta and durably persists the session state
    /// before returning: an acknowledged change survives a crash; a
    /// failed write rolls the session back ("nothing changed"). Packs
    /// touching the daemon-owned alert settings are overridden.
    pub async fn apply_settings(&self, pack: &mut SettingsPack) -> Result<(), EngineError> {
        if pack.get_alert_mask().is_some() {
            pack.alert_mask(ALERT_MASK.bits_i32());
        }
        if pack.get_alert_queue_size().is_some() {
            pack.alert_queue_size(ALERT_QUEUE_SIZE)
                .expect("engine queue size is in domain");
        }
        let _serialized = self.session_state_lock.lock().await;
        let session = self.session()?;
        let previous = session.settings()?;
        session.apply_settings(pack)?;
        let result = match session.save_state(session_state_flags()) {
            Ok(bytes) => self.persist_session_state(bytes).await,
            Err(e) => Err(e.into()),
        };
        if result.is_err()
            && let Err(rollback) = session.apply_settings(&previous)
        {
            tracing::warn!("cannot roll back settings after a failed persist: {rollback}");
        }
        result
    }

    /// The full effective settings.
    pub fn settings(&self) -> Result<SettingsPack, EngineError> {
        Ok(self.session()?.settings()?)
    }

    /// Current session stats counters (index layout:
    /// [`rbtorrent::session_stats_metrics`]).
    pub async fn session_stats(&self) -> Result<Vec<i64>, EngineError> {
        let stats = self.session()?.session_stats();
        Ok(stats.await?)
    }

    pub fn pause_session(&self) -> Result<(), EngineError> {
        Ok(self.session()?.pause()?)
    }

    pub fn resume_session(&self) -> Result<(), EngineError> {
        Ok(self.session()?.resume()?)
    }

    pub fn is_session_paused(&self) -> Result<bool, EngineError> {
        Ok(self.session()?.is_paused()?)
    }

    pub fn listen_port(&self) -> Result<u16, EngineError> {
        Ok(self.session()?.listen_port()?)
    }

    pub fn ssl_listen_port(&self) -> Result<u16, EngineError> {
        Ok(self.session()?.ssl_listen_port()?)
    }

    pub fn is_listening(&self) -> Result<bool, EngineError> {
        Ok(self.session()?.is_listening()?)
    }

    pub fn is_dht_running(&self) -> Result<bool, EngineError> {
        Ok(self.session()?.is_dht_running()?)
    }

    /// Replaces the session's IP filter and durably persists the session
    /// state (like [`Engine::apply_settings`], rolled back on failure).
    pub async fn set_ip_filter(&self, filter: &rbtorrent::IpFilter) -> Result<(), EngineError> {
        let _serialized = self.session_state_lock.lock().await;
        let session = self.session()?;
        let previous = session.get_ip_filter()?;
        session.set_ip_filter(filter)?;
        let result = match session.save_state(session_state_flags()) {
            Ok(bytes) => self.persist_session_state(bytes).await,
            Err(e) => Err(e.into()),
        };
        if result.is_err()
            && let Err(rollback) = session.set_ip_filter(&previous)
        {
            tracing::warn!("cannot roll back the IP filter after a failed persist: {rollback}");
        }
        result
    }

    pub fn get_ip_filter(&self) -> Result<rbtorrent::IpFilter, EngineError> {
        Ok(self.session()?.get_ip_filter()?)
    }

    /// Requests a `StateUpdate` event with the status of changed torrents.
    pub fn post_torrent_updates(&self) -> Result<(), EngineError> {
        Ok(self.session()?.post_torrent_updates(0)?)
    }

    /// Registers interest in periodic `StateUpdate` events: the ticker
    /// posts updates every second while a [`StateInterest`] is alive.
    pub fn state_interest(&self) -> StateInterest {
        self.state_interest.fetch_add(1, Ordering::SeqCst);
        StateInterest(Arc::clone(&self.state_interest))
    }

    /// The torrent-creation job manager.
    pub fn jobs(&self) -> &jobs::JobManager {
        &self.jobs
    }

    // ---- correlated torrent queries ------------------------------------------

    /// The serialization lock for one `(torrent, operation-class)` pair,
    /// created on demand (the map holds weak refs and prunes dead ones).
    fn op_lock(&self, id: u32, class: OpClass) -> Arc<tokio::sync::Mutex<()>> {
        let mut locks = self.op_locks.lock().unwrap();
        locks.retain(|_, weak| weak.strong_count() > 0);
        match locks.get(&(id, class)).and_then(std::sync::Weak::upgrade) {
            Some(lock) => lock,
            None => {
                let lock = Arc::new(tokio::sync::Mutex::new(()));
                locks.insert((id, class), Arc::downgrade(&lock));
                lock
            }
        }
    }

    pub async fn trackers(&self, entry: &TorrentEntry) -> Result<Vec<TrackerInfo>, EngineError> {
        let id = entry.id;
        let session = self.session()?;
        let handle = session
            .find_torrent(entry.info_hash)
            .ok_or(EngineError::NotFound)?;
        correlate::request(
            &self.events,
            &self.shutdown_token,
            move || {
                handle.post_trackers();
                Ok(())
            },
            |e| match &e.kind {
                EventKind::Trackers(list) if correlate::is_torrent(e, id) => Some(Ok(list.clone())),
                _ => None,
            },
            correlate::DEFAULT_TIMEOUT,
        )
        .await
    }

    pub async fn peers(&self, entry: &TorrentEntry) -> Result<Vec<PeerSnapshot>, EngineError> {
        let id = entry.id;
        let session = self.session()?;
        let handle = session
            .find_torrent(entry.info_hash)
            .ok_or(EngineError::NotFound)?;
        correlate::request(
            &self.events,
            &self.shutdown_token,
            move || {
                handle.post_peer_info();
                Ok(())
            },
            |e| match &e.kind {
                EventKind::Peers(list) if correlate::is_torrent(e, id) => {
                    Some(Ok(list.iter().map(PeerSnapshot::from_info).collect()))
                }
                _ => None,
            },
            correlate::DEFAULT_TIMEOUT,
        )
        .await
    }

    /// Moves the torrent's storage to `path` (`mode` is one of the
    /// [`TorrentHandle::MOVE_ALWAYS_REPLACE_FILES`]-family constants),
    /// returning the new storage path when the move completed.
    pub async fn move_storage(
        &self,
        entry: &TorrentEntry,
        path: &str,
        mode: u32,
    ) -> Result<String, EngineError> {
        let id = entry.id;
        let path = path.to_owned();
        // StorageMoved alerts carry no request key: a concurrent move
        // would consume this one's response. Find the handle after the
        // lock so a removed-while-queued torrent gets NotFound, not a timeout.
        let guard = self.op_lock(id, OpClass::MoveStorage).lock_owned().await;
        let session = self.session()?;
        let handle = session
            .find_torrent(entry.info_hash)
            .ok_or(EngineError::NotFound)?;
        correlate::request_serialized(
            &self.events,
            &self.shutdown_token,
            guard,
            id,
            move || {
                handle.move_storage(&path, mode);
                Ok(())
            },
            move |e| {
                if !correlate::is_torrent(e, id) {
                    return None;
                }
                match &e.kind {
                    EventKind::StorageMoved { path } => Some(Ok(path.clone())),
                    EventKind::StorageMovedFailed { error } => Some(Err(torrent_error(error))),
                    _ => None,
                }
            },
            correlate::MOVE_STORAGE_TIMEOUT,
        )
        .await
    }

    /// Renames a file within the torrent, returning the accepted name.
    pub async fn rename_file(
        &self,
        entry: &TorrentEntry,
        index: i32,
        name: &str,
    ) -> Result<String, EngineError> {
        let id = entry.id;
        let name = name.to_owned();
        // FileRenamed alerts carry the index but not the requested name:
        // concurrent renames of one index would collide (see move_storage).
        let guard = self.op_lock(id, OpClass::RenameFile).lock_owned().await;
        let session = self.session()?;
        let handle = session
            .find_torrent(entry.info_hash)
            .ok_or(EngineError::NotFound)?;
        correlate::request_serialized(
            &self.events,
            &self.shutdown_token,
            guard,
            id,
            move || {
                handle.rename_file(index, &name)?;
                Ok(())
            },
            move |e| {
                if !correlate::is_torrent(e, id) {
                    return None;
                }
                match &e.kind {
                    EventKind::FileRenamed { index: i, new_name } if *i == index => {
                        Some(Ok(new_name.clone()))
                    }
                    EventKind::FileRenameFailed { index: i, error } if *i == index => {
                        Some(Err(torrent_error(error)))
                    }
                    _ => None,
                }
            },
            correlate::DEFAULT_TIMEOUT,
        )
        .await
    }

    pub async fn read_piece(
        &self,
        entry: &TorrentEntry,
        piece: i32,
    ) -> Result<Vec<u8>, EngineError> {
        let id = entry.id;
        let session = self.session()?;
        let handle = session
            .find_torrent(entry.info_hash)
            .ok_or(EngineError::NotFound)?;
        correlate::request(
            &self.events,
            &self.shutdown_token,
            move || {
                handle.read_piece(piece);
                Ok(())
            },
            move |e| {
                if !correlate::is_torrent(e, id) {
                    return None;
                }
                match &e.kind {
                    EventKind::ReadPiece {
                        piece: p,
                        data,
                        error,
                    } if *p == piece => match error {
                        Some(_) => Some(Err(torrent_error(error))),
                        None => Some(Ok(data.clone())),
                    },
                    _ => None,
                }
            },
            correlate::DEFAULT_TIMEOUT,
        )
        .await
    }

    /// Scrapes a tracker (`tracker_index` -1 = the working one), returning
    /// `(tracker_url, complete, incomplete)`.
    pub async fn scrape_tracker(
        &self,
        entry: &TorrentEntry,
        tracker_index: i32,
    ) -> Result<(Option<String>, i32, i32), EngineError> {
        let id = entry.id;
        // Resolve the target tracker URL first: libtorrent silently
        // ignores out-of-range indexes (we would wait out the timeout),
        // and scrape replies carry no request key, so matching by torrent
        // alone could consume an automatic scrape's reply.
        let trackers = self.trackers(entry).await?;
        let expected_url = if tracker_index == -1 {
            if trackers.is_empty() {
                return Err(EngineError::Invalid(
                    "torrent has no trackers to scrape".to_owned(),
                ));
            }
            // Mirror libtorrent: the last working tracker, else the first.
            let working = self
                .with_handle(entry, |h| h.status(0))?
                .map_err(EngineError::from)?
                .current_tracker();
            if working.is_empty() {
                trackers[0].url.clone()
            } else {
                working
            }
        } else {
            let count = trackers.len();
            match usize::try_from(tracker_index)
                .ok()
                .and_then(|i| trackers.get(i))
            {
                Some(tracker) => tracker.url.clone(),
                None => {
                    return Err(EngineError::Invalid(format!(
                        "tracker index {tracker_index} is outside 0..{count}"
                    )));
                }
            }
        };
        // Find the handle only after the lock (see move_storage).
        let guard = self.op_lock(id, OpClass::Scrape).lock_owned().await;
        let session = self.session()?;
        let handle = session
            .find_torrent(entry.info_hash)
            .ok_or(EngineError::NotFound)?;
        correlate::request_serialized(
            &self.events,
            &self.shutdown_token,
            guard,
            id,
            move || {
                handle.scrape_tracker(tracker_index);
                Ok(())
            },
            move |e| {
                if !correlate::is_torrent(e, id) {
                    return None;
                }
                // A reply without a URL cannot be attributed; accept it
                // rather than risk waiting out the timeout.
                let for_us = |url: &Option<String>| {
                    url.is_none() || url.as_deref() == Some(expected_url.as_str())
                };
                match &e.kind {
                    EventKind::ScrapeReply {
                        tracker_url,
                        incomplete,
                        complete,
                    } if for_us(tracker_url) => {
                        Some(Ok((tracker_url.clone(), *complete, *incomplete)))
                    }
                    EventKind::ScrapeFailed {
                        tracker_url,
                        error_message,
                    } if for_us(tracker_url) => {
                        Some(Err(EngineError::Invalid(error_message.clone())))
                    }
                    _ => None,
                }
            },
            correlate::DEFAULT_TIMEOUT,
        )
        .await
    }

    /// Closes and reopens all listen and outgoing sockets (e.g. after a
    /// network change).
    pub fn reopen_network_sockets(&self, map_ports: bool) -> Result<(), EngineError> {
        let options = if map_ports {
            Session::REOPEN_MAP_PORTS
        } else {
            0
        };
        Ok(self.session()?.reopen_network_sockets(options)?)
    }

    /// Downloaded byte counts, indexed by file.
    pub async fn file_progress(&self, entry: &TorrentEntry) -> Result<Vec<i64>, EngineError> {
        let id = entry.id;
        let session = self.session()?;
        let handle = session
            .find_torrent(entry.info_hash)
            .ok_or(EngineError::NotFound)?;
        correlate::request(
            &self.events,
            &self.shutdown_token,
            move || {
                handle.post_file_progress(0);
                Ok(())
            },
            |e| match &e.kind {
                EventKind::FileProgress(progress) if correlate::is_torrent(e, id) => {
                    Some(Ok(progress.clone()))
                }
                _ => None,
            },
            correlate::DEFAULT_TIMEOUT,
        )
        .await
    }

    // ---- shutdown -----------------------------------------------------------

    /// Flushes everything worth flushing without stopping anything: posts
    /// resume saves for every torrent that changed (dirty torrents flush
    /// even if libtorrent considers them clean — their last write failed),
    /// waits for the writes within `deadline`, then durably persists the
    /// session state. Safe while running (writes are idempotent and the
    /// session-state lock serializes against live settings applies);
    /// shutdown uses it as its final flush, and the Windows session-end
    /// path checkpoints ahead of the OS deadline.
    pub async fn checkpoint(&self, deadline: tokio::time::Instant) {
        // The block scopes the session Arc: at shutdown it must drop
        // before Arc::into_inner, or graceful close becomes a blocking drop.
        if let Ok(session) = self.session() {
            for entry in self.registry.list() {
                let Some(handle) = session.find_torrent(entry.info_hash) else {
                    continue;
                };
                if handle.need_save_resume_data() || self.dirty.contains(entry.id) {
                    self.inflight.inc();
                    if !handle.save_resume_data(
                        TorrentHandle::RESUME_SAVE_INFO_DICT
                            | TorrentHandle::RESUME_FLUSH_DISK_CACHE,
                    ) {
                        self.inflight.dec();
                    }
                }
            }
        }
        if tokio::time::timeout_at(deadline, self.inflight.wait_zero())
            .await
            .is_err()
        {
            tracing::warn!("resume data flush did not complete before its deadline");
        }

        // Persist session state (settings, DHT state, IP filter) within
        // the deadline: a stuck filesystem must not hang here. Behind
        // the same lock as live settings writes, so an in-flight apply
        // cannot enqueue an older snapshot after this final one.
        if let Ok(session) = self.session() {
            match tokio::time::timeout_at(deadline, self.session_state_lock.lock()).await {
                Ok(_serialized) => match session.save_state(session_state_flags()) {
                    Ok(bytes) => {
                        // Ordered behind any still-queued session-state writes.
                        let write =
                            tokio::time::timeout_at(deadline, self.persist_session_state(bytes))
                                .await;
                        match write {
                            Ok(Ok(())) => {}
                            Ok(Err(e)) => tracing::warn!("cannot write session state: {e}"),
                            Err(_) => tracing::warn!("session state write timed out"),
                        }
                    }
                    Err(e) => tracing::warn!("cannot serialize session state: {e}"),
                },
                Err(_) => {
                    tracing::warn!("session state save skipped: a settings write is stuck");
                }
            }
        }
    }

    /// Gracefully shuts the engine down: quiesces resume-save producers,
    /// flushes resume data and session state within the configured grace
    /// period, stops all tasks, and closes the session.
    pub async fn shutdown(&self) {
        self.shutdown_with(self.grace).await;
    }

    /// [`Engine::shutdown`] with an explicit grace period (the OS
    /// session-end path has a far smaller budget than a regular stop).
    pub async fn shutdown_with(&self, grace: Duration) {
        let deadline = tokio::time::Instant::now() + grace;

        // Quiesce the save producers: the pump goes drain-only (keeps
        // consuming alerts and persisting responses, initiates no saves),
        // and the sweep/ticker are stopped and joined so nothing can
        // start a save behind our back once the final flush begins.
        self.quiesce.store(true, Ordering::SeqCst);
        // End correlated waits: their holders drop the session references
        // the graceful close below needs back.
        self.shutdown_token.cancel();
        // Cancelled jobs stop producing output; joined after persistence.
        self.jobs.cancel_all();
        let _ = self.aux_shutdown.send(true);
        let sweep = self.sweep_task.lock().unwrap().take();
        if let Some(task) = sweep {
            let _ = task.await;
        }
        // The ticker holds a session Arc; it must also end before the close.
        let ticker = self.ticker_task.lock().unwrap().take();
        if let Some(task) = ticker {
            let _ = task.await;
        }

        // Final flush; the drain-only pump persists the alerts.
        self.checkpoint(deadline).await;

        // From here on, API calls fail with ShuttingDown.
        let session = self.session.lock().unwrap().take();

        // Stop the pump (drops its Alerts stream and session clone). A
        // pump blocked on a persist send cannot see the signal, so abort
        // at the deadline — a wedged pump must not keep the session alive.
        let _ = self.pump_shutdown.send(true);
        let pump = self.pump_task.lock().unwrap().take();
        if let Some(mut task) = pump
            && tokio::time::timeout_at(deadline, &mut task).await.is_err()
        {
            tracing::warn!("alert pump did not stop in time; aborting it");
            task.abort();
            let _ = task.await;
        }

        // Dropping our sender lets the persister drain its queue and
        // exit. The join is unconditional: a detached persister would
        // keep renaming and deleting files in the state directory while
        // a successor engine (the settings-restart path) may already own
        // it — losing writes is bad, corrupting the next generation's
        // state is worse. The queue is bounded and writes are local, so
        // only a genuinely hung disk can hold this up; process
        // supervisors remain the backstop for that.
        self.persist_tx.lock().unwrap().take();
        let persister = self.persister_task.lock().unwrap().take();
        if let Some(mut task) = persister
            && tokio::time::timeout_at(deadline, &mut task).await.is_err()
        {
            tracing::warn!("persister still draining past the grace; waiting for it");
            let _ = task.await;
        }

        self.jobs.join_workers(deadline).await;

        // Close the session gracefully (tracker goodbyes etc.). In-flight
        // API holders were cancelled at shutdown start (shutdown token,
        // request kill); this wait only absorbs their scheduling lag.
        if let Some(arc) = session {
            const SESSION_RELEASE_WAIT: Duration = Duration::from_secs(2);
            let release_deadline = tokio::time::Instant::now() + SESSION_RELEASE_WAIT;
            while Arc::strong_count(&arc) > 1 && tokio::time::Instant::now() < release_deadline {
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
            match Arc::into_inner(arc) {
                Some(session) => session.close().await,
                None => {
                    tracing::warn!(
                        "session still referenced at shutdown; its last owner will block on drop"
                    );
                }
            }
        }
    }
}

fn torrent_error(error: &Option<rbtorrent::Error>) -> EngineError {
    match error {
        Some(e) => EngineError::Torrent(e.clone()),
        None => EngineError::Invalid("the operation failed without an error code".to_owned()),
    }
}

/// An owned snapshot of one peer connection, copied out of a
/// [`EventKind::Peers`] event ([`rbtorrent::PeerInfo`] is a borrowed view).
#[derive(Clone, Debug)]
pub struct PeerSnapshot {
    pub address: Option<std::net::SocketAddr>,
    pub local_endpoint: Option<std::net::SocketAddr>,
    pub peer_id: [u8; 20],
    pub client: String,
    pub connection_type: rbtorrent::ConnectionType,
    pub flags: rbtorrent::PeerFlags,
    pub source: rbtorrent::PeerSourceFlags,
    pub progress_ppm: i32,
    pub down_speed: i32,
    pub up_speed: i32,
    pub payload_down_speed: i32,
    pub payload_up_speed: i32,
    pub total_download: i64,
    pub total_upload: i64,
    pub last_request_us: i64,
    pub last_active_us: i64,
    pub num_hashfails: i32,
    pub failcount: i32,
    pub download_rate_peak: i32,
    pub upload_rate_peak: i32,
    pub num_pieces: i32,
    pub rtt: i32,
}

impl PeerSnapshot {
    fn from_info(peer: &rbtorrent::PeerInfo) -> PeerSnapshot {
        PeerSnapshot {
            address: peer.remote_endpoint(),
            local_endpoint: peer.local_endpoint(),
            peer_id: peer.pid(),
            client: peer.client(),
            connection_type: peer.connection_type(),
            flags: peer.flags(),
            source: peer.source(),
            progress_ppm: peer.progress_ppm(),
            down_speed: peer.down_speed(),
            up_speed: peer.up_speed(),
            payload_down_speed: peer.payload_down_speed(),
            payload_up_speed: peer.payload_up_speed(),
            total_download: peer.total_download(),
            total_upload: peer.total_upload(),
            last_request_us: peer.last_request_us(),
            last_active_us: peer.last_active_us(),
            num_hashfails: peer.num_hashfails(),
            failcount: peer.failcount(),
            download_rate_peak: peer.download_rate_peak(),
            upload_rate_peak: peer.upload_rate_peak(),
            num_pieces: peer.num_pieces(),
            rtt: peer.rtt(),
        }
    }
}

/// Quarantines a corrupt state file by renaming it to `<name>.corrupt`.
async fn quarantine(path: &std::path::Path) {
    let mut target = path.as_os_str().to_owned();
    target.push(".corrupt");
    if let Err(e) = tokio::fs::rename(path, &target).await {
        tracing::warn!("cannot quarantine {}: {e}", path.display());
    }
}

/// Posts torrent status updates every second while subscribers exist,
/// feeding `torrentChanged` subscriptions via `StateUpdate` events.
async fn run_state_ticker(
    session: Arc<Session>,
    interest: Arc<AtomicUsize>,
    mut shutdown: watch::Receiver<bool>,
) {
    let mut tick = tokio::time::interval(STATE_UPDATE_INTERVAL);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            _ = async { let _ = shutdown.wait_for(|&stop| stop).await; } => break,
            _ = tick.tick() => {
                if interest.load(Ordering::SeqCst) > 0 {
                    let _ = session.post_torrent_updates(0);
                }
            }
        }
    }
}

/// Periodically saves resume data for torrents that changed. Holds a
/// session Arc; shutdown joins this task (and the state ticker) before
/// closing the session.
async fn run_sweep(
    session: Arc<Session>,
    registry: Arc<Registry>,
    inflight: Arc<Inflight>,
    mut shutdown: watch::Receiver<bool>,
) {
    let mut tick = tokio::time::interval(SWEEP_INTERVAL);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    tick.tick().await; // the first tick fires immediately; skip it
    loop {
        tokio::select! {
            _ = async { let _ = shutdown.wait_for(|&stop| stop).await; } => break,
            _ = tick.tick() => {
                for entry in registry.list() {
                    let Some(handle) = session.find_torrent(entry.info_hash) else {
                        continue;
                    };
                    if handle.need_save_resume_data() {
                        // Inc before posting; undo a refused post (an
                        // expired torrent), which produces no alert.
                        inflight.inc();
                        if !handle.save_resume_data(
                            TorrentHandle::RESUME_ONLY_IF_MODIFIED
                                | TorrentHandle::RESUME_SAVE_INFO_DICT,
                        ) {
                            inflight.dec();
                        }
                    }
                }
            }
        }
    }
}
