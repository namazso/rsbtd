// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Proxy, protocol encryption, anonymity, i2p, web seeds, alerts,
//! the session tick and the resolver.

use libctorrent_sys as sys;

use super::enums::{EncLevel, EncPolicy, ProxyType};
use super::error::in_range;
use super::{SettingKey, SettingsError, SettingsPack};

impl SettingsPack {
    /// A configured proxy overrides `listen_interfaces` with a single
    /// interface just to reach the proxy: no incoming TCP, no port
    /// mapping, no local service discovery, and incompatible with SSL
    /// torrents.
    #[inline]
    pub(crate) fn proxy_hostname(&mut self, value: &str) -> &mut Self {
        self.set_str(
            SettingKey::from_generated(sys::CT_SET_PROXY_HOSTNAME),
            value,
        );
        self
    }

    /// Reads `proxy_hostname` if set in this pack.
    #[inline]
    pub fn get_proxy_hostname(&self) -> Option<String> {
        self.get_str(SettingKey::from_generated(sys::CT_SET_PROXY_HOSTNAME))
    }

    /// Credentials (if any) for connecting to the proxy.
    #[inline]
    pub(crate) fn proxy_username(&mut self, value: &str) -> &mut Self {
        self.set_str(
            SettingKey::from_generated(sys::CT_SET_PROXY_USERNAME),
            value,
        );
        self
    }

    /// Reads `proxy_username` if set in this pack.
    #[inline]
    pub fn get_proxy_username(&self) -> Option<String> {
        self.get_str(SettingKey::from_generated(sys::CT_SET_PROXY_USERNAME))
    }

    #[inline]
    pub(crate) fn proxy_password(&mut self, value: &str) -> &mut Self {
        self.set_str(
            SettingKey::from_generated(sys::CT_SET_PROXY_PASSWORD),
            value,
        );
        self
    }

    /// Reads `proxy_password` if set in this pack.
    #[inline]
    pub fn get_proxy_password(&self) -> Option<String> {
        self.get_str(SettingKey::from_generated(sys::CT_SET_PROXY_PASSWORD))
    }

    /// Hostname of the i2p SAM bridge (port in `i2p_port`). Unset
    /// means i2p torrents are unsupported. Independent of the proxy
    /// settings.
    #[inline]
    pub(crate) fn i2p_hostname(&mut self, value: &str) -> &mut Self {
        self.set_str(SettingKey::from_generated(sys::CT_SET_I2P_HOSTNAME), value);
        self
    }

    /// Reads `i2p_hostname` if set in this pack.
    #[inline]
    pub fn get_i2p_hostname(&self) -> Option<String> {
        self.get_str(SettingKey::from_generated(sys::CT_SET_I2P_HOSTNAME))
    }

    /// Lets i2p torrents also get peers from non-tracker sources and
    /// connect to regular IPs, forgoing i2p's anonymization.
    #[inline]
    pub(crate) fn allow_i2p_mixed(&mut self, value: bool) -> &mut Self {
        self.set_bool(
            SettingKey::from_generated(sys::CT_SET_ALLOW_I2P_MIXED),
            value,
        );
        self
    }

    /// Reads `allow_i2p_mixed` if set in this pack.
    #[inline]
    pub fn get_allow_i2p_mixed(&self) -> Option<bool> {
        self.get_bool(SettingKey::from_generated(sys::CT_SET_ALLOW_I2P_MIXED))
    }

    /// Partially hides the client's identity: generic tracker
    /// user-agent (except private torrents), local IPs and
    /// `announce_ip` not sent to trackers, no client version in the
    /// extension handshake.
    #[inline]
    pub fn anonymous_mode(&mut self, value: bool) -> &mut Self {
        self.set_bool(
            SettingKey::from_generated(sys::CT_SET_ANONYMOUS_MODE),
            value,
        );
        self
    }

    /// Reads `anonymous_mode` if set in this pack.
    #[inline]
    pub fn get_anonymous_mode(&self) -> Option<bool> {
        self.get_bool(SettingKey::from_generated(sys::CT_SET_ANONYMOUS_MODE))
    }

    /// Whether web seed downloads are reported to the tracker and
    /// counted in stats and download-rate reporting.
    #[inline]
    pub fn report_web_seed_downloads(&mut self, value: bool) -> &mut Self {
        self.set_bool(
            SettingKey::from_generated(sys::CT_SET_REPORT_WEB_SEED_DOWNLOADS),
            value,
        );
        self
    }

