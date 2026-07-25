// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! [`RsbtData`]: the daemon's per-torrent client data, persisted inside
//! resume data under rbtorrent's `"rbt-data"` key.

use rbtorrent::ClientData;
use serde::{Deserialize, Serialize};

/// Per-torrent daemon state libtorrent knows nothing about, riding the
/// resume-data pipeline: attached at add time, embedded in every resume
/// write, restored with the torrent.
///
/// Compatibility contract: new fields must be `#[serde(default)]` so blobs
/// written by older builds keep deserializing; unknown fields are ignored
/// by serde, so newer blobs already load on older builds (and libtorrent
/// ignores the whole key, so any build's files load anywhere).
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct RsbtData {}

impl ClientData for RsbtData {
    fn to_bencode(&self) -> Vec<u8> {
        serde_bencode::to_bytes(self).expect("RsbtData is always bencodable")
    }

    fn from_bencode(bytes: Option<&[u8]>) -> rbtorrent::Result<Self> {
        match bytes {
            // Resume data written before client data existed.
            None => Ok(RsbtData::default()),
            Some(bytes) => serde_bencode::from_bytes(bytes)
                .map_err(|e| rbtorrent::Error::client(format!("corrupt rbt-data: {e}"))),
        }
    }
}
