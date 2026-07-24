// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! DHT, local service discovery, UPnP/NAT-PMP and the IP notifier.

use libctorrent_sys as sys;

use super::error::in_range;
use super::{SettingKey, SettingsError, SettingsPack};

impl SettingsPack {
    /// Back-up DHT bootstrap nodes, used when no other nodes are known.
    /// Changes after the DHT has started may not take effect until it is
    /// restarted.
    ///
    /// An empty slice stores an empty list, which overrides libtorrent's
    /// built-in default (`dht.libtorrent.org:25401`) and disables
    /// bootstrapping from well-known nodes; the built-in default cannot
    /// be restored through this setter. Nodes with a malformed hostname
    /// are rejected (libtorrent would fail to resolve them, silently
    /// losing the bootstrap path).
    pub fn dht_bootstrap_nodes(&mut self, nodes: &[HostPort]) -> Result<&mut Self, SettingsError> {
        let mut parts = Vec::with_capacity(nodes.len());
        for node in nodes {
            super::tokens::host_token("dht_bootstrap_nodes", "hostname", &node.host)?;
            parts.push(format!("{}:{}", node.host, node.port));
        }
        self.set_str(
            SettingKey::from_generated(sys::CT_SET_DHT_BOOTSTRAP_NODES),
            &parts.join(","),
        );
        Ok(self)
    }

    /// Reads `dht_bootstrap_nodes` if set in this pack, as the raw string.
    #[inline]
    pub fn get_dht_bootstrap_nodes(&self) -> Option<String> {
        self.get_str(SettingKey::from_generated(sys::CT_SET_DHT_BOOTSTRAP_NODES))
    }

    /// `None` if the setting is absent from this pack or any entry
    /// does not parse.
    pub fn get_dht_bootstrap_nodes_parsed(&self) -> Option<Vec<HostPort>> {
        let raw = self.get_dht_bootstrap_nodes()?;
        let mut out = Vec::new();
        for entry in raw.split(',').filter(|e| !e.is_empty()) {
            let (host, port) = entry.rsplit_once(':')?;
            out.push(HostPort {
                host: host.to_owned(),
                port: port.parse().ok()?,
            });
        }
        Some(out)
    }

    /// Overrides the NAT-PMP gateway address instead of resolving the
    /// default gateway. Only read when NAT-PMP starts; toggle NAT-PMP off
    /// and on to apply a change.
    #[inline]
    pub fn natpmp_gateway(&mut self, value: &str) -> &mut Self {
        self.set_str(
            SettingKey::from_generated(sys::CT_SET_NATPMP_GATEWAY),
            value,
        );
        self
    }

    /// Reads `natpmp_gateway` if set in this pack.
    #[inline]
    pub fn get_natpmp_gateway(&self) -> Option<String> {
        self.get_str(SettingKey::from_generated(sys::CT_SET_NATPMP_GATEWAY))
    }

    /// Use the DHT only for torrents whose trackers have all failed,
    /// instead of unconditionally.
    #[inline]
    pub fn use_dht_as_fallback(&mut self, value: bool) -> &mut Self {
        self.set_bool(
            SettingKey::from_generated(sys::CT_SET_USE_DHT_AS_FALLBACK),
            value,
        );
        self
    }

    /// Reads `use_dht_as_fallback` if set in this pack.
    #[inline]
    pub fn get_use_dht_as_fallback(&self) -> Option<bool> {
        self.get_bool(SettingKey::from_generated(sys::CT_SET_USE_DHT_AS_FALLBACK))
    }

    /// Ignore UPnP broadcast responses from devices outside our subnet
    /// (avoids talking to other people's routers by mistake).
    #[inline]
    pub fn upnp_ignore_nonrouters(&mut self, value: bool) -> &mut Self {
        self.set_bool(
            SettingKey::from_generated(sys::CT_SET_UPNP_IGNORE_NONROUTERS),
            value,
        );
        self
    }

    /// Reads `upnp_ignore_nonrouters` if set in this pack.
    #[inline]
    pub fn get_upnp_ignore_nonrouters(&self) -> Option<bool> {
        self.get_bool(SettingKey::from_generated(
            sys::CT_SET_UPNP_IGNORE_NONROUTERS,
        ))
    }

    /// Starts and stops the UPnP service, which tries to forward the
    /// listen and DHT ports on local UPnP router devices.
    #[inline]
    pub fn enable_upnp(&mut self, value: bool) -> &mut Self {
        self.set_bool(SettingKey::from_generated(sys::CT_SET_ENABLE_UPNP), value);
        self
    }

