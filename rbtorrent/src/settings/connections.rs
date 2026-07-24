// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Peer connections, transport enablement, listen/outgoing
//! interfaces, ports and the peer list.

use libctorrent_sys as sys;

use super::error::in_range;
use super::{SettingKey, SettingsError, SettingsPack};

impl SettingsPack {
    /// The IP addresses and/or interface names outgoing TCP peer
    /// connections are bound to, round-robin. When set, incoming
    /// connections and packets on a local interface or IP *not* in
    /// this list are rejected. Entries carry no port; IPv6 addresses
    /// go in bare (unbracketed), with an optional `%scope`.
    ///
    /// An empty slice restores the default (unrestricted). Each entry
    /// must be a device name, IPv4 address, or bare IPv6 address; a
    /// malformed entry would make every outgoing bind through it fail
    /// and (when it hides a listen address) reject incoming
    /// connections, so it is rejected here instead.
    pub fn outgoing_interfaces<S: AsRef<str>>(
        &mut self,
        entries: &[S],
    ) -> Result<&mut Self, SettingsError> {
        for entry in entries {
            super::tokens::bare_host_token("outgoing_interfaces", "entry", entry.as_ref())?;
        }
        let joined = entries
            .iter()
            .map(|e| e.as_ref())
            .collect::<Vec<_>>()
            .join(",");
        self.set_str(
            SettingKey::from_generated(sys::CT_SET_OUTGOING_INTERFACES),
            &joined,
        );
        Ok(self)
    }

    /// Reads `outgoing_interfaces` if set in this pack.
    #[inline]
    pub fn get_outgoing_interfaces(&self) -> Option<String> {
        self.get_str(SettingKey::from_generated(sys::CT_SET_OUTGOING_INTERFACES))
    }

    /// The endpoints (IP address or device name, plus port) to listen
    /// on for incoming uTP and TCP peer connections, also used for
    /// *outgoing* uTP, UDP tracker and DHT traffic. Port 0 lets the
    /// OS pick; `ssl` accepts only SSL peers; `local` restricts the
    /// endpoint to its local network and hides it from non-local
    /// tracker announces. With no listen endpoints, networking is
    /// disabled: no DHT, no outgoing uTP or tracker connections, no
    /// incoming connections (outgoing TCP still works, subject to
    /// `outgoing_interfaces`).
    ///
    /// Endpoints with a malformed address token are rejected (a
    /// malformed entry would be skipped by libtorrent, silently
    /// removing a listen socket). An empty slice is accepted but
    /// leaves the session with no listen sockets at all — see above.
    pub fn listen_interfaces(
        &mut self,
        endpoints: &[ListenEndpoint],
    ) -> Result<&mut Self, SettingsError> {
        let mut parts = Vec::with_capacity(endpoints.len());
        for e in endpoints {
            super::tokens::host_token("listen_interfaces", "address", &e.addr)?;
            let mut s = format!("{}:{}", e.addr, e.port);
            if e.ssl {
                s.push('s');
            }
            if e.local {
                s.push('l');
            }
            parts.push(s);
        }
        self.set_str(
            SettingKey::from_generated(sys::CT_SET_LISTEN_INTERFACES),
            &parts.join(","),
        );
        Ok(self)
    }

    /// Reads `listen_interfaces` if set in this pack, as the raw string.
    #[inline]
    pub fn get_listen_interfaces(&self) -> Option<String> {
        self.get_str(SettingKey::from_generated(sys::CT_SET_LISTEN_INTERFACES))
    }

    /// `None` if the setting is absent from this pack or any entry
    /// does not parse.
    pub fn get_listen_interfaces_parsed(&self) -> Option<Vec<ListenEndpoint>> {
        let raw = self.get_listen_interfaces()?;
        let mut out = Vec::new();
        for entry in raw.split(',').filter(|e| !e.is_empty()) {
            let (addr, rest) = entry.rsplit_once(':')?;
            let port_str = rest.trim_end_matches(['s', 'l']);
            let flags = &rest[port_str.len()..];
            out.push(ListenEndpoint {
                addr: addr.to_owned(),
                port: port_str.parse().ok()?,
                ssl: flags.contains('s'),
                local: flags.contains('l'),
            });
        }
        Some(out)
    }