    /// Reads `report_web_seed_downloads` if set in this pack.
    #[inline]
    pub fn get_report_web_seed_downloads(&self) -> Option<bool> {
        self.get_bool(SettingKey::from_generated(
            sys::CT_SET_REPORT_WEB_SEED_DOWNLOADS,
        ))
    }

    /// Bans web seeds that send bad data.
    #[inline]
    pub fn ban_web_seeds(&mut self, value: bool) -> &mut Self {
        self.set_bool(SettingKey::from_generated(sys::CT_SET_BAN_WEB_SEEDS), value);
        self
    }

    /// Reads `ban_web_seeds` if set in this pack.
    #[inline]
    pub fn get_ban_web_seeds(&self) -> Option<bool> {
        self.get_bool(SettingKey::from_generated(sys::CT_SET_BAN_WEB_SEEDS))
    }

    /// When both encryption methods are allowed and offered, prefer
    /// RC4 over plain text.
    #[inline]
    pub fn prefer_rc4(&mut self, value: bool) -> &mut Self {
        self.set_bool(SettingKey::from_generated(sys::CT_SET_PREFER_RC4), value);
        self
    }

    /// Reads `prefer_rc4` if set in this pack.
    #[inline]
    pub fn get_prefer_rc4(&self) -> Option<bool> {
        self.get_bool(SettingKey::from_generated(sys::CT_SET_PREFER_RC4))
    }

    /// Resolves hostnames through the configured proxy (SOCKS5 and
    /// HTTP only).
    #[inline]
    pub(crate) fn proxy_hostnames(&mut self, value: bool) -> &mut Self {
        self.set_bool(
            SettingKey::from_generated(sys::CT_SET_PROXY_HOSTNAMES),
            value,
        );
        self
    }

    /// Reads `proxy_hostnames` if set in this pack.
    #[inline]
    pub fn get_proxy_hostnames(&self) -> Option<bool> {
        self.get_bool(SettingKey::from_generated(sys::CT_SET_PROXY_HOSTNAMES))
    }

    /// Routes peer connections — anything carrying torrent payload,
    /// including web seeds — through the configured proxy. Tracker and
    /// DHT traffic do not count as peer connections.
    #[inline]
    pub(crate) fn proxy_peer_connections(&mut self, value: bool) -> &mut Self {
        self.set_bool(
            SettingKey::from_generated(sys::CT_SET_PROXY_PEER_CONNECTIONS),
            value,
        );
        self
    }

    /// Reads `proxy_peer_connections` if set in this pack.
    #[inline]
    pub fn get_proxy_peer_connections(&self) -> Option<bool> {
        self.get_bool(SettingKey::from_generated(
            sys::CT_SET_PROXY_PEER_CONNECTIONS,
        ))
    }

    /// Routes tracker connections through the configured proxy.
    #[inline]
    pub(crate) fn proxy_tracker_connections(&mut self, value: bool) -> &mut Self {
        self.set_bool(
            SettingKey::from_generated(sys::CT_SET_PROXY_TRACKER_CONNECTIONS),
            value,
        );
        self
    }

    /// Reads `proxy_tracker_connections` if set in this pack.
    #[inline]
    pub fn get_proxy_tracker_connections(&self) -> Option<bool> {
        self.get_bool(SettingKey::from_generated(
            sys::CT_SET_PROXY_TRACKER_CONNECTIONS,
        ))
    }

    /// Includes the local UDP socket's IP and port in the SOCKS5 UDP
    /// ASSOCIATE command, letting the proxy forward incoming packets
    /// before any outgoing ones; breaks when a NAT sits between client
    /// and proxy.
    #[inline]
    pub(crate) fn socks5_udp_send_local_ep(&mut self, value: bool) -> &mut Self {
        self.set_bool(
            SettingKey::from_generated(sys::CT_SET_SOCKS5_UDP_SEND_LOCAL_EP),
            value,
        );
        self
    }

    /// Reads `socks5_udp_send_local_ep` if set in this pack.
    #[inline]
    pub fn get_socks5_udp_send_local_ep(&self) -> Option<bool> {
        self.get_bool(SettingKey::from_generated(
            sys::CT_SET_SOCKS5_UDP_SEND_LOCAL_EP,
        ))
    }

