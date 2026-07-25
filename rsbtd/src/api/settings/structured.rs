// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The structured settings: typed GraphQL objects and enums translated
//! to one or more backing libtorrent settings.
//!
//! Validation is strict: values and combinations libtorrent would
//! silently ignore are rejected. Readback builds the public value from
//! the effective pack, so a read always reflects what libtorrent is
//! using. Error messages name fields as they appear in the GraphQL
//! schema (camelCase).

use std::num::NonZeroU16;

use async_graphql::{Enum, InputObject, MaybeUndefined, SimpleObject};
use rbtorrent::SettingsPack;
use rbtorrent::settings as lt;

use super::{Nullable, SettingsError, defined, invalid, missing, nullable, rb_err};

/// The qBittorrent version `userAgent: QBITTORRENT` impersonates.
/// Bump deliberately: trackers whitelist specific versions.
pub const QBITTORRENT_COMPAT_VERSION: &str = "5.2.3";

/// Ports where 0 (ephemeral) makes no sense, as the rbtorrent types
/// that carry the constraint.
fn port_for(place: &str, port: u16) -> Result<NonZeroU16, SettingsError> {
    NonZeroU16::new(port).ok_or_else(|| invalid(format!("{place} must be in 1..=65535")))
}

/// Converts an effective int setting to `u16` for the public view.
fn effective_port(name: &str, value: i32) -> Result<u16, SettingsError> {
    u16::try_from(value).map_err(|_| invalid(format!("{name}: effective value out of range")))
}

// ---- user_agent ---------------------------------------------------------

/// The user agent identity presented to trackers and peers.
#[derive(Enum, Clone, Copy, PartialEq, Eq, Debug)]
pub enum UserAgent {
    /// No user agent: an empty user-agent string.
    None,
    /// The stock identity of the embedded BitTorrent engine.
    Libtorrent,
    /// This daemon's own identity.
    Rsbtd,
    /// Impersonates qBittorrent (a fixed, known-good version) for
    /// trackers that whitelist specific clients.
    #[graphql(name = "QBITTORRENT")]
    QBittorrent,
    /// The effective value was set outside this API (e.g. session state
    /// persisted by a different daemon version). Read-only: writing it
    /// is rejected.
    Unrecognized,
}

pub(super) fn read_user_agent(pack: &SettingsPack) -> Result<UserAgent, SettingsError> {
    let raw = pack.get_user_agent().ok_or_else(|| missing("user_agent"))?;
    // Prefix matching, not exact: persisted state carries version-stamped
    // strings that must survive daemon/libtorrent version bumps.
    Ok(if raw.is_empty() {
        UserAgent::None
    } else if raw.starts_with("libtorrent/") {
        UserAgent::Libtorrent
    } else if raw.starts_with("rsbtd/") {
        UserAgent::Rsbtd
    } else if raw.starts_with("qBittorrent/") {
        UserAgent::QBittorrent
    } else {
        UserAgent::Unrecognized
    })
}

pub(super) fn write_user_agent(
    delta: &mut SettingsPack,
    value: MaybeUndefined<UserAgent>,
) -> Result<(), SettingsError> {
    let Some(choice) = defined(value, "user_agent")? else {
        return Ok(());
    };
    let value = match choice {
        UserAgent::None => String::new(),
        UserAgent::Libtorrent => format!("libtorrent/{}", rbtorrent::libtorrent_version()),
        UserAgent::Rsbtd => format!("rsbtd/{}", env!("CARGO_PKG_VERSION")),
        UserAgent::QBittorrent => format!("qBittorrent/{QBITTORRENT_COMPAT_VERSION}"),
        UserAgent::Unrecognized => {
            return Err(invalid(
                "userAgent: UNRECOGNIZED is read-only; choose a concrete identity",
            ));
        }
    };
    delta.user_agent(&value);
    Ok(())
}

// ---- proxy ----------------------------------------------------------------

const PROXY_BACKING: &[&str] = &[
    "proxy_type",
    "proxy_hostname",
    "proxy_port",
    "proxy_username",
    "proxy_password",
    "proxy_hostnames",
    "proxy_peer_connections",
    "proxy_tracker_connections",
    "socks5_udp_send_local_ep",
    "proxy_send_host_in_connect",
];

/// The protocol spoken to the proxy server.
#[derive(Enum, Clone, Copy, PartialEq, Eq, Debug)]
pub enum ProxyProtocol {
    /// SOCKS4; requires `username`, cannot resolve hostnames remotely.
    #[graphql(name = "SOCKS4")]
    Socks4,
    /// SOCKS5 without authentication.
    #[graphql(name = "SOCKS5")]
    Socks5,
    /// SOCKS5 with username/password authentication (RFC 1929).
    #[graphql(name = "SOCKS5_PASSWORD")]
    Socks5Password,
    /// An HTTP proxy supporting CONNECT, without authentication.
    Http,
    /// An HTTP proxy with basic authentication.
    HttpPassword,
}

impl ProxyProtocol {
    /// `None` for `ProxyType::None` (no proxy configured).
    fn from_lt(ty: lt::ProxyType) -> Option<Self> {
        match ty {
            lt::ProxyType::None => None,
            lt::ProxyType::Socks4 => Some(ProxyProtocol::Socks4),
            lt::ProxyType::Socks5 => Some(ProxyProtocol::Socks5),
            lt::ProxyType::Socks5Pw => Some(ProxyProtocol::Socks5Password),
            lt::ProxyType::Http => Some(ProxyProtocol::Http),
            lt::ProxyType::HttpPw => Some(ProxyProtocol::HttpPassword),
        }
    }
}

