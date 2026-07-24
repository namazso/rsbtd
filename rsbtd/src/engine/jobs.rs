// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Torrent-creation jobs.
//!
//! Creating a .torrent hashes every payload byte, so it runs on a
//! blocking thread: `start` returns a job id immediately, progress is a
//! watch channel, and the result is written to an output path or kept in
//! memory. `cancel` takes effect through the hashing progress callback:
//! the worker aborts within one piece and its output is thrown away.
//! Terminal jobs are pruned [`JOB_TTL`] after their end state is first
//! observed (pruning piggy-backs on manager access).

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use rbtorrent::{CreateFlags, CreateTorrent, list_files, set_piece_hashes};
use tokio::sync::watch;

use super::EngineError;
use super::persist::write_atomic;

/// How long a finished/failed/cancelled job stays queryable.
pub const JOB_TTL: Duration = Duration::from_secs(3600);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JobState {
    /// Enumerating files.
    Listing,
    /// Reading payload data and hashing pieces.
    Hashing,
    Finished,
    Failed,
    Cancelled,
}

impl JobState {
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            JobState::Finished | JobState::Failed | JobState::Cancelled
        )
    }
}

#[derive(Clone, Debug)]
pub struct JobSnapshot {
    pub id: u64,
    pub state: JobState,
    pub pieces_done: u32,
    pub pieces_total: u32,
    pub error: Option<String>,
    /// The generated .torrent (only when finished without an output path).
    pub torrent: Option<Arc<Vec<u8>>>,
    /// Where the .torrent was written (when an output path was given).
    pub output_path: Option<PathBuf>,
}

pub struct CreateParams {
    /// The file or directory to create the torrent from (daemon-local).
    pub source: PathBuf,
    /// Piece size in bytes (0 = automatic; power of two, ≤ 128 MiB).
    pub piece_size: u32,
    pub flags: CreateFlags,
    /// `(url, tier)` pairs.
    pub trackers: Vec<(String, i32)>,
    pub url_seeds: Vec<String>,
    pub comment: Option<String>,
    pub creator: Option<String>,
    pub private: bool,
    /// Write the .torrent here (atomic); otherwise keep it in memory.
    pub output_path: Option<PathBuf>,
}

struct JobEntry {
    snapshot: watch::Sender<JobSnapshot>,
    cancel: Arc<AtomicBool>,
    /// The blocking worker's handle, joined at engine shutdown.
    worker: Option<tokio::task::JoinHandle<()>>,
    /// When a terminal state was first observed by the manager.
    terminal_seen: Option<Instant>,
}

#[derive(Default)]
pub struct JobManager {
    jobs: Mutex<HashMap<u64, JobEntry>>,
    next_id: AtomicU64,
    /// Set by [`JobManager::cancel_all`]: no new jobs may start.
    stopping: AtomicBool,
}

impl JobManager {
    pub fn new() -> JobManager {
        JobManager::default()
    }

    /// Starts a creation job, returning its initial snapshot.
    pub async fn start(&self, params: CreateParams) -> Result<JobSnapshot, EngineError> {
        // Async metadata lookup: a synchronous exists() on an unhealthy
        // mount (NFS/FUSE) would block the request's runtime worker.
        match tokio::fs::try_exists(&params.source).await {
            Ok(true) => {}
            Ok(false) => {
                return Err(EngineError::Invalid(format!(
                    "source path {} does not exist",
                    params.source.display()
                )));
            }
            Err(e) => {
                return Err(EngineError::Invalid(format!(
                    "cannot access source path {}: {e}",
                    params.source.display()
                )));
            }
        }
        let id = self.next_id.fetch_add(1, Ordering::SeqCst) + 1;
        let initial = JobSnapshot {
            id,
            state: JobState::Listing,
            pieces_done: 0,
            pieces_total: 0,
            error: None,
            torrent: None,
            output_path: params.output_path.clone(),
        };
        let (snapshot, _) = watch::channel(initial.clone());
        let cancel = Arc::new(AtomicBool::new(false));
        {
            let mut jobs = self.jobs.lock().unwrap();
            if self.stopping.load(Ordering::SeqCst) {
                return Err(EngineError::ShuttingDown);
            }
            prune(&mut jobs);
            let worker = tokio::task::spawn_blocking({
                let snapshot = snapshot.clone();
                let cancel = Arc::clone(&cancel);
                move || run_job(params, &snapshot, &cancel)
            });
            jobs.insert(
                id,
                JobEntry {
                    snapshot,
                    cancel,
                    worker: Some(worker),
                    terminal_seen: None,
                },
            );
        }
        Ok(initial)
    }

    pub fn get(&self, id: u64) -> Option<JobSnapshot> {
        let mut jobs = self.jobs.lock().unwrap();
        prune(&mut jobs);
        jobs.get(&id).map(|j| j.snapshot.borrow().clone())
    }

    /// Snapshots of all (unpruned) jobs, oldest first.
    pub fn list(&self) -> Vec<JobSnapshot> {
        let mut jobs = self.jobs.lock().unwrap();
        prune(&mut jobs);
        let mut list: Vec<JobSnapshot> =
            jobs.values().map(|j| j.snapshot.borrow().clone()).collect();
        list.sort_by_key(|s| s.id);
        list
    }

