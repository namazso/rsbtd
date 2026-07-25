// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! [`RsbtData`]: the daemon's per-torrent client data, persisted inside
//! resume data under rbtorrent's `"rbt-data"` key.

use rbtorrent::ClientData;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Per-torrent daemon state libtorrent knows nothing about, riding the
/// resume-data pipeline: attached at add time, embedded in every resume
/// write, restored with the torrent.
///
/// Compatibility contract: new fields must be `#[serde(default)]` so blobs
/// written by older builds keep deserializing; unknown fields are ignored
/// by serde, so newer blobs already load on older builds (and libtorrent
/// ignores the whole key, so any build's files load anywhere).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RsbtData {
    /// The torrent's durable identity: minted at add time (or at first
    /// restore of a pre-uuid resume record), stable for the torrent's
    /// lifetime, and the stem of its resume file.
    #[serde(with = "uuid_bencode", default = "Uuid::new_v4")]
    pub uuid: Uuid,
}

impl RsbtData {
    /// Fresh per-torrent state with a newly minted identity.
    pub fn new() -> RsbtData {
        RsbtData {
            uuid: Uuid::new_v4(),
        }
    }
}

impl Default for RsbtData {
    /// Mints a fresh identity: there is no identity-less `RsbtData`.
    fn default() -> RsbtData {
        RsbtData::new()
    }
}

impl ClientData for RsbtData {
    fn to_bencode(&self) -> Vec<u8> {
        serde_bencode::to_bytes(self).expect("RsbtData is always bencodable")
    }

    fn from_bencode(bytes: Option<&[u8]>) -> rbtorrent::Result<Self> {
        match bytes {
            // Resume data written before client data existed: mint the
            // torrent's identity now (the restore migration persists it).
            None => Ok(RsbtData::new()),
            Some(bytes) => serde_bencode::from_bytes(bytes)
                .map_err(|e| rbtorrent::Error::client(format!("corrupt rbt-data: {e}"))),
        }
    }
}

/// Bencodes the uuid as a 16-byte string (`16:<raw bytes>`): uuid's own
/// serde impl would emit the 36-char text form, because serde_bencode
/// reports itself as human-readable.
mod uuid_bencode {
    use serde::{Deserializer, Serializer};
    use uuid::Uuid;

    pub fn serialize<S: Serializer>(uuid: &Uuid, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_bytes(uuid.as_bytes())
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Uuid, D::Error> {
        struct Visitor;

        impl serde::de::Visitor<'_> for Visitor {
            type Value = Uuid;

            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("a 16-byte string")
            }

            fn visit_bytes<E: serde::de::Error>(self, v: &[u8]) -> Result<Uuid, E> {
                Uuid::from_slice(v).map_err(|_| E::invalid_length(v.len(), &self))
            }
        }

        deserializer.deserialize_bytes(Visitor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The wire format is pinned: a dict with the uuid as a 16-byte
    /// string — not an integer list, not a hyphenated text form.
    #[test]
    fn uuid_is_bencoded_as_a_byte_string() {
        let data = RsbtData::new();
        let expected = [b"d4:uuid16:".as_slice(), data.uuid.as_bytes(), b"e"].concat();
        assert_eq!(data.to_bencode(), expected);
    }

    #[test]
    fn round_trips() {
        let data = RsbtData::new();
        let restored = RsbtData::from_bencode(Some(&data.to_bencode())).unwrap();
        assert_eq!(restored.uuid, data.uuid);
    }

    /// Pre-uuid records mint an identity, fresh every call (persisting it
    /// is the restore migration's job).
    #[test]
    fn absent_blob_mints() {
        let a = RsbtData::from_bencode(None).unwrap();
        let b = RsbtData::from_bencode(None).unwrap();
        assert_ne!(a.uuid, b.uuid);
        assert!(!a.uuid.is_nil());
    }

    /// A blob without the key (defensive; released builds never wrote
    /// client data at all) mints via the serde default instead of erroring.
    #[test]
    fn missing_key_mints() {
        let data = RsbtData::from_bencode(Some(b"de")).unwrap();
        assert!(!data.uuid.is_nil());
    }

    #[test]
    fn wrong_length_is_corrupt() {
        assert!(RsbtData::from_bencode(Some(b"d4:uuid3:abce")).is_err());
    }
}