    /// Sends the hostname instead of the resolved IP in HTTP CONNECT
    /// requests, e.g. for man-in-the-middle proxies.
    #[inline]
    pub(crate) fn proxy_send_host_in_connect(&mut self, value: bool) -> &mut Self {
        self.set_bool(
            SettingKey::from_generated(sys::CT_SET_PROXY_SEND_HOST_IN_CONNECT),
            value,
        );
        self
    }

    /// Reads `proxy_send_host_in_connect` if set in this pack.
    #[inline]
    pub fn get_proxy_send_host_in_connect(&self) -> Option<bool> {
        self.get_bool(SettingKey::from_generated(
            sys::CT_SET_PROXY_SEND_HOST_IN_CONNECT,
        ))
    }

    /// The `peer_timeout` used for url seeds; usually lower, since web
    /// servers are expected to be more reliable.
    #[inline]
    pub fn urlseed_timeout(&mut self, value: i32) -> &mut Self {
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_URLSEED_TIMEOUT),
            value,
        );
        self
    }

    /// Reads `urlseed_timeout` if set in this pack.
    #[inline]
    pub fn get_urlseed_timeout(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(sys::CT_SET_URLSEED_TIMEOUT))
    }

    /// Seconds before retrying a url seed that sent no valid
    /// `retry-after` header.
    #[inline]
    pub fn urlseed_wait_retry(&mut self, value: i32) -> &mut Self {
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_URLSEED_WAIT_RETRY),
            value,
        );
        self
    }

    /// Reads `urlseed_wait_retry` if set in this pack.
    #[inline]
    pub fn get_urlseed_wait_retry(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(sys::CT_SET_URLSEED_WAIT_RETRY))
    }

    /// Milliseconds between internal session ticks — the resolution of
    /// bandwidth quota distribution to peers. Lower (around 100) is
    /// finer-grained; higher saves CPU.
    ///
    /// Accepts `1..=1000`. (libtorrent's negative test-only fast path
    /// races into an integer division by zero, so it is not exposed.)
    #[inline]
    pub fn tick_interval(&mut self, value: i32) -> Result<&mut Self, SettingsError> {
        in_range("tick_interval", value, 1..=1000)?;
        self.set_int(SettingKey::from_generated(sys::CT_SET_TICK_INTERVAL), value);
        Ok(self)
    }

    /// Reads `tick_interval` if set in this pack.
    #[inline]
    pub fn get_tick_interval(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(sys::CT_SET_TICK_INTERVAL))
    }

    /// Max alerts queued internally; once full, further alerts are
    /// dropped until the client drains the queue.
    ///
    /// Accepts `1..`. At 0 (or below) libtorrent's queue check
    /// `size / (1 + priority) >= limit` holds for every alert, so all of
    /// them are dropped silently — including the alerts-dropped
    /// notification itself.
    #[inline]
    pub fn alert_queue_size(&mut self, value: i32) -> Result<&mut Self, SettingsError> {
        in_range("alert_queue_size", value, 1..=i32::MAX)?;
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_ALERT_QUEUE_SIZE),
            value,
        );
        Ok(self)
    }

    /// Reads `alert_queue_size` if set in this pack.
    #[inline]
    pub fn get_alert_queue_size(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(sys::CT_SET_ALERT_QUEUE_SIZE))
    }

    /// Bitmask of alert_category_t flags selecting which alerts to
    /// receive.
    #[inline]
    pub fn alert_mask(&mut self, value: i32) -> &mut Self {
        self.set_int(SettingKey::from_generated(sys::CT_SET_ALERT_MASK), value);
        self
    }

    /// Reads `alert_mask` if set in this pack.
    #[inline]
    pub fn get_alert_mask(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(sys::CT_SET_ALERT_MASK))
    }

    /// Protocol-encryption policy for outgoing and incoming
    /// connections respectively (see enc_policy). Encryption costs
    /// extra CPU, per-peer buffer copies, and handshake round-trips.
    #[inline]
    pub fn out_enc_policy(&mut self, value: EncPolicy) -> &mut Self {
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_OUT_ENC_POLICY),
            value as i32,
        );
        self
    }

    /// `None` if unset or set to a value these bindings don't know.
    #[inline]
    pub fn get_out_enc_policy(&self) -> Option<EncPolicy> {
        self.get_int(SettingKey::from_generated(sys::CT_SET_OUT_ENC_POLICY))
            .and_then(EncPolicy::from_raw)
    }

    #[inline]
    pub fn in_enc_policy(&mut self, value: EncPolicy) -> &mut Self {
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_IN_ENC_POLICY),
            value as i32,
        );
        self
    }

    /// `None` if unset or set to a value these bindings don't know.
    #[inline]
    pub fn get_in_enc_policy(&self) -> Option<EncPolicy> {
        self.get_int(SettingKey::from_generated(sys::CT_SET_IN_ENC_POLICY))
            .and_then(EncPolicy::from_raw)
    }

    /// Encryption level offered to and selected from peers (see
    /// enc_level).
    #[inline]
    pub fn allowed_enc_level(&mut self, value: EncLevel) -> &mut Self {
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_ALLOWED_ENC_LEVEL),
            value as i32,
        );
        self
    }

    /// `None` if unset or set to a value these bindings don't know.
    #[inline]
    pub fn get_allowed_enc_level(&self) -> Option<EncLevel> {
        self.get_int(SettingKey::from_generated(sys::CT_SET_ALLOWED_ENC_LEVEL))
            .and_then(EncLevel::from_raw)
    }

    #[inline]
    pub(crate) fn proxy_type(&mut self, value: ProxyType) -> &mut Self {
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_PROXY_TYPE),
            value as i32,
        );
        self
    }

    /// `None` if unset or set to a value these bindings don't know.
    #[inline]
    pub fn get_proxy_type(&self) -> Option<ProxyType> {
        self.get_int(SettingKey::from_generated(sys::CT_SET_PROXY_TYPE))
            .and_then(ProxyType::from_raw)
    }

    #[inline]
    pub(crate) fn proxy_port(&mut self, value: i32) -> &mut Self {
        self.set_int(SettingKey::from_generated(sys::CT_SET_PROXY_PORT), value);
        self
    }

    /// Reads `proxy_port` if set in this pack.
    #[inline]
    pub fn get_proxy_port(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(sys::CT_SET_PROXY_PORT))
    }

    /// Port of the i2p SAM bridge; see `i2p_hostname`.
    #[inline]
    pub(crate) fn i2p_port(&mut self, value: i32) -> &mut Self {
        self.set_int(SettingKey::from_generated(sys::CT_SET_I2P_PORT), value);
        self
    }

    /// Reads `i2p_port` if set in this pack.
    #[inline]
    pub fn get_i2p_port(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(sys::CT_SET_I2P_PORT))
    }

    /// Largest sequential web seed request in bytes (values below the
    /// piece size are ignored). Higher values mean fewer, larger HTTP
    /// requests but less request parallelism.
    #[inline]
    pub fn urlseed_max_request_bytes(&mut self, value: i32) -> &mut Self {
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_URLSEED_MAX_REQUEST_BYTES),
            value,
        );
        self
    }

    /// Reads `urlseed_max_request_bytes` if set in this pack.
    #[inline]
    pub fn get_urlseed_max_request_bytes(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(
            sys::CT_SET_URLSEED_MAX_REQUEST_BYTES,
        ))
    }

    /// Seconds to wait before retrying a web seed name lookup.
    #[inline]
    pub fn web_seed_name_lookup_retry(&mut self, value: i32) -> &mut Self {
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_WEB_SEED_NAME_LOOKUP_RETRY),
            value,
        );
        self
    }

    /// Reads `web_seed_name_lookup_retry` if set in this pack.
    #[inline]
    pub fn get_web_seed_name_lookup_retry(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(
            sys::CT_SET_WEB_SEED_NAME_LOOKUP_RETRY,
        ))
    }

    /// Max web seeds connected per torrent at any given time.
    #[inline]
    pub fn max_web_seed_connections(&mut self, value: i32) -> &mut Self {
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_MAX_WEB_SEED_CONNECTIONS),
            value,
        );
        self
    }

    /// Reads `max_web_seed_connections` if set in this pack.
    #[inline]
    pub fn get_max_web_seed_connections(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(
            sys::CT_SET_MAX_WEB_SEED_CONNECTIONS,
        ))
    }

    /// Seconds a resolved host name stays cached (negative means
    /// zero); failed lookups are cached for 1/8th of this.
    #[inline]
    pub fn resolver_cache_timeout(&mut self, value: i32) -> &mut Self {
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_RESOLVER_CACHE_TIMEOUT),
            value,
        );
        self
    }

    /// Reads `resolver_cache_timeout` if set in this pack.
    #[inline]
    pub fn get_resolver_cache_timeout(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(
            sys::CT_SET_RESOLVER_CACHE_TIMEOUT,
        ))
    }

    /// SAM session tunnel parameters: quantity of inbound/outbound
    /// tunnels (1..16) and hops per tunnel (0..7). Take effect on the
    /// next SAM reconnect, not immediately.
    #[inline]
    pub(crate) fn i2p_inbound_quantity(&mut self, value: i32) -> &mut Self {
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_I2P_INBOUND_QUANTITY),
            value,
        );
        self
    }

    /// Reads `i2p_inbound_quantity` if set in this pack.
    #[inline]
    pub fn get_i2p_inbound_quantity(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(sys::CT_SET_I2P_INBOUND_QUANTITY))
    }

    #[inline]
    pub(crate) fn i2p_outbound_quantity(&mut self, value: i32) -> &mut Self {
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_I2P_OUTBOUND_QUANTITY),
            value,
        );
        self
    }

    /// Reads `i2p_outbound_quantity` if set in this pack.
    #[inline]
    pub fn get_i2p_outbound_quantity(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(
            sys::CT_SET_I2P_OUTBOUND_QUANTITY,
        ))
    }

    #[inline]
    pub(crate) fn i2p_inbound_length(&mut self, value: i32) -> &mut Self {
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_I2P_INBOUND_LENGTH),
            value,
        );
        self
    }

    /// Reads `i2p_inbound_length` if set in this pack.
    #[inline]
    pub fn get_i2p_inbound_length(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(sys::CT_SET_I2P_INBOUND_LENGTH))
    }

    #[inline]
    pub(crate) fn i2p_outbound_length(&mut self, value: i32) -> &mut Self {
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_I2P_OUTBOUND_LENGTH),
            value,
        );
        self
    }

    /// Reads `i2p_outbound_length` if set in this pack.
    #[inline]
    pub fn get_i2p_outbound_length(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(sys::CT_SET_I2P_OUTBOUND_LENGTH))
    }

    /// Variance for I2P inbound/outbound tunnel lengths (-7..7).
    #[inline]
    pub(crate) fn i2p_inbound_length_variance(&mut self, value: i32) -> &mut Self {
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_I2P_INBOUND_LENGTH_VARIANCE),
            value,
        );
        self
    }

    /// Reads `i2p_inbound_length_variance` if set in this pack.
    #[inline]
    pub fn get_i2p_inbound_length_variance(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(
            sys::CT_SET_I2P_INBOUND_LENGTH_VARIANCE,
        ))
    }

    #[inline]
    pub(crate) fn i2p_outbound_length_variance(&mut self, value: i32) -> &mut Self {
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_I2P_OUTBOUND_LENGTH_VARIANCE),
            value,
        );
        self
    }

    /// Reads `i2p_outbound_length_variance` if set in this pack.
    #[inline]
    pub fn get_i2p_outbound_length_variance(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(
            sys::CT_SET_I2P_OUTBOUND_LENGTH_VARIANCE,
        ))
    }
}