/// A proxy for outgoing connections. On write, combinations the
/// protocol cannot express (e.g. credentials on plain SOCKS5) are
/// rejected rather than silently ignored.
#[derive(Debug, Clone, PartialEq, SimpleObject, InputObject)]
#[graphql(input_name = "ProxySettingsInput")]
pub struct ProxySettings {
    /// The protocol spoken to the proxy server.
    pub protocol: ProxyProtocol,
    /// Hostname or address of the proxy server.
    pub hostname: String,
    /// Port of the proxy server (1..=65535).
    pub port: u16,
    /// Required by SOCKS4 and the `*_PASSWORD` protocols, forbidden
    /// otherwise (empty when unused).
    #[graphql(default)]
    pub username: String,
    /// Required by the `*_PASSWORD` protocols, forbidden otherwise
    /// (empty when unused).
    #[graphql(default)]
    pub password: String,
    /// Resolve hostnames through the proxy instead of locally, hiding
    /// DNS lookups from the local network (not supported by SOCKS4).
    pub resolve_hostnames: bool,
    /// Route peer connections through the proxy.
    pub peer_connections: bool,
    /// Route tracker announces and scrapes through the proxy.
    pub tracker_connections: bool,
    /// SOCKS5 only: send the actual local port in the UDP ASSOCIATE
    /// command instead of 0, for proxies that require it.
    #[graphql(default = false)]
    pub socks5_udp_send_local_endpoint: bool,
    /// HTTP only: send the hostname instead of the resolved IP in
    /// CONNECT requests.
    #[graphql(default = false)]
    pub send_hostname_in_connect: bool,
}

pub(super) fn read_proxy(pack: &SettingsPack) -> Result<Option<ProxySettings>, SettingsError> {
    let ty = pack.get_proxy_type().ok_or_else(|| missing("proxy"))?;
    if ty == lt::ProxyType::None {
        return Ok(None);
    }
    let protocol = ProxyProtocol::from_lt(ty)
        .ok_or_else(|| invalid("proxy: effective proxy_type is not representable"))?;
    let get = |v: Option<bool>| v.ok_or_else(|| missing("proxy"));
    Ok(Some(ProxySettings {
        protocol,
        hostname: pack.get_proxy_hostname().ok_or_else(|| missing("proxy"))?,
        port: effective_port(
            "proxy.port",
            pack.get_proxy_port().ok_or_else(|| missing("proxy"))?,
        )?,
        username: pack.get_proxy_username().ok_or_else(|| missing("proxy"))?,
        password: pack.get_proxy_password().ok_or_else(|| missing("proxy"))?,
        resolve_hostnames: get(pack.get_proxy_hostnames())?,
        peer_connections: get(pack.get_proxy_peer_connections())?,
        tracker_connections: get(pack.get_proxy_tracker_connections())?,
        socks5_udp_send_local_endpoint: get(pack.get_socks5_udp_send_local_ep())?,
        send_hostname_in_connect: get(pack.get_proxy_send_host_in_connect())?,
    }))
}

pub(super) fn write_proxy(
    delta: &mut SettingsPack,
    value: MaybeUndefined<ProxySettings>,
) -> Result<(), SettingsError> {
    let p = match nullable(value) {
        Nullable::Unchanged => return Ok(()),
        Nullable::Disable => {
            let _ = delta.proxy(None).map_err(|e| rb_err("proxy", &e))?;
            return Ok(());
        }
        Nullable::Set(p) => p,
    };
    let port = port_for("proxy.port", p.port)?;

    // The flat wire shape can state combinations no protocol can
    // express; reject what the rbtorrent ADT has no slot for. Value
    // rules (empty host/credentials, host token shape) live in
    // rbtorrent's `proxy` setter.
    let wants_auth = matches!(
        p.protocol,
        ProxyProtocol::Socks5Password | ProxyProtocol::HttpPassword
    );
    if !wants_auth && p.protocol != ProxyProtocol::Socks4 && !p.username.is_empty() {
        return Err(invalid("proxy: this protocol does not use `username`"));
    }
    if !wants_auth && !p.password.is_empty() {
        return Err(invalid("proxy: this protocol does not use `password`"));
    }
    if p.resolve_hostnames && p.protocol == ProxyProtocol::Socks4 {
        return Err(invalid(
            "proxy: SOCKS4 cannot resolve hostnames on the proxy (`resolveHostnames`)",
        ));
    }
    if p.socks5_udp_send_local_endpoint
        && !matches!(
            p.protocol,
            ProxyProtocol::Socks5 | ProxyProtocol::Socks5Password
        )
    {
        return Err(invalid(
            "proxy: `socks5UdpSendLocalEndpoint` is only valid for SOCKS5 protocols",
        ));
    }
    if p.send_hostname_in_connect
        && !matches!(
            p.protocol,
            ProxyProtocol::Http | ProxyProtocol::HttpPassword
        )
    {
        return Err(invalid(
            "proxy: `sendHostnameInConnect` is only valid for HTTP protocols",
        ));
    }

    let auth = wants_auth.then(|| lt::Credentials {
        username: p.username.clone(),
        password: p.password.clone(),
    });
    let protocol = match p.protocol {
        ProxyProtocol::Socks4 => lt::ProxyProtocol::Socks4 {
            username: p.username.clone(),
        },
        ProxyProtocol::Socks5 | ProxyProtocol::Socks5Password => lt::ProxyProtocol::Socks5 {
            auth,
            resolve_hostnames: p.resolve_hostnames,
            udp_send_local_endpoint: p.socks5_udp_send_local_endpoint,
        },
        ProxyProtocol::Http | ProxyProtocol::HttpPassword => lt::ProxyProtocol::Http {
            auth,
            resolve_hostnames: p.resolve_hostnames,
            send_hostname_in_connect: p.send_hostname_in_connect,
        },
    };
    let config = lt::ProxyConfig {
        protocol,
        host: p.hostname.clone(),
        port,
        peer_connections: p.peer_connections,
        tracker_connections: p.tracker_connections,
    };
    let _ = delta
        .proxy(Some(&config))
        .map_err(|e| rb_err("proxy", &e))?;
    Ok(())
}

