// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Trackers, announces and scrapes, HTTP limits, auto-management,
//! queueing and seeding limits.

use libctorrent_sys as sys;

use super::error::in_range;
use super::{SettingKey, SettingsError, SettingsPack};

impl SettingsPack {
    /// Client identification sent to trackers (and in extended headers
    /// to peers), conventionally "name/version". Must not contain \r
    /// or \n.
    #[inline]
    pub fn user_agent(&mut self, value: &str) -> &mut Self {
        self.set_str(SettingKey::from_generated(sys::CT_SET_USER_AGENT), value);
        self
    }

    /// Reads `user_agent` if set in this pack.
    #[inline]
    pub fn get_user_agent(&self) -> Option<String> {
        self.get_str(SettingKey::from_generated(sys::CT_SET_USER_AGENT))
    }

    /// IP address sent to trackers as the `&ip=` parameter; empty
    /// (default) omits it. Only useful when the seed runs on the
    /// tracker's host and the tracker accepts the parameter, which
    /// normal trackers don't.
    #[inline]
    pub fn announce_ip(&mut self, value: &str) -> &mut Self {
        self.set_str(SettingKey::from_generated(sys::CT_SET_ANNOUNCE_IP), value);
        self
    }

    /// Reads `announce_ip` if set in this pack.
    #[inline]
    pub fn get_announce_ip(&self) -> Option<String> {
        self.get_str(SettingKey::from_generated(sys::CT_SET_ANNOUNCE_IP))
    }

    /// Client name/version sent to peers in the handshake (UTF-8);
    /// empty falls back to `user_agent`.
    #[inline]
    pub fn handshake_client_version(&mut self, value: &str) -> &mut Self {
        self.set_str(
            SettingKey::from_generated(sys::CT_SET_HANDSHAKE_CLIENT_VERSION),
            value,
        );
        self
    }

    /// Reads `handshake_client_version` if set in this pack.
    #[inline]
    pub fn get_handshake_client_version(&self) -> Option<String> {
        self.get_str(SettingKey::from_generated(
            sys::CT_SET_HANDSHAKE_CLIENT_VERSION,
        ))
    }

    /// Prefers seeding torrents over downloading ones when handing out
    /// active slots.
    #[inline]
    pub fn auto_manage_prefer_seeds(&mut self, value: bool) -> &mut Self {
        self.set_bool(
            SettingKey::from_generated(sys::CT_SET_AUTO_MANAGE_PREFER_SEEDS),
            value,
        );
        self
    }

    /// Reads `auto_manage_prefer_seeds` if set in this pack.
    #[inline]
    pub fn get_auto_manage_prefer_seeds(&self) -> Option<bool> {
        self.get_bool(SettingKey::from_generated(
            sys::CT_SET_AUTO_MANAGE_PREFER_SEEDS,
        ))
    }

    /// Exempts torrents without payload transfer from the
    /// `active_seeds`/`active_downloads` limits, so idle torrents do
    /// not block the active slots.
    #[inline]
    pub fn dont_count_slow_torrents(&mut self, value: bool) -> &mut Self {
        self.set_bool(
            SettingKey::from_generated(sys::CT_SET_DONT_COUNT_SLOW_TORRENTS),
            value,
        );
        self
    }

    /// Reads `dont_count_slow_torrents` if set in this pack.
    #[inline]
    pub fn get_dont_count_slow_torrents(&self) -> Option<bool> {
        self.get_bool(SettingKey::from_generated(
            sys::CT_SET_DONT_COUNT_SLOW_TORRENTS,
        ))
    }

    /// Multi-tracker behavior: `announce_to_all_trackers` announces to
    /// every tracker in the active tier in parallel;
    /// `announce_to_all_tiers` announces to one tracker of each tier
    /// (uTorrent behavior). Both false follows the multi-tracker
    /// specification.
    #[inline]
    pub fn announce_to_all_tiers(&mut self, value: bool) -> &mut Self {
        self.set_bool(
            SettingKey::from_generated(sys::CT_SET_ANNOUNCE_TO_ALL_TIERS),
            value,
        );
        self
    }