/// Username/password credentials for an authenticated proxy. Both parts
/// must be non-empty; SOCKS5 additionally caps each at 255 bytes (the
/// wire format's length field).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Credentials {
    pub username: String,
    pub password: String,
}

/// The protocol spoken to the proxy server. Capabilities a protocol
/// does not have (remote DNS on SOCKS4, UDP ASSOCIATE outside SOCKS5,
/// CONNECT hostnames outside HTTP) are not representable.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProxyProtocol {
    /// SOCKS4. The username is sent as the userid and must be
    /// non-empty; SOCKS4 cannot resolve hostnames remotely.
    Socks4 { username: String },
    /// SOCKS5, optionally with RFC 1929 username/password
    /// authentication.
    Socks5 {
        auth: Option<Credentials>,
        /// Resolve hostnames through the proxy instead of locally,
        /// hiding DNS lookups from the local network.
        resolve_hostnames: bool,
        /// Send the actual local port in the UDP ASSOCIATE command
        /// instead of 0, for proxies that require it.
        udp_send_local_endpoint: bool,
    },
    /// An HTTP proxy supporting CONNECT, optionally with basic
    /// authentication.
    Http {
        auth: Option<Credentials>,
        /// Resolve hostnames through the proxy instead of locally.
        resolve_hostnames: bool,
        /// Send the hostname instead of the resolved IP in CONNECT
        /// requests.
        send_hostname_in_connect: bool,
    },
}