// ---- i2p ------------------------------------------------------------------

const I2P_BACKING: &[&str] = &[
    "i2p_hostname",
    "i2p_port",
    "allow_i2p_mixed",
    "i2p_inbound_quantity",
    "i2p_outbound_quantity",
    "i2p_inbound_length",
    "i2p_outbound_length",
    "i2p_inbound_length_variance",
    "i2p_outbound_length_variance",
];

/// One direction of I2P tunnel configuration.
// Both names pinned: the default rename mangles "I2p" to "I2P"/"I2Pt".
#[derive(Debug, Clone, PartialEq, SimpleObject, InputObject)]
#[graphql(name = "I2pTunnel", input_name = "I2pTunnelInput")]
pub struct I2pTunnel {
    /// Number of parallel tunnels (1..=16).
    pub tunnels: u8,
    /// Hops per tunnel (0..=7). More hops, more anonymity, more latency.
    pub hops: u8,
    /// Random variance applied to `hops` (-7..=7).
    pub hop_variance: i8,
}

/// The I2P SAM bridge configuration.
// Both names pinned: the default rename mangles "I2p" to "I2P"/"I2Ps".
#[derive(Debug, Clone, PartialEq, SimpleObject, InputObject)]
#[graphql(name = "I2pSettings", input_name = "I2pSettingsInput")]
pub struct I2pSettings {
    /// Hostname of the SAM bridge: a hostname, IPv4 address, or IPv6
    /// literal (stored bare; a bracketed literal is accepted and
    /// unbracketed).
    pub hostname: String,
    /// Port of the SAM bridge.
    pub port: u16,
    /// Allow mixing regular peers with I2P peers in the same swarm
    /// (weakens the anonymity of the I2P side).
    pub allow_mixed: bool,
    /// Inbound tunnel configuration.
    pub inbound: I2pTunnel,
    /// Outbound tunnel configuration.
    pub outbound: I2pTunnel,
}

pub(super) fn read_i2p(pack: &SettingsPack) -> Result<Option<I2pSettings>, SettingsError> {
    let Some(config) = pack.get_i2p() else {
        return Err(invalid("i2p: effective values are not representable"));
    };
    let tunnel = |t: lt::I2pTunnels| I2pTunnel {
        tunnels: t.quantity,
        hops: t.length,
        hop_variance: t.variance,
    };
    Ok(config.map(|c| I2pSettings {
        hostname: c.sam_host,
        port: c.sam_port.get(),
        allow_mixed: c.allow_mixed,
        inbound: tunnel(c.inbound),
        outbound: tunnel(c.outbound),
    }))
}

pub(super) fn write_i2p(
    delta: &mut SettingsPack,
    value: MaybeUndefined<I2pSettings>,
) -> Result<(), SettingsError> {
    let c = match nullable(value) {
        Nullable::Unchanged => return Ok(()),
        Nullable::Disable => {
            let _ = delta.i2p(None).map_err(|e| rb_err("i2p", &e))?;
            return Ok(());
        }
        Nullable::Set(c) => c,
    };
    // i2p_hostname is a standalone resolver hostname, so an IPv6
    // literal goes in without brackets; accept the bracketed form but
    // store it bare.
    let hostname = c
        .hostname
        .strip_prefix('[')
        .and_then(|h| h.strip_suffix(']'))
        .unwrap_or(&c.hostname);
    let tunnels = |t: &I2pTunnel| lt::I2pTunnels {
        quantity: t.tunnels,
        length: t.hops,
        variance: t.hop_variance,
    };
    let config = lt::I2pConfig {
        sam_host: hostname.to_owned(),
        sam_port: port_for("i2p.port", c.port)?,
        inbound: tunnels(&c.inbound),
        outbound: tunnels(&c.outbound),
        allow_mixed: c.allow_mixed,
    };
    let _ = delta.i2p(Some(&config)).map_err(|e| rb_err("i2p", &e))?;
    Ok(())
}

// ---- encryption -------------------------------------------------------------

const ENCRYPTION_BACKING: &[&str] = &[
    "out_enc_policy",
    "in_enc_policy",
    "allowed_enc_level",
    "prefer_rc4",
    "announce_crypto_support",
];

