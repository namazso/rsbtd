// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The daemon's torrent registry.
//!
//! libtorrent has no "list all torrents" API in these bindings, so the
//! engine tracks every torrent it adds. The durable key is the torrent's
//! uuid (minted at add time, persisted in resume data, the resume filename
//! stem); entries are also indexed by the session-unique torrent id (alert
//! correlation) and the rbtorrent client-data token (removal alerts).

use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::SystemTime;

use uuid::Uuid;

/// One registered torrent: identity and persistence metadata only. Live
/// handles are never stored (they borrow the session); derive one on
/// demand with [`Engine::with_handle`](super::Engine::with_handle).
#[derive(Debug)]
pub struct TorrentEntry {
    /// Session-unique id ([`TorrentHandle::id`](rbtorrent::TorrentHandle::id)),
    /// stable across resume but not across daemon restarts.
    pub id: u32,
    /// The client-data token minted at add time; resolves the live handle
    /// via [`Session::find_torrent_by_token`](rbtorrent::Session::find_torrent_by_token).
    pub token: u64,
    /// The torrent's durable identity (and resume filename stem).
    pub uuid: Uuid,
    pub added_at: SystemTime,
}

#[derive(Default)]
struct Inner {
    by_uuid: HashMap<Uuid, Arc<TorrentEntry>>,
    by_id: HashMap<u32, Uuid>,
    by_token: HashMap<u64, Uuid>,
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
    pub fn upsert(&self, id: u32, token: u64, uuid: Uuid) -> Arc<TorrentEntry> {
        let mut inner = self.inner.write().unwrap();
        if let Some(existing) = inner.by_id.get(&id).copied()
            && let Some(entry) = inner.by_uuid.get(&existing)
        {
            return Arc::clone(entry);
        }
        let entry = Arc::new(TorrentEntry {
            id,
            token,
            uuid,
            added_at: SystemTime::now(),
        });
        inner.by_uuid.insert(uuid, Arc::clone(&entry));
        inner.by_id.insert(id, uuid);
        inner.by_token.insert(token, uuid);
        entry
    }

    fn remove(&self, uuid: Uuid) -> Option<Arc<TorrentEntry>> {
        let mut inner = self.inner.write().unwrap();
        let entry = inner.by_uuid.remove(&uuid)?;
        // Only drop index entries that still point at this torrent.
        if inner.by_id.get(&entry.id) == Some(&uuid) {
            inner.by_id.remove(&entry.id);
        }
        if inner.by_token.get(&entry.token) == Some(&uuid) {
            inner.by_token.remove(&entry.token);
        }
        Some(entry)
    }

    /// Removes a torrent by its client-data token, returning its entry.
    pub fn remove_by_token(&self, token: u64) -> Option<Arc<TorrentEntry>> {
        let uuid = *self.inner.read().unwrap().by_token.get(&token)?;
        self.remove(uuid)
    }

    pub fn remove_by_id(&self, id: u32) -> Option<Arc<TorrentEntry>> {
        let uuid = *self.inner.read().unwrap().by_id.get(&id)?;
        self.remove(uuid)
    }

    pub fn get(&self, id: u32) -> Option<Arc<TorrentEntry>> {
        let inner = self.inner.read().unwrap();
        let uuid = inner.by_id.get(&id)?;
        inner.by_uuid.get(uuid).cloned()
    }

    /// Looks a torrent up by its durable uuid.
    pub fn find(&self, uuid: &Uuid) -> Option<Arc<TorrentEntry>> {
        self.inner.read().unwrap().by_uuid.get(uuid).cloned()
    }

    /// Looks a torrent up by its client-data token.
    pub fn find_by_token(&self, token: u64) -> Option<Arc<TorrentEntry>> {
        let inner = self.inner.read().unwrap();
        let uuid = inner.by_token.get(&token)?;
        inner.by_uuid.get(uuid).cloned()
    }

    /// All registered torrents (arbitrary order).
    pub fn list(&self) -> Vec<Arc<TorrentEntry>> {
        self.inner
            .read()
            .unwrap()
            .by_uuid
            .values()
            .cloned()
            .collect()
    }

    pub fn len(&self) -> usize {
        self.inner.read().unwrap().by_uuid.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indexes_by_uuid_id_and_token() {
        let registry = Registry::new();
        let uuid = Uuid::from_u128(1);

        let entry = registry.upsert(7, 100, uuid);
        assert_eq!(entry.uuid, uuid);
        // Same id: the existing entry wins, whatever the other params say.
        let again = registry.upsert(7, 999, Uuid::from_u128(2));
        assert_eq!(again.uuid, uuid);
        assert_eq!(again.token, 100);
        assert_eq!(registry.len(), 1);

        assert_eq!(registry.get(7).unwrap().uuid, uuid);
        assert_eq!(registry.find(&uuid).unwrap().id, 7);
        assert!(registry.find(&Uuid::from_u128(2)).is_none());
        assert_eq!(registry.find_by_token(100).unwrap().uuid, uuid);
        assert!(registry.find_by_token(999).is_none());

        let removed = registry.remove_by_token(100).unwrap();
        assert_eq!(removed.uuid, uuid);
        assert!(registry.is_empty());
        assert!(registry.remove_by_token(100).is_none());

        let uuid2 = Uuid::from_u128(3);
        registry.upsert(8, 101, uuid2);
        assert_eq!(registry.remove_by_id(8).unwrap().uuid, uuid2);
        assert!(registry.find(&uuid2).is_none());
    }
}