/// A proxy for outgoing connections: the whole `proxy_*`/`socks5_*`
/// settings group, written atomically by [`SettingsPack::proxy`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProxyConfig {
    pub protocol: ProxyProtocol,
    /// Hostname or address of the proxy server.
    pub host: String,
    /// Port of the proxy server (port 0 could never be connected to,
    /// so it is not representable).
    pub port: std::num::NonZeroU16,
    /// Route peer connections through the proxy.
    pub peer_connections: bool,
    /// Route tracker announces and scrapes through the proxy.
    pub tracker_connections: bool,
}

/// The `proxy_*`/`socks5_*` settings owned by [`SettingsPack::proxy`].
pub(crate) const PROXY_KEYS: [SettingKey; 10] = [
    SettingKey::from_generated(sys::CT_SET_PROXY_TYPE),
    SettingKey::from_generated(sys::CT_SET_PROXY_HOSTNAME),
    SettingKey::from_generated(sys::CT_SET_PROXY_PORT),
    SettingKey::from_generated(sys::CT_SET_PROXY_USERNAME),
    SettingKey::from_generated(sys::CT_SET_PROXY_PASSWORD),
    SettingKey::from_generated(sys::CT_SET_PROXY_HOSTNAMES),
    SettingKey::from_generated(sys::CT_SET_PROXY_PEER_CONNECTIONS),
    SettingKey::from_generated(sys::CT_SET_PROXY_TRACKER_CONNECTIONS),
    SettingKey::from_generated(sys::CT_SET_SOCKS5_UDP_SEND_LOCAL_EP),
    SettingKey::from_generated(sys::CT_SET_PROXY_SEND_HOST_IN_CONNECT),
];