/// How strongly to insist on protocol encryption for one direction.
#[derive(Enum, Clone, Copy, PartialEq, Eq, Debug)]
pub enum EncryptionPolicy {
    /// Only encrypted connections; unencrypted peers are dropped.
    Forced,
    /// Prefer encrypted connections, allow unencrypted ones.
    Enabled,
    /// Only unencrypted connections.
    Disabled,
}

impl EncryptionPolicy {
    fn to_lt(self) -> lt::EncPolicy {
        match self {
            EncryptionPolicy::Forced => lt::EncPolicy::PeForced,
            EncryptionPolicy::Enabled => lt::EncPolicy::PeEnabled,
            EncryptionPolicy::Disabled => lt::EncPolicy::PeDisabled,
        }
    }

    fn from_lt(p: lt::EncPolicy) -> Self {
        match p {
            lt::EncPolicy::PeForced => EncryptionPolicy::Forced,
            lt::EncPolicy::PeEnabled => EncryptionPolicy::Enabled,
            lt::EncPolicy::PeDisabled => EncryptionPolicy::Disabled,
        }
    }
}

/// The encryption methods allowed on an encrypted connection. At least
/// one must be enabled.
#[derive(Debug, Clone, PartialEq, SimpleObject, InputObject)]
#[graphql(input_name = "EncryptionMethodsInput")]
pub struct EncryptionMethods {
    /// Allow the plaintext method (header obfuscation only).
    pub plaintext: bool,
    /// Allow the RC4 method (full-stream encryption).
    pub rc4: bool,
}

/// The BitTorrent protocol-encryption policy.
#[derive(Debug, Clone, PartialEq, SimpleObject, InputObject)]
#[graphql(input_name = "EncryptionSettingsInput")]
pub struct EncryptionSettings {
    /// Policy for incoming connections.
    pub incoming: EncryptionPolicy,
    /// Policy for outgoing connections.
    pub outgoing: EncryptionPolicy,
    /// The encryption methods to allow; at least one must be enabled.
    pub methods: EncryptionMethods,
    /// Prefer RC4 when both methods are enabled (invalid otherwise).
    pub prefer_rc4: bool,
    /// Announce crypto support to trackers.
    pub announce_support: bool,
}

pub(super) fn read_encryption(pack: &SettingsPack) -> Result<EncryptionSettings, SettingsError> {
    let level = pack
        .get_allowed_enc_level()
        .ok_or_else(|| missing("encryption"))?;
    let (plaintext, rc4) = match level {
        lt::EncLevel::PePlaintext => (true, false),
        lt::EncLevel::PeRc4 => (false, true),
        lt::EncLevel::PeBoth => (true, true),
    };
    let policy = |p: Option<lt::EncPolicy>| {
        p.map(EncryptionPolicy::from_lt)
            .ok_or_else(|| missing("encryption"))
    };
    Ok(EncryptionSettings {
        incoming: policy(pack.get_in_enc_policy())?,
        outgoing: policy(pack.get_out_enc_policy())?,
        methods: EncryptionMethods { plaintext, rc4 },
        prefer_rc4: pack.get_prefer_rc4().ok_or_else(|| missing("encryption"))?,
        announce_support: pack
            .get_announce_crypto_support()
            .ok_or_else(|| missing("encryption"))?,
    })
}

pub(super) fn write_encryption(
    delta: &mut SettingsPack,
    value: MaybeUndefined<EncryptionSettings>,
) -> Result<(), SettingsError> {
    let Some(e) = defined(value, "encryption")? else {
        return Ok(());
    };
    let level = match (e.methods.plaintext, e.methods.rc4) {
        (true, true) => lt::EncLevel::PeBoth,
        (true, false) => lt::EncLevel::PePlaintext,
        (false, true) => lt::EncLevel::PeRc4,
        (false, false) => {
            return Err(invalid(
                "encryption: at least one of `methods.plaintext`, `methods.rc4` must be enabled",
            ));
        }
    };
    if e.prefer_rc4 && level != lt::EncLevel::PeBoth {
        return Err(invalid(
            "encryption: `preferRc4` is only meaningful when both methods are enabled",
        ));
    }
    delta
        .in_enc_policy(e.incoming.to_lt())
        .out_enc_policy(e.outgoing.to_lt())
        .allowed_enc_level(level)
        .prefer_rc4(e.prefer_rc4)
        .announce_crypto_support(e.announce_support);
    Ok(())
}

// ---- peer_transports ---------------------------------------------------------

const PEER_TRANSPORTS_BACKING: &[&str] = &[
    "enable_incoming_tcp",
    "enable_outgoing_tcp",
    "enable_incoming_utp",
    "enable_outgoing_utp",
];

/// Whether one transport is enabled, per connection direction.
#[derive(Debug, Clone, PartialEq, SimpleObject, InputObject)]
#[graphql(input_name = "TransportDirectionsInput")]
pub struct TransportDirections {
    /// Accept incoming connections over this transport.
    pub incoming: bool,
    /// Make outgoing connections over this transport.
    pub outgoing: bool,
}