    /// Reads `enable_upnp` if set in this pack.
    #[inline]
    pub fn get_enable_upnp(&self) -> Option<bool> {
        self.get_bool(SettingKey::from_generated(sys::CT_SET_ENABLE_UPNP))
    }

    /// Starts and stops the NAT-PMP service, which tries to forward the
    /// listen and DHT ports on the router.
    #[inline]
    pub fn enable_natpmp(&mut self, value: bool) -> &mut Self {
        self.set_bool(SettingKey::from_generated(sys::CT_SET_ENABLE_NATPMP), value);
        self
    }

    /// Reads `enable_natpmp` if set in this pack.
    #[inline]
    pub fn get_enable_natpmp(&self) -> Option<bool> {
        self.get_bool(SettingKey::from_generated(sys::CT_SET_ENABLE_NATPMP))
    }

    /// Starts and stops Local Service Discovery, which broadcasts the
    /// info-hashes of non-private torrents to find local-network peers.
    #[inline]
    pub fn enable_lsd(&mut self, value: bool) -> &mut Self {
        self.set_bool(SettingKey::from_generated(sys::CT_SET_ENABLE_LSD), value);
        self
    }

    /// Reads `enable_lsd` if set in this pack.
    #[inline]
    pub fn get_enable_lsd(&self) -> Option<bool> {
        self.get_bool(SettingKey::from_generated(sys::CT_SET_ENABLE_LSD))
    }

    /// Starts and stops the DHT node.
    #[inline]
    pub fn enable_dht(&mut self, value: bool) -> &mut Self {
        self.set_bool(SettingKey::from_generated(sys::CT_SET_ENABLE_DHT), value);
        self
    }

    /// Reads `enable_dht` if set in this pack.
    #[inline]
    pub fn get_enable_dht(&self) -> Option<bool> {
        self.get_bool(SettingKey::from_generated(sys::CT_SET_ENABLE_DHT))
    }

    /// Starts and stops the internal IP/route change notifier. When
    /// disabled, network changes require manually reopening the session's
    /// network sockets.
    #[inline]
    pub fn enable_ip_notifier(&mut self, value: bool) -> &mut Self {
        self.set_bool(
            SettingKey::from_generated(sys::CT_SET_ENABLE_IP_NOTIFIER),
            value,
        );
        self
    }

    /// Reads `enable_ip_notifier` if set in this pack.
    #[inline]
    pub fn get_enable_ip_notifier(&self) -> Option<bool> {
        self.get_bool(SettingKey::from_generated(sys::CT_SET_ENABLE_IP_NOTIFIER))
    }

    /// Prefer routing-table nodes whose IDs are derived from their source
    /// IP per BEP 42.
    #[inline]
    pub fn dht_prefer_verified_node_ids(&mut self, value: bool) -> &mut Self {
        self.set_bool(
            SettingKey::from_generated(sys::CT_SET_DHT_PREFER_VERIFIED_NODE_IDS),
            value,
        );
        self
    }

    /// Reads `dht_prefer_verified_node_ids` if set in this pack.
    #[inline]
    pub fn get_dht_prefer_verified_node_ids(&self) -> Option<bool> {
        self.get_bool(SettingKey::from_generated(
            sys::CT_SET_DHT_PREFER_VERIFIED_NODE_IDS,
        ))
    }

    /// Restrict the routing table to one entry per IP and reject nodes in
    /// the same /24 (/64 for IPv6) within a bucket, mitigating node-ID
    /// spoofing attacks.
    #[inline]
    pub fn dht_restrict_routing_ips(&mut self, value: bool) -> &mut Self {
        self.set_bool(
            SettingKey::from_generated(sys::CT_SET_DHT_RESTRICT_ROUTING_IPS),
            value,
        );
        self
    }

    /// Reads `dht_restrict_routing_ips` if set in this pack.
    #[inline]
    pub fn get_dht_restrict_routing_ips(&self) -> Option<bool> {
        self.get_bool(SettingKey::from_generated(
            sys::CT_SET_DHT_RESTRICT_ROUTING_IPS,
        ))
    }

    /// Prevent DHT searches from adding nodes with very close CIDR
    /// distance, mitigating certain attacks.
    #[inline]
    pub fn dht_restrict_search_ips(&mut self, value: bool) -> &mut Self {
        self.set_bool(
            SettingKey::from_generated(sys::CT_SET_DHT_RESTRICT_SEARCH_IPS),
            value,
        );
        self
    }

    /// Reads `dht_restrict_search_ips` if set in this pack.
    #[inline]
    pub fn get_dht_restrict_search_ips(&self) -> Option<bool> {
        self.get_bool(SettingKey::from_generated(
            sys::CT_SET_DHT_RESTRICT_SEARCH_IPS,
        ))
    }