/// One direction of I2P tunnel configuration.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct I2pTunnels {
    /// Number of parallel tunnels; the SAM protocol accepts 1..=16.
    pub quantity: u8,
    /// Hops per tunnel (0..=7). More hops, more anonymity, more
    /// latency.
    pub length: u8,
    /// Random variance applied to `length` (-7..=7).
    pub variance: i8,
}

/// The I2P SAM bridge configuration: the whole `i2p_*` settings group,
/// written atomically by [`SettingsPack::i2p`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct I2pConfig {
    /// Hostname or address of the SAM bridge. An IPv6 literal goes in
    /// bare (unbracketed).
    pub sam_host: String,
    /// Port of the SAM bridge.
    pub sam_port: std::num::NonZeroU16,
    /// Inbound tunnel configuration.
    pub inbound: I2pTunnels,
    /// Outbound tunnel configuration.
    pub outbound: I2pTunnels,
    /// Allow mixing clearnet peers into i2p swarms (deanonymizing).
    pub allow_mixed: bool,
}

/// The `i2p_*` settings owned by [`SettingsPack::i2p`].
pub(crate) const I2P_KEYS: [SettingKey; 9] = [
    SettingKey::from_generated(sys::CT_SET_I2P_HOSTNAME),
    SettingKey::from_generated(sys::CT_SET_I2P_PORT),
    SettingKey::from_generated(sys::CT_SET_ALLOW_I2P_MIXED),
    SettingKey::from_generated(sys::CT_SET_I2P_INBOUND_QUANTITY),
    SettingKey::from_generated(sys::CT_SET_I2P_OUTBOUND_QUANTITY),
    SettingKey::from_generated(sys::CT_SET_I2P_INBOUND_LENGTH),
    SettingKey::from_generated(sys::CT_SET_I2P_OUTBOUND_LENGTH),
    SettingKey::from_generated(sys::CT_SET_I2P_INBOUND_LENGTH_VARIANCE),
    SettingKey::from_generated(sys::CT_SET_I2P_OUTBOUND_LENGTH_VARIANCE),
];

/// Validates one auth slot: both parts non-empty, each at most
/// `max_len` bytes.
fn credentials(auth: &Option<Credentials>, max_len: usize) -> Result<(&str, &str), SettingsError> {
    let Some(c) = auth else { return Ok(("", "")) };
    if c.username.is_empty() {
        return Err(SettingsError::new(
            "proxy",
            "auth username must not be empty",
        ));
    }
    if c.password.is_empty() {
        return Err(SettingsError::new(
            "proxy",
            "auth password must not be empty",
        ));
    }
    if c.username.len() > max_len || c.password.len() > max_len {
        return Err(SettingsError::new(
            "proxy",
            format!("SOCKS5 credentials are limited to {max_len} bytes each"),
        ));
    }
    Ok((&c.username, &c.password))
}