    /// The client fingerprint used as the peer id prefix (the remaining
    /// bytes are randomized per torrent and per incoming connection).
    ///
    /// Accepts at most 19 bytes: a fingerprint of 20 bytes or more
    /// becomes the *entire* peer id, making it deterministic — two
    /// clients sharing it refuse each other as self-connections.
    pub fn peer_fingerprint(&mut self, value: &str) -> Result<&mut Self, SettingsError> {
        if value.len() >= 20 {
            return Err(SettingsError::new(
                "peer_fingerprint",
                format!(
                    "{} bytes would make the whole peer id deterministic; use at most 19",
                    value.len()
                ),
            ));
        }
        self.set_str(
            SettingKey::from_generated(sys::CT_SET_PEER_FINGERPRINT),
            value,
        );
        Ok(self)
    }

    /// Reads `peer_fingerprint` if set in this pack.
    #[inline]
    pub fn get_peer_fingerprint(&self) -> Option<String> {
        self.get_str(SettingKey::from_generated(sys::CT_SET_PEER_FINGERPRINT))
    }

    /// Allows multiple peer connections from the same IP address.
    /// Rejecting them (the default) curbs abusive peers; allowing them
    /// makes same-peer detection less reliable in edge cases.
    #[inline]
    pub fn allow_multiple_connections_per_ip(&mut self, value: bool) -> &mut Self {
        self.set_bool(
            SettingKey::from_generated(sys::CT_SET_ALLOW_MULTIPLE_CONNECTIONS_PER_IP),
            value,
        );
        self
    }

    /// Reads `allow_multiple_connections_per_ip` if set in this pack.
    #[inline]
    pub fn get_allow_multiple_connections_per_ip(&self) -> Option<bool> {
        self.get_bool(SettingKey::from_generated(
            sys::CT_SET_ALLOW_MULTIPLE_CONNECTIONS_PER_IP,
        ))
    }

    /// Parole mode: a peer that participated in a piece failing its
    /// hash check may only download whole pieces; failing one bans it,
    /// passing one releases it from parole.
    #[inline]
    pub fn use_parole_mode(&mut self, value: bool) -> &mut Self {
        self.set_bool(
            SettingKey::from_generated(sys::CT_SET_USE_PAROLE_MODE),
            value,
        );
        self
    }

    /// Reads `use_parole_mode` if set in this pack.
    #[inline]
    pub fn get_use_parole_mode(&self) -> Option<bool> {
        self.get_bool(SettingKey::from_generated(sys::CT_SET_USE_PAROLE_MODE))
    }

    /// Closes peer connections that have no utility for either end,
    /// e.g. when both sides have completed their downloads.
    #[inline]
    pub fn close_redundant_connections(&mut self, value: bool) -> &mut Self {
        self.set_bool(
            SettingKey::from_generated(sys::CT_SET_CLOSE_REDUNDANT_CONNECTIONS),
            value,
        );
        self
    }

    /// Reads `close_redundant_connections` if set in this pack.
    #[inline]
    pub fn get_close_redundant_connections(&self) -> Option<bool> {
        self.get_bool(SettingKey::from_generated(
            sys::CT_SET_CLOSE_REDUNDANT_CONNECTIONS,
        ))
    }

    /// Enables incoming/outgoing, TCP and uTP peer connections per
    /// direction and transport. Disabled outgoing transports are not
    /// used; disabled incoming connections are rejected. Applies only
    /// to peer connections, not trackers or other connections.
    #[inline]
    pub fn enable_outgoing_utp(&mut self, value: bool) -> &mut Self {
        self.set_bool(
            SettingKey::from_generated(sys::CT_SET_ENABLE_OUTGOING_UTP),
            value,
        );
        self
    }

