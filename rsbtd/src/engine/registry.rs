// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The daemon's torrent registry.
//!
//! libtorrent has no "list all torrents" API in these bindings, so the
//! engine tracks every torrent it adds. Entries are keyed by the
//! session-unique torrent id and indexed by v1/v2 info-hashes. The
//! `resume_key` (persistence filename stem) is fixed at insertion so a
//! hybrid magnet's late-arriving second hash does not orphan its resume file.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::SystemTime;

use rbtorrent::{InfoHash, Sha1Hash, Sha256Hash};

/// One registered torrent: identity and persistence metadata only. Live
/// handles are never stored (they borrow the session); derive one on
/// demand with [`Engine::with_handle`](super::Engine::with_handle).
#[derive(Debug)]
pub struct TorrentEntry {
    /// Session-unique id ([`TorrentHandle::id`](rbtorrent::TorrentHandle::id)),
    /// stable across resume but not across daemon restarts.
    pub id: u32,
    /// Info-hashes as known at insertion (see [`Registry::reindex`]).
    pub info_hash: InfoHash,
    /// Filename stem of the persisted resume data, fixed at insertion.
    pub resume_key: String,
    pub added_at: SystemTime,
}

/// The filename stem used to persist a torrent's resume data: lowercase hex
/// of the v1 hash when present, else of the v2 hash.
pub fn resume_key(info_hash: &InfoHash) -> String {
    match (info_hash.v1(), info_hash.v2()) {
        (Some(v1), _) => v1.to_string(),
        (None, Some(v2)) => v2.to_string(),
        (None, None) => String::new(),
    }
}

#[derive(Default)]
struct Inner {
    by_id: HashMap<u32, Arc<TorrentEntry>>,
    by_v1: HashMap<Sha1Hash, u32>,
    by_v2: HashMap<Sha256Hash, u32>,
}

/// Thread-safe torrent registry (short, non-blocking critical sections).
#[derive(Default)]
pub struct Registry {
    inner: RwLock<Inner>,
}

impl Registry {
    pub fn new() -> Registry {
        Registry::default()
    }

    /// Registers a torrent, or returns the existing entry for its id.
    pub fn upsert(&self, id: u32, info_hash: InfoHash) -> Arc<TorrentEntry> {
        let mut inner = self.inner.write().unwrap();
        if let Some(entry) = inner.by_id.get(&id) {
            return Arc::clone(entry);
        }
        let entry = Arc::new(TorrentEntry {
            id,
            info_hash,
            resume_key: resume_key(&info_hash),
            added_at: SystemTime::now(),
        });
        inner.by_id.insert(id, Arc::clone(&entry));
        if let Some(v1) = info_hash.v1() {
            inner.by_v1.insert(v1, id);
        }
        if let Some(v2) = info_hash.v2() {
            inner.by_v2.insert(v2, id);
        }
        entry
    }

    /// Refreshes the hash indexes of `id` to `hashes` (called on metadata
    /// arrival: a hybrid magnet learns its second hash then). Returns the
    /// updated entry.
    pub fn reindex(&self, id: u32, hashes: InfoHash) -> Option<Arc<TorrentEntry>> {
        let mut inner = self.inner.write().unwrap();
        let entry = inner.by_id.get(&id)?;
        if hashes == entry.info_hash {
            return Some(Arc::clone(entry));
        }
        let updated = Arc::new(TorrentEntry {
            id,
            info_hash: hashes,
            resume_key: entry.resume_key.clone(),
            added_at: entry.added_at,
        });
        inner.by_id.insert(id, Arc::clone(&updated));
        if let Some(v1) = hashes.v1() {
            inner.by_v1.insert(v1, id);
        }
        if let Some(v2) = hashes.v2() {
            inner.by_v2.insert(v2, id);
        }
        Some(updated)
    }

    /// Removes a torrent by any of its info-hashes, returning its entry.
    pub fn remove_by_hash(&self, info_hash: &InfoHash) -> Option<Arc<TorrentEntry>> {
        let id = self.lookup_id(info_hash)?;
        self.remove_by_id(id)
    }

    pub fn remove_by_id(&self, id: u32) -> Option<Arc<TorrentEntry>> {
        let mut inner = self.inner.write().unwrap();
        let entry = inner.by_id.remove(&id)?;
        // Only drop index entries that still point at this torrent.
        if let Some(v1) = entry.info_hash.v1()
            && inner.by_v1.get(&v1) == Some(&id)
        {
            inner.by_v1.remove(&v1);
        }
        if let Some(v2) = entry.info_hash.v2()
            && inner.by_v2.get(&v2) == Some(&id)
        {
            inner.by_v2.remove(&v2);
        }
        Some(entry)
    }

    pub fn get(&self, id: u32) -> Option<Arc<TorrentEntry>> {
        self.inner.read().unwrap().by_id.get(&id).cloned()
    }

    /// Looks a torrent up by v1 or v2 hash (either matches).
    pub fn find(&self, info_hash: &InfoHash) -> Option<Arc<TorrentEntry>> {
        let id = self.lookup_id(info_hash)?;
        self.get(id)
    }

    fn lookup_id(&self, info_hash: &InfoHash) -> Option<u32> {
        let inner = self.inner.read().unwrap();
        if let Some(v1) = info_hash.v1()
            && let Some(&id) = inner.by_v1.get(&v1)
        {
            return Some(id);
        }
        if let Some(v2) = info_hash.v2()
            && let Some(&id) = inner.by_v2.get(&v2)
        {
            return Some(id);
        }
        None
    }

    /// All registered torrents (arbitrary order).
    pub fn list(&self) -> Vec<Arc<TorrentEntry>> {
        self.inner.read().unwrap().by_id.values().cloned().collect()
    }

    pub fn len(&self) -> usize {
        self.inner.read().unwrap().by_id.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Parses a user-supplied hex info-hash (40 chars = v1, 64 chars = v2).
pub fn parse_info_hash(hex_str: &str) -> Option<InfoHash> {
    let bytes = hex_decode(hex_str)?;
    match bytes.len() {
        20 => Some(InfoHash::from_v1(Sha1Hash(bytes.try_into().unwrap()))),
        32 => Some(InfoHash::from_v2(Sha256Hash(bytes.try_into().unwrap()))),
        _ => None,
    }
}

fn hex_decode(s: &str) -> Option<Vec<u8>> {
    hex::decode(s).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_info_hashes() {
        let v1 = "0123456789abcdef0123456789abcdef01234567";
        let parsed = parse_info_hash(v1).unwrap();
        assert_eq!(parsed.v1().unwrap().to_string(), v1);
        assert!(parsed.v2().is_none());

        let v2 = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        let parsed = parse_info_hash(v2).unwrap();
        assert_eq!(parsed.v2().unwrap().to_string(), v2);
        assert!(parsed.v1().is_none());

        assert!(parse_info_hash("").is_none());
        assert!(parse_info_hash("abcd").is_none());
        assert!(parse_info_hash("zz23456789abcdef0123456789abcdef01234567").is_none());
    }

    #[test]
    fn resume_key_prefers_v1() {
        let v1 = Sha1Hash([0x11; 20]);
        let v2 = Sha256Hash([0x22; 32]);
        assert_eq!(
            resume_key(&InfoHash::new(Some(v1), Some(v2))),
            v1.to_string()
        );
        assert_eq!(resume_key(&InfoHash::from_v2(v2)), v2.to_string());
    }
}