impl SettingsPack {
    /// Configures (or, with `None`, removes) the proxy for outgoing
    /// connections, staging the whole `proxy_*`/`socks5_*` settings
    /// group. `None` resets every setting of the group to its default.
    ///
    /// The host may be a hostname, an IPv4 address, or an IPv6 address
    /// (bare or bracketed); it is stored bare, because libtorrent's
    /// resolver parses only bare literals — a stored `[...]` would fall
    /// through to a DNS lookup of the bracketed string and never
    /// connect.
    pub fn proxy(&mut self, config: Option<&ProxyConfig>) -> Result<&mut Self, SettingsError> {
        let Some(c) = config else {
            for key in PROXY_KEYS {
                self.set_default(key);
            }
            return Ok(self);
        };
        let host = match c.host.strip_prefix('[') {
            Some(rest) => rest.strip_suffix(']').ok_or_else(|| {
                SettingsError::new(
                    "proxy",
                    format!("host {:?} has unbalanced brackets", c.host),
                )
            })?,
            None => c.host.as_str(),
        };
        if host.contains(['[', ']']) {
            return Err(SettingsError::new(
                "proxy",
                format!("host {:?} contains stray brackets", c.host),
            ));
        }
        super::tokens::bare_host_token("proxy", "host", host)?;
        let (ty, username, password, resolve, udp_local, host_in_connect) = match &c.protocol {
            ProxyProtocol::Socks4 { username } => {
                if username.is_empty() {
                    return Err(SettingsError::new(
                        "proxy",
                        "SOCKS4 sends the username as its userid; it must not be empty",
                    ));
                }
                (
                    ProxyType::Socks4,
                    username.as_str(),
                    "",
                    false,
                    false,
                    false,
                )
            }
            ProxyProtocol::Socks5 {
                auth,
                resolve_hostnames,
                udp_send_local_endpoint,
            } => {
                let (u, p) = credentials(auth, 255)?;
                let ty = if auth.is_some() {
                    ProxyType::Socks5Pw
                } else {
                    ProxyType::Socks5
                };
                (
                    ty,
                    u,
                    p,
                    *resolve_hostnames,
                    *udp_send_local_endpoint,
                    false,
                )
            }
            ProxyProtocol::Http {
                auth,
                resolve_hostnames,
                send_hostname_in_connect,
            } => {
                let (u, p) = credentials(auth, usize::MAX)?;
                let ty = if auth.is_some() {
                    ProxyType::HttpPw
                } else {
                    ProxyType::Http
                };
                (
                    ty,
                    u,
                    p,
                    *resolve_hostnames,
                    false,
                    *send_hostname_in_connect,
                )
            }
        };
        self.proxy_type(ty)
            .proxy_hostname(host)
            .proxy_port(i32::from(c.port.get()))
            .proxy_username(username)
            .proxy_password(password)
            .proxy_hostnames(resolve)
            .proxy_peer_connections(c.peer_connections)
            .proxy_tracker_connections(c.tracker_connections)
            .socks5_udp_send_local_ep(udp_local)
            .proxy_send_host_in_connect(host_in_connect);
        Ok(self)
    }

    /// The outer `None` means the group is absent from this pack or
    /// the raw values do not fit the model (an out-of-enum
    /// `proxy_type`, or a port outside `1..=65535`); `Some(None)`
    /// means "no proxy". Write-side rules (non-empty credentials, the
    /// SOCKS4 userid) are not re-enforced on read.
    pub fn get_proxy(&self) -> Option<Option<ProxyConfig>> {
        let ty = self.get_proxy_type()?;
        if ty == ProxyType::None {
            return Some(None);
        }
        let port = std::num::NonZeroU16::new(u16::try_from(self.get_proxy_port()?).ok()?)?;
        let auth = Some(Credentials {
            username: self.get_proxy_username()?,
            password: self.get_proxy_password()?,
        });
        let resolve_hostnames = self.get_proxy_hostnames()?;
        let protocol = match ty {
            ProxyType::None => unreachable!("handled above"),
            ProxyType::Socks4 => ProxyProtocol::Socks4 {
                username: self.get_proxy_username()?,
            },
            ProxyType::Socks5 | ProxyType::Socks5Pw => ProxyProtocol::Socks5 {
                auth: if ty == ProxyType::Socks5Pw {
                    auth
                } else {
                    None
                },
                resolve_hostnames,
                udp_send_local_endpoint: self.get_socks5_udp_send_local_ep()?,
            },
            ProxyType::Http | ProxyType::HttpPw => ProxyProtocol::Http {
                auth: if ty == ProxyType::HttpPw { auth } else { None },
                resolve_hostnames,
                send_hostname_in_connect: self.get_proxy_send_host_in_connect()?,
            },
        };
        Some(Some(ProxyConfig {
            protocol,
            host: self.get_proxy_hostname()?,
            port,
            peer_connections: self.get_proxy_peer_connections()?,
            tracker_connections: self.get_proxy_tracker_connections()?,
        }))
    }