/// The peer transport protocols in use, per connection direction.
#[derive(Debug, Clone, PartialEq, SimpleObject, InputObject)]
#[graphql(input_name = "PeerTransportsInput")]
pub struct PeerTransports {
    /// Plain TCP peer connections.
    pub tcp: TransportDirections,
    /// uTP (micro transport protocol) peer connections over UDP.
    pub utp: TransportDirections,
}

pub(super) fn read_peer_transports(pack: &SettingsPack) -> Result<PeerTransports, SettingsError> {
    let get = |v: Option<bool>| v.ok_or_else(|| missing("peer_transports"));
    Ok(PeerTransports {
        tcp: TransportDirections {
            incoming: get(pack.get_enable_incoming_tcp())?,
            outgoing: get(pack.get_enable_outgoing_tcp())?,
        },
        utp: TransportDirections {
            incoming: get(pack.get_enable_incoming_utp())?,
            outgoing: get(pack.get_enable_outgoing_utp())?,
        },
    })
}

pub(super) fn write_peer_transports(
    delta: &mut SettingsPack,
    value: MaybeUndefined<PeerTransports>,
) -> Result<(), SettingsError> {
    let Some(t) = defined(value, "peer_transports")? else {
        return Ok(());
    };
    delta
        .enable_incoming_tcp(t.tcp.incoming)
        .enable_outgoing_tcp(t.tcp.outgoing)
        .enable_incoming_utp(t.utp.incoming)
        .enable_outgoing_utp(t.utp.outgoing);
    Ok(())
}

// ---- outgoing_port_range ------------------------------------------------------

/// An inclusive port range (`first` >= 1, `last` >= `first`).
#[derive(Debug, Clone, PartialEq, SimpleObject, InputObject)]
#[graphql(input_name = "PortRangeInput")]
pub struct PortRange {
    /// First port of the range (inclusive).
    pub first: u16,
    /// Last port of the range (inclusive).
    pub last: u16,
}

pub(super) fn read_outgoing_port_range(
    pack: &SettingsPack,
) -> Result<Option<PortRange>, SettingsError> {
    let Some(range) = pack.get_outgoing_ports() else {
        return Err(invalid(
            "outgoingPortRange: effective values are not representable",
        ));
    };
    Ok(range.map(|r| PortRange {
        first: *r.start(),
        last: *r.end(),
    }))
}

pub(super) fn write_outgoing_port_range(
    delta: &mut SettingsPack,
    value: MaybeUndefined<PortRange>,
) -> Result<(), SettingsError> {
    let range = match nullable(value) {
        Nullable::Unchanged => return Ok(()),
        // None restores OS-selected ephemeral ports.
        Nullable::Disable => None,
        Nullable::Set(r) => Some(r.first..=r.last),
    };
    let _ = delta
        .outgoing_ports(range)
        .map_err(|e| rb_err("outgoing_port_range", &e))?;
    Ok(())
}

// ---- disk_io_cache / mmap_write_mode -----------------------------------

/// OS cache interaction for disk reads.
#[derive(Enum, Clone, Copy, PartialEq, Eq, Debug)]
pub enum IoReadMode {
    /// Use the OS page cache normally.
    EnableOsCache,
    /// Bypass the OS page cache (O_DIRECT-style).
    DisableOsCache,
}

impl IoReadMode {
    fn to_lt(self) -> lt::IoBufferMode {
        match self {
            IoReadMode::EnableOsCache => lt::IoBufferMode::EnableOsCache,
            IoReadMode::DisableOsCache => lt::IoBufferMode::DisableOsCache,
        }
    }

    fn from_lt(m: lt::IoBufferMode) -> Self {
        match m {
            lt::IoBufferMode::EnableOsCache => IoReadMode::EnableOsCache,
            lt::IoBufferMode::DisableOsCache => IoReadMode::DisableOsCache,
            // A foreign state blob may carry the write-only mode; upstream
            // read paths treat it like normal cached reads.
            lt::IoBufferMode::WriteThrough => IoReadMode::EnableOsCache,
        }
    }
}

/// OS cache interaction for disk writes.
#[derive(Enum, Clone, Copy, PartialEq, Eq, Debug)]
pub enum IoWriteMode {
    /// Use the OS page cache normally.
    EnableOsCache,
    /// Bypass the OS page cache (O_DIRECT-style).
    DisableOsCache,
    /// Write through the OS page cache but flush immediately.
    WriteThrough,
}

impl IoWriteMode {
    fn to_lt(self) -> lt::IoBufferMode {
        match self {
            IoWriteMode::EnableOsCache => lt::IoBufferMode::EnableOsCache,
            IoWriteMode::DisableOsCache => lt::IoBufferMode::DisableOsCache,
            IoWriteMode::WriteThrough => lt::IoBufferMode::WriteThrough,
        }
    }

    fn from_lt(m: lt::IoBufferMode) -> Self {
        match m {
            lt::IoBufferMode::EnableOsCache => IoWriteMode::EnableOsCache,
            lt::IoBufferMode::DisableOsCache => IoWriteMode::DisableOsCache,
            lt::IoBufferMode::WriteThrough => IoWriteMode::WriteThrough,
        }
    }
}

/// OS cache behavior of disk reads and writes. Write-through only exists
/// for writes; libtorrent's read paths have no equivalent.
#[derive(Debug, Clone, PartialEq, SimpleObject, InputObject)]
#[graphql(input_name = "DiskIoCacheInput")]
pub struct DiskIoCache {
    /// Cache mode for disk reads.
    pub read: IoReadMode,
    /// Cache mode for disk writes.
    pub write: IoWriteMode,
}