    /// Enlarge the first DHT routing-table buckets to 128/64/32/16 nodes
    /// instead of the standard 8.
    #[inline]
    pub fn dht_extended_routing_table(&mut self, value: bool) -> &mut Self {
        self.set_bool(
            SettingKey::from_generated(sys::CT_SET_DHT_EXTENDED_ROUTING_TABLE),
            value,
        );
        self
    }

    /// Reads `dht_extended_routing_table` if set in this pack.
    #[inline]
    pub fn get_dht_extended_routing_table(&self) -> Option<bool> {
        self.get_bool(SettingKey::from_generated(
            sys::CT_SET_DHT_EXTENDED_ROUTING_TABLE,
        ))
    }

    /// Keep *branch factor* outstanding requests aimed at the closest
    /// nodes, querying closer nodes as soon as they are learned; faster
    /// lookups at the cost of more outstanding queries.
    #[inline]
    pub fn dht_aggressive_lookups(&mut self, value: bool) -> &mut Self {
        self.set_bool(
            SettingKey::from_generated(sys::CT_SET_DHT_AGGRESSIVE_LOOKUPS),
            value,
        );
        self
    }

    /// Reads `dht_aggressive_lookups` if set in this pack.
    #[inline]
    pub fn get_dht_aggressive_lookups(&self) -> Option<bool> {
        self.get_bool(SettingKey::from_generated(
            sys::CT_SET_DHT_AGGRESSIVE_LOOKUPS,
        ))
    }

    /// when set, perform lookups in a way that is slightly more expensive,
    /// but which minimizes the amount of information leaked about you.
    #[inline]
    pub fn dht_privacy_lookups(&mut self, value: bool) -> &mut Self {
        self.set_bool(
            SettingKey::from_generated(sys::CT_SET_DHT_PRIVACY_LOOKUPS),
            value,
        );
        self
    }

    /// Reads `dht_privacy_lookups` if set in this pack.
    #[inline]
    pub fn get_dht_privacy_lookups(&self) -> Option<bool> {
        self.get_bool(SettingKey::from_generated(sys::CT_SET_DHT_PRIVACY_LOOKUPS))
    }

    /// Ignore nodes whose IDs are not correctly derived from their
    /// external IP, answering their queries with an "invalid node ID"
    /// error.
    #[inline]
    pub fn dht_enforce_node_id(&mut self, value: bool) -> &mut Self {
        self.set_bool(
            SettingKey::from_generated(sys::CT_SET_DHT_ENFORCE_NODE_ID),
            value,
        );
        self
    }

    /// Reads `dht_enforce_node_id` if set in this pack.
    #[inline]
    pub fn get_dht_enforce_node_id(&self) -> Option<bool> {
        self.get_bool(SettingKey::from_generated(sys::CT_SET_DHT_ENFORCE_NODE_ID))
    }

    /// Ignore DHT messages from parts of the internet no traffic is
    /// expected from.
    #[inline]
    pub fn dht_ignore_dark_internet(&mut self, value: bool) -> &mut Self {
        self.set_bool(
            SettingKey::from_generated(sys::CT_SET_DHT_IGNORE_DARK_INTERNET),
            value,
        );
        self
    }

    /// Reads `dht_ignore_dark_internet` if set in this pack.
    #[inline]
    pub fn get_dht_ignore_dark_internet(&self) -> Option<bool> {
        self.get_bool(SettingKey::from_generated(
            sys::CT_SET_DHT_IGNORE_DARK_INTERNET,
        ))
    }

    /// Run the DHT node in read-only mode: it stops answering queries and
    /// marks outgoing queries `ro` so other nodes don't add it to their
    /// routing tables. Meant for low-power or ephemeral devices.
    #[inline]
    pub fn dht_read_only(&mut self, value: bool) -> &mut Self {
        self.set_bool(SettingKey::from_generated(sys::CT_SET_DHT_READ_ONLY), value);
        self
    }

    /// Reads `dht_read_only` if set in this pack.
    #[inline]
    pub fn get_dht_read_only(&self) -> Option<bool> {
        self.get_bool(SettingKey::from_generated(sys::CT_SET_DHT_READ_ONLY))
    }

