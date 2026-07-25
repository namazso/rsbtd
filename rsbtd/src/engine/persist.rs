// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! State-directory persistence: atomic file writes and the persister task.
//!
//! All resume-data and session-state writes go through one queue processed
//! sequentially by the persister task, so a delete enqueued after a write
//! of the same file cannot be reordered ahead of it. The queue decouples
//! alert processing from disk latency and back-pressures the pump when full.

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use tokio::sync::{broadcast, mpsc};

use super::events::{Event, EventKind, TorrentRef};
use super::{DirtyResume, Inflight};

/// Filename of the serialized session state within the state directory.
pub const SESSION_STATE_FILE: &str = "session.state";
/// Subdirectory of the state directory holding per-torrent resume files.
pub const TORRENTS_DIR: &str = "torrents";
/// Extension of per-torrent resume files.
pub const RESUME_EXT: &str = "resume";

#[derive(Clone, Debug)]
pub struct StatePaths {
    pub root: PathBuf,
}

impl StatePaths {
    pub fn new(root: PathBuf) -> StatePaths {
        StatePaths { root }
    }

    pub fn session_state(&self) -> PathBuf {
        self.root.join(SESSION_STATE_FILE)
    }

    pub fn torrents_dir(&self) -> PathBuf {
        self.root.join(TORRENTS_DIR)
    }

    pub fn resume_file(&self, resume_key: &str) -> PathBuf {
        self.torrents_dir()
            .join(format!("{resume_key}.{RESUME_EXT}"))
    }

    /// Creates the state directory tree. New directories get owner-only
    /// permissions: the session state can contain proxy credentials, and
    /// resume files disclose activity. Pre-existing dirs keep their mode.
    pub fn create_dirs(&self) -> std::io::Result<()> {
        create_dir_private(&self.torrents_dir())
    }
}

#[cfg(unix)]
fn create_dir_private(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::DirBuilderExt as _;
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(path)
}

#[cfg(not(unix))]
fn create_dir_private(path: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(path)
}