    /// Configures (or, with `None`, disables) the I2P SAM bridge,
    /// staging the whole `i2p_*` settings group. `None` resets every
    /// setting of the group to its default (an empty SAM host disables
    /// i2p).
    ///
    /// Tunnel parameters outside what the SAM protocol accepts
    /// (quantity `1..=16`, length `0..=7`, variance `-7..=7`) are
    /// rejected: the I2P router would refuse the session, silently
    /// disabling i2p transport.
    pub fn i2p(&mut self, config: Option<&I2pConfig>) -> Result<&mut Self, SettingsError> {
        let Some(c) = config else {
            for key in I2P_KEYS {
                self.set_default(key);
            }
            return Ok(self);
        };
        super::tokens::bare_host_token("i2p", "SAM host", &c.sam_host)?;
        for (dir, t) in [("inbound", &c.inbound), ("outbound", &c.outbound)] {
            if !(1..=16).contains(&t.quantity) {
                return Err(SettingsError::new(
                    "i2p",
                    format!(
                        "{dir} quantity {} is outside the valid range 1..=16",
                        t.quantity
                    ),
                ));
            }
            if t.length > 7 {
                return Err(SettingsError::new(
                    "i2p",
                    format!("{dir} length {} is outside the valid range 0..=7", t.length),
                ));
            }
            if !(-7..=7).contains(&t.variance) {
                return Err(SettingsError::new(
                    "i2p",
                    format!(
                        "{dir} variance {} is outside the valid range -7..=7",
                        t.variance
                    ),
                ));
            }
        }
        self.i2p_hostname(&c.sam_host)
            .i2p_port(i32::from(c.sam_port.get()))
            .allow_i2p_mixed(c.allow_mixed)
            .i2p_inbound_quantity(i32::from(c.inbound.quantity))
            .i2p_inbound_length(i32::from(c.inbound.length))
            .i2p_inbound_length_variance(i32::from(c.inbound.variance))
            .i2p_outbound_quantity(i32::from(c.outbound.quantity))
            .i2p_outbound_length(i32::from(c.outbound.length))
            .i2p_outbound_length_variance(i32::from(c.outbound.variance));
        Ok(self)
    }

    /// The outer `None` means the group is absent from this pack or
    /// the raw values do not fit the model; `Some(None)` means "i2p
    /// disabled" (empty SAM host).
    pub fn get_i2p(&self) -> Option<Option<I2pConfig>> {
        let sam_host = self.get_i2p_hostname()?;
        if sam_host.is_empty() {
            return Some(None);
        }
        let tunnels = |quantity: Option<i32>, length: Option<i32>, variance: Option<i32>| {
            Some(I2pTunnels {
                quantity: u8::try_from(quantity?).ok()?,
                length: u8::try_from(length?).ok()?,
                variance: i8::try_from(variance?).ok()?,
            })
        };
        Some(Some(I2pConfig {
            sam_host,
            sam_port: std::num::NonZeroU16::new(u16::try_from(self.get_i2p_port()?).ok()?)?,
            inbound: tunnels(
                self.get_i2p_inbound_quantity(),
                self.get_i2p_inbound_length(),
                self.get_i2p_inbound_length_variance(),
            )?,
            outbound: tunnels(
                self.get_i2p_outbound_quantity(),
                self.get_i2p_outbound_length(),
                self.get_i2p_outbound_length_variance(),
            )?,
            allow_mixed: self.get_allow_i2p_mixed()?,
        }))
    }
}