    /// Seconds between local network (LSD) announces for a torrent.
    ///
    /// Accepts `1..=2_147_483`.
    #[inline]
    pub fn local_service_announce_interval(
        &mut self,
        value: i32,
    ) -> Result<&mut Self, SettingsError> {
        in_range("local_service_announce_interval", value, 1..=2_147_483)?;
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_LOCAL_SERVICE_ANNOUNCE_INTERVAL),
            value,
        );
        Ok(self)
    }

    /// Reads `local_service_announce_interval` if set in this pack.
    #[inline]
    pub fn get_local_service_announce_interval(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(
            sys::CT_SET_LOCAL_SERVICE_ANNOUNCE_INTERVAL,
        ))
    }

    /// Seconds between announcing torrents to the DHT.
    ///
    /// Accepts `1..=2_147_483`.
    #[inline]
    pub fn dht_announce_interval(&mut self, value: i32) -> Result<&mut Self, SettingsError> {
        in_range("dht_announce_interval", value, 1..=2_147_483)?;
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_DHT_ANNOUNCE_INTERVAL),
            value,
        );
        Ok(self)
    }

    /// Reads `dht_announce_interval` if set in this pack.
    #[inline]
    pub fn get_dht_announce_interval(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(
            sys::CT_SET_DHT_ANNOUNCE_INTERVAL,
        ))
    }

    /// Average bytes per second the DHT may send; incoming requests are
    /// dropped while the quota is exhausted.
    #[inline]
    pub fn dht_upload_rate_limit(&mut self, value: i32) -> &mut Self {
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_DHT_UPLOAD_RATE_LIMIT),
            value,
        );
        self
    }

    /// Reads `dht_upload_rate_limit` if set in this pack.
    #[inline]
    pub fn get_dht_upload_rate_limit(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(
            sys::CT_SET_DHT_UPLOAD_RATE_LIMIT,
        ))
    }

    /// UPnP port-mapping lease duration in seconds; 0 means permanent.
    /// Use 0 for routers that mishandle expiration; otherwise keep it at
    /// 5 minutes or more.
    ///
    /// Accepts `0..=715_827_882`.
    #[inline]
    pub fn upnp_lease_duration(&mut self, value: i32) -> Result<&mut Self, SettingsError> {
        in_range("upnp_lease_duration", value, 0..=715_827_882)?;
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_UPNP_LEASE_DURATION),
            value,
        );
        Ok(self)
    }

    /// Reads `upnp_lease_duration` if set in this pack.
    #[inline]
    pub fn get_upnp_lease_duration(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(sys::CT_SET_UPNP_LEASE_DURATION))
    }

    /// Max peers sent in a reply to `get_peers`.
    ///
    /// Accepts `0..=i32::MAX`.
    #[inline]
    pub fn dht_max_peers_reply(&mut self, value: i32) -> Result<&mut Self, SettingsError> {
        in_range("dht_max_peers_reply", value, 0..=i32::MAX)?;
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_DHT_MAX_PEERS_REPLY),
            value,
        );
        Ok(self)
    }

    /// Reads `dht_max_peers_reply` if set in this pack.
    #[inline]
    pub fn get_dht_max_peers_reply(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(sys::CT_SET_DHT_MAX_PEERS_REPLY))
    }

    /// Concurrent search requests per lookup (kademlia's alpha).
    ///
    /// Accepts `1..=127`.
    #[inline]
    pub fn dht_search_branching(&mut self, value: i32) -> Result<&mut Self, SettingsError> {
        in_range("dht_search_branching", value, 1..=127)?;
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_DHT_SEARCH_BRANCHING),
            value,
        );
        Ok(self)
    }

    /// Reads `dht_search_branching` if set in this pack.
    #[inline]
    pub fn get_dht_search_branching(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(sys::CT_SET_DHT_SEARCH_BRANCHING))
    }

    /// Failed contact attempts before a routing-table node with no ready
    /// replacement is removed (nodes with replacements go immediately).
    #[inline]
    pub fn dht_max_fail_count(&mut self, value: i32) -> &mut Self {
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_DHT_MAX_FAIL_COUNT),
            value,
        );
        self
    }

    /// Reads `dht_max_fail_count` if set in this pack.
    #[inline]
    pub fn get_dht_max_fail_count(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(sys::CT_SET_DHT_MAX_FAIL_COUNT))
    }

    /// Upper bound on torrents tracked from the DHT, bounding the memory
    /// malicious nodes can make us allocate.
    #[inline]
    pub fn dht_max_torrents(&mut self, value: i32) -> &mut Self {
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_DHT_MAX_TORRENTS),
            value,
        );
        self
    }

    /// Reads `dht_max_torrents` if set in this pack.
    #[inline]
    pub fn get_dht_max_torrents(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(sys::CT_SET_DHT_MAX_TORRENTS))
    }

    /// Max number of items the DHT will store.
    ///
    /// Accepts `1..=i32::MAX`.
    #[inline]
    pub fn dht_max_dht_items(&mut self, value: i32) -> Result<&mut Self, SettingsError> {
        in_range("dht_max_dht_items", value, 1..=i32::MAX)?;
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_DHT_MAX_DHT_ITEMS),
            value,
        );
        Ok(self)
    }

    /// Reads `dht_max_dht_items` if set in this pack.
    #[inline]
    pub fn get_dht_max_dht_items(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(sys::CT_SET_DHT_MAX_DHT_ITEMS))
    }

    /// Max peers the DHT stores per torrent.
    #[inline]
    pub fn dht_max_peers(&mut self, value: i32) -> &mut Self {
        self.set_int(SettingKey::from_generated(sys::CT_SET_DHT_MAX_PEERS), value);
        self
    }

    /// Reads `dht_max_peers` if set in this pack.
    #[inline]
    pub fn get_dht_max_peers(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(sys::CT_SET_DHT_MAX_PEERS))
    }

    /// Seconds a DHT node stays banned for exceeding the rate limit
    /// (averaged over 10 seconds to allow bursts).
    #[inline]
    pub fn dht_block_timeout(&mut self, value: i32) -> &mut Self {
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_DHT_BLOCK_TIMEOUT),
            value,
        );
        self
    }

    /// Reads `dht_block_timeout` if set in this pack.
    #[inline]
    pub fn get_dht_block_timeout(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(sys::CT_SET_DHT_BLOCK_TIMEOUT))
    }

    /// Max packets per second a DHT node may send without getting banned.
    #[inline]
    pub fn dht_block_ratelimit(&mut self, value: i32) -> &mut Self {
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_DHT_BLOCK_RATELIMIT),
            value,
        );
        self
    }

    /// Reads `dht_block_ratelimit` if set in this pack.
    #[inline]
    pub fn get_dht_block_ratelimit(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(sys::CT_SET_DHT_BLOCK_RATELIMIT))
    }

    /// Seconds until an immutable/mutable item expires; 0 (the default)
    /// means never.
    #[inline]
    pub fn dht_item_lifetime(&mut self, value: i32) -> &mut Self {
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_DHT_ITEM_LIFETIME),
            value,
        );
        self
    }

    /// Reads `dht_item_lifetime` if set in this pack.
    #[inline]
    pub fn get_dht_item_lifetime(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(sys::CT_SET_DHT_ITEM_LIFETIME))
    }

    /// Seconds between recomputations of the precomputed info-hashes
    /// sample served to `sample_infohashes` requests.
    ///
    /// Accepts `0..=21_600`.
    #[inline]
    pub fn dht_sample_infohashes_interval(
        &mut self,
        value: i32,
    ) -> Result<&mut Self, SettingsError> {
        in_range("dht_sample_infohashes_interval", value, 0..=21_600)?;
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_DHT_SAMPLE_INFOHASHES_INTERVAL),
            value,
        );
        Ok(self)
    }

    /// Reads `dht_sample_infohashes_interval` if set in this pack.
    #[inline]
    pub fn get_dht_sample_infohashes_interval(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(
            sys::CT_SET_DHT_SAMPLE_INFOHASHES_INTERVAL,
        ))
    }

    /// Max elements in the sampled info-hashes subset; storage
    /// implementations may clamp it so UDP packets fit.
    #[inline]
    pub fn dht_max_infohashes_sample_count(&mut self, value: i32) -> &mut Self {
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_DHT_MAX_INFOHASHES_SAMPLE_COUNT),
            value,
        );
        self
    }

    /// Reads `dht_max_infohashes_sample_count` if set in this pack.
    #[inline]
    pub fn get_dht_max_infohashes_sample_count(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(
            sys::CT_SET_DHT_MAX_INFOHASHES_SAMPLE_COUNT,
        ))
    }

    /// NAT-PMP and PCP port-mapping lease duration, in seconds.
    #[inline]
    pub fn natpmp_lease_duration(&mut self, value: i32) -> &mut Self {
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_NATPMP_LEASE_DURATION),
            value,
        );
        self
    }

    /// Reads `natpmp_lease_duration` if set in this pack.
    #[inline]
    pub fn get_natpmp_lease_duration(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(
            sys::CT_SET_NATPMP_LEASE_DURATION,
        ))
    }
}

/// A hostname (or address) and port, as used by
/// [`SettingsPack::dht_bootstrap_nodes`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostPort {
    /// A hostname, IPv4 address, or bracketed IPv6 address.
    pub host: String,
    /// Port 0 could never be connected to, so it is not representable.
    pub port: std::num::NonZeroU16,
}