/// Writes `bytes` to `path` atomically: unique temp file in the target
/// directory, fsync, rename, best-effort fsync of the directory. The
/// pid/counter suffix keeps concurrent writers (even to the same target)
/// from clobbering each other's temp file; ordering between writes to one
/// target is the caller's concern (the persister queue serializes them).
pub fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    static TMP_SEQ: AtomicU64 = AtomicU64::new(0);
    let dir = path.parent().ok_or(std::io::ErrorKind::InvalidInput)?;
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(format!(
        ".{}.{}.tmp",
        std::process::id(),
        TMP_SEQ.fetch_add(1, Ordering::Relaxed)
    ));
    let tmp = PathBuf::from(tmp);
    let result = (|| {
        let mut file = std::fs::File::create(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        std::fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
        return result;
    }
    if let Ok(dir) = std::fs::File::open(dir) {
        let _ = dir.sync_all();
    }
    Ok(())
}

/// One unit of work for the persister task.
#[derive(Debug)]
pub enum PersistOp {
    /// Write a torrent's resume data; publishes `ResumeDataSaved`/`Failed`
    /// and releases its in-flight token when done. `ack` (when present)
    /// receives the durable write's result, for callers that promised
    /// durability.
    WriteResume {
        resume_key: String,
        torrent: TorrentRef,
        bytes: Vec<u8>,
        ack: Option<tokio::sync::oneshot::Sender<std::io::Result<()>>>,
    },
    /// Delete a torrent's resume file (missing file is not an error).
    DeleteResume { resume_key: String },
    /// Write the serialized session state. `ack` (when present) receives
    /// the durable write's result, for callers that promised durability.
    WriteSessionState {
        bytes: Vec<u8>,
        ack: Option<tokio::sync::oneshot::Sender<std::io::Result<()>>>,
    },
}

/// Runs the persister until every [`PersistOp`] sender is dropped.
pub async fn run_persister(
    paths: StatePaths,
    mut rx: mpsc::Receiver<PersistOp>,
    events: broadcast::Sender<Arc<Event>>,
    inflight: Arc<Inflight>,
    dirty: Arc<DirtyResume>,
) {
    while let Some(op) = rx.recv().await {
        let paths = paths.clone();
        match op {
            PersistOp::WriteResume {
                resume_key,
                torrent,
                bytes,
                ack,
            } => {
                let path = paths.resume_file(&resume_key);
                let result =
                    match tokio::task::spawn_blocking(move || write_atomic(&path, &bytes)).await {
                        Ok(r) => r,
                        Err(e) => Err(std::io::Error::other(format!(
                            "resume write task failed: {e}"
                        ))),
                    };
                let kind = match &result {
                    Ok(()) => {
                        dirty.remove(torrent.id);
                        EventKind::ResumeDataSaved
                    }
                    Err(e) => {
                        tracing::warn!(%torrent.info_hash, "resume write failed: {e}");
                        // On-disk state now uncertain; the pump retries
                        // and shutdown flushes it.
                        dirty.insert(torrent.id);
                        EventKind::ResumeDataFailed {
                            message: format!("cannot write resume file: {e}"),
                        }
                    }
                };
                let _ = events.send(Arc::new(Event {
                    torrent: Some(torrent),
                    kind,
                }));
                inflight.dec();
                if let Some(ack) = ack {
                    let _ = ack.send(result);
                }
            }
            PersistOp::DeleteResume { resume_key } => {
                let path = paths.resume_file(&resume_key);
                let result = tokio::task::spawn_blocking(move || std::fs::remove_file(&path)).await;
                if let Ok(Err(e)) = result
                    && e.kind() != std::io::ErrorKind::NotFound
                {
                    tracing::warn!(resume_key, "cannot delete resume file: {e}");
                }
            }
            PersistOp::WriteSessionState { bytes, ack } => {
                let path = paths.session_state();
                let result =
                    match tokio::task::spawn_blocking(move || write_atomic(&path, &bytes)).await {
                        Ok(r) => r,
                        Err(e) => Err(std::io::Error::other(format!(
                            "session state write task failed: {e}"
                        ))),
                    };
                if let Err(e) = &result {
                    tracing::warn!("cannot write session state: {e}");
                }
                if let Some(ack) = ack {
                    let _ = ack.send(result);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// New state directories are owner-only (they hold credentials and
    /// activity records); existing directories keep their mode.
    #[cfg(unix)]
    #[test]
    fn create_dirs_makes_new_directories_private() {
        use std::os::unix::fs::PermissionsExt as _;

        let dir = tempfile::tempdir().unwrap();
        let paths = StatePaths::new(dir.path().join("state"));
        paths.create_dirs().unwrap();
        for p in [&paths.root, &paths.torrents_dir()] {
            let mode = std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o700, "{}", p.display());
        }

        // An existing root keeps its mode; the new subdir is restricted.
        let existing = tempfile::tempdir().unwrap();
        std::fs::set_permissions(existing.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
        let paths = StatePaths::new(existing.path().to_path_buf());
        paths.create_dirs().unwrap();
        let mode = std::fs::metadata(existing.path())
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o755);
        let mode = std::fs::metadata(paths.torrents_dir())
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o700);
    }

    /// Concurrent writers (even to the same target): every observable
    /// content is exactly one payload, and no temp files are left behind.
    #[test]
    fn write_atomic_concurrent_writers_do_not_corrupt() {
        // On Windows, opening the target while another thread's rename is
        // superseding it can transiently fail with ERROR_ACCESS_DENIED
        // (the replaced file is delete-pending), so the read retries.
        fn read_current(path: &Path) -> Vec<u8> {
            for _ in 0..100 {
                match std::fs::read(path) {
                    Err(e) if cfg!(windows) && e.kind() == std::io::ErrorKind::PermissionDenied => {
                        std::thread::sleep(std::time::Duration::from_millis(1));
                    }
                    result => return result.unwrap(),
                }
            }
            std::fs::read(path).unwrap()
        }

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("file.bin");
        let payloads: Vec<Vec<u8>> = (0u8..8).map(|i| vec![i; 4096]).collect();
        std::thread::scope(|scope| {
            for payload in &payloads {
                scope.spawn(|| {
                    for _ in 0..20 {
                        write_atomic(&path, payload).unwrap();
                        let read = read_current(&path);
                        assert!(payloads.contains(&read), "torn or mixed write observed");
                    }
                });
            }
        });
        let entries: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(entries, vec![std::ffi::OsString::from("file.bin")]);
    }

    /// A failed write cleans up its temp file.
    #[test]
    fn write_atomic_failure_leaves_no_tmp() {
        let dir = tempfile::tempdir().unwrap();
        // The target is a directory: the rename must fail.
        let path = dir.path().join("occupied");
        std::fs::create_dir(&path).unwrap();
        write_atomic(&path, b"data").unwrap_err();
        let entries: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(entries, vec![std::ffi::OsString::from("occupied")]);
    }

    #[test]
    fn write_atomic_replaces_and_leaves_no_tmp() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("file.bin");
        write_atomic(&path, b"one").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"one");
        write_atomic(&path, b"two").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"two");
        let entries: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(entries, vec![std::ffi::OsString::from("file.bin")]);
    }
}
