// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Small shared value types.

use std::fmt;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV6};

use libctorrent_sys as sys;

/// A SHA-1 digest (v1 info-hash, peer id, DHT target, ...).
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Sha1Hash(pub [u8; 20]);

/// A SHA-256 digest (v2 info-hash, merkle root, ...).
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Sha256Hash(pub [u8; 32]);

impl Sha1Hash {
    pub fn is_all_zeros(&self) -> bool {
        self.0 == [0; 20]
    }

    pub(crate) fn from_ct(h: &sys::ct_sha1) -> Sha1Hash {
        Sha1Hash(h.data)
    }
}

impl Sha256Hash {
    pub fn is_all_zeros(&self) -> bool {
        self.0 == [0; 32]
    }

    pub(crate) fn from_ct(h: &sys::ct_sha256) -> Sha256Hash {
        Sha256Hash(h.data)
    }
}

/// A borrowed piece bitfield (bit `i` describes piece `i`), as returned by
/// [`TorrentStatus::pieces`](crate::TorrentStatus::pieces) and friends.
/// Bits are packed MSB-first within each byte (libtorrent wire layout).
#[derive(Clone, Copy)]
pub struct PieceBitfield<'a> {
    bytes: &'a [u8],
    bits: usize,
}

impl<'a> PieceBitfield<'a> {
    /// # Safety
    /// `ptr` must point to at least `bits.div_ceil(8)` readable bytes that
    /// outlive `'a`.
    pub(crate) unsafe fn from_raw(ptr: *const u8, bits: usize) -> PieceBitfield<'a> {
        // SAFETY: caller guarantees the range; libtorrent's bitfield stores
        // whole 32-bit words, so ceil(bits/8) bytes are always in bounds.
        let bytes = unsafe { std::slice::from_raw_parts(ptr, bits.div_ceil(8)) };
        PieceBitfield { bytes, bits }
    }

    /// The number of pieces covered by this bitfield.
    pub fn len(&self) -> usize {
        self.bits
    }

    pub fn is_empty(&self) -> bool {
        self.bits == 0
    }

    /// Whether the bit for piece `index` is set; `None` if out of range.
    pub fn get(&self, index: usize) -> Option<bool> {
        if index >= self.bits {
            return None;
        }
        Some(self.bytes[index / 8] & (0x80 >> (index % 8)) != 0)
    }

    /// The number of set bits.
    pub fn count_ones(&self) -> usize {
        self.iter().filter(|&b| b).count()
    }

    /// Iterates over all bits, piece 0 first.
    pub fn iter(&self) -> impl Iterator<Item = bool> + 'a {
        let bytes = self.bytes;
        (0..self.bits).map(move |i| bytes[i / 8] & (0x80 >> (i % 8)) != 0)
    }
}

impl fmt::Debug for PieceBitfield<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "PieceBitfield {{ len: {}, set: {} }}",
            self.len(),
            self.count_ones()
        )
    }
}

fn write_hex(f: &mut fmt::Formatter<'_>, bytes: &[u8]) -> fmt::Result {
    for b in bytes {
        write!(f, "{b:02x}")?;
    }
    Ok(())
}

impl fmt::Debug for Sha1Hash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_hex(f, &self.0)
    }
}

impl fmt::Display for Sha1Hash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_hex(f, &self.0)
    }
}

impl fmt::Debug for Sha256Hash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_hex(f, &self.0)
    }
}

impl fmt::Display for Sha256Hash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_hex(f, &self.0)
    }
}

/// The info-hash(es) of a torrent: v1 (SHA-1) and/or v2 (SHA-256).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct InfoHash {
    v1: Option<Sha1Hash>,
    v2: Option<Sha256Hash>,
}

impl InfoHash {
    /// An info-hash from its parts; at least one of `v1`/`v2` should be set.
    pub fn new(v1: Option<Sha1Hash>, v2: Option<Sha256Hash>) -> InfoHash {
        InfoHash { v1, v2 }
    }

    pub fn from_v1(v1: Sha1Hash) -> InfoHash {
        InfoHash {
            v1: Some(v1),
            v2: None,
        }
    }

    pub fn from_v2(v2: Sha256Hash) -> InfoHash {
        InfoHash {
            v1: None,
            v2: Some(v2),
        }
    }

    pub fn v1(&self) -> Option<Sha1Hash> {
        self.v1
    }

    pub fn v2(&self) -> Option<Sha256Hash> {
        self.v2
    }

    /// Whether the two identify the same torrent: some hash version is
    /// present on both sides and equal. Plain equality is stricter (both
    /// options must match), which misreads hybrid torrents whose hash
    /// set widened over time — e.g. a v2-only magnet later known by both
    /// hashes.
    pub fn overlaps(&self, other: &InfoHash) -> bool {
        (self.v1.is_some() && self.v1 == other.v1) || (self.v2.is_some() && self.v2 == other.v2)
    }

    pub(crate) fn from_ct(h: sys::ct_info_hash) -> InfoHash {
        let v1 = Sha1Hash(h.v1.data);
        let v2 = Sha256Hash(h.v2.data);
        InfoHash {
            v1: (!v1.is_all_zeros()).then_some(v1),
            v2: (!v2.is_all_zeros()).then_some(v2),
        }
    }

    /// The shim representation: an all-zero hash means "absent".
    pub(crate) fn to_ct(self) -> sys::ct_info_hash {
        sys::ct_info_hash {
            v1: sys::ct_sha1 {
                data: self.v1.map_or([0; 20], |h| h.0),
            },
            v2: sys::ct_sha256 {
                data: self.v2.map_or([0; 32], |h| h.0),
            },
        }
    }
}

/// A request for a byte range within a piece.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PeerRequest {
    pub piece: i32,
    pub start: i32,
    pub length: i32,
}

impl PeerRequest {
    pub(crate) fn from_ct(r: &sys::ct_peer_request) -> PeerRequest {
        PeerRequest {
            piece: r.piece,
            start: r.start,
            length: r.length,
        }
    }
}

pub(crate) fn socket_addr_to_ct(addr: SocketAddr) -> sys::ct_endpoint {
    let mut ep = sys::ct_endpoint {
        addr: [0; 16],
        scope_id: 0,
        port: addr.port(),
        is_v6: 0,
    };
    match addr {
        SocketAddr::V4(v4) => ep.addr[..4].copy_from_slice(&v4.ip().octets()),
        SocketAddr::V6(v6) => {
            ep.addr = v6.ip().octets();
            ep.scope_id = v6.scope_id();
            ep.is_v6 = 1;
        }
    }
    ep
}

pub(crate) fn socket_addr_from_ct(ep: &sys::ct_endpoint) -> SocketAddr {
    if ep.is_v6 != 0 {
        SocketAddr::V6(SocketAddrV6::new(
            Ipv6Addr::from(ep.addr),
            ep.port,
            0,
            ep.scope_id,
        ))
    } else {
        SocketAddr::new(ip_addr_from_ct(ep), ep.port)
    }
}

pub(crate) fn ip_addr_from_ct(ep: &sys::ct_endpoint) -> IpAddr {
    if ep.is_v6 != 0 {
        IpAddr::V6(Ipv6Addr::from(ep.addr))
    } else {
        IpAddr::V4(Ipv4Addr::new(
            ep.addr[0], ep.addr[1], ep.addr[2], ep.addr[3],
        ))
    }
}