    /// Reads `announce_to_all_tiers` if set in this pack.
    #[inline]
    pub fn get_announce_to_all_tiers(&self) -> Option<bool> {
        self.get_bool(SettingKey::from_generated(
            sys::CT_SET_ANNOUNCE_TO_ALL_TIERS,
        ))
    }

    #[inline]
    pub fn announce_to_all_trackers(&mut self, value: bool) -> &mut Self {
        self.set_bool(
            SettingKey::from_generated(sys::CT_SET_ANNOUNCE_TO_ALL_TRACKERS),
            value,
        );
        self
    }

    /// Reads `announce_to_all_trackers` if set in this pack.
    #[inline]
    pub fn get_announce_to_all_trackers(&self) -> Option<bool> {
        self.get_bool(SettingKey::from_generated(
            sys::CT_SET_ANNOUNCE_TO_ALL_TRACKERS,
        ))
    }

    /// Tries a hostname's UDP trackers before its HTTP ones; false
    /// respects tier order with no protocol preference.
    #[inline]
    pub fn prefer_udp_trackers(&mut self, value: bool) -> &mut Self {
        self.set_bool(
            SettingKey::from_generated(sys::CT_SET_PREFER_UDP_TRACKERS),
            value,
        );
        self
    }

    /// Reads `prefer_udp_trackers` if set in this pack.
    #[inline]
    pub fn get_prefer_udp_trackers(&self) -> Option<bool> {
        self.get_bool(SettingKey::from_generated(sys::CT_SET_PREFER_UDP_TRACKERS))
    }

    /// Starts an auto-managed torrent that was queued (paused) by the
    /// queueing mechanism when a peer connects to it, instead of
    /// leaving it queued.
    #[inline]
    pub fn incoming_starts_queued_torrents(&mut self, value: bool) -> &mut Self {
        self.set_bool(
            SettingKey::from_generated(sys::CT_SET_INCOMING_STARTS_QUEUED_TORRENTS),
            value,
        );
        self
    }

    /// Reads `incoming_starts_queued_torrents` if set in this pack.
    #[inline]
    pub fn get_incoming_starts_queued_torrents(&self) -> Option<bool> {
        self.get_bool(SettingKey::from_generated(
            sys::CT_SET_INCOMING_STARTS_QUEUED_TORRENTS,
        ))
    }

    /// Sends the user-agent in every web seed request rather than only
    /// the first per HTTP connection.
    #[inline]
    pub fn always_send_user_agent(&mut self, value: bool) -> &mut Self {
        self.set_bool(
            SettingKey::from_generated(sys::CT_SET_ALWAYS_SEND_USER_AGENT),
            value,
        );
        self
    }

    /// Reads `always_send_user_agent` if set in this pack.
    #[inline]
    pub fn get_always_send_user_agent(&self) -> Option<bool> {
        self.get_bool(SettingKey::from_generated(
            sys::CT_SET_ALWAYS_SEND_USER_AGENT,
        ))
    }

    /// Applies the IP filter (if one is set) to trackers as well as
    /// peers; false exempts trackers.
    #[inline]
    pub fn apply_ip_filter_to_trackers(&mut self, value: bool) -> &mut Self {
        self.set_bool(
            SettingKey::from_generated(sys::CT_SET_APPLY_IP_FILTER_TO_TRACKERS),
            value,
        );
        self
    }

    /// Reads `apply_ip_filter_to_trackers` if set in this pack.
    #[inline]
    pub fn get_apply_ip_filter_to_trackers(&self) -> Option<bool> {
        self.get_bool(SettingKey::from_generated(
            sys::CT_SET_APPLY_IP_FILTER_TO_TRACKERS,
        ))
    }