    /// Reads `enable_outgoing_utp` if set in this pack.
    #[inline]
    pub fn get_enable_outgoing_utp(&self) -> Option<bool> {
        self.get_bool(SettingKey::from_generated(sys::CT_SET_ENABLE_OUTGOING_UTP))
    }

    #[inline]
    pub fn enable_incoming_utp(&mut self, value: bool) -> &mut Self {
        self.set_bool(
            SettingKey::from_generated(sys::CT_SET_ENABLE_INCOMING_UTP),
            value,
        );
        self
    }

    /// Reads `enable_incoming_utp` if set in this pack.
    #[inline]
    pub fn get_enable_incoming_utp(&self) -> Option<bool> {
        self.get_bool(SettingKey::from_generated(sys::CT_SET_ENABLE_INCOMING_UTP))
    }

    #[inline]
    pub fn enable_outgoing_tcp(&mut self, value: bool) -> &mut Self {
        self.set_bool(
            SettingKey::from_generated(sys::CT_SET_ENABLE_OUTGOING_TCP),
            value,
        );
        self
    }

    /// Reads `enable_outgoing_tcp` if set in this pack.
    #[inline]
    pub fn get_enable_outgoing_tcp(&self) -> Option<bool> {
        self.get_bool(SettingKey::from_generated(sys::CT_SET_ENABLE_OUTGOING_TCP))
    }

    #[inline]
    pub fn enable_incoming_tcp(&mut self, value: bool) -> &mut Self {
        self.set_bool(
            SettingKey::from_generated(sys::CT_SET_ENABLE_INCOMING_TCP),
            value,
        );
        self
    }

    /// Reads `enable_incoming_tcp` if set in this pack.
    #[inline]
    pub fn get_enable_incoming_tcp(&self) -> Option<bool> {
        self.get_bool(SettingKey::from_generated(sys::CT_SET_ENABLE_INCOMING_TCP))
    }

    /// Whether seeding (and finished) torrents attempt outgoing peer
    /// connections; only worth disabling when outgoing connections are
    /// costly and peers are directly reachable anyway.
    #[inline]
    pub fn seeding_outgoing_connections(&mut self, value: bool) -> &mut Self {
        self.set_bool(
            SettingKey::from_generated(sys::CT_SET_SEEDING_OUTGOING_CONNECTIONS),
            value,
        );
        self
    }

    /// Reads `seeding_outgoing_connections` if set in this pack.
    #[inline]
    pub fn get_seeding_outgoing_connections(&self) -> Option<bool> {
        self.get_bool(SettingKey::from_generated(
            sys::CT_SET_SEEDING_OUTGOING_CONNECTIONS,
        ))
    }

    /// Refuses outgoing peer connections to ports below 1024, a
    /// precaution against being used in a DDoS attack.
    #[inline]
    pub fn no_connect_privileged_ports(&mut self, value: bool) -> &mut Self {
        self.set_bool(
            SettingKey::from_generated(sys::CT_SET_NO_CONNECT_PRIVILEGED_PORTS),
            value,
        );
        self
    }

    /// Reads `no_connect_privileged_ports` if set in this pack.
    #[inline]
    pub fn get_no_connect_privileged_ports(&self) -> Option<bool> {
        self.get_bool(SettingKey::from_generated(
            sys::CT_SET_NO_CONNECT_PRIVILEGED_PORTS,
        ))
    }

    /// Spreads connection attempts more evenly over time (possibly
    /// below `connection_speed`) when close to the connection limit,
    /// instead of connecting and timing out in batches.
    #[inline]
    pub fn smooth_connects(&mut self, value: bool) -> &mut Self {
        self.set_bool(
            SettingKey::from_generated(sys::CT_SET_SMOOTH_CONNECTS),
            value,
        );
        self
    }