    pub fn watch(&self, id: u64) -> Option<watch::Receiver<JobSnapshot>> {
        self.jobs
            .lock()
            .unwrap()
            .get(&id)
            .map(|j| j.snapshot.subscribe())
    }

    /// Cancels a job; returns whether it existed and was still running.
    pub fn cancel(&self, id: u64) -> bool {
        let jobs = self.jobs.lock().unwrap();
        let Some(job) = jobs.get(&id) else {
            return false;
        };
        job.cancel.store(true, Ordering::SeqCst);
        commit_terminal(&job.snapshot, |s| s.state = JobState::Cancelled)
    }

    /// Cancels every job and refuses new ones (engine shutdown).
    pub fn cancel_all(&self) {
        let jobs = self.jobs.lock().unwrap();
        self.stopping.store(true, Ordering::SeqCst);
        for job in jobs.values() {
            job.cancel.store(true, Ordering::SeqCst);
            commit_terminal(&job.snapshot, |s| s.state = JobState::Cancelled);
        }
    }

    /// Joins the worker threads, waiting until `deadline` at most. A
    /// worker stuck in a file read can outlive the deadline; it is then
    /// abandoned with a warning (cancelled, it produces no output).
    pub async fn join_workers(&self, deadline: tokio::time::Instant) {
        let workers: Vec<(u64, tokio::task::JoinHandle<()>)> = {
            let mut jobs = self.jobs.lock().unwrap();
            jobs.iter_mut()
                .filter_map(|(id, job)| job.worker.take().map(|w| (*id, w)))
                .collect()
        };
        for (id, worker) in workers {
            if tokio::time::timeout_at(deadline, worker).await.is_err() {
                tracing::warn!("creation job {id} is still hashing; abandoning its worker thread");
            }
        }
    }
}

/// Commits a terminal transition unless one already happened (the first
/// terminal state wins). Returns whether this commit won.
fn commit_terminal(
    snapshot: &watch::Sender<JobSnapshot>,
    apply: impl FnOnce(&mut JobSnapshot),
) -> bool {
    snapshot.send_if_modified(|s| {
        if s.state.is_terminal() {
            return false;
        }
        apply(s);
        true
    })
}

/// Drops terminal jobs whose end state was observed over [`JOB_TTL`] ago.
fn prune(jobs: &mut HashMap<u64, JobEntry>) {
    let now = Instant::now();
    jobs.retain(|_, job| {
        if !job.snapshot.borrow().state.is_terminal() {
            return true;
        }
        // Keep the entry while its worker runs (a cancelled job is
        // terminal long before the thread ends) so shutdown can join it.
        if job.worker.as_ref().is_some_and(|w| !w.is_finished()) {
            return true;
        }
        match job.terminal_seen {
            None => {
                job.terminal_seen = Some(now);
                true
            }
            Some(seen) => now.duration_since(seen) < JOB_TTL,
        }
    });
}

/// The blocking worker: list, hash, generate, publish.
fn run_job(params: CreateParams, snapshot: &watch::Sender<JobSnapshot>, cancel: &AtomicBool) {
    let output_path = params.output_path.clone();
    let result = build(params, snapshot, cancel);
    if cancel.load(Ordering::SeqCst) {
        // Cancelled: discard the result.
        return;
    }
    match result {
        Ok(bytes) => match output_path {
            Some(path) => match write_atomic(&path, &bytes) {
                Ok(()) => {
                    let finished = commit_terminal(snapshot, |s| {
                        s.state = JobState::Finished;
                        s.pieces_done = s.pieces_total;
                    });
                    if !finished {
                        // A concurrent cancel won; discard the output.
                        let _ = std::fs::remove_file(&path);
                    }
                }
                Err(e) => {
                    commit_terminal(snapshot, |s| {
                        s.state = JobState::Failed;
                        s.error = Some(format!("cannot write {}: {e}", path.display()));
                    });
                }
            },
            None => {
                commit_terminal(snapshot, |s| {
                    s.state = JobState::Finished;
                    s.pieces_done = s.pieces_total;
                    s.torrent = Some(Arc::new(bytes));
                });
            }
        },
        Err(message) => {
            commit_terminal(snapshot, |s| {
                s.state = JobState::Failed;
                s.error = Some(message);
            });
        }
    }
}

