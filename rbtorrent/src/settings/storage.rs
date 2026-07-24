// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Piece picking, the request pipeline, disk I/O, storage,
//! hashing and metadata.

use libctorrent_sys as sys;

use super::enums::{IoBufferMode, MmapWriteMode, SuggestMode};
use super::error::in_range;
use super::{SettingKey, SettingsError, SettingsPack};

impl SettingsPack {
    /// Send `have` messages to peers that already have the piece; mainly
    /// useful for collecting statistics.
    #[inline]
    pub fn send_redundant_have(&mut self, value: bool) -> &mut Self {
        self.set_bool(
            SettingKey::from_generated(sys::CT_SET_SEND_REDUNDANT_HAVE),
            value,
        );
        self
    }

    /// Reads `send_redundant_have` if set in this pack.
    #[inline]
    pub fn get_send_redundant_have(&self) -> Option<bool> {
        self.get_bool(SettingKey::from_generated(sys::CT_SET_SEND_REDUNDANT_HAVE))
    }

    /// Pick partial pieces before rarer ones. When false, rare pieces win
    /// unless the number of partial pieces grows out of proportion.
    #[inline]
    pub fn prioritize_partial_pieces(&mut self, value: bool) -> &mut Self {
        self.set_bool(
            SettingKey::from_generated(sys::CT_SET_PRIORITIZE_PARTIAL_PIECES),
            value,
        );
        self
    }

    /// Reads `prioritize_partial_pieces` if set in this pack.
    #[inline]
    pub fn get_prioritize_partial_pieces(&self) -> Option<bool> {
        self.get_bool(SettingKey::from_generated(
            sys::CT_SET_PRIORITIZE_PARTIAL_PIECES,
        ))
    }

    /// Assume all downloaded data is correct, skipping hash checks. Only
    /// useful for simulation and testing.
    #[inline]
    pub fn disable_hash_checks(&mut self, value: bool) -> &mut Self {
        self.set_bool(
            SettingKey::from_generated(sys::CT_SET_DISABLE_HASH_CHECKS),
            value,
        );
        self
    }

    /// Reads `disable_hash_checks` if set in this pack.
    #[inline]
    pub fn get_disable_hash_checks(&self) -> Option<bool> {
        self.get_bool(SettingKey::from_generated(sys::CT_SET_DISABLE_HASH_CHECKS))
    }

    /// Linux-only: open files with `O_NOATIME`, which may improve disk
    /// performance.
    #[inline]
    pub fn no_atime_storage(&mut self, value: bool) -> &mut Self {
        self.set_bool(
            SettingKey::from_generated(sys::CT_SET_NO_ATIME_STORAGE),
            value,
        );
        self
    }

    /// Reads `no_atime_storage` if set in this pack.
    #[inline]
    pub fn get_no_atime_storage(&self) -> Option<bool> {
        self.get_bool(SettingKey::from_generated(sys::CT_SET_NO_ATIME_STORAGE))
    }

    /// Include redundant bytes in the downloaded counter reported to
    /// trackers.
    #[inline]
    pub fn report_true_downloaded(&mut self, value: bool) -> &mut Self {
        self.set_bool(
            SettingKey::from_generated(sys::CT_SET_REPORT_TRUE_DOWNLOADED),
            value,
        );
        self
    }

    /// Reads `report_true_downloaded` if set in this pack.
    #[inline]
    pub fn get_report_true_downloaded(&self) -> Option<bool> {
        self.get_bool(SettingKey::from_generated(
            sys::CT_SET_REPORT_TRUE_DOWNLOADED,
        ))
    }

    /// Only request a block twice once every remaining piece has at least
    /// one outstanding request. Reduces redundant downloads at the cost of
    /// some end-game speed; when false, peers are always kept busy even if
    /// it duplicates requests.
    #[inline]
    pub fn strict_end_game_mode(&mut self, value: bool) -> &mut Self {
        self.set_bool(
            SettingKey::from_generated(sys::CT_SET_STRICT_END_GAME_MODE),
            value,
        );
        self
    }

    /// Reads `strict_end_game_mode` if set in this pack.
    #[inline]
    pub fn get_strict_end_game_mode(&self) -> Option<bool> {
        self.get_bool(SettingKey::from_generated(sys::CT_SET_STRICT_END_GAME_MODE))
    }