pub(super) fn read_disk_io_cache(pack: &SettingsPack) -> Result<DiskIoCache, SettingsError> {
    Ok(DiskIoCache {
        read: pack
            .get_disk_io_read_mode()
            .map(IoReadMode::from_lt)
            .ok_or_else(|| missing("disk_io_cache"))?,
        write: pack
            .get_disk_io_write_mode()
            .map(IoWriteMode::from_lt)
            .ok_or_else(|| missing("disk_io_cache"))?,
    })
}

pub(super) fn write_disk_io_cache(
    delta: &mut SettingsPack,
    value: MaybeUndefined<DiskIoCache>,
) -> Result<(), SettingsError> {
    let Some(c) = defined(value, "disk_io_cache")? else {
        return Ok(());
    };
    delta
        .disk_io_read_mode(c.read.to_lt())
        .disk_io_write_mode(c.write.to_lt());
    Ok(())
}

/// How to flush pieces to disk with the mmap disk I/O backend.
#[derive(Enum, Clone, Copy, PartialEq, Eq, Debug)]
pub enum MmapWriteMode {
    /// Always write files with pwrite() (never memory-map them).
    AlwaysPwrite,
    /// Always write files via memory-mapped views.
    AlwaysMmapWrite,
    /// Memory-map for writing only when the file fits the mmap size
    /// cutoff.
    AutoMmapWrite,
}

impl MmapWriteMode {
    fn to_lt(self) -> lt::MmapWriteMode {
        match self {
            MmapWriteMode::AlwaysPwrite => lt::MmapWriteMode::AlwaysPwrite,
            MmapWriteMode::AlwaysMmapWrite => lt::MmapWriteMode::AlwaysMmapWrite,
            MmapWriteMode::AutoMmapWrite => lt::MmapWriteMode::AutoMmapWrite,
        }
    }

    fn from_lt(m: lt::MmapWriteMode) -> Self {
        match m {
            lt::MmapWriteMode::AlwaysPwrite => MmapWriteMode::AlwaysPwrite,
            lt::MmapWriteMode::AlwaysMmapWrite => MmapWriteMode::AlwaysMmapWrite,
            lt::MmapWriteMode::AutoMmapWrite => MmapWriteMode::AutoMmapWrite,
        }
    }
}

pub(super) fn read_mmap_write_mode(pack: &SettingsPack) -> Result<MmapWriteMode, SettingsError> {
    pack.get_disk_write_mode()
        .map(MmapWriteMode::from_lt)
        .ok_or_else(|| missing("mmap_write_mode"))
}

pub(super) fn write_mmap_write_mode(
    delta: &mut SettingsPack,
    value: MaybeUndefined<MmapWriteMode>,
) -> Result<(), SettingsError> {
    if let Some(m) = defined(value, "mmap_write_mode")? {
        delta.disk_write_mode(m.to_lt());
    }
    Ok(())
}

// ---- integer enums ------------------------------------------------------------

/// Whether to send `suggest` messages to peers.
#[derive(Enum, Clone, Copy, PartialEq, Eq, Debug)]
pub enum SuggestMode {
    /// Send no piece suggestions.
    None,
    /// Suggest pieces that are in the read cache (cheap to serve).
    ReadCache,
}

impl SuggestMode {
    fn to_lt(self) -> lt::SuggestMode {
        match self {
            SuggestMode::None => lt::SuggestMode::NoPieceSuggestions,
            SuggestMode::ReadCache => lt::SuggestMode::SuggestReadCache,
        }
    }

    fn from_lt(m: lt::SuggestMode) -> Self {
        match m {
            lt::SuggestMode::NoPieceSuggestions => SuggestMode::None,
            lt::SuggestMode::SuggestReadCache => SuggestMode::ReadCache,
        }
    }
}

pub(super) fn read_suggest_mode(pack: &SettingsPack) -> Result<SuggestMode, SettingsError> {
    pack.get_suggest_mode()
        .map(SuggestMode::from_lt)
        .ok_or_else(|| missing("suggest_mode"))
}

pub(super) fn write_suggest_mode(
    delta: &mut SettingsPack,
    value: MaybeUndefined<SuggestMode>,
) -> Result<(), SettingsError> {
    if let Some(m) = defined(value, "suggest_mode")? {
        delta.suggest_mode(m.to_lt());
    }
    Ok(())
}

/// Which algorithm chooses peers to unchoke.
#[derive(Enum, Clone, Copy, PartialEq, Eq, Debug)]
pub enum ChokingAlgorithm {
    /// A fixed number of unchoke slots (`unchokeSlotsLimit`).
    FixedSlots,
    /// Open up unchoke slots based on the upload rate achieved to
    /// peers, controlled by `rateChokerInitialThreshold`.
    RateBased,
}

impl ChokingAlgorithm {
    fn to_lt(self) -> lt::ChokingAlgorithm {
        match self {
            ChokingAlgorithm::FixedSlots => lt::ChokingAlgorithm::FixedSlotsChoker,
            ChokingAlgorithm::RateBased => lt::ChokingAlgorithm::RateBasedChoker,
        }
    }

