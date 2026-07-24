// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Value enums for libtorrent's enum-typed integer settings.

use libctorrent_sys as sys;

/// Values for the `mmap_write_mode`-family settings
/// (`lt::settings_pack::mmap_write_mode_t`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum MmapWriteMode {
    /// Never mmap for writes; always use normal write calls.
    AlwaysPwrite = sys::CT_MMAP_WRITE_MODE_ALWAYS_PWRITE as i32,
    /// Prefer memory-mapped writes (for large files where it makes sense).
    AlwaysMmapWrite = sys::CT_MMAP_WRITE_MODE_ALWAYS_MMAP_WRITE as i32,
    /// Choose per save path based on the kind of storage behind it.
    AutoMmapWrite = sys::CT_MMAP_WRITE_MODE_AUTO_MMAP_WRITE as i32,
}

impl MmapWriteMode {
    pub(crate) fn from_raw(raw: i32) -> Option<Self> {
        match raw {
            x if x == sys::CT_MMAP_WRITE_MODE_ALWAYS_PWRITE as i32 => Some(Self::AlwaysPwrite),
            x if x == sys::CT_MMAP_WRITE_MODE_ALWAYS_MMAP_WRITE as i32 => {
                Some(Self::AlwaysMmapWrite)
            }
            x if x == sys::CT_MMAP_WRITE_MODE_AUTO_MMAP_WRITE as i32 => Some(Self::AutoMmapWrite),
            _ => None,
        }
    }
}

/// Values for the `suggest_mode`-family settings
/// (`lt::settings_pack::suggest_mode_t`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum SuggestMode {
    /// Send no suggest messages.
    NoPieceSuggestions = sys::CT_SUGGEST_MODE_NO_PIECE_SUGGESTIONS as i32,
    /// Suggest the pieces currently in the read cache, hinting peers
    /// toward cache-friendly piece selection.
    SuggestReadCache = sys::CT_SUGGEST_MODE_SUGGEST_READ_CACHE as i32,
}

impl SuggestMode {
    pub(crate) fn from_raw(raw: i32) -> Option<Self> {
        match raw {
            x if x == sys::CT_SUGGEST_MODE_NO_PIECE_SUGGESTIONS as i32 => {
                Some(Self::NoPieceSuggestions)
            }
            x if x == sys::CT_SUGGEST_MODE_SUGGEST_READ_CACHE as i32 => {
                Some(Self::SuggestReadCache)
            }
            _ => None,
        }
    }
}

/// Values for the `choking_algorithm`-family settings
/// (`lt::settings_pack::choking_algorithm_t`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum ChokingAlgorithm {
    /// The traditional choker with a fixed number of unchoke slots
    /// (`unchoke_slots_limit`).
    FixedSlotsChoker = sys::CT_CHOKING_ALGORITHM_FIXED_SLOTS_CHOKER as i32,
    /// Opens unchoke slots based on the upload rate achieved to peers,
    /// with each additional slot requiring a higher marginal rate; the
    /// initial threshold is `rate_choker_initial_threshold`.
    RateBasedChoker = sys::CT_CHOKING_ALGORITHM_RATE_BASED_CHOKER as i32,
}

impl ChokingAlgorithm {
    pub(crate) fn from_raw(raw: i32) -> Option<Self> {
        match raw {
            x if x == sys::CT_CHOKING_ALGORITHM_FIXED_SLOTS_CHOKER as i32 => {
                Some(Self::FixedSlotsChoker)
            }
            x if x == sys::CT_CHOKING_ALGORITHM_RATE_BASED_CHOKER as i32 => {
                Some(Self::RateBasedChoker)
            }
            _ => None,
        }
    }
}

/// Values for the `seed_choking_algorithm`-family settings
/// (`lt::settings_pack::seed_choking_algorithm_t`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum SeedChokingAlgorithm {
    /// Round-robins the unchoked peers, distributing upload bandwidth
    /// uniformly and fairly.
    RoundRobin = sys::CT_SEED_CHOKING_ALGORITHM_ROUND_ROBIN as i32,
    /// Unchokes the peers we can send to fastest, better utilizing the
    /// available capacity.
    FastestUpload = sys::CT_SEED_CHOKING_ALGORITHM_FASTEST_UPLOAD as i32,
    /// Prioritizes peers that just started or are about to finish,
    /// forcing mid-download peers to trade with each other.
    AntiLeech = sys::CT_SEED_CHOKING_ALGORITHM_ANTI_LEECH as i32,
}

impl SeedChokingAlgorithm {
    pub(crate) fn from_raw(raw: i32) -> Option<Self> {
        match raw {
            x if x == sys::CT_SEED_CHOKING_ALGORITHM_ROUND_ROBIN as i32 => Some(Self::RoundRobin),
            x if x == sys::CT_SEED_CHOKING_ALGORITHM_FASTEST_UPLOAD as i32 => {
                Some(Self::FastestUpload)
            }
            x if x == sys::CT_SEED_CHOKING_ALGORITHM_ANTI_LEECH as i32 => Some(Self::AntiLeech),
            _ => None,
        }
    }
}

/// Values for the `io_buffer_mode`-family settings
/// (`lt::settings_pack::io_buffer_mode_t`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum IoBufferMode {
    EnableOsCache = sys::CT_IO_BUFFER_MODE_ENABLE_OS_CACHE as i32,

    DisableOsCache = sys::CT_IO_BUFFER_MODE_DISABLE_OS_CACHE as i32,

    WriteThrough = sys::CT_IO_BUFFER_MODE_WRITE_THROUGH as i32,
}