    /// Skip checking existing files when resume data is incomplete or
    /// missing, assuming no data and going straight to download mode.
    #[inline]
    pub fn no_recheck_incomplete_resume(&mut self, value: bool) -> &mut Self {
        self.set_bool(
            SettingKey::from_generated(sys::CT_SET_NO_RECHECK_INCOMPLETE_RESUME),
            value,
        );
        self
    }

    /// Reads `no_recheck_incomplete_resume` if set in this pack.
    #[inline]
    pub fn get_no_recheck_incomplete_resume(&self) -> Option<bool> {
        self.get_bool(SettingKey::from_generated(
            sys::CT_SET_NO_RECHECK_INCOMPLETE_RESUME,
        ))
    }

    /// Whether libtorrent advertises share-mode support.
    #[inline]
    pub fn support_share_mode(&mut self, value: bool) -> &mut Self {
        self.set_bool(
            SettingKey::from_generated(sys::CT_SET_SUPPORT_SHARE_MODE),
            value,
        );
        self
    }

    /// Reads `support_share_mode` if set in this pack.
    #[inline]
    pub fn get_support_share_mode(&self) -> Option<bool> {
        self.get_bool(SettingKey::from_generated(sys::CT_SET_SUPPORT_SHARE_MODE))
    }

    /// Report the number of redundant bytes to the tracker.
    #[inline]
    pub fn report_redundant_bytes(&mut self, value: bool) -> &mut Self {
        self.set_bool(
            SettingKey::from_generated(sys::CT_SET_REPORT_REDUNDANT_BYTES),
            value,
        );
        self
    }

    /// Reads `report_redundant_bytes` if set in this pack.
    #[inline]
    pub fn get_report_redundant_bytes(&self) -> Option<bool> {
        self.get_bool(SettingKey::from_generated(
            sys::CT_SET_REPORT_REDUNDANT_BYTES,
        ))
    }

    /// Download torrents with very high piece/seed availability
    /// sequentially, which is more efficient for disk I/O.
    #[inline]
    pub fn auto_sequential(&mut self, value: bool) -> &mut Self {
        self.set_bool(
            SettingKey::from_generated(sys::CT_SET_AUTO_SEQUENTIAL),
            value,
        );
        self
    }

    /// Reads `auto_sequential` if set in this pack.
    #[inline]
    pub fn get_auto_sequential(&self) -> Option<bool> {
        self.get_bool(SettingKey::from_generated(sys::CT_SET_AUTO_SEQUENTIAL))
    }

    /// Prefer downloading 4 MiB extents of adjacent pieces, improving disk
    /// I/O throughput for torrents with small pieces.
    #[inline]
    pub fn piece_extent_affinity(&mut self, value: bool) -> &mut Self {
        self.set_bool(
            SettingKey::from_generated(sys::CT_SET_PIECE_EXTENT_AFFINITY),
            value,
        );
        self
    }

    /// Reads `piece_extent_affinity` if set in this pack.
    #[inline]
    pub fn get_piece_extent_affinity(&self) -> Option<bool> {
        self.get_bool(SettingKey::from_generated(
            sys::CT_SET_PIECE_EXTENT_AFFINITY,
        ))
    }

    /// Windows-only: use `SetFileValidData()` to pre-allocate disk space.
    /// Requires Administrator privileges and may reveal previously deleted
    /// data from the disk.
    #[inline]
    pub fn enable_set_file_valid_data(&mut self, value: bool) -> &mut Self {
        self.set_bool(
            SettingKey::from_generated(sys::CT_SET_ENABLE_SET_FILE_VALID_DATA),
            value,
        );
        self
    }

    /// Reads `enable_set_file_valid_data` if set in this pack.
    #[inline]
    pub fn get_enable_set_file_valid_data(&self) -> Option<bool> {
        self.get_bool(SettingKey::from_generated(
            sys::CT_SET_ENABLE_SET_FILE_VALID_DATA,
        ))
    }