    /// Reads `smooth_connects` if set in this pack.
    #[inline]
    pub fn get_smooth_connects(&self) -> Option<bool> {
        self.get_bool(SettingKey::from_generated(sys::CT_SET_SMOOTH_CONNECTS))
    }

    /// Falls back to an OS-chosen port (binding to port 0) when the
    /// configured listen port cannot be bound; false makes it fail.
    #[inline]
    pub fn listen_system_port_fallback(&mut self, value: bool) -> &mut Self {
        self.set_bool(
            SettingKey::from_generated(sys::CT_SET_LISTEN_SYSTEM_PORT_FALLBACK),
            value,
        );
        self
    }

    /// Reads `listen_system_port_fallback` if set in this pack.
    #[inline]
    pub fn get_listen_system_port_fallback(&self) -> Option<bool> {
        self.get_bool(SettingKey::from_generated(
            sys::CT_SET_LISTEN_SYSTEM_PORT_FALLBACK,
        ))
    }

    /// Allows multiple connections to peers presenting the same peer
    /// ID; may help with multi-homed peers at some extra network load.
    #[inline]
    pub fn allow_multiple_connections_per_pid(&mut self, value: bool) -> &mut Self {
        self.set_bool(
            SettingKey::from_generated(sys::CT_SET_ALLOW_MULTIPLE_CONNECTIONS_PER_PID),
            value,
        );
        self
    }

    /// Reads `allow_multiple_connections_per_pid` if set in this pack.
    #[inline]
    pub fn get_allow_multiple_connections_per_pid(&self) -> Option<bool> {
        self.get_bool(SettingKey::from_generated(
            sys::CT_SET_ALLOW_MULTIPLE_CONNECTIONS_PER_PID,
        ))
    }

    /// Seconds without any activity before a peer connection is closed
    /// as timed out. The protocol specifies 120; a keep-alive is sent
    /// at half this time.
    ///
    /// Accepts `1..=536_870_911`.
    #[inline]
    pub fn peer_timeout(&mut self, value: i32) -> Result<&mut Self, SettingsError> {
        in_range("peer_timeout", value, 1..=536_870_911)?;
        self.set_int(SettingKey::from_generated(sys::CT_SET_PEER_TIMEOUT), value);
        Ok(self)
    }

