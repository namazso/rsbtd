// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The alert pump: the single task that polls the session's alert stream.
//!
//! rbtorrent has no hidden pump — `Session::add_torrent` futures and every
//! other alert-correlated operation only make progress while
//! [`rbtorrent::Alerts::next_batch`] is polled. This sole holder of the
//! alert stream translates alerts into owned events and performs engine
//! side effects. Alert views borrow the batch (raw pointers), so each
//! batch is processed fully synchronously; persist ops are sent after the
//! batch is dropped. The queue is pinned effectively unlimited
//! ([`super::ALERT_QUEUE_SIZE`]) and delivery is lossless: a dropped alert
//! voids the engine's invariants and aborts the process, so persist sends
//! may block — alerts simply accumulate meanwhile.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use rbtorrent::alerts::Alert;
use rbtorrent::{AlertType, InfoHash, RawAlert, Session, TorrentHandle};
use tokio::sync::{broadcast, mpsc, watch};

use super::events::{Event, EventKind, TorrentRef, TrackerInfo};
use super::persist::PersistOp;
use super::registry::Registry;
use super::{DirtyResume, Inflight};

/// Retry cadence for saves whose previous write failed.
const RETRY_INTERVAL: Duration = Duration::from_secs(1);

pub(super) struct PumpCtx {
    pub session: Arc<Session>,
    pub registry: Arc<Registry>,
    pub events: broadcast::Sender<Arc<Event>>,
    pub persist: mpsc::Sender<PersistOp>,
    pub inflight: Arc<Inflight>,
    pub dirty: Arc<DirtyResume>,
    /// Drain-only mode: keep consuming alerts and persisting responses,
    /// but initiate no resume saves (shutdown owns the final flush).
    pub quiesce: Arc<AtomicBool>,
    pub shutdown: watch::Receiver<bool>,
}

pub(super) async fn run(ctx: PumpCtx) {
    let PumpCtx {
        session,
        registry,
        events,
        persist,
        inflight,
        dirty,
        quiesce,
        mut shutdown,
    } = ctx;
    // The stream borrows our local Arc clone; both die with this task.
    let mut alerts = session.alerts();
    let mut retry = tokio::time::interval(RETRY_INTERVAL);
    retry.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        tokio::select! {
            biased;
            _ = async { let _ = shutdown.wait_for(|&stop| stop).await; } => break,
            _ = retry.tick(), if !dirty.is_empty() => {
                // Re-request saves whose previous write failed, asking only
                // for as much as the persist queue can take right now.
                if !quiesce.load(Ordering::SeqCst) {
                    let mut budget = persist.capacity();
                    for id in dirty.snapshot() {
                        let Some(entry) = registry.get(id) else {
                            // Removed while marked dirty.
                            dirty.remove(id);
                            continue;
                        };
                        let Some(handle) = session.find_torrent(entry.info_hash) else {
                            // Left the session while marked dirty.
                            dirty.remove(id);
                            continue;
                        };
                        if budget == 0 {
                            break;
                        }
                        budget -= 1;
                        inflight.inc();
                        if !handle.save_resume_data(TorrentHandle::RESUME_SAVE_INFO_DICT) {
                            inflight.dec();
                            dirty.remove(id);
                        }
                    }
                }
            }
            result = alerts.next_batch() => {
                let ops = match result {
                    Ok(batch) => {
                        let mut state = BatchState {
                            session: &session,
                            registry: &registry,
                            events: &events,
                            inflight: &inflight,
                            dirty: &dirty,
                            quiesce: &quiesce,
                            ops: Vec::new(),
                        };
                        for alert in batch.iter() {
                            state.handle_alert(&alert);
                        }
                        state.ops
                    }
                    Err(e) => {
                        tracing::error!("alert stream error: {e}");
                        Vec::new()
                    }
                };
                // A full persist queue back-pressures the pump here;
                // blocking is safe (alerts accumulate in the unlimited
                // alert queue). The persister strictly outlives the pump,
                // so a failed send means the persister task died.
                for op in ops {
                    if persist.send(op).await.is_err() {
                        super::fatal("persister task died; nothing can be persisted anymore");
                    }
                }
            }
        }
    }
    tracing::debug!("alert pump stopped");
}

struct BatchState<'e> {
    session: &'e Session,
    registry: &'e Registry,
    events: &'e broadcast::Sender<Arc<Event>>,
    inflight: &'e Inflight,
    dirty: &'e DirtyResume,
    quiesce: &'e AtomicBool,
    ops: Vec<PersistOp>,
}

