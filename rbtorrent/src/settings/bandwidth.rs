// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Bandwidth and rate limits, choking, send buffers and uTP
//! congestion control.

use libctorrent_sys as sys;

use super::enums::{BandwidthMixedAlgo, ChokingAlgorithm, SeedChokingAlgorithm};
use super::error::in_range;
use super::{SettingKey, SettingsError, SettingsPack};

impl SettingsPack {
    /// Count estimated TCP/IP overhead against the rate limiters, so the
    /// total traffic stays within the limits.
    #[inline]
    pub fn rate_limit_ip_overhead(&mut self, value: bool) -> &mut Self {
        self.set_bool(
            SettingKey::from_generated(sys::CT_SET_RATE_LIMIT_IP_OVERHEAD),
            value,
        );
        self
    }

    /// Reads `rate_limit_ip_overhead` if set in this pack.
    #[inline]
    pub fn get_rate_limit_ip_overhead(&self) -> Option<bool> {
        self.get_bool(SettingKey::from_generated(
            sys::CT_SET_RATE_LIMIT_IP_OVERHEAD,
        ))
    }

    /// Seconds between peer choke/unchoke re-evaluations. The protocol
    /// defines 30; it should stay well above TCP ramp-up time.
    #[inline]
    pub fn unchoke_interval(&mut self, value: i32) -> &mut Self {
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_UNCHOKE_INTERVAL),
            value,
        );
        self
    }

    /// Reads `unchoke_interval` if set in this pack.
    #[inline]
    pub fn get_unchoke_interval(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(sys::CT_SET_UNCHOKE_INTERVAL))
    }

    /// Seconds between rotations of the optimistically unchoked peer.
    #[inline]
    pub fn optimistic_unchoke_interval(&mut self, value: i32) -> &mut Self {
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_OPTIMISTIC_UNCHOKE_INTERVAL),
            value,
        );
        self
    }

    /// Reads `optimistic_unchoke_interval` if set in this pack.
    #[inline]
    pub fn get_optimistic_unchoke_interval(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(
            sys::CT_SET_OPTIMISTIC_UNCHOKE_INTERVAL,
        ))
    }

    /// Minimum send buffer target size in bytes (including bytes pending
    /// disk reads); effectively the initial window that determines how
    /// fast the send rate ramps up. Set it to fit at least a few blocks
    /// for snappy seeding.
    #[inline]
    pub fn send_buffer_low_watermark(&mut self, value: i32) -> &mut Self {
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_SEND_BUFFER_LOW_WATERMARK),
            value,
        );
        self
    }

    /// Reads `send_buffer_low_watermark` if set in this pack.
    #[inline]
    pub fn get_send_buffer_low_watermark(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(
            sys::CT_SET_SEND_BUFFER_LOW_WATERMARK,
        ))
    }

    /// Upper limit of the send buffer, in bytes: when it holds fewer
    /// bytes than this, another 16 kiB block is read onto it. Too small
    /// hurts upload rate; too large wastes memory.
    ///
    /// Accepts `1..=i32::MAX`.
    #[inline]
    pub fn send_buffer_watermark(&mut self, value: i32) -> Result<&mut Self, SettingsError> {
        in_range("send_buffer_watermark", value, 1..=i32::MAX)?;
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_SEND_BUFFER_WATERMARK),
            value,
        );
        Ok(self)
    }

    /// Reads `send_buffer_watermark` if set in this pack.
    #[inline]
    pub fn get_send_buffer_watermark(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(
            sys::CT_SET_SEND_BUFFER_WATERMARK,
        ))
    }

    /// Percentage of the current upload rate used as the actual send
    /// buffer watermark, clamped to `send_buffer_watermark`.
    #[inline]
    pub fn send_buffer_watermark_factor(&mut self, value: i32) -> &mut Self {
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_SEND_BUFFER_WATERMARK_FACTOR),
            value,
        );
        self
    }

    /// Reads `send_buffer_watermark_factor` if set in this pack.
    #[inline]
    pub fn get_send_buffer_watermark_factor(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(
            sys::CT_SET_SEND_BUFFER_WATERMARK_FACTOR,
        ))
    }

    /// Algorithm deciding how many peers to unchoke; among the unchoked,
    /// downloading torrents always favor tit-for-tat. See
    /// `ChokingAlgorithm`.
    #[inline]
    pub fn choking_algorithm(&mut self, value: ChokingAlgorithm) -> &mut Self {
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_CHOKING_ALGORITHM),
            value as i32,
        );
        self
    }

    /// `None` if unset or set to a value these bindings don't know.
    #[inline]
    pub fn get_choking_algorithm(&self) -> Option<ChokingAlgorithm> {
        self.get_int(SettingKey::from_generated(sys::CT_SET_CHOKING_ALGORITHM))
            .and_then(ChokingAlgorithm::from_raw)
    }

    /// How peers are selected for unchoking when seeding, where
    /// tit-for-tat does not apply. See `SeedChokingAlgorithm`.
    #[inline]
    pub fn seed_choking_algorithm(&mut self, value: SeedChokingAlgorithm) -> &mut Self {
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_SEED_CHOKING_ALGORITHM),
            value as i32,
        );
        self
    }

    /// `None` if unset or set to a value these bindings don't know.
    #[inline]
    pub fn get_seed_choking_algorithm(&self) -> Option<SeedChokingAlgorithm> {
        self.get_int(SettingKey::from_generated(
            sys::CT_SET_SEED_CHOKING_ALGORITHM,
        ))
        .and_then(SeedChokingAlgorithm::from_raw)
    }

    /// Number of optimistic unchoke slots; more finds good peers faster
    /// but uses more bandwidth. 0 means automatic (20% of the allowed
    /// upload slots).
    #[inline]
    pub fn num_optimistic_unchoke_slots(&mut self, value: u16) -> &mut Self {
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_NUM_OPTIMISTIC_UNCHOKE_SLOTS),
            i32::from(value),
        );
        self
    }

    /// Reads `num_optimistic_unchoke_slots` if set in this pack.
    #[inline]
    pub fn get_num_optimistic_unchoke_slots(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(
            sys::CT_SET_NUM_OPTIMISTIC_UNCHOKE_SLOTS,
        ))
    }

    /// Session-global upload rate limit, in bytes per second; 0 means
    /// unlimited. Peers on the local network are not rate limited by
    /// default.
    #[inline]
    pub fn upload_rate_limit(&mut self, value: i32) -> &mut Self {
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_UPLOAD_RATE_LIMIT),
            value,
        );
        self
    }

    /// Reads `upload_rate_limit` if set in this pack.
    #[inline]
    pub fn get_upload_rate_limit(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(sys::CT_SET_UPLOAD_RATE_LIMIT))
    }

    /// Session-global download rate limit, in bytes per second; 0 means
    /// unlimited.
    #[inline]
    pub fn download_rate_limit(&mut self, value: i32) -> &mut Self {
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_DOWNLOAD_RATE_LIMIT),
            value,
        );
        self
    }

    /// Reads `download_rate_limit` if set in this pack.
    #[inline]
    pub fn get_download_rate_limit(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(sys::CT_SET_DOWNLOAD_RATE_LIMIT))
    }

    /// Max unchoked peers in the session (may be ignored by some
    /// `choking_algorithm` values); -1 means all peers are always
    /// unchoked.
    #[inline]
    pub fn unchoke_slots_limit(&mut self, value: i32) -> &mut Self {
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_UNCHOKE_SLOTS_LIMIT),
            value,
        );
        self
    }

    /// Reads `unchoke_slots_limit` if set in this pack.
    #[inline]
    pub fn get_unchoke_slots_limit(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(sys::CT_SET_UNCHOKE_SLOTS_LIMIT))
    }

    /// Target delay for uTP sockets, in milliseconds. Higher values make
    /// uTP more aggressive and queue longer at the upload bottleneck; too
    /// low and measurement noise makes it send too slowly.
    ///
    /// Accepts `1..=2_147_483`.
    #[inline]
    pub fn utp_target_delay(&mut self, value: i32) -> Result<&mut Self, SettingsError> {
        in_range("utp_target_delay", value, 1..=2_147_483)?;
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_UTP_TARGET_DELAY),
            value,
        );
        Ok(self)
    }

    /// Reads `utp_target_delay` if set in this pack.
    #[inline]
    pub fn get_utp_target_delay(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(sys::CT_SET_UTP_TARGET_DELAY))
    }

    /// Max bytes the uTP congestion window may grow in one RTT.
    #[inline]
    pub fn utp_gain_factor(&mut self, value: i32) -> &mut Self {
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_UTP_GAIN_FACTOR),
            value,
        );
        self
    }

    /// Reads `utp_gain_factor` if set in this pack.
    #[inline]
    pub fn get_utp_gain_factor(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(sys::CT_SET_UTP_GAIN_FACTOR))
    }

    /// Shortest allowed uTP socket timeout, in milliseconds (the timeout
    /// otherwise follows the connection's RTT).
    #[inline]
    pub fn utp_min_timeout(&mut self, value: i32) -> &mut Self {
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_UTP_MIN_TIMEOUT),
            value,
        );
        self
    }

    /// Reads `utp_min_timeout` if set in this pack.
    #[inline]
    pub fn get_utp_min_timeout(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(sys::CT_SET_UTP_MIN_TIMEOUT))
    }

    /// SYN packets sent (and timed out) before giving up and closing the
    /// socket.
    #[inline]
    pub fn utp_syn_resends(&mut self, value: i32) -> &mut Self {
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_UTP_SYN_RESENDS),
            value,
        );
        self
    }

    /// Reads `utp_syn_resends` if set in this pack.
    #[inline]
    pub fn get_utp_syn_resends(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(sys::CT_SET_UTP_SYN_RESENDS))
    }

    /// FIN packets sent (and timed out) before giving up and closing the
    /// socket.
    #[inline]
    pub fn utp_fin_resends(&mut self, value: i32) -> &mut Self {
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_UTP_FIN_RESENDS),
            value,
        );
        self
    }

    /// Reads `utp_fin_resends` if set in this pack.
    #[inline]
    pub fn get_utp_fin_resends(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(sys::CT_SET_UTP_FIN_RESENDS))
    }

    /// Times a packet is sent (and lost or timed out) before giving up
    /// and closing the connection.
    #[inline]
    pub fn utp_num_resends(&mut self, value: i32) -> &mut Self {
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_UTP_NUM_RESENDS),
            value,
        );
        self
    }

    /// Reads `utp_num_resends` if set in this pack.
    #[inline]
    pub fn get_utp_num_resends(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(sys::CT_SET_UTP_NUM_RESENDS))
    }

    /// Timeout in milliseconds for the initial uTP SYN packet, doubled
    /// for each consecutive timeout.
    ///
    /// Accepts `1..=i32::MAX`.
    #[inline]
    pub fn utp_connect_timeout(&mut self, value: i32) -> Result<&mut Self, SettingsError> {
        in_range("utp_connect_timeout", value, 1..=i32::MAX)?;
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_UTP_CONNECT_TIMEOUT),
            value,
        );
        Ok(self)
    }

    /// Reads `utp_connect_timeout` if set in this pack.
    #[inline]
    pub fn get_utp_connect_timeout(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(sys::CT_SET_UTP_CONNECT_TIMEOUT))
    }

    /// Percentage multiplier applied to the uTP congestion window on
    /// packet loss. Do not change unless you know what you're doing.
    ///
    /// Accepts `0..=100`.
    #[inline]
    pub fn utp_loss_multiplier(&mut self, value: i32) -> Result<&mut Self, SettingsError> {
        in_range("utp_loss_multiplier", value, 0..=100)?;
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_UTP_LOSS_MULTIPLIER),
            value,
        );
        Ok(self)
    }

    /// Reads `utp_loss_multiplier` if set in this pack.
    #[inline]
    pub fn get_utp_loss_multiplier(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(sys::CT_SET_UTP_LOSS_MULTIPLIER))
    }

    /// How to balance bandwidth between TCP and uTP connections; since
    /// uTP yields to TCP, it would otherwise be starved in mixed swarms.
    /// See `BandwidthMixedAlgo`.
    #[inline]
    pub fn mixed_mode_algorithm(&mut self, value: BandwidthMixedAlgo) -> &mut Self {
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_MIXED_MODE_ALGORITHM),
            value as i32,
        );
        self
    }

    /// `None` if unset or set to a value these bindings don't know.
    #[inline]
    pub fn get_mixed_mode_algorithm(&self) -> Option<BandwidthMixedAlgo> {
        self.get_int(SettingKey::from_generated(sys::CT_SET_MIXED_MODE_ALGORITHM))
            .and_then(BandwidthMixedAlgo::from_raw)
    }

    /// Download rate (bytes per second) below which — together with
    /// `inactive_up_rate` — a torrent counts as inactive for the queuing
    /// mechanism (requires `dont_count_slow_torrents`).
    #[inline]
    pub fn inactive_down_rate(&mut self, value: i32) -> &mut Self {
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_INACTIVE_DOWN_RATE),
            value,
        );
        self
    }

    /// Reads `inactive_down_rate` if set in this pack.
    #[inline]
    pub fn get_inactive_down_rate(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(sys::CT_SET_INACTIVE_DOWN_RATE))
    }

    /// Upload-rate counterpart of `inactive_down_rate`.
    #[inline]
    pub fn inactive_up_rate(&mut self, value: i32) -> &mut Self {
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_INACTIVE_UP_RATE),
            value,
        );
        self
    }

    /// Reads `inactive_up_rate` if set in this pack.
    #[inline]
    pub fn get_inactive_up_rate(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(sys::CT_SET_INACTIVE_UP_RATE))
    }

    /// Milliseconds after a uTP congestion-window reduction during which
    /// further packet loss does not reduce it again.
    #[inline]
    pub fn utp_cwnd_reduce_timer(&mut self, value: i32) -> &mut Self {
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_UTP_CWND_REDUCE_TIMER),
            value,
        );
        self
    }

    /// Reads `utp_cwnd_reduce_timer` if set in this pack.
    #[inline]
    pub fn get_utp_cwnd_reduce_timer(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(
            sys::CT_SET_UTP_CWND_REDUCE_TIMER,
        ))
    }

    /// Not-sent low watermark for socket send buffers (the Linux-specific
    /// `TCP_NOTSENT_LOWAT` option).
    #[inline]
    pub fn send_not_sent_low_watermark(&mut self, value: i32) -> &mut Self {
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_SEND_NOT_SENT_LOW_WATERMARK),
            value,
        );
        self
    }

    /// Reads `send_not_sent_low_watermark` if set in this pack.
    #[inline]
    pub fn get_send_not_sent_low_watermark(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(
            sys::CT_SET_SEND_NOT_SENT_LOW_WATERMARK,
        ))
    }

    /// Starting threshold for the rate-based choker; higher values yield
    /// fewer unchoke slots, lower values more.
    #[inline]
    pub fn rate_choker_initial_threshold(&mut self, value: i32) -> &mut Self {
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_RATE_CHOKER_INITIAL_THRESHOLD),
            value,
        );
        self
    }

    /// Reads `rate_choker_initial_threshold` if set in this pack.
    #[inline]
    pub fn get_rate_choker_initial_threshold(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(
            sys::CT_SET_RATE_CHOKER_INITIAL_THRESHOLD,
        ))
    }
}