    fn from_lt(a: lt::ChokingAlgorithm) -> Self {
        match a {
            lt::ChokingAlgorithm::FixedSlotsChoker => ChokingAlgorithm::FixedSlots,
            lt::ChokingAlgorithm::RateBasedChoker => ChokingAlgorithm::RateBased,
        }
    }
}

pub(super) fn read_choking_algorithm(
    pack: &SettingsPack,
) -> Result<ChokingAlgorithm, SettingsError> {
    pack.get_choking_algorithm()
        .map(ChokingAlgorithm::from_lt)
        .ok_or_else(|| missing("choking_algorithm"))
}

pub(super) fn write_choking_algorithm(
    delta: &mut SettingsPack,
    value: MaybeUndefined<ChokingAlgorithm>,
) -> Result<(), SettingsError> {
    if let Some(a) = defined(value, "choking_algorithm")? {
        delta.choking_algorithm(a.to_lt());
    }
    Ok(())
}

/// Which algorithm chooses peers to unchoke while seeding.
#[derive(Enum, Clone, Copy, PartialEq, Eq, Debug)]
pub enum SeedChokingAlgorithm {
    /// Round-robin between peers, to distribute pieces widely.
    RoundRobin,
    /// Prefer the peers we upload to the fastest.
    FastestUpload,
    /// Prefer peers with the least progress, to counter leech-only
    /// clients.
    AntiLeech,
}

impl SeedChokingAlgorithm {
    fn to_lt(self) -> lt::SeedChokingAlgorithm {
        match self {
            SeedChokingAlgorithm::RoundRobin => lt::SeedChokingAlgorithm::RoundRobin,
            SeedChokingAlgorithm::FastestUpload => lt::SeedChokingAlgorithm::FastestUpload,
            SeedChokingAlgorithm::AntiLeech => lt::SeedChokingAlgorithm::AntiLeech,
        }
    }

    fn from_lt(a: lt::SeedChokingAlgorithm) -> Self {
        match a {
            lt::SeedChokingAlgorithm::RoundRobin => SeedChokingAlgorithm::RoundRobin,
            lt::SeedChokingAlgorithm::FastestUpload => SeedChokingAlgorithm::FastestUpload,
            lt::SeedChokingAlgorithm::AntiLeech => SeedChokingAlgorithm::AntiLeech,
        }
    }
}

pub(super) fn read_seed_choking_algorithm(
    pack: &SettingsPack,
) -> Result<SeedChokingAlgorithm, SettingsError> {
    pack.get_seed_choking_algorithm()
        .map(SeedChokingAlgorithm::from_lt)
        .ok_or_else(|| missing("seed_choking_algorithm"))
}

pub(super) fn write_seed_choking_algorithm(
    delta: &mut SettingsPack,
    value: MaybeUndefined<SeedChokingAlgorithm>,
) -> Result<(), SettingsError> {
    if let Some(a) = defined(value, "seed_choking_algorithm")? {
        delta.seed_choking_algorithm(a.to_lt());
    }
    Ok(())
}

/// How rate limits are divided between TCP and uTP peers when both
/// transports are in use.
#[derive(Enum, Clone, Copy, PartialEq, Eq, Debug)]
pub enum MixedModeAlgorithm {
    /// Throttle TCP down to give uTP connections headroom.
    PreferTcp,
    /// Divide bandwidth proportionally to the number of peers on each
    /// transport.
    PeerProportional,
}

impl MixedModeAlgorithm {
    fn to_lt(self) -> lt::BandwidthMixedAlgo {
        match self {
            MixedModeAlgorithm::PreferTcp => lt::BandwidthMixedAlgo::PreferTcp,
            MixedModeAlgorithm::PeerProportional => lt::BandwidthMixedAlgo::PeerProportional,
        }
    }

    fn from_lt(a: lt::BandwidthMixedAlgo) -> Self {
        match a {
            lt::BandwidthMixedAlgo::PreferTcp => MixedModeAlgorithm::PreferTcp,
            lt::BandwidthMixedAlgo::PeerProportional => MixedModeAlgorithm::PeerProportional,
        }
    }
}

pub(super) fn read_mixed_mode_algorithm(
    pack: &SettingsPack,
) -> Result<MixedModeAlgorithm, SettingsError> {
    pack.get_mixed_mode_algorithm()
        .map(MixedModeAlgorithm::from_lt)
        .ok_or_else(|| missing("mixed_mode_algorithm"))
}

pub(super) fn write_mixed_mode_algorithm(
    delta: &mut SettingsPack,
    value: MaybeUndefined<MixedModeAlgorithm>,
) -> Result<(), SettingsError> {
    if let Some(a) = defined(value, "mixed_mode_algorithm")? {
        delta.mixed_mode_algorithm(a.to_lt());
    }
    Ok(())
}

// ---- structured strings ---------------------------------------------------

pub(super) fn read_outgoing_interfaces(pack: &SettingsPack) -> Result<Vec<String>, SettingsError> {
    let raw = pack
        .get_outgoing_interfaces()
        .ok_or_else(|| missing("outgoing_interfaces"))?;
    if raw.is_empty() {
        return Ok(Vec::new());
    }
    Ok(raw.split(',').map(str::to_owned).collect())
}