    /// Includes `&supportcrypto=1` in HTTP tracker announces when
    /// incoming encrypted connections are enabled.
    #[inline]
    pub fn announce_crypto_support(&mut self, value: bool) -> &mut Self {
        self.set_bool(
            SettingKey::from_generated(sys::CT_SET_ANNOUNCE_CRYPTO_SUPPORT),
            value,
        );
        self
    }

    /// Reads `announce_crypto_support` if set in this pack.
    #[inline]
    pub fn get_announce_crypto_support(&self) -> Option<bool> {
        self.get_bool(SettingKey::from_generated(
            sys::CT_SET_ANNOUNCE_CRYPTO_SUPPORT,
        ))
    }

    /// Validates certificates of HTTPS trackers, HTTPS web seeds and
    /// wss:// trackers against the system certificate store; may need
    /// disabling on systems without one.
    #[inline]
    pub fn validate_https_trackers(&mut self, value: bool) -> &mut Self {
        self.set_bool(
            SettingKey::from_generated(sys::CT_SET_VALIDATE_HTTPS_TRACKERS),
            value,
        );
        self
    }

    /// Reads `validate_https_trackers` if set in this pack.
    #[inline]
    pub fn get_validate_https_trackers(&self) -> Option<bool> {
        self.get_bool(SettingKey::from_generated(
            sys::CT_SET_VALIDATE_HTTPS_TRACKERS,
        ))
    }

    /// SSRF mitigations for tracker and web seed requests: loopback
    /// HTTP(S) tracker requests must have a path starting with
    /// "/announce", local-network web seeds may not carry query
    /// strings, and global web seeds may not redirect to local
    /// addresses (redirect targets included).
    #[inline]
    pub fn ssrf_mitigation(&mut self, value: bool) -> &mut Self {
        self.set_bool(
            SettingKey::from_generated(sys::CT_SET_SSRF_MITIGATION),
            value,
        );
        self
    }

    /// Reads `ssrf_mitigation` if set in this pack.
    #[inline]
    pub fn get_ssrf_mitigation(&self) -> Option<bool> {
        self.get_bool(SettingKey::from_generated(sys::CT_SET_SSRF_MITIGATION))
    }

    /// When false, trackers and web seeds with internationalized
    /// (IDNA) hostnames are ignored, avoiding unicode-encoding
    /// attacks.
    #[inline]
    pub fn allow_idna(&mut self, value: bool) -> &mut Self {
        self.set_bool(SettingKey::from_generated(sys::CT_SET_ALLOW_IDNA), value);
        self
    }

    /// Reads `allow_idna` if set in this pack.
    #[inline]
    pub fn get_allow_idna(&self) -> Option<bool> {
        self.get_bool(SettingKey::from_generated(sys::CT_SET_ALLOW_IDNA))
    }