    /// Linux-only: set the no-copy-on-write flag (`FS_NOCOW_FL`) on
    /// downloaded files. Mitigates heavy fragmentation on CoW filesystems
    /// like btrfs, but disables checksumming/compression and restricts
    /// reflinks for those files.
    #[inline]
    pub fn disk_disable_copy_on_write(&mut self, value: bool) -> &mut Self {
        self.set_bool(
            SettingKey::from_generated(sys::CT_SET_DISK_DISABLE_COPY_ON_WRITE),
            value,
        );
        self
    }

    /// Reads `disk_disable_copy_on_write` if set in this pack.
    #[inline]
    pub fn get_disk_disable_copy_on_write(&self) -> Option<bool> {
        self.get_bool(SettingKey::from_generated(
            sys::CT_SET_DISK_DISABLE_COPY_ON_WRITE,
        ))
    }

    /// Seconds from sending a request until it times out with no piece
    /// response.
    #[inline]
    pub fn piece_timeout(&mut self, value: i32) -> &mut Self {
        self.set_int(SettingKey::from_generated(sys::CT_SET_PIECE_TIMEOUT), value);
        self
    }

    /// Reads `piece_timeout` if set in this pack.
    #[inline]
    pub fn get_piece_timeout(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(sys::CT_SET_PIECE_TIMEOUT))
    }

    /// Seconds a 16 kiB block is expected to arrive within before it is
    /// requested from a different peer.
    #[inline]
    pub fn request_timeout(&mut self, value: i32) -> &mut Self {
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_REQUEST_TIMEOUT),
            value,
        );
        self
    }

    /// Reads `request_timeout` if set in this pack.
    #[inline]
    pub fn get_request_timeout(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(sys::CT_SET_REQUEST_TIMEOUT))
    }

    /// Request queue length, expressed as the number of seconds of
    /// transfer it represents at the current download rate.
    #[inline]
    pub fn request_queue_time(&mut self, value: i32) -> &mut Self {
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_REQUEST_QUEUE_TIME),
            value,
        );
        self
    }

    /// Reads `request_queue_time` if set in this pack.
    #[inline]
    pub fn get_request_queue_time(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(sys::CT_SET_REQUEST_QUEUE_TIME))
    }

    /// Max block requests a peer may queue up in the client; excess
    /// requests are dropped. Higher values allow faster upload to a
    /// single peer.
    ///
    /// Accepts `0..=i32::MAX`.
    #[inline]
    pub fn max_allowed_in_request_queue(&mut self, value: i32) -> Result<&mut Self, SettingsError> {
        in_range("max_allowed_in_request_queue", value, 0..=i32::MAX)?;
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_MAX_ALLOWED_IN_REQUEST_QUEUE),
            value,
        );
        Ok(self)
    }

    /// Reads `max_allowed_in_request_queue` if set in this pack.
    #[inline]
    pub fn get_max_allowed_in_request_queue(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(
            sys::CT_SET_MAX_ALLOWED_IN_REQUEST_QUEUE,
        ))
    }

    /// Hard cap on outstanding requests to a single peer; takes
    /// precedence over `request_queue_time`.
    #[inline]
    pub fn max_out_request_queue(&mut self, value: i32) -> &mut Self {
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_MAX_OUT_REQUEST_QUEUE),
            value,
        );
        self
    }

    /// Reads `max_out_request_queue` if set in this pack.
    #[inline]
    pub fn get_max_out_request_queue(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(
            sys::CT_SET_MAX_OUT_REQUEST_QUEUE,
        ))
    }

    /// If a whole piece can be downloaded from a peer within this many
    /// seconds, prefer requesting whole pieces from it (better disk cache
    /// locality, easier attribution of hash failures).
    #[inline]
    pub fn whole_pieces_threshold(&mut self, value: i32) -> &mut Self {
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_WHOLE_PIECES_THRESHOLD),
            value,
        );
        self
    }

    /// Reads `whole_pieces_threshold` if set in this pack.
    #[inline]
    pub fn get_whole_pieces_threshold(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(
            sys::CT_SET_WHOLE_PIECES_THRESHOLD,
        ))
    }

    /// Upper limit on the number of file handles the session keeps open
    /// (mind the process file-descriptor limit).
    ///
    /// Accepts `1..=i32::MAX`.
    #[inline]
    pub fn file_pool_size(&mut self, value: i32) -> Result<&mut Self, SettingsError> {
        in_range("file_pool_size", value, 1..=i32::MAX)?;
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_FILE_POOL_SIZE),
            value,
        );
        Ok(self)
    }

    /// Reads `file_pool_size` if set in this pack.
    #[inline]
    pub fn get_file_pool_size(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(sys::CT_SET_FILE_POOL_SIZE))
    }

    /// Number of pieces picked at random before switching to rarest-first
    /// picking.
    #[inline]
    pub fn initial_picker_threshold(&mut self, value: i32) -> &mut Self {
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_INITIAL_PICKER_THRESHOLD),
            value,
        );
        self
    }

    /// Reads `initial_picker_threshold` if set in this pack.
    #[inline]
    pub fn get_initial_picker_threshold(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(
            sys::CT_SET_INITIAL_PICKER_THRESHOLD,
        ))
    }

    /// Number of allowed-fast pieces to send to peers supporting the fast
    /// extension.
    #[inline]
    pub fn allowed_fast_set_size(&mut self, value: i32) -> &mut Self {
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_ALLOWED_FAST_SET_SIZE),
            value,
        );
        self
    }

    /// Reads `allowed_fast_set_size` if set in this pack.
    #[inline]
    pub fn get_allowed_fast_set_size(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(
            sys::CT_SET_ALLOWED_FAST_SET_SIZE,
        ))
    }

    /// Whether to send suggest messages biasing peers toward requesting
    /// certain pieces; see `SuggestMode`.
    #[inline]
    pub fn suggest_mode(&mut self, value: SuggestMode) -> &mut Self {
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_SUGGEST_MODE),
            value as i32,
        );
        self
    }

    /// `None` if unset or set to a value these bindings don't know.
    #[inline]
    pub fn get_suggest_mode(&self) -> Option<SuggestMode> {
        self.get_int(SettingKey::from_generated(sys::CT_SET_SUGGEST_MODE))
            .and_then(SuggestMode::from_raw)
    }

    /// Max bytes waiting in the disk I/O write queue; when reached, peers
    /// stop reading from their sockets until the disk thread catches up.
    /// Too low a value severely limits download rate.
    ///
    /// Accepts `0..=i32::MAX`.
    #[inline]
    pub fn max_queued_disk_bytes(&mut self, value: i32) -> Result<&mut Self, SettingsError> {
        in_range("max_queued_disk_bytes", value, 0..=i32::MAX)?;
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_MAX_QUEUED_DISK_BYTES),
            value,
        );
        Ok(self)
    }

    /// Reads `max_queued_disk_bytes` if set in this pack.
    #[inline]
    pub fn get_max_queued_disk_bytes(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(
            sys::CT_SET_MAX_QUEUED_DISK_BYTES,
        ))
    }

    /// How files are opened for writing with respect to OS caching; see
    /// `IoBufferMode`. Disabling the cache keeps the torrent traffic from
    /// evicting other processes' cached data.
    #[inline]
    pub fn disk_io_write_mode(&mut self, value: IoBufferMode) -> &mut Self {
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_DISK_IO_WRITE_MODE),
            value as i32,
        );
        self
    }

    /// `None` if unset or set to a value these bindings don't know.
    #[inline]
    pub fn get_disk_io_write_mode(&self) -> Option<IoBufferMode> {
        self.get_int(SettingKey::from_generated(sys::CT_SET_DISK_IO_WRITE_MODE))
            .and_then(IoBufferMode::from_raw)
    }

    #[inline]
    pub fn disk_io_read_mode(&mut self, value: IoBufferMode) -> &mut Self {
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_DISK_IO_READ_MODE),
            value as i32,
        );
        self
    }

    /// `None` if unset or set to a value these bindings don't know.
    #[inline]
    pub fn get_disk_io_read_mode(&self) -> Option<IoBufferMode> {
        self.get_int(SettingKey::from_generated(sys::CT_SET_DISK_IO_READ_MODE))
            .and_then(IoBufferMode::from_raw)
    }

    /// Pieces to send a peer, when seeding, before rotating another peer
    /// into the unchoke set.
    #[inline]
    pub fn seeding_piece_quota(&mut self, value: i32) -> &mut Self {
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_SEEDING_PIECE_QUOTA),
            value,
        );
        self
    }

    /// Reads `seeding_piece_quota` if set in this pack.
    #[inline]
    pub fn get_seeding_piece_quota(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(sys::CT_SET_SEEDING_PIECE_QUOTA))
    }

    /// Seconds after a disk write error before an auto-managed torrent is
    /// taken out of upload mode to re-test the error condition.
    #[inline]
    pub fn optimistic_disk_retry(&mut self, value: i32) -> &mut Self {
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_OPTIMISTIC_DISK_RETRY),
            value,
        );
        self
    }

    /// Reads `optimistic_disk_retry` if set in this pack.
    #[inline]
    pub fn get_optimistic_disk_retry(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(
            sys::CT_SET_OPTIMISTIC_DISK_RETRY,
        ))
    }

    /// Max suggested piece indices remembered per peer, bounding the RAM
    /// a suggest-message flood can use.
    ///
    /// Accepts `1..=i32::MAX`.
    #[inline]
    pub fn max_suggest_pieces(&mut self, value: i32) -> Result<&mut Self, SettingsError> {
        in_range("max_suggest_pieces", value, 1..=i32::MAX)?;
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_MAX_SUGGEST_PIECES),
            value,
        );
        Ok(self)
    }

    /// Reads `max_suggest_pieces` if set in this pack.
    #[inline]
    pub fn get_max_suggest_pieces(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(sys::CT_SET_MAX_SUGGEST_PIECES))
    }

    /// Target upload:download ratio for share-mode torrents. Values below
    /// 2 make no sense, and very high values may prevent downloading
    /// anything at all.
    #[inline]
    pub fn share_mode_target(&mut self, value: i32) -> &mut Self {
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_SHARE_MODE_TARGET),
            value,
        );
        self
    }

    /// Reads `share_mode_target` if set in this pack.
    #[inline]
    pub fn get_share_mode_target(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(sys::CT_SET_SHARE_MODE_TARGET))
    }

    /// Max size in bytes accepted by the metadata extension (magnet
    /// links).
    ///
    /// Accepts `1..=i32::MAX`.
    #[inline]
    pub fn max_metadata_size(&mut self, value: i32) -> Result<&mut Self, SettingsError> {
        in_range("max_metadata_size", value, 1..=i32::MAX)?;
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_MAX_METADATA_SIZE),
            value,
        );
        Ok(self)
    }

    /// Reads `max_metadata_size` if set in this pack.
    #[inline]
    pub fn get_max_metadata_size(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(sys::CT_SET_MAX_METADATA_SIZE))
    }

    /// Disk I/O threads for full-check piece hashing, in addition to
    /// `aio_threads`. These threads also perform the disk reads, so 1 (the
    /// default) is best for sequential-access storage like hard drives.
    ///
    /// Accepts `0..=1_073_741_823`.
    #[inline]
    pub fn hashing_threads(&mut self, value: i32) -> Result<&mut Self, SettingsError> {
        in_range("hashing_threads", value, 0..=1_073_741_823)?;
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_HASHING_THREADS),
            value,
        );
        Ok(self)
    }

    /// Reads `hashing_threads` if set in this pack.
    #[inline]
    pub fn get_hashing_threads(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(sys::CT_SET_HASHING_THREADS))
    }

    /// Outstanding blocks (16 kiB each) while checking torrents; higher
    /// is faster but uses more memory.
    ///
    /// Accepts `1..=131_071`.
    #[inline]
    pub fn checking_mem_usage(&mut self, value: i32) -> Result<&mut Self, SettingsError> {
        in_range("checking_mem_usage", value, 1..=131_071)?;
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_CHECKING_MEM_USAGE),
            value,
        );
        Ok(self)
    }

    /// Reads `checking_mem_usage` if set in this pack.
    #[inline]
    pub fn get_checking_mem_usage(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(sys::CT_SET_CHECKING_MEM_USAGE))
    }

    /// If > 0, announce pieces to peers this many milliseconds before
    /// they are expected to complete (and before they are hash checked).
    #[inline]
    pub fn predictive_piece_announce(&mut self, value: i32) -> &mut Self {
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_PREDICTIVE_PIECE_ANNOUNCE),
            value,
        );
        self
    }

    /// Reads `predictive_piece_announce` if set in this pack.
    #[inline]
    pub fn get_predictive_piece_announce(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(
            sys::CT_SET_PREDICTIVE_PIECE_ANNOUNCE,
        ))
    }

    /// Number of I/O threads to use (for some aio back-ends).
    ///
    /// Accepts `1..=i32::MAX`.
    #[inline]
    pub fn aio_threads(&mut self, value: i32) -> Result<&mut Self, SettingsError> {
        in_range("aio_threads", value, 1..=i32::MAX)?;
        self.set_int(SettingKey::from_generated(sys::CT_SET_AIO_THREADS), value);
        Ok(self)
    }

    /// Reads `aio_threads` if set in this pack.
    #[inline]
    pub fn get_aio_threads(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(sys::CT_SET_AIO_THREADS))
    }

    /// Seconds between closing the least recently opened file, keeping
    /// the OS disk cache bounded (needed mainly on Windows); 0 disables.
    #[inline]
    pub fn close_file_interval(&mut self, value: i32) -> &mut Self {
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_CLOSE_FILE_INTERVAL),
            value,
        );
        self
    }

    /// Reads `close_file_interval` if set in this pack.
    #[inline]
    pub fn get_close_file_interval(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(sys::CT_SET_CLOSE_FILE_INTERVAL))
    }

    /// Max number of pieces allowed in metadata received via magnet
    /// links.
    ///
    /// Accepts `1..=1_073_741_823`.
    #[inline]
    pub fn max_piece_count(&mut self, value: i32) -> Result<&mut Self, SettingsError> {
        in_range("max_piece_count", value, 1..=1_073_741_823)?;
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_MAX_PIECE_COUNT),
            value,
        );
        Ok(self)
    }

    /// Reads `max_piece_count` if set in this pack.
    #[inline]
    pub fn get_max_piece_count(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(sys::CT_SET_MAX_PIECE_COUNT))
    }

    /// Max bencoded tokens parsed in metadata received from peers (a DoS
    /// guard); may need raising for very large torrents.
    ///
    /// Accepts `1..=i32::MAX`.
    #[inline]
    pub fn metadata_token_limit(&mut self, value: i32) -> Result<&mut Self, SettingsError> {
        in_range("metadata_token_limit", value, 1..=i32::MAX)?;
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_METADATA_TOKEN_LIMIT),
            value,
        );
        Ok(self)
    }

    /// Reads `metadata_token_limit` if set in this pack.
    #[inline]
    pub fn get_metadata_token_limit(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(sys::CT_SET_METADATA_TOKEN_LIMIT))
    }

    /// Whether mmap_disk_io writes via memory-mapped files or normal
    /// write calls; see `MmapWriteMode`. Some OSes/filesystems (Windows,
    /// ZFS, Btrfs) do not support mixing write calls with mmap, and since
    /// large files are always read via mmap, forcing write calls there
    /// may cause corruption.
    #[inline]
    pub fn disk_write_mode(&mut self, value: MmapWriteMode) -> &mut Self {
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_DISK_WRITE_MODE),
            value as i32,
        );
        self
    }

    /// `None` if unset or set to a value these bindings don't know.
    #[inline]
    pub fn get_disk_write_mode(&self) -> Option<MmapWriteMode> {
        self.get_int(SettingKey::from_generated(sys::CT_SET_DISK_WRITE_MODE))
            .and_then(MmapWriteMode::from_raw)
    }

    /// Files smaller than this many 16 kiB blocks use pread/pwrite
    /// instead of memory mapping (mmap_disk_io only).
    #[inline]
    pub fn mmap_file_size_cutoff(&mut self, value: i32) -> &mut Self {
        self.set_int(
            SettingKey::from_generated(sys::CT_SET_MMAP_FILE_SIZE_CUTOFF),
            value,
        );
        self
    }

    /// Reads `mmap_file_size_cutoff` if set in this pack.
    #[inline]
    pub fn get_mmap_file_size_cutoff(&self) -> Option<i32> {
        self.get_int(SettingKey::from_generated(
            sys::CT_SET_MMAP_FILE_SIZE_CUTOFF,
        ))
    }
}