pub(super) fn write_outgoing_interfaces(
    delta: &mut SettingsPack,
    value: MaybeUndefined<Vec<String>>,
) -> Result<(), SettingsError> {
    let Some(list) = defined(value, "outgoing_interfaces")? else {
        return Ok(());
    };
    let _ = delta
        .outgoing_interfaces(&list)
        .map_err(|e| rb_err("outgoing_interfaces", &e))?;
    Ok(())
}

/// One listen endpoint for incoming connections.
#[derive(Debug, Clone, PartialEq, SimpleObject, InputObject)]
#[graphql(input_name = "ListenInterfaceInput")]
pub struct ListenInterface {
    /// An IP address or network device name — hostnames are not
    /// resolved here, and a nonexistent device opens no socket (silently,
    /// beyond a listen-failed event). Windows device names are GUID
    /// strings in curly braces.
    pub interface: String,
    /// Port 0 asks the OS for an ephemeral port.
    pub port: u16,
    /// Accept SSL/TLS peer connections on this endpoint.
    #[graphql(default = false)]
    pub ssl: bool,
    /// Treat this endpoint as local-network only: do not announce it
    /// to trackers/DHT beyond the local network.
    #[graphql(default = false)]
    pub local: bool,
}

pub(super) fn read_listen_interfaces(
    pack: &SettingsPack,
) -> Result<Vec<ListenInterface>, SettingsError> {
    let Some(list) = pack.get_listen_interfaces_parsed() else {
        return Err(invalid(
            "listenInterfaces: cannot parse the effective value",
        ));
    };
    Ok(list
        .into_iter()
        .map(|e| ListenInterface {
            interface: e.addr,
            port: e.port,
            ssl: e.ssl,
            local: e.local,
        })
        .collect())
}

pub(super) fn write_listen_interfaces(
    delta: &mut SettingsPack,
    value: MaybeUndefined<Vec<ListenInterface>>,
) -> Result<(), SettingsError> {
    let Some(list) = defined(value, "listen_interfaces")? else {
        return Ok(());
    };
    // Port 0 is allowed: it asks the OS for an ephemeral port.
    let endpoints: Vec<lt::ListenEndpoint> = list
        .into_iter()
        .map(|e| lt::ListenEndpoint {
            addr: e.interface,
            port: e.port,
            ssl: e.ssl,
            local: e.local,
        })
        .collect();
    let _ = delta
        .listen_interfaces(&endpoints)
        .map_err(|e| rb_err("listen_interfaces", &e))?;
    Ok(())
}

/// A hostname (or address) and port.
#[derive(Debug, Clone, PartialEq, SimpleObject, InputObject)]
#[graphql(input_name = "HostPortInput")]
pub struct HostPort {
    /// A hostname, IPv4 address, or bracketed IPv6 address.
    pub hostname: String,
    /// TCP or UDP port (1..=65535).
    pub port: u16,
}

pub(super) fn read_dht_bootstrap_nodes(
    pack: &SettingsPack,
) -> Result<Vec<HostPort>, SettingsError> {
    let Some(list) = pack.get_dht_bootstrap_nodes_parsed() else {
        return Err(invalid(
            "dhtBootstrapNodes: cannot parse the effective value",
        ));
    };
    Ok(list
        .into_iter()
        .map(|n| HostPort {
            hostname: n.host,
            port: n.port.get(),
        })
        .collect())
}

pub(super) fn write_dht_bootstrap_nodes(
    delta: &mut SettingsPack,
    value: MaybeUndefined<Vec<HostPort>>,
) -> Result<(), SettingsError> {
    let Some(list) = defined(value, "dht_bootstrap_nodes")? else {
        return Ok(());
    };
    let nodes = list
        .into_iter()
        .map(|n| {
            Ok(lt::HostPort {
                host: n.hostname,
                port: port_for("dhtBootstrapNodes.port", n.port)?,
            })
        })
        .collect::<Result<Vec<_>, SettingsError>>()?;
    let _ = delta
        .dht_bootstrap_nodes(&nodes)
        .map_err(|e| rb_err("dht_bootstrap_nodes", &e))?;
    Ok(())
}

// ---- the table ------------------------------------------------------------

/// `(public field, backing libtorrent settings)` per structured setting.
/// Field names match the [`super::Settings`] field identifiers.
pub(super) static STRUCTURED_BACKING: &[(&str, &[&str])] = &[
    ("user_agent", &["user_agent"]),
    ("proxy", PROXY_BACKING),
    ("i2p", I2P_BACKING),
    ("encryption", ENCRYPTION_BACKING),
    ("peer_transports", PEER_TRANSPORTS_BACKING),
    (
        "outgoing_port_range",
        &["outgoing_port", "num_outgoing_ports"],
    ),
    (
        "disk_io_cache",
        &["disk_io_read_mode", "disk_io_write_mode"],
    ),
    ("mmap_write_mode", &["disk_write_mode"]),
    ("suggest_mode", &["suggest_mode"]),
    ("choking_algorithm", &["choking_algorithm"]),
    ("seed_choking_algorithm", &["seed_choking_algorithm"]),
    ("mixed_mode_algorithm", &["mixed_mode_algorithm"]),
    ("outgoing_interfaces", &["outgoing_interfaces"]),
    ("listen_interfaces", &["listen_interfaces"]),
    ("dht_bootstrap_nodes", &["dht_bootstrap_nodes"]),
];