    /// Seconds from sending a tracker request until it is considered
    /// timed out.
    ///
    /// Accepts `1..=i32::MAX`.
    #[inline]
    pub fn tracker_completion_timeout(&mut self, value: i32) -> Result<&mut Self, SettingsError> {
        in_range("tracker_completion_timeout", value, 1..=i32::MAX)?;
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_TRACKER_COMPLETION_TIMEOUT),
            value,
        );
        Ok(self)
    }

    /// Reads `tracker_completion_timeout` if set in this pack.
    #[inline]
    pub fn get_tracker_completion_timeout(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(
            sys::CT_SET_TRACKER_COMPLETION_TIMEOUT,
        ))
    }

    /// Seconds without receiving any tracker data before timing out —
    /// the timeout that fires when a tracker is down.
    ///
    /// Accepts `1..=i32::MAX`.
    #[inline]
    pub fn tracker_receive_timeout(&mut self, value: i32) -> Result<&mut Self, SettingsError> {
        in_range("tracker_receive_timeout", value, 1..=i32::MAX)?;
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_TRACKER_RECEIVE_TIMEOUT),
            value,
        );
        Ok(self)
    }

    /// Reads `tracker_receive_timeout` if set in this pack.
    #[inline]
    pub fn get_tracker_receive_timeout(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(
            sys::CT_SET_TRACKER_RECEIVE_TIMEOUT,
        ))
    }

    /// Seconds to wait on a `stopped` announce before timing out;
    /// usually shorter so the client quits faster. 0 suppresses
    /// `stopped` announces entirely.
    #[inline]
    pub fn stop_tracker_timeout(&mut self, value: i32) -> &mut Self {
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_STOP_TRACKER_TIMEOUT),
            value,
        );
        self
    }

    /// Reads `stop_tracker_timeout` if set in this pack.
    #[inline]
    pub fn get_stop_tracker_timeout(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(sys::CT_SET_STOP_TRACKER_TIMEOUT))
    }

    /// Number of peers requested from each tracker (the `&num_want=`
    /// parameter).
    #[inline]
    pub fn num_want(&mut self, value: i32) -> &mut Self {
        self.set_int(SettingKey::from_generated(sys::CT_SET_NUM_WANT), value);
        self
    }

    /// Reads `num_want` if set in this pack.
    #[inline]
    pub fn get_num_want(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(sys::CT_SET_NUM_WANT))
    }

    /// Queue limits for auto-managed torrents; excess torrents are
    /// paused until slots free up. `active_downloads` and
    /// `active_seeds` cap downloading and seeding torrents (target is
    /// min(active_downloads + active_seeds, active_limit));
    /// `active_checking` caps simultaneous checking torrents;
    /// `active_limit` is a hard cap on all active auto-managed
    /// torrents; `active_dht_limit`, `active_tracker_limit` and
    /// `active_lsd_limit` cap how many torrents announce to the DHT,
    /// trackers and LSD respectively. -1 means unlimited.
    /// Non-auto-managed torrents are not counted.
    #[inline]
    pub fn active_downloads(&mut self, value: i32) -> &mut Self {
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_ACTIVE_DOWNLOADS),
            value,
        );
        self
    }

    /// Reads `active_downloads` if set in this pack.
    #[inline]
    pub fn get_active_downloads(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(sys::CT_SET_ACTIVE_DOWNLOADS))
    }

    #[inline]
    pub fn active_seeds(&mut self, value: i32) -> &mut Self {
        self.set_int(SettingKey::from_generated(sys::CT_SET_ACTIVE_SEEDS), value);
        self
    }

    /// Reads `active_seeds` if set in this pack.
    #[inline]
    pub fn get_active_seeds(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(sys::CT_SET_ACTIVE_SEEDS))
    }

    #[inline]
    pub fn active_checking(&mut self, value: i32) -> &mut Self {
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_ACTIVE_CHECKING),
            value,
        );
        self
    }

    /// Reads `active_checking` if set in this pack.
    #[inline]
    pub fn get_active_checking(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(sys::CT_SET_ACTIVE_CHECKING))
    }

    #[inline]
    pub fn active_dht_limit(&mut self, value: i32) -> &mut Self {
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_ACTIVE_DHT_LIMIT),
            value,
        );
        self
    }

    /// Reads `active_dht_limit` if set in this pack.
    #[inline]
    pub fn get_active_dht_limit(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(sys::CT_SET_ACTIVE_DHT_LIMIT))
    }

    #[inline]
    pub fn active_tracker_limit(&mut self, value: i32) -> &mut Self {
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_ACTIVE_TRACKER_LIMIT),
            value,
        );
        self
    }

    /// Reads `active_tracker_limit` if set in this pack.
    #[inline]
    pub fn get_active_tracker_limit(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(sys::CT_SET_ACTIVE_TRACKER_LIMIT))
    }

    #[inline]
    pub fn active_lsd_limit(&mut self, value: i32) -> &mut Self {
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_ACTIVE_LSD_LIMIT),
            value,
        );
        self
    }

    /// Reads `active_lsd_limit` if set in this pack.
    #[inline]
    pub fn get_active_lsd_limit(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(sys::CT_SET_ACTIVE_LSD_LIMIT))
    }

    #[inline]
    pub fn active_limit(&mut self, value: i32) -> &mut Self {
        self.set_int(SettingKey::from_generated(sys::CT_SET_ACTIVE_LIMIT), value);
        self
    }

    /// Reads `active_limit` if set in this pack.
    #[inline]
    pub fn get_active_limit(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(sys::CT_SET_ACTIVE_LIMIT))
    }

    /// Seconds between updates/rotations of the torrent queue.
    #[inline]
    pub fn auto_manage_interval(&mut self, value: i32) -> &mut Self {
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_AUTO_MANAGE_INTERVAL),
            value,
        );
        self
    }

    /// Reads `auto_manage_interval` if set in this pack.
    #[inline]
    pub fn get_auto_manage_interval(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(sys::CT_SET_AUTO_MANAGE_INTERVAL))
    }

    /// Seconds a torrent may be an active seed before it is considered
    /// to have met the seed-limit criteria.
    #[inline]
    pub fn seed_time_limit(&mut self, value: i32) -> &mut Self {
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_SEED_TIME_LIMIT),
            value,
        );
        self
    }

    /// Reads `seed_time_limit` if set in this pack.
    #[inline]
    pub fn get_seed_time_limit(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(sys::CT_SET_SEED_TIME_LIMIT))
    }

    /// Seconds between scrapes of queued (auto-managed, paused)
    /// torrents, tracking their downloader/seed ratio for queueing
    /// decisions; `auto_scrape_min_interval` is the global minimum
    /// interval between any two automatic scrapes.
    #[inline]
    pub fn auto_scrape_interval(&mut self, value: i32) -> &mut Self {
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_AUTO_SCRAPE_INTERVAL),
            value,
        );
        self
    }

    /// Reads `auto_scrape_interval` if set in this pack.
    #[inline]
    pub fn get_auto_scrape_interval(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(sys::CT_SET_AUTO_SCRAPE_INTERVAL))
    }

    #[inline]
    pub fn auto_scrape_min_interval(&mut self, value: i32) -> &mut Self {
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_AUTO_SCRAPE_MIN_INTERVAL),
            value,
        );
        self
    }

    /// Reads `auto_scrape_min_interval` if set in this pack.
    #[inline]
    pub fn get_auto_scrape_min_interval(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(
            sys::CT_SET_AUTO_SCRAPE_MIN_INTERVAL,
        ))
    }

    /// Minimum announce interval (seconds) honored from a tracker
    /// response, mitigating hammering of mis-configured trackers.
    #[inline]
    pub fn min_announce_interval(&mut self, value: i32) -> &mut Self {
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_MIN_ANNOUNCE_INTERVAL),
            value,
        );
        self
    }

    /// Reads `min_announce_interval` if set in this pack.
    #[inline]
    pub fn get_min_announce_interval(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(
            sys::CT_SET_MIN_ANNOUNCE_INTERVAL,
        ))
    }

    /// Seconds a newly started torrent counts as active regardless of
    /// transfer rate, giving it a fair chance to start downloading.
    #[inline]
    pub fn auto_manage_startup(&mut self, value: i32) -> &mut Self {
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_AUTO_MANAGE_STARTUP),
            value,
        );
        self
    }

    /// Reads `auto_manage_startup` if set in this pack.
    #[inline]
    pub fn get_auto_manage_startup(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(sys::CT_SET_AUTO_MANAGE_STARTUP))
    }

    /// Seconds UDP tracker connection tokens are kept. The protocol
    /// specifies 60; higher values save packets but only work if the
    /// tracker's expiry matches.
    #[inline]
    pub fn udp_tracker_token_expiry(&mut self, value: i32) -> &mut Self {
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_UDP_TRACKER_TOKEN_EXPIRY),
            value,
        );
        self
    }

    /// Reads `udp_tracker_token_expiry` if set in this pack.
    #[inline]
    pub fn get_udp_tracker_token_expiry(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(
            sys::CT_SET_UDP_TRACKER_TOKEN_EXPIRY,
        ))
    }

    /// Backoff aggressiveness for retrying failing trackers: the retry
    /// delay in seconds is `5 + 5 * x / 100 * fails^2`, with this
    /// setting as x.
    ///
    /// Accepts `0..=26_630`.
    #[inline]
    pub fn tracker_backoff(&mut self, value: i32) -> Result<&mut Self, SettingsError> {
        in_range("tracker_backoff", value, 0..=26_630)?;
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_TRACKER_BACKOFF),
            value,
        );
        Ok(self)
    }

    /// Reads `tracker_backoff` if set in this pack.
    #[inline]
    pub fn get_tracker_backoff(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(sys::CT_SET_TRACKER_BACKOFF))
    }

    /// A seeding torrent reaching the share ratio (up/down), seed time
    /// ratio (time seeding / time downloading) or seed time limit is
    /// considered done and loses seeding priority (it may still seed).
    /// Ratios are in percent.
    #[inline]
    pub fn share_ratio_limit(&mut self, value: i32) -> &mut Self {
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_SHARE_RATIO_LIMIT),
            value,
        );
        self
    }

    /// Reads `share_ratio_limit` if set in this pack.
    #[inline]
    pub fn get_share_ratio_limit(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(sys::CT_SET_SHARE_RATIO_LIMIT))
    }

    #[inline]
    pub fn seed_time_ratio_limit(&mut self, value: i32) -> &mut Self {
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_SEED_TIME_RATIO_LIMIT),
            value,
        );
        self
    }

    /// Reads `seed_time_ratio_limit` if set in this pack.
    #[inline]
    pub fn get_seed_time_ratio_limit(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(
            sys::CT_SET_SEED_TIME_RATIO_LIMIT,
        ))
    }

    /// Max bytes allowed in an HTTP response when announcing to
    /// trackers or downloading .torrent files by URL.
    #[inline]
    pub fn max_http_recv_buffer_size(&mut self, value: i32) -> &mut Self {
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_MAX_HTTP_RECV_BUFFER_SIZE),
            value,
        );
        self
    }

    /// Reads `max_http_recv_buffer_size` if set in this pack.
    #[inline]
    pub fn get_max_http_recv_buffer_size(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(
            sys::CT_SET_MAX_HTTP_RECV_BUFFER_SIZE,
        ))
    }

    /// Once the limit is hit, tracker requests are queued and issued
    /// when an outstanding announce completes.
    ///
    /// Accepts `1..=i32::MAX`.
    #[inline]
    pub fn max_concurrent_http_announces(
        &mut self,
        value: i32,
    ) -> Result<&mut Self, SettingsError> {
        in_range("max_concurrent_http_announces", value, 1..=i32::MAX)?;
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_MAX_CONCURRENT_HTTP_ANNOUNCES),
            value,
        );
        Ok(self)
    }

    /// Reads `max_concurrent_http_announces` if set in this pack.
    #[inline]
    pub fn get_max_concurrent_http_announces(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(
            sys::CT_SET_MAX_CONCURRENT_HTTP_ANNOUNCES,
        ))
    }

    /// Port reported to trackers (HTTP `port` parameter and DHT)
    /// instead of the actual listening port; 0 (default) reports the
    /// listening port. Only for setups where the externally reachable
    /// port differs, e.g. a reverse tunnel through NAT-PMP. Does not
    /// affect the listening port or LSD announcements.
    #[inline]
    pub fn announce_port(&mut self, value: u16) -> &mut Self {
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_ANNOUNCE_PORT),
            i32::from(value),
        );
        self
    }

    /// Reads `announce_port` if set in this pack.
    #[inline]
    pub fn get_announce_port(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(sys::CT_SET_ANNOUNCE_PORT))
    }
}