impl IoBufferMode {
    pub(crate) fn from_raw(raw: i32) -> Option<Self> {
        match raw {
            x if x == sys::CT_IO_BUFFER_MODE_ENABLE_OS_CACHE as i32 => Some(Self::EnableOsCache),
            x if x == sys::CT_IO_BUFFER_MODE_DISABLE_OS_CACHE as i32 => Some(Self::DisableOsCache),
            x if x == sys::CT_IO_BUFFER_MODE_WRITE_THROUGH as i32 => Some(Self::WriteThrough),
            _ => None,
        }
    }
}

/// Values for the `bandwidth_mixed_algo`-family settings
/// (`lt::settings_pack::bandwidth_mixed_algo_t`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum BandwidthMixedAlgo {
    /// No mixed-mode balancing (uTP yields to TCP).
    PreferTcp = sys::CT_BANDWIDTH_MIXED_ALGO_PREFER_TCP as i32,
    /// Leaves uTP unthrottled; limits TCP to its proportional share of
    /// throughput by connection count.
    PeerProportional = sys::CT_BANDWIDTH_MIXED_ALGO_PEER_PROPORTIONAL as i32,
}

impl BandwidthMixedAlgo {
    pub(crate) fn from_raw(raw: i32) -> Option<Self> {
        match raw {
            x if x == sys::CT_BANDWIDTH_MIXED_ALGO_PREFER_TCP as i32 => Some(Self::PreferTcp),
            x if x == sys::CT_BANDWIDTH_MIXED_ALGO_PEER_PROPORTIONAL as i32 => {
                Some(Self::PeerProportional)
            }
            _ => None,
        }
    }
}

/// Values for the `enc_policy`-family settings
/// (`lt::settings_pack::enc_policy`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum EncPolicy {
    /// Only encrypted connections: unencrypted incoming connections are
    /// closed and no plaintext retry is made for failed outgoing ones.
    PeForced = sys::CT_ENC_POLICY_PE_FORCED as i32,
    /// Prefer encrypted connections, but accept unencrypted incoming ones
    /// and fall back to plaintext when outgoing encryption fails.
    PeEnabled = sys::CT_ENC_POLICY_PE_ENABLED as i32,
    /// Only non-encrypted connections are allowed.
    PeDisabled = sys::CT_ENC_POLICY_PE_DISABLED as i32,
}

impl EncPolicy {
    pub(crate) fn from_raw(raw: i32) -> Option<Self> {
        match raw {
            x if x == sys::CT_ENC_POLICY_PE_FORCED as i32 => Some(Self::PeForced),
            x if x == sys::CT_ENC_POLICY_PE_ENABLED as i32 => Some(Self::PeEnabled),
            x if x == sys::CT_ENC_POLICY_PE_DISABLED as i32 => Some(Self::PeDisabled),
            _ => None,
        }
    }
}

/// Values for the `enc_level`-family settings
/// (`lt::settings_pack::enc_level`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum EncLevel {
    /// Use only plaintext encryption.
    PePlaintext = sys::CT_ENC_LEVEL_PE_PLAINTEXT as i32,
    /// Use only RC4 encryption.
    PeRc4 = sys::CT_ENC_LEVEL_PE_RC4 as i32,
    /// Allow both.
    PeBoth = sys::CT_ENC_LEVEL_PE_BOTH as i32,
}

impl EncLevel {
    pub(crate) fn from_raw(raw: i32) -> Option<Self> {
        match raw {
            x if x == sys::CT_ENC_LEVEL_PE_PLAINTEXT as i32 => Some(Self::PePlaintext),
            x if x == sys::CT_ENC_LEVEL_PE_RC4 as i32 => Some(Self::PeRc4),
            x if x == sys::CT_ENC_LEVEL_PE_BOTH as i32 => Some(Self::PeBoth),
            _ => None,
        }
    }
}

/// Values for the `proxy_type`-family settings
/// (`lt::settings_pack::proxy_type_t`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum ProxyType {
    /// No proxy server is used and all other fields are ignored.
    None = sys::CT_PROXY_TYPE_NONE as i32,
    /// A SOCKS4 proxy; requires a username.
    Socks4 = sys::CT_PROXY_TYPE_SOCKS4 as i32,
    /// A SOCKS5 proxy (RFC 1928) without authentication; username and
    /// password are ignored.
    Socks5 = sys::CT_PROXY_TYPE_SOCKS5 as i32,
    /// A SOCKS5 proxy with plaintext username/password authentication
    /// (RFC 1929).
    Socks5Pw = sys::CT_PROXY_TYPE_SOCKS5_PW as i32,
    /// An HTTP proxy without authorization; non-HTTP transports use the
    /// CONNECT method.
    Http = sys::CT_PROXY_TYPE_HTTP as i32,
    /// An HTTP proxy requiring username/password authorization.
    HttpPw = sys::CT_PROXY_TYPE_HTTP_PW as i32,
}

impl ProxyType {
    pub(crate) fn from_raw(raw: i32) -> Option<Self> {
        match raw {
            x if x == sys::CT_PROXY_TYPE_NONE as i32 => Some(Self::None),
            x if x == sys::CT_PROXY_TYPE_SOCKS4 as i32 => Some(Self::Socks4),
            x if x == sys::CT_PROXY_TYPE_SOCKS5 as i32 => Some(Self::Socks5),
            x if x == sys::CT_PROXY_TYPE_SOCKS5_PW as i32 => Some(Self::Socks5Pw),
            x if x == sys::CT_PROXY_TYPE_HTTP as i32 => Some(Self::Http),
            x if x == sys::CT_PROXY_TYPE_HTTP_PW as i32 => Some(Self::HttpPw),
            _ => None,
        }
    }
}