    /// Reads `peer_timeout` if set in this pack.
    #[inline]
    pub fn get_peer_timeout(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(sys::CT_SET_PEER_TIMEOUT))
    }

    /// Max failed connection attempts to a peer before giving up on
    /// it. A success resets the counter; re-learning the peer from a
    /// non-DHT source decrements it, allowing another try.
    ///
    /// Accepts `1..=16_383`.
    #[inline]
    pub fn max_failcount(&mut self, value: i32) -> Result<&mut Self, SettingsError> {
        in_range("max_failcount", value, 1..=16_383)?;
        self.set_int(SettingKey::from_generated(sys::CT_SET_MAX_FAILCOUNT), value);
        Ok(self)
    }

    /// Reads `max_failcount` if set in this pack.
    #[inline]
    pub fn get_max_failcount(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(sys::CT_SET_MAX_FAILCOUNT))
    }

    /// Seconds to wait before reconnecting to a peer, multiplied by
    /// its failcount.
    ///
    /// Accepts `0..=65_535`.
    #[inline]
    pub fn min_reconnect_time(&mut self, value: i32) -> Result<&mut Self, SettingsError> {
        in_range("min_reconnect_time", value, 0..=65_535)?;
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_MIN_RECONNECT_TIME),
            value,
        );
        Ok(self)
    }

    /// Reads `min_reconnect_time` if set in this pack.
    #[inline]
    pub fn get_min_reconnect_time(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(sys::CT_SET_MIN_RECONNECT_TIME))
    }

    /// Seconds until an in-progress peer connection attempt is
    /// considered timed out; stale half-open connections can delay
    /// connecting other peers.
    ///
    /// Accepts `1..=2_147_482_623`.
    #[inline]
    pub fn peer_connect_timeout(&mut self, value: i32) -> Result<&mut Self, SettingsError> {
        in_range("peer_connect_timeout", value, 1..=2_147_482_623)?;
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_PEER_CONNECT_TIMEOUT),
            value,
        );
        Ok(self)
    }

    /// Reads `peer_connect_timeout` if set in this pack.
    #[inline]
    pub fn get_peer_connect_timeout(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(sys::CT_SET_PEER_CONNECT_TIMEOUT))
    }

    /// Connection attempts per second. Negative means the default of
    /// 200; 0 disables outgoing connections entirely.
    #[inline]
    pub fn connection_speed(&mut self, value: i32) -> &mut Self {
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_CONNECTION_SPEED),
            value,
        );
        self
    }

    /// Reads `connection_speed` if set in this pack.
    #[inline]
    pub fn get_connection_speed(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(sys::CT_SET_CONNECTION_SPEED))
    }

    /// Seconds a mutually uninteresting and uninterested peer is kept
    /// before being disconnected.
    #[inline]
    pub fn inactivity_timeout(&mut self, value: i32) -> &mut Self {
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_INACTIVITY_TIMEOUT),
            value,
        );
        self
    }

    /// Reads `inactivity_timeout` if set in this pack.
    #[inline]
    pub fn get_inactivity_timeout(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(sys::CT_SET_INACTIVITY_TIMEOUT))
    }

    /// Seconds to wait for a peer's handshake response before
    /// disconnecting it.
    ///
    /// Accepts `1..=536_870_911`.
    #[inline]
    pub fn handshake_timeout(&mut self, value: i32) -> Result<&mut Self, SettingsError> {
        in_range("handshake_timeout", value, 1..=536_870_911)?;
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_HANDSHAKE_TIMEOUT),
            value,
        );
        Ok(self)
    }

    /// Reads `handshake_timeout` if set in this pack.
    #[inline]
    pub fn get_handshake_timeout(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(sys::CT_SET_HANDSHAKE_TIMEOUT))
    }

    /// First port of the outgoing-connection bind range; see
    /// [`SettingsPack::outgoing_ports`].
    #[inline]
    pub(crate) fn outgoing_port(&mut self, value: i32) -> &mut Self {
        self.set_int(SettingKey::from_generated(sys::CT_SET_OUTGOING_PORT), value);
        self
    }

    /// Reads `outgoing_port` if set in this pack.
    #[inline]
    pub fn get_outgoing_port(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(sys::CT_SET_OUTGOING_PORT))
    }

    #[inline]
    pub(crate) fn num_outgoing_ports(&mut self, value: i32) -> &mut Self {
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_NUM_OUTGOING_PORTS),
            value,
        );
        self
    }

    /// Reads `num_outgoing_ports` if set in this pack.
    #[inline]
    pub fn get_num_outgoing_ports(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(sys::CT_SET_NUM_OUTGOING_PORTS))
    }

    /// Binds outgoing peer connections to this port range (`None`
    /// restores the default: OS-selected ephemeral ports), staging
    /// `outgoing_port` and `num_outgoing_ports` together.
    ///
    /// Keeping the pair a `u16` range makes the wrap-around and
    /// signed-overflow hazards of the raw settings unrepresentable. A
    /// range starting at port 0 is rejected: the wrapped port would
    /// silently fall back to ephemeral ports.
    ///
    /// Setting a range limits the ability to keep multiple connections
    /// to the same client and should be rare; a small range causes
    /// reconnect failures from sockets in `TIME_WAIT`.
    pub fn outgoing_ports(
        &mut self,
        range: Option<std::ops::RangeInclusive<u16>>,
    ) -> Result<&mut Self, SettingsError> {
        let Some(range) = range else {
            self.outgoing_port(0).num_outgoing_ports(0);
            return Ok(self);
        };
        let (first, last) = (*range.start(), *range.end());
        if first == 0 {
            return Err(SettingsError::new(
                "outgoing_ports",
                "the range must not start at port 0",
            ));
        }
        if last < first {
            return Err(SettingsError::new(
                "outgoing_ports",
                format!("the range {first}..={last} is empty"),
            ));
        }
        self.outgoing_port(i32::from(first))
            .num_outgoing_ports(i32::from(last) - i32::from(first));
        Ok(self)
    }

    /// The outer `None` means the pair is absent from this pack or
    /// does not fit the model; `Some(None)` means OS-selected
    /// ephemeral ports (the default).
    pub fn get_outgoing_ports(&self) -> Option<Option<std::ops::RangeInclusive<u16>>> {
        let first = self.get_outgoing_port()?;
        let count = self.get_num_outgoing_ports()?;
        if first <= 0 {
            return Some(None);
        }
        let first = u16::try_from(first).ok()?;
        let last = (i64::from(first) + i64::from(count.max(0))).min(65_535);
        let last = u16::try_from(last).expect("clamped to port range");
        Some(Some(first..=last))
    }

    /// DSCP code point (RFC 8622) in the IP header of every packet sent
    /// to peers, including web seeds. 0 means no marking; 1 is Lower
    /// Effort.
    ///
    /// Accepts `0..=63`. libtorrent stores the whole traffic-class byte
    /// (it masks with `0xfc` when applying), so the code point occupies
    /// the top six bits of the stored value.
    #[inline]
    pub fn peer_dscp(&mut self, value: i32) -> Result<&mut Self, SettingsError> {
        in_range("peer_dscp", value, 0..=63)?;
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_PEER_DSCP),
            value << 2,
        );
        Ok(self)
    }

    /// Reads `peer_dscp` (the code point, `0..=63`) if set in this pack.
    #[inline]
    pub fn get_peer_dscp(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(sys::CT_SET_PEER_DSCP))
            .map(|raw| (raw & 0xfc) >> 2)
    }

    /// Max number of known (not necessarily connected) peers kept per
    /// torrent; eviction starts at 90% of the limit. 0 means unlimited.
    ///
    /// Accepts `0..=22_605_091`.
    #[inline]
    pub fn max_peerlist_size(&mut self, value: i32) -> Result<&mut Self, SettingsError> {
        in_range("max_peerlist_size", value, 0..=22_605_091)?;
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_MAX_PEERLIST_SIZE),
            value,
        );
        Ok(self)
    }

    /// Reads `max_peerlist_size` if set in this pack.
    #[inline]
    pub fn get_max_peerlist_size(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(sys::CT_SET_MAX_PEERLIST_SIZE))
    }

    /// The `max_peerlist_size` used for paused torrents, to save
    /// memory.
    ///
    /// Accepts `0..=22_605_091`.
    #[inline]
    pub fn max_paused_peerlist_size(&mut self, value: i32) -> Result<&mut Self, SettingsError> {
        in_range("max_paused_peerlist_size", value, 0..=22_605_091)?;
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_MAX_PAUSED_PEERLIST_SIZE),
            value,
        );
        Ok(self)
    }

    /// Reads `max_paused_peerlist_size` if set in this pack.
    #[inline]
    pub fn get_max_paused_peerlist_size(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(
            sys::CT_SET_MAX_PAUSED_PEERLIST_SIZE,
        ))
    }

    /// Receive/send buffer sizes set on peer sockets, in bytes; 0
    /// keeps the OS default. uTP peers, DHT and UDP tracker traffic
    /// share one UDP socket buffer per listen interface — too small
    /// and packets may be dropped.
    #[inline]
    pub fn recv_socket_buffer_size(&mut self, value: i32) -> &mut Self {
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_RECV_SOCKET_BUFFER_SIZE),
            value,
        );
        self
    }

    /// Reads `recv_socket_buffer_size` if set in this pack.
    #[inline]
    pub fn get_recv_socket_buffer_size(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(
            sys::CT_SET_RECV_SOCKET_BUFFER_SIZE,
        ))
    }

    #[inline]
    pub fn send_socket_buffer_size(&mut self, value: i32) -> &mut Self {
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_SEND_SOCKET_BUFFER_SIZE),
            value,
        );
        self
    }

    /// Reads `send_socket_buffer_size` if set in this pack.
    #[inline]
    pub fn get_send_socket_buffer_size(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(
            sys::CT_SET_SEND_SOCKET_BUFFER_SIZE,
        ))
    }

    /// Best-effort cap on a single peer connection's receive buffer:
    /// growth stops here, but the buffer is always allowed to reach the
    /// current message's size, so one large legal message (a piece
    /// message is 16 KiB of payload plus headers, other messages up to
    /// about 1 MiB) can exceed the cap.
    ///
    /// Accepts `16_384..=i32::MAX`.
    #[inline]
    pub fn max_peer_recv_buffer_size(&mut self, value: i32) -> Result<&mut Self, SettingsError> {
        in_range("max_peer_recv_buffer_size", value, 16_384..=i32::MAX)?;
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_MAX_PEER_RECV_BUFFER_SIZE),
            value,
        );
        Ok(self)
    }

    /// Reads `max_peer_recv_buffer_size` if set in this pack.
    #[inline]
    pub fn get_max_peer_recv_buffer_size(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(
            sys::CT_SET_MAX_PEER_RECV_BUFFER_SIZE,
        ))
    }

    /// Max peers accepted from a single peer's PEX message; entries
    /// beyond this are ignored.
    #[inline]
    pub fn max_pex_peers(&mut self, value: i32) -> &mut Self {
        self.set_int(SettingKey::from_generated(sys::CT_SET_MAX_PEX_PEERS), value);
        self
    }

    /// Reads `max_pex_peers` if set in this pack.
    #[inline]
    pub fn get_max_pex_peers(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(sys::CT_SET_MAX_PEX_PEERS))
    }

    /// Global limit on open peer connections; a hard floor of two per
    /// torrent applies regardless.
    #[inline]
    pub fn connections_limit(&mut self, value: i32) -> &mut Self {
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_CONNECTIONS_LIMIT),
            value,
        );
        self
    }

    /// Reads `connections_limit` if set in this pack.
    #[inline]
    pub fn get_connections_limit(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(sys::CT_SET_CONNECTIONS_LIMIT))
    }

    /// Incoming connections accepted beyond `connections_limit` in
    /// order to potentially replace existing ones.
    #[inline]
    pub fn connections_slack(&mut self, value: i32) -> &mut Self {
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_CONNECTIONS_SLACK),
            value,
        );
        self
    }

    /// Reads `connections_slack` if set in this pack.
    #[inline]
    pub fn get_connections_slack(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(sys::CT_SET_CONNECTIONS_SLACK))
    }

    /// Backlog passed to listen(); worth raising only for servers
    /// expecting many incoming connections. Takes effect when
    /// `listen_interfaces` is next updated.
    #[inline]
    pub fn listen_queue_size(&mut self, value: i32) -> &mut Self {
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_LISTEN_QUEUE_SIZE),
            value,
        );
        self
    }

    /// Reads `listen_queue_size` if set in this pack.
    #[inline]
    pub fn get_listen_queue_size(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(sys::CT_SET_LISTEN_QUEUE_SIZE))
    }

    /// Peers to connect immediately on a torrent's first tracker
    /// response instead of waiting for the once-a-second connect
    /// scheduler; the `u8` argument enforces libtorrent's cap of 255.
    #[inline]
    pub fn torrent_connect_boost(&mut self, value: u8) -> &mut Self {
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_TORRENT_CONNECT_BOOST),
            i32::from(value),
        );
        self
    }

    /// Reads `torrent_connect_boost` if set in this pack.
    #[inline]
    pub fn get_torrent_connect_boost(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(
            sys::CT_SET_TORRENT_CONNECT_BOOST,
        ))
    }

    /// Percent of peers to optimistically disconnect every
    /// `peer_turnover_interval` seconds when connected to more than
    /// `peer_turnover_cutoff` percent of the connection limit.
    ///
    /// Accepts `0..=100`.
    #[inline]
    pub fn peer_turnover(&mut self, value: i32) -> Result<&mut Self, SettingsError> {
        in_range("peer_turnover", value, 0..=100)?;
        self.set_int(SettingKey::from_generated(sys::CT_SET_PEER_TURNOVER), value);
        Ok(self)
    }

    /// Reads `peer_turnover` if set in this pack.
    #[inline]
    pub fn get_peer_turnover(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(sys::CT_SET_PEER_TURNOVER))
    }

    #[inline]
    pub fn peer_turnover_cutoff(&mut self, value: i32) -> &mut Self {
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_PEER_TURNOVER_CUTOFF),
            value,
        );
        self
    }

    /// Reads `peer_turnover_cutoff` if set in this pack.
    #[inline]
    pub fn get_peer_turnover_cutoff(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(sys::CT_SET_PEER_TURNOVER_CUTOFF))
    }

    #[inline]
    pub fn peer_turnover_interval(&mut self, value: i32) -> &mut Self {
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_PEER_TURNOVER_INTERVAL),
            value,
        );
        self
    }

    /// Reads `peer_turnover_interval` if set in this pack.
    #[inline]
    pub fn get_peer_turnover_interval(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(
            sys::CT_SET_PEER_TURNOVER_INTERVAL,
        ))
    }

    /// Downloading torrents are prioritized for the limited connection
    /// attempts; every n:th attempt goes to a seeding/finished torrent
    /// instead. This setting is n.
    #[inline]
    pub fn connect_seed_every_n_download(&mut self, value: i32) -> &mut Self {
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_CONNECT_SEED_EVERY_N_DOWNLOAD),
            value,
        );
        self
    }

    /// Reads `connect_seed_every_n_download` if set in this pack.
    #[inline]
    pub fn get_connect_seed_every_n_download(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(
            sys::CT_SET_CONNECT_SEED_EVERY_N_DOWNLOAD,
        ))
    }

    /// How many times to retry a failed listen-port bind, incrementing
    /// the port by one each time.
    #[inline]
    pub fn max_retry_port_bind(&mut self, value: i32) -> &mut Self {
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_MAX_RETRY_PORT_BIND),
            value,
        );
        self
    }

    /// Reads `max_retry_port_bind` if set in this pack.
    #[inline]
    pub fn get_max_retry_port_bind(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(sys::CT_SET_MAX_RETRY_PORT_BIND))
    }
}

/// One endpoint of the `listen_interfaces` setting, written by
/// [`SettingsPack::listen_interfaces`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListenEndpoint {
    /// An IP address (IPv6 bracketed), device name, or hostname token.
    pub addr: String,
    /// Port 0 asks the OS for an ephemeral port.
    pub port: u16,
    /// Accept SSL/TLS peer connections on this endpoint.
    pub ssl: bool,
    /// Local-network only: assumed unreachable from beyond the local
    /// network and not announced to trackers outside it.
    pub local: bool,
}

impl ListenEndpoint {
    /// A plain (non-SSL, non-local) listen endpoint.
    pub fn new(addr: impl Into<String>, port: u16) -> ListenEndpoint {
        ListenEndpoint {
            addr: addr.into(),
            port,
            ssl: false,
            local: false,
        }
    }
}