fn build(
    params: CreateParams,
    snapshot: &watch::Sender<JobSnapshot>,
    cancel: &AtomicBool,
) -> Result<Vec<u8>, String> {
    let files =
        list_files(&params.source, params.flags).map_err(|e| format!("cannot list files: {e}"))?;
    let mut ct = CreateTorrent::new(&files, params.piece_size, params.flags)
        .map_err(|e| format!("cannot create torrent: {e}"))?;
    for (url, tier) in &params.trackers {
        ct.add_tracker(url, *tier)
            .map_err(|e| format!("cannot add tracker {url}: {e}"))?;
    }
    for url in &params.url_seeds {
        ct.add_url_seed(url)
            .map_err(|e| format!("cannot add url seed {url}: {e}"))?;
    }
    if let Some(comment) = &params.comment {
        ct.set_comment(comment).map_err(|e| e.to_string())?;
    }
    if let Some(creator) = &params.creator {
        ct.set_creator(creator).map_err(|e| e.to_string())?;
    }
    ct.set_priv(params.private);

    let total = ct.num_pieces();
    snapshot.send_if_modified(|s| {
        if s.state.is_terminal() {
            return false;
        }
        s.state = JobState::Hashing;
        s.pieces_total = total;
        true
    });

    // Hashing reads files relative to the source's parent directory.
    let base = params
        .source
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    set_piece_hashes(
        &mut ct,
        &base,
        Some(|piece: u32| {
            snapshot.send_if_modified(|s| {
                if s.state.is_terminal() {
                    return false;
                }
                s.pieces_done = piece + 1;
                true
            });
            !cancel.load(Ordering::SeqCst)
        }),
    )
    .map_err(|e| format!("hashing failed: {e}"))?;

    ct.generate().map_err(|e| format!("cannot generate: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params(source: PathBuf, output_path: Option<PathBuf>) -> CreateParams {
        CreateParams {
            source,
            piece_size: 0,
            flags: CreateFlags::empty(),
            trackers: Vec::new(),
            url_seeds: Vec::new(),
            comment: None,
            creator: None,
            private: false,
            output_path,
        }
    }

    fn payload_file(dir: &std::path::Path) -> PathBuf {
        let file = dir.join("payload.bin");
        std::fs::write(&file, vec![7u8; 100_000]).unwrap();
        file
    }

    #[tokio::test]
    async fn job_finishes_in_memory() {
        let dir = tempfile::tempdir().unwrap();
        let manager = JobManager::new();
        let job = manager
            .start(params(payload_file(dir.path()), None))
            .await
            .unwrap();
        let mut rx = manager.watch(job.id).unwrap();
        let snapshot = loop {
            let s = rx.borrow_and_update().clone();
            if s.state.is_terminal() {
                break s;
            }
            rx.changed().await.unwrap();
        };
        assert_eq!(snapshot.state, JobState::Finished);
        assert!(snapshot.torrent.is_some());
    }

    /// The first terminal state wins: a cancellation is never
    /// overwritten by a later completion commit (or progress update).
    #[test]
    fn first_terminal_state_wins() {
        let (snapshot, _rx) = watch::channel(JobSnapshot {
            id: 1,
            state: JobState::Hashing,
            pieces_done: 0,
            pieces_total: 4,
            error: None,
            torrent: None,
            output_path: None,
        });
        assert!(commit_terminal(&snapshot, |s| s.state = JobState::Cancelled));
        assert!(!commit_terminal(&snapshot, |s| {
            s.state = JobState::Finished;
            s.torrent = Some(Arc::new(vec![1]));
        }));
        let s = snapshot.borrow().clone();
        assert_eq!(s.state, JobState::Cancelled);
        assert!(s.torrent.is_none());
    }

    /// The sparse payload is far too large to hash within the timeout, so
    /// the join only completes promptly if the progress callback aborts
    /// the cancelled run.
    #[cfg(unix)]
    #[tokio::test]
    async fn cancel_aborts_hashing_mid_run() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("payload.bin");
        std::fs::File::create(&file)
            .unwrap()
            .set_len(1 << 40)
            .unwrap();
        let manager = JobManager::new();
        let mut params = params(file, None);
        params.piece_size = 1 << 20;
        let job = manager.start(params).await.unwrap();
        let mut rx = manager.watch(job.id).unwrap();
        loop {
            let s = rx.borrow_and_update().clone();
            assert!(!s.state.is_terminal(), "job ended early: {s:?}");
            if s.state == JobState::Hashing && s.pieces_done > 0 {
                break;
            }
            rx.changed().await.unwrap();
        }
        assert!(manager.cancel(job.id));
        let worker = {
            let mut jobs = manager.jobs.lock().unwrap();
            jobs.get_mut(&job.id).unwrap().worker.take().unwrap()
        };
        tokio::time::timeout(Duration::from_secs(60), worker)
            .await
            .expect("cancelled worker kept hashing")
            .unwrap();
        let snapshot = manager.get(job.id).unwrap();
        assert_eq!(snapshot.state, JobState::Cancelled);
        assert!(snapshot.torrent.is_none());
    }

    #[tokio::test]
    async fn cancel_all_joins_workers_and_refuses_new_jobs() {
        let dir = tempfile::tempdir().unwrap();
        let file = payload_file(dir.path());
        let manager = JobManager::new();
        let job = manager.start(params(file.clone(), None)).await.unwrap();
        manager.cancel_all();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(60);
        manager.join_workers(deadline).await;
        assert!(
            manager
                .jobs
                .lock()
                .unwrap()
                .values()
                .all(|j| j.worker.is_none())
        );
        assert!(manager.get(job.id).unwrap().state.is_terminal());
        assert!(matches!(
            manager.start(params(file, None)).await,
            Err(EngineError::ShuttingDown)
        ));
    }
}