impl BatchState<'_> {
    fn handle_alert(&mut self, alert: &Alert<'_>) {
        match alert {
            Alert::AddTorrent(a) => {
                if a.error().is_some() {
                    // The add_torrent future surfaces the error to its caller.
                    return;
                }
                let handle = a.handle();
                let entry = self.registry.upsert(handle.id(), handle.info_hashes());
                let torrent = TorrentRef {
                    id: entry.id,
                    info_hash: entry.info_hash,
                };
                // No resume write here: the alert's params are libtorrent's
                // selected-field snapshot, not full resume state, and this
                // arm also runs for restores, whose on-disk records must
                // stay untouched. Engine::add_torrent persists the initial
                // record for new adds from the original params.
                self.publish(Some(torrent), EventKind::TorrentAdded);
            }
            Alert::TorrentRemoved(a) => {
                let info_hash = a.info_hashes();
                let torrent = match self.registry.remove_by_hash(&info_hash) {
                    Some(entry) => {
                        self.dirty.remove(entry.id);
                        self.ops.push(PersistOp::DeleteResume {
                            resume_key: entry.resume_key.clone(),
                        });
                        TorrentRef {
                            id: entry.id,
                            info_hash: entry.info_hash,
                        }
                    }
                    None => TorrentRef { id: 0, info_hash },
                };
                self.publish(Some(torrent), EventKind::TorrentRemoved);
            }
            Alert::TorrentFinished(a) => {
                if let Some(torrent) = self.torrent_of(a) {
                    self.request_save(
                        torrent.id,
                        TorrentHandle::RESUME_SAVE_INFO_DICT
                            | TorrentHandle::RESUME_FLUSH_DISK_CACHE,
                    );
                    self.publish(Some(torrent), EventKind::TorrentFinished);
                }
            }
            Alert::SaveResumeData(a) => {
                let Some(torrent) = self.torrent_of(a) else {
                    self.inflight.dec();
                    return;
                };
                let Some(entry) = self.registry.get(torrent.id) else {
                    // Removed while the save was in flight.
                    self.inflight.dec();
                    return;
                };
                match a.write_resume_data() {
                    Ok(bytes) => self.ops.push(PersistOp::WriteResume {
                        resume_key: entry.resume_key.clone(),
                        torrent,
                        bytes,
                        ack: None,
                    }),
                    Err(e) => {
                        self.inflight.dec();
                        self.publish(
                            Some(torrent),
                            EventKind::ResumeDataFailed {
                                message: format!("cannot serialize resume data: {e}"),
                            },
                        );
                    }
                }
            }
            Alert::SaveResumeDataFailed(a) => {
                self.inflight.dec();
                let torrent = self.torrent_of(a);
                let message = a
                    .error()
                    .map_or_else(|| "unknown error".to_owned(), |e| e.to_string());
                self.publish(torrent, EventKind::ResumeDataFailed { message });
            }
            Alert::StateUpdate(a) => {
                self.publish(None, EventKind::StateUpdate(a.statuses()));
            }
            Alert::StateChanged(a) => {
                let torrent = self.torrent_of(a);
                self.publish(
                    torrent,
                    EventKind::StateChanged {
                        state: a.state(),
                        prev_state: a.prev_state(),
                    },
                );
            }
            Alert::TorrentError(a) => {
                let torrent = self.torrent_of(a);
                self.publish(
                    torrent,
                    EventKind::TorrentError {
                        error: a.error(),
                        filename: a.filename().into_owned(),
                    },
                );
            }
            Alert::MetadataFailed(a) => {
                let torrent = self.torrent_of(a);
                self.publish(torrent, EventKind::MetadataFailed { error: a.error() });
            }
            Alert::FileRenamed(a) => {
                let torrent = self.torrent_of(a);
                self.publish(
                    torrent,
                    EventKind::FileRenamed {
                        index: a.index(),
                        new_name: a.new_name().into_owned(),
                    },
                );
            }
            Alert::FileRenameFailed(a) => {
                let torrent = self.torrent_of(a);
                self.publish(
                    torrent,
                    EventKind::FileRenameFailed {
                        index: a.index(),
                        error: a.error(),
                    },
                );
            }
            Alert::StorageMoved(a) => {
                let torrent = self.torrent_of(a);
                if let Some(t) = &torrent {
                    // Unconditional (no ONLY_IF_MODIFIED): the metadata-less
                    // move branch never sets libtorrent's need-save flag, so
                    // neither the sweep nor the shutdown flush would ever
                    // persist the new path.
                    self.request_save(t.id, TorrentHandle::RESUME_SAVE_INFO_DICT);
                }
                self.publish(
                    torrent,
                    EventKind::StorageMoved {
                        path: a.storage_path().into_owned(),
                    },
                );
            }
            Alert::StorageMovedFailed(a) => {
                let torrent = self.torrent_of(a);
                self.publish(torrent, EventKind::StorageMovedFailed { error: a.error() });
            }
            Alert::ReadPiece(a) => {
                let torrent = self.torrent_of(a);
                self.publish(
                    torrent,
                    EventKind::ReadPiece {
                        piece: a.piece(),
                        data: a.data().to_vec(),
                        error: a.error(),
                    },
                );
            }
            Alert::TrackerList(a) => {
                let torrent = self.torrent_of(a);
                let trackers = a
                    .iter()
                    .map(|t| TrackerInfo {
                        url: t.url.into_owned(),
                        trackerid: t.trackerid.into_owned(),
                        tier: t.tier,
                        fail_limit: t.fail_limit,
                        source: t.source,
                        verified: t.verified,
                    })
                    .collect();
                self.publish(torrent, EventKind::Trackers(trackers));
            }
            Alert::PeerInfo(a) => {
                let torrent = self.torrent_of(a);
                self.publish(torrent, EventKind::Peers(a.peers()));
            }
            Alert::FileProgress(a) => {
                let torrent = self.torrent_of(a);
                self.publish(torrent, EventKind::FileProgress(a.progress().to_vec()));
            }
            Alert::ScrapeReply(a) => {
                let torrent = self.torrent_of(a);
                let tracker_url = a.tracker_url().map(|u| u.into_owned());
                self.publish(
                    torrent,
                    EventKind::ScrapeReply {
                        tracker_url,
                        incomplete: a.incomplete(),
                        complete: a.complete(),
                    },
                );
            }
            Alert::ScrapeFailed(a) => {
                let torrent = self.torrent_of(a);
                let tracker_url = a.tracker_url().map(|u| u.into_owned());
                self.publish(
                    torrent,
                    EventKind::ScrapeFailed {
                        tracker_url,
                        error_message: a.error_message().into_owned(),
                    },
                );
            }
            Alert::TorrentDeleted(a) => {
                let torrent = self.torrent_by_hash(a.info_hashes());
                self.publish(Some(torrent), EventKind::TorrentDeleted);
            }
            Alert::TorrentDeleteFailed(a) => {
                let torrent = self.torrent_by_hash(a.info_hashes());
                self.publish(
                    Some(torrent),
                    EventKind::TorrentDeleteFailed { error: a.error() },
                );
            }
            Alert::AlertsDropped(a) => {
                // The queue is pinned to i32::MAX (see ALERT_QUEUE_SIZE),
                // so this cannot happen short of resource exhaustion.
                // Alerts were irrecoverably lost and every downstream
                // invariant is void; die loudly instead of limping on.
                let dropped: Vec<String> = (0..128)
                    .filter(|&ty| a.dropped(ty))
                    .map(|ty| match AlertType::from_raw(ty) {
                        Some(known) => format!("{known:?}"),
                        None => format!("#{ty}"),
                    })
                    .collect();
                super::fatal(format!(
                    "libtorrent dropped alerts ({})",
                    dropped.join(", ")
                ));
            }
            Alert::SessionError(a) => {
                let error = a.error();
                tracing::error!("session error: {error}");
                self.publish(None, EventKind::SessionError { error });
            }
            Alert::Other(raw) if raw.alert_type() == Some(AlertType::MetadataReceived) => {
                if let Some(handle) = raw.torrent_handle() {
                    let id = handle.id();
                    if let Some(entry) = self.registry.reindex(id, handle.info_hashes()) {
                        // Persist the freshly arrived metadata with the
                        // resume data so a restart re-adds a full torrent.
                        self.request_save(id, TorrentHandle::RESUME_SAVE_INFO_DICT);
                        self.publish(
                            Some(TorrentRef {
                                id,
                                info_hash: entry.info_hash,
                            }),
                            EventKind::MetadataReceived,
                        );
                    }
                }
            }
            _ => {}
        }
    }

    fn publish(&self, torrent: Option<TorrentRef>, kind: EventKind) {
        let _ = self.events.send(Arc::new(Event { torrent, kind }));
    }

    /// Requests resume data for a registered torrent, counting it in flight.
    /// No-op while quiescing: shutdown owns the final flush, and a save
    /// initiated here could complete after shutdown stops waiting.
    fn request_save(&self, id: u32, flags: u32) {
        if self.quiesce.load(Ordering::SeqCst) {
            return;
        }
        if let Some(entry) = self.registry.get(id) {
            let Some(handle) = self.session.find_torrent(entry.info_hash) else {
                return;
            };
            // Inc before posting; undo a refused post (an expired
            // torrent), which produces no alert to decrement on.
            self.inflight.inc();
            if !handle.save_resume_data(flags) {
                self.inflight.dec();
            }
        }
    }

    /// The torrent an alert belongs to, via its (still valid) handle.
    fn torrent_of(&self, raw: &RawAlert<'_>) -> Option<TorrentRef> {
        let handle = raw.torrent_handle()?;
        let id = handle.id();
        if id == 0 {
            return None;
        }
        let info_hash = match self.registry.get(id) {
            Some(entry) => entry.info_hash,
            None => handle.info_hashes(),
        };
        Some(TorrentRef { id, info_hash })
    }

    /// A torrent ref from an alert carrying info-hashes directly (for
    /// alerts whose handle may already be invalid, e.g. post-removal).
    fn torrent_by_hash(&self, info_hash: InfoHash) -> TorrentRef {
        let id = self.registry.find(&info_hash).map_or(0, |entry| entry.id);
        TorrentRef { id, info_hash }
    }
}
