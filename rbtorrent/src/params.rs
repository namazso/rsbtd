// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! [`AddTorrentParams`]: everything needed to add a torrent to a session,
//! obtained by loading a .torrent file, parsing a magnet link, restoring
//! resume data, or built up field by field.

use std::borrow::Cow;
use std::net::SocketAddr;
use std::path::Path;
use std::ptr::NonNull;

use libctorrent_sys as sys;

use crate::error::{Error, Result, with_error};
use crate::info::TorrentInfo;
use crate::types::{InfoHash, Sha256Hash, socket_addr_from_ct, socket_addr_to_ct};
use crate::util::{path_bytes, span_to_slice, str_view, take_ct_str, view_to_cow};

/// Flags controlling a torrent's behavior and how it is added (`lt::torrent_flags_t`).
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct TorrentFlags(u64);

#[allow(missing_docs)]
impl TorrentFlags {
    pub const SEED_MODE: Self = Self(sys::CT_TORRENT_FLAG_SEED_MODE as u64);
    pub const UPLOAD_MODE: Self = Self(sys::CT_TORRENT_FLAG_UPLOAD_MODE as u64);
    pub const SHARE_MODE: Self = Self(sys::CT_TORRENT_FLAG_SHARE_MODE as u64);
    pub const APPLY_IP_FILTER: Self = Self(sys::CT_TORRENT_FLAG_APPLY_IP_FILTER as u64);
    pub const PAUSED: Self = Self(sys::CT_TORRENT_FLAG_PAUSED as u64);
    pub const AUTO_MANAGED: Self = Self(sys::CT_TORRENT_FLAG_AUTO_MANAGED as u64);
    pub const DUPLICATE_IS_ERROR: Self = Self(sys::CT_TORRENT_FLAG_DUPLICATE_IS_ERROR as u64);
    pub const UPDATE_SUBSCRIBE: Self = Self(sys::CT_TORRENT_FLAG_UPDATE_SUBSCRIBE as u64);
    pub const SUPER_SEEDING: Self = Self(sys::CT_TORRENT_FLAG_SUPER_SEEDING as u64);
    pub const SEQUENTIAL_DOWNLOAD: Self = Self(sys::CT_TORRENT_FLAG_SEQUENTIAL_DOWNLOAD as u64);
    pub const STOP_WHEN_READY: Self = Self(sys::CT_TORRENT_FLAG_STOP_WHEN_READY as u64);
    pub const NEED_SAVE_RESUME: Self = Self(sys::CT_TORRENT_FLAG_NEED_SAVE_RESUME as u64);
    pub const DISABLE_DHT: Self = Self(sys::CT_TORRENT_FLAG_DISABLE_DHT as u64);
    pub const DISABLE_LSD: Self = Self(sys::CT_TORRENT_FLAG_DISABLE_LSD as u64);
    pub const DISABLE_PEX: Self = Self(sys::CT_TORRENT_FLAG_DISABLE_PEX as u64);
    pub const NO_VERIFY_FILES: Self = Self(sys::CT_TORRENT_FLAG_NO_VERIFY_FILES as u64);
    pub const DEFAULT_DONT_DOWNLOAD: Self = Self(sys::CT_TORRENT_FLAG_DEFAULT_DONT_DOWNLOAD as u64);
    pub const I2P_TORRENT: Self = Self(sys::CT_TORRENT_FLAG_I2P_TORRENT as u64);
    pub const DISABLE_V1_HASHES: Self = Self(sys::CT_TORRENT_FLAG_DISABLE_V1_HASHES as u64);
    // Not sys::CT_TORRENT_FLAGS_ALL: bindgen folds that macro to `-1i32`.
    pub const ALL: Self = Self(u64::MAX);

    /// The flags a fresh [`AddTorrentParams`] starts with.
    pub const DEFAULT: Self = Self(sys::CT_TORRENT_FLAGS_DEFAULT as u64);

    pub const fn empty() -> Self {
        Self(0)
    }

    pub const fn bits(self) -> u64 {
        self.0
    }

    pub const fn from_bits(bits: u64) -> Self {
        Self(bits)
    }

    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    pub const fn intersects(self, other: Self) -> bool {
        self.0 & other.0 != 0
    }
}

impl std::ops::BitOr for TorrentFlags {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}

impl std::ops::BitOrAssign for TorrentFlags {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

impl std::ops::BitAnd for TorrentFlags {
    type Output = Self;
    fn bitand(self, rhs: Self) -> Self {
        Self(self.0 & rhs.0)
    }
}

impl std::ops::BitAndAssign for TorrentFlags {
    fn bitand_assign(&mut self, rhs: Self) {
        self.0 &= rhs.0;
    }
}

impl std::ops::Not for TorrentFlags {
    type Output = Self;
    fn not(self) -> Self {
        Self(!self.0)
    }
}

impl std::fmt::Debug for TorrentFlags {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "TorrentFlags({:#x})", self.0)
    }
}

/// How storage is allocated for a torrent (`lt::storage_mode_t`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum StorageMode {
    /// All files are pre-allocated up front.
    Allocate,
    /// Files are allocated sparsely, growing as data arrives (default).
    #[default]
    Sparse,
}

/// A file or piece download priority (`lt::download_priority_t`): 0
/// disables downloading, 1 is the lowest and 7 the highest priority.
/// Values outside 0..=7 are unrepresentable (libtorrent only asserts the
/// range).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct DownloadPriority(u8);

impl DownloadPriority {
    pub const DONT_DOWNLOAD: Self = Self(0);
    pub const LOW: Self = Self(1);
    pub const DEFAULT: Self = Self(4);
    pub const TOP: Self = Self(7);

    /// A priority from its raw value; `None` outside 0..=7.
    pub const fn new(value: u8) -> Option<Self> {
        if value <= Self::TOP.0 {
            Some(Self(value))
        } else {
            None
        }
    }

    /// The raw priority value (0..=7).
    pub const fn value(self) -> u8 {
        self.0
    }
}

impl TryFrom<u8> for DownloadPriority {
    type Error = Error;

    fn try_from(value: u8) -> Result<Self> {
        Self::new(value).ok_or_else(|| Error::binding("download priority is outside 0..=7"))
    }
}

impl From<DownloadPriority> for u8 {
    fn from(priority: DownloadPriority) -> u8 {
        priority.0
    }
}

impl Default for DownloadPriority {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// Parser limits protecting against maliciously crafted .torrent data
/// (`lt::load_torrent_limits`).
#[derive(Clone, Copy, Debug)]
pub struct LoadTorrentLimits {
    /// The maximum size of a .torrent file to load into RAM.
    pub max_buffer_size: i32,
    /// The maximum number of pieces allowed in the torrent.
    pub max_pieces: i32,
    /// The maximum recursion depth in the bdecoded structure.
    pub max_decode_depth: i32,
    /// The maximum number of bdecode tokens.
    pub max_decode_tokens: i32,
    /// The maximum number of files sharing the same filename.
    pub max_duplicate_filenames: i32,
    /// The maximum depth of the directory structure.
    pub max_directory_depth: i32,
}

impl Default for LoadTorrentLimits {
    fn default() -> Self {
        // SAFETY: trivial constant accessor.
        let d = unsafe { sys::ct_load_torrent_limits_default() };
        LoadTorrentLimits {
            max_buffer_size: d.max_buffer_size,
            max_pieces: d.max_pieces,
            max_decode_depth: d.max_decode_depth,
            max_decode_tokens: d.max_decode_tokens,
            max_duplicate_filenames: d.max_duplicate_filenames,
            max_directory_depth: d.max_directory_depth,
        }
    }
}

impl LoadTorrentLimits {
    pub(crate) fn to_ct(self) -> sys::ct_load_torrent_limits {
        sys::ct_load_torrent_limits {
            max_buffer_size: self.max_buffer_size,
            max_pieces: self.max_pieces,
            max_decode_depth: self.max_decode_depth,
            max_decode_tokens: self.max_decode_tokens,
            max_duplicate_filenames: self.max_duplicate_filenames,
            max_directory_depth: self.max_directory_depth,
        }
    }
}

/// The parameters for adding a torrent to a session
/// (`lt::add_torrent_params`); also the representation of a parsed
/// .torrent file, magnet link, or resume data. To add a torrent, set at
/// least [`set_save_path`](Self::set_save_path) and either
/// [`set_ti`](Self::set_ti) or [`set_info_hashes`](Self::set_info_hashes).
///
/// String and slice getters borrow from this object and are invalidated
/// by mutation (borrow-enforced). Setters returning `&mut Self` are
/// infallible: their only failure mode is allocation, which aborts.
pub struct AddTorrentParams {
    ptr: NonNull<sys::ct_add_torrent_params>,
}

// SAFETY: owns its heap object; all &self accessors are const on the C++
// side and &mut self is required for mutation.
unsafe impl Send for AddTorrentParams {}
unsafe impl Sync for AddTorrentParams {}

macro_rules! atp_scalar {
    ($(#[$doc:meta])* $ty:ty, $getter:ident, $ct_getter:ident, $setter:ident, $ct_setter:ident) => {
        $(#[$doc])*
        pub fn $getter(&self) -> $ty {
            // SAFETY: self.ptr is a valid handle.
            unsafe { sys::$ct_getter(self.as_ptr()) }
        }

        pub fn $setter(&mut self, value: $ty) -> &mut Self {
            // SAFETY: self.ptr is a valid handle.
            unsafe { sys::$ct_setter(self.ptr.as_ptr(), value) };
            self
        }
    };
}

macro_rules! atp_string {
    ($(#[$doc:meta])* $getter:ident, $ct_getter:ident, $setter:ident, $ct_setter:ident) => {
        $(#[$doc])*
        pub fn $getter(&self) -> Cow<'_, str> {
            // SAFETY: the view borrows the params object for '_.
            unsafe { view_to_cow(sys::$ct_getter(self.as_ptr())) }
        }

        pub fn $setter(&mut self, value: &str) -> &mut Self {
            // SAFETY: the view is only read during the call.
            unsafe { sys::$ct_setter(self.ptr.as_ptr(), str_view(value)) };
            self
        }
    };
}

impl AddTorrentParams {
    /// Empty params with libtorrent's default flags.
    pub fn new() -> AddTorrentParams {
        // SAFETY: ct_atp_new has no preconditions.
        let ptr = unsafe { sys::ct_atp_new() };
        AddTorrentParams {
            ptr: NonNull::new(ptr).expect("out of memory"),
        }
    }

    /// Loads and parses a .torrent file.
    pub fn from_torrent_file(path: impl AsRef<Path>) -> Result<AddTorrentParams> {
        Self::from_torrent_file_with_limits(path, &LoadTorrentLimits::default())
    }

    /// Loads and parses a .torrent file with explicit parser limits.
    pub fn from_torrent_file_with_limits(
        path: impl AsRef<Path>,
        limits: &LoadTorrentLimits,
    ) -> Result<AddTorrentParams> {
        let path = path_bytes(path.as_ref())?;
        let view = sys::ct_str_view {
            ptr: path.as_ptr().cast(),
            len: path.len(),
        };
        let limits = limits.to_ct();
        // SAFETY: the path view and limits are only read during the call.
        let ptr = with_error(|err| unsafe { sys::ct_load_torrent_file(view, &limits, err) })?;
        // SAFETY: on success the shim returns an owned handle.
        Ok(unsafe { Self::from_owned_ptr(ptr) })
    }

    /// Parses a bencoded .torrent buffer.
    pub fn from_torrent_buffer(buffer: &[u8]) -> Result<AddTorrentParams> {
        Self::from_torrent_buffer_with_limits(buffer, &LoadTorrentLimits::default())
    }

    /// Buffers larger than `limits.max_buffer_size` are rejected.
    pub fn from_torrent_buffer_with_limits(
        buffer: &[u8],
        limits: &LoadTorrentLimits,
    ) -> Result<AddTorrentParams> {
        let within_limit =
            usize::try_from(limits.max_buffer_size).is_ok_and(|max| buffer.len() <= max);
        if !within_limit {
            return Err(Error::binding(
                "torrent buffer exceeds the max_buffer_size limit",
            ));
        }
        let span = sys::ct_span {
            ptr: buffer.as_ptr(),
            len: buffer.len(),
        };
        let limits = limits.to_ct();
        // SAFETY: the span and limits are only read during the call.
        let ptr = with_error(|err| unsafe { sys::ct_load_torrent_buffer(span, &limits, err) })?;
        // SAFETY: on success the shim returns an owned handle.
        Ok(unsafe { Self::from_owned_ptr(ptr) })
    }

    /// Parses a `magnet:` URI.
    pub fn from_magnet_uri(uri: &str) -> Result<AddTorrentParams> {
        // SAFETY: the view is only read during the call.
        let ptr = with_error(|err| unsafe { sys::ct_parse_magnet_uri(str_view(uri), err) })?;
        // SAFETY: on success the shim returns an owned handle.
        Ok(unsafe { Self::from_owned_ptr(ptr) })
    }

    /// Generates a magnet URI from these params (from `ti`/`info_hashes`,
    /// `trackers`, `name`, `url_seeds`, `dht_nodes`, `file_priorities` and
    /// `peers`). Fails if no info-hash is available.
    pub fn make_magnet_uri(&self) -> Result<String> {
        // SAFETY: self.ptr is a valid handle.
        let s = with_error(|err| unsafe { sys::ct_make_magnet_uri(self.as_ptr(), err) })?;
        let uri = take_ct_str(s);
        if uri.is_empty() {
            return Err(Error::binding(
                "cannot make a magnet URI without an info-hash",
            ));
        }
        Ok(uri)
    }

    /// SAFETY: `ptr` must be an owned, non-null handle.
    pub(crate) unsafe fn from_owned_ptr(ptr: *mut sys::ct_add_torrent_params) -> Self {
        AddTorrentParams {
            ptr: NonNull::new(ptr).expect("shim returned success with NULL"),
        }
    }

    pub(crate) fn as_ptr(&self) -> *const sys::ct_add_torrent_params {
        self.ptr.as_ptr()
    }

    // ---- torrent metadata -------------------------------------------------

    /// The torrent metadata, if present. The returned handle shares the
    /// underlying object.
    pub fn ti(&self) -> Option<TorrentInfo> {
        // SAFETY: the shim returns an owned handle or NULL.
        unsafe { TorrentInfo::from_owned_ptr(sys::ct_atp_get_ti(self.as_ptr())) }
    }

    /// Sets the torrent metadata (shares, does not copy).
    pub fn set_ti(&mut self, ti: &TorrentInfo) -> &mut Self {
        // SAFETY: both handles are valid; the shim copies the shared_ptr.
        unsafe { sys::ct_atp_set_ti(self.ptr.as_ptr(), ti.as_ptr()) };
        self
    }

    pub fn clear_ti(&mut self) -> &mut Self {
        // SAFETY: NULL clears the field.
        unsafe { sys::ct_atp_set_ti(self.ptr.as_ptr(), std::ptr::null()) };
        self
    }

    /// The `add_torrent_params` version, for forward binary compatibility.
    pub fn version(&self) -> i32 {
        // SAFETY: self.ptr is a valid handle.
        unsafe { sys::ct_atp_version(self.as_ptr()) }
    }

    // ---- strings ------------------------------------------------------------

    atp_string!(
        /// The name of the torrent, used when the metadata isn't available.
        name, ct_atp_name, set_name, ct_atp_set_name
    );
    atp_string!(
        /// The directory the torrent is (or will be) stored in.
        save_path, ct_atp_save_path, set_save_path, ct_atp_set_save_path
    );
    atp_string!(
        /// Optional subdirectory (relative to the save path) for the part
        /// file holding pieces of zero-priority files.
        part_file_dir, ct_atp_part_file_dir,
        set_part_file_dir, ct_atp_set_part_file_dir
    );
    atp_string!(
        /// The default tracker id sent when announcing.
        trackerid, ct_atp_trackerid, set_trackerid, ct_atp_set_trackerid
    );
    atp_string!(
        /// The comment from the .torrent file.
        comment, ct_atp_comment, set_comment, ct_atp_set_comment
    );
    atp_string!(
        /// The "created by" string from the .torrent file.
        created_by, ct_atp_created_by, set_created_by, ct_atp_set_created_by
    );
    atp_string!(
        /// The root certificate for SSL torrents added by info-hash only.
        root_certificate, ct_atp_root_certificate,
        set_root_certificate, ct_atp_set_root_certificate
    );

    // ---- trackers --------------------------------------------------------------

    /// The tracker URLs with their tiers. Trackers without an explicit
    /// tier entry inherit the previous tracker's tier (or 0).
    pub fn trackers(&self) -> impl Iterator<Item = (Cow<'_, str>, i32)> + '_ {
        // SAFETY: counts and views are valid while &self is borrowed.
        unsafe {
            let num = sys::ct_atp_num_trackers(self.as_ptr());
            let num_tiers = sys::ct_atp_num_tracker_tiers(self.as_ptr());
            let mut tier = 0;
            (0..num).map(move |i| {
                if i < num_tiers {
                    tier = sys::ct_atp_tracker_tier(self.as_ptr(), i);
                }
                (view_to_cow(sys::ct_atp_tracker(self.as_ptr(), i)), tier)
            })
        }
    }

    pub fn add_tracker(&mut self, url: &str, tier: i32) -> &mut Self {
        // SAFETY: the view is only read during the call.
        unsafe { sys::ct_atp_add_tracker(self.ptr.as_ptr(), str_view(url), tier) };
        self
    }

    pub fn clear_trackers(&mut self) -> &mut Self {
        // SAFETY: self.ptr is a valid handle.
        unsafe { sys::ct_atp_clear_trackers(self.ptr.as_ptr()) };
        self
    }

    // ---- DHT nodes --------------------------------------------------------------

    /// DHT nodes (hostname, port) to add to the session when the torrent
    /// is added.
    pub fn dht_nodes(&self) -> Vec<(String, u16)> {
        // SAFETY: indices bounds-checked; views read before any mutation.
        unsafe {
            let num = sys::ct_atp_num_dht_nodes(self.as_ptr());
            (0..num)
                .filter_map(|i| {
                    let mut host = sys::ct_str_view {
                        ptr: std::ptr::null(),
                        len: 0,
                    };
                    let mut port = 0i32;
                    sys::ct_atp_dht_node(self.as_ptr(), i, &mut host, &mut port).then(|| {
                        (
                            view_to_cow(host).into_owned(),
                            u16::try_from(port).unwrap_or(0),
                        )
                    })
                })
                .collect()
        }
    }

    pub fn add_dht_node(&mut self, host: &str, port: u16) -> &mut Self {
        // SAFETY: the view is only read during the call.
        unsafe { sys::ct_atp_add_dht_node(self.ptr.as_ptr(), str_view(host), i32::from(port)) };
        self
    }

    pub fn clear_dht_nodes(&mut self) -> &mut Self {
        // SAFETY: self.ptr is a valid handle.
        unsafe { sys::ct_atp_clear_dht_nodes(self.ptr.as_ptr()) };
        self
    }

    // ---- web seeds --------------------------------------------------------------

    /// HTTP web seed URLs (BEP 19).
    pub fn url_seeds(&self) -> impl ExactSizeIterator<Item = Cow<'_, str>> + '_ {
        // SAFETY: views are valid while &self is borrowed.
        unsafe {
            let num = sys::ct_atp_num_url_seeds(self.as_ptr());
            (0..num).map(move |i| view_to_cow(sys::ct_atp_url_seed(self.as_ptr(), i)))
        }
    }

    pub fn add_url_seed(&mut self, url: &str) -> &mut Self {
        // SAFETY: the view is only read during the call.
        unsafe { sys::ct_atp_add_url_seed(self.ptr.as_ptr(), str_view(url)) };
        self
    }

    pub fn clear_url_seeds(&mut self) -> &mut Self {
        // SAFETY: self.ptr is a valid handle.
        unsafe { sys::ct_atp_clear_url_seeds(self.ptr.as_ptr()) };
        self
    }

    // ---- storage mode / flags / info-hash ------------------------------------------

    pub fn storage_mode(&self) -> StorageMode {
        // SAFETY: self.ptr is a valid handle.
        let mode = unsafe { sys::ct_atp_storage_mode(self.as_ptr()) };
        if mode == sys::CT_STORAGE_MODE_ALLOCATE as i32 {
            StorageMode::Allocate
        } else {
            StorageMode::Sparse
        }
    }

    pub fn set_storage_mode(&mut self, mode: StorageMode) -> &mut Self {
        let raw = match mode {
            StorageMode::Allocate => sys::CT_STORAGE_MODE_ALLOCATE as i32,
            StorageMode::Sparse => sys::CT_STORAGE_MODE_SPARSE as i32,
        };
        // SAFETY: self.ptr is a valid handle.
        unsafe { sys::ct_atp_set_storage_mode(self.ptr.as_ptr(), raw) };
        self
    }

    /// Flags controlling the torrent. Starts as [`TorrentFlags::DEFAULT`];
    /// prefer ORing flags in (or ANDing them out) over replacing the set.
    pub fn flags(&self) -> TorrentFlags {
        // SAFETY: self.ptr is a valid handle.
        TorrentFlags(unsafe { sys::ct_atp_flags(self.as_ptr()) })
    }

    pub fn set_flags(&mut self, flags: TorrentFlags) -> &mut Self {
        // SAFETY: self.ptr is a valid handle.
        unsafe { sys::ct_atp_set_flags(self.ptr.as_ptr(), flags.bits()) };
        self
    }

    /// The torrent's info-hash(es), for adding by hash alone (magnet
    /// style). Ignored when [`ti`](Self::ti) is set.
    pub fn info_hashes(&self) -> InfoHash {
        // SAFETY: self.ptr is a valid handle.
        let h = unsafe { sys::ct_atp_info_hashes(self.as_ptr()) };
        InfoHash::from_ct(h)
    }

    pub fn set_info_hashes(&mut self, value: InfoHash) -> &mut Self {
        let h = value.to_ct();
        // SAFETY: h is only read during the call.
        unsafe { sys::ct_atp_set_info_hashes(self.ptr.as_ptr(), &h) };
        self
    }

    // ---- limits and counters ------------------------------------------------------

    atp_scalar!(
        /// Maximum number of unchoked peers (-1 = unlimited).
        i32, max_uploads, ct_atp_max_uploads,
        set_max_uploads, ct_atp_set_max_uploads
    );
    atp_scalar!(
        /// Maximum number of peer connections (-1 = unlimited).
        i32, max_connections, ct_atp_max_connections,
        set_max_connections, ct_atp_set_max_connections
    );
    atp_scalar!(
        /// Upload rate limit in bytes/s (-1 = unlimited).
        i32, upload_limit, ct_atp_upload_limit,
        set_upload_limit, ct_atp_set_upload_limit
    );
    atp_scalar!(
        /// Download rate limit in bytes/s (-1 = unlimited).
        i32, download_limit, ct_atp_download_limit,
        set_download_limit, ct_atp_set_download_limit
    );
    atp_scalar!(
        /// Cached scrape data: seeds in the swarm (-1 = unknown).
        i32, num_complete, ct_atp_num_complete,
        set_num_complete, ct_atp_set_num_complete
    );
    atp_scalar!(
        /// Cached scrape data: non-seed peers in the swarm (-1 = unknown).
        i32, num_incomplete, ct_atp_num_incomplete,
        set_num_incomplete, ct_atp_set_num_incomplete
    );
    atp_scalar!(
        /// Cached scrape data: completed downloads (-1 = unknown).
        i32, num_downloaded, ct_atp_num_downloaded,
        set_num_downloaded, ct_atp_set_num_downloaded
    );

    // ---- resume statistics -----------------------------------------------------------

    atp_scalar!(
        /// Total bytes uploaded by this torrent so far.
        i64, total_uploaded, ct_atp_total_uploaded,
        set_total_uploaded, ct_atp_set_total_uploaded
    );
    atp_scalar!(
        /// Total bytes downloaded by this torrent so far.
        i64, total_downloaded, ct_atp_total_downloaded,
        set_total_downloaded, ct_atp_set_total_downloaded
    );
    atp_scalar!(
        /// Seconds this torrent has been started.
        i32, active_time, ct_atp_active_time,
        set_active_time, ct_atp_set_active_time
    );
    atp_scalar!(
        /// Seconds this torrent has been finished.
        i32, finished_time, ct_atp_finished_time,
        set_finished_time, ct_atp_set_finished_time
    );
    atp_scalar!(
        /// Seconds this torrent has been seeding.
        i32, seeding_time, ct_atp_seeding_time,
        set_seeding_time, ct_atp_set_seeding_time
    );
    atp_scalar!(
        /// Posix time the torrent was first added (0 = now).
        i64, added_time, ct_atp_added_time,
        set_added_time, ct_atp_set_added_time
    );
    atp_scalar!(
        /// Posix time the torrent finished downloading (0 = unknown).
        i64, completed_time, ct_atp_completed_time,
        set_completed_time, ct_atp_set_completed_time
    );
    atp_scalar!(
        /// Posix time a complete copy was last seen in the swarm (0 = unknown).
        i64, last_seen_complete, ct_atp_last_seen_complete,
        set_last_seen_complete, ct_atp_set_last_seen_complete
    );
    atp_scalar!(
        /// Posix time payload was last received (0 = unknown).
        i64, last_download, ct_atp_last_download,
        set_last_download, ct_atp_set_last_download
    );
    atp_scalar!(
        /// Posix time payload was last sent (0 = unknown).
        i64, last_upload, ct_atp_last_upload,
        set_last_upload, ct_atp_set_last_upload
    );
    atp_scalar!(
        /// Posix creation date from the .torrent file (0 = unknown).
        i64, creation_date, ct_atp_creation_date,
        set_creation_date, ct_atp_set_creation_date
    );

    // ---- priorities ----------------------------------------------------------------------

    /// Per-file download priorities; files beyond the end of the slice get the
    /// default priority.
    pub fn file_priorities(&self) -> &[DownloadPriority] {
        // SAFETY: the span borrows the params object for '_;
        // DownloadPriority is repr(transparent) over u8.
        unsafe {
            let span = sys::ct_atp_file_priorities(self.as_ptr());
            if span.ptr.is_null() {
                return &[];
            }
            std::slice::from_raw_parts(span.ptr.cast(), span.len)
        }
    }

    pub fn set_file_priorities(&mut self, priorities: &[DownloadPriority]) -> &mut Self {
        // SAFETY: repr(transparent) cast; the slice is only read during
        // the call.
        unsafe {
            sys::ct_atp_set_file_priorities(
                self.ptr.as_ptr(),
                priorities.as_ptr().cast(),
                priorities.len(),
            )
        };
        self
    }

    /// Per-piece download priorities (file priorities take precedence).
    pub fn piece_priorities(&self) -> &[DownloadPriority] {
        // SAFETY: as file_priorities.
        unsafe {
            let span = sys::ct_atp_piece_priorities(self.as_ptr());
            if span.ptr.is_null() {
                return &[];
            }
            std::slice::from_raw_parts(span.ptr.cast(), span.len)
        }
    }

    pub fn set_piece_priorities(&mut self, priorities: &[DownloadPriority]) -> &mut Self {
        // SAFETY: as set_file_priorities.
        unsafe {
            sys::ct_atp_set_piece_priorities(
                self.ptr.as_ptr(),
                priorities.as_ptr().cast(),
                priorities.len(),
            )
        };
        self
    }

    // ---- piece state (resume data) ----------------------------------------------------------

    /// Which pieces we already have (resume data).
    pub fn have_pieces(&self) -> Vec<bool> {
        // SAFETY: the view borrows the params object during the call.
        bitfield_to_bools(unsafe { sys::ct_atp_have_pieces(self.as_ptr()) })
    }

    pub fn set_have_pieces(&mut self, pieces: &[bool]) -> &mut Self {
        let (bytes, bits) = bools_to_bitfield(pieces);
        // SAFETY: the byte buffer is only read during the call.
        unsafe { sys::ct_atp_set_have_pieces(self.ptr.as_ptr(), bytes.as_ptr(), bits) };
        self
    }

    /// Which pieces have been verified, when in seed mode (resume data).
    pub fn verified_pieces(&self) -> Vec<bool> {
        // SAFETY: the view borrows the params object during the call.
        bitfield_to_bools(unsafe { sys::ct_atp_verified_pieces(self.as_ptr()) })
    }

    pub fn set_verified_pieces(&mut self, pieces: &[bool]) -> &mut Self {
        let (bytes, bits) = bools_to_bitfield(pieces);
        // SAFETY: the byte buffer is only read during the call.
        unsafe { sys::ct_atp_set_verified_pieces(self.ptr.as_ptr(), bytes.as_ptr(), bits) };
        self
    }

    /// Partially downloaded pieces: `(piece index, one bool per 16 kiB block)`,
    /// in piece-index order (resume data).
    pub fn unfinished_pieces(&self) -> Vec<(i32, Vec<bool>)> {
        // SAFETY: indices bounds-checked; views converted before any mutation.
        unsafe {
            let num = sys::ct_atp_num_unfinished_pieces(self.as_ptr());
            (0..num)
                .filter_map(|i| {
                    let mut piece = 0i32;
                    let mut blocks = sys::ct_bitfield_view {
                        bytes: std::ptr::null(),
                        num_bits: 0,
                    };
                    sys::ct_atp_unfinished_piece(self.as_ptr(), i, &mut piece, &mut blocks)
                        .then(|| (piece, bitfield_to_bools(blocks)))
                })
                .collect()
        }
    }

    pub fn add_unfinished_piece(&mut self, piece: i32, blocks: &[bool]) -> &mut Self {
        let (bytes, bits) = bools_to_bitfield(blocks);
        // SAFETY: the byte buffer is only read during the call.
        unsafe { sys::ct_atp_add_unfinished_piece(self.ptr.as_ptr(), piece, bytes.as_ptr(), bits) };
        self
    }

    pub fn clear_unfinished_pieces(&mut self) -> &mut Self {
        // SAFETY: self.ptr is a valid handle.
        unsafe { sys::ct_atp_clear_unfinished_pieces(self.ptr.as_ptr()) };
        self
    }

    // ---- v2 merkle trees -----------------------------------------------------------------------

    /// The number of per-file merkle tree slots (0 or the torrent's file count).
    pub fn num_merkle_trees(&self) -> usize {
        // SAFETY: self.ptr is a valid handle.
        unsafe { sys::ct_atp_num_merkle_trees(self.as_ptr()) }
    }

    /// The known merkle tree hashes for a file (v2 torrents; see
    /// [`merkle_tree_mask`](Self::merkle_tree_mask) for which nodes they are).
    pub fn merkle_tree(&self, file: i32) -> Vec<Sha256Hash> {
        // SAFETY: the span borrows the params object during the call.
        let bytes = unsafe { span_to_slice(sys::ct_atp_merkle_tree(self.as_ptr(), file)) };
        bytes
            .chunks_exact(32)
            .map(|c| Sha256Hash(c.try_into().unwrap()))
            .collect()
    }

    /// Which nodes of the full merkle tree the [`merkle_tree`](Self::merkle_tree)
    /// hashes correspond to. Empty means the hashes are the full tree.
    pub fn merkle_tree_mask(&self, file: i32) -> Vec<bool> {
        // SAFETY: the view borrows the params object during the call.
        bitfield_to_bools(unsafe { sys::ct_atp_merkle_tree_mask(self.as_ptr(), file) })
    }

    /// Which v2 leaf hashes have been verified against the root. Empty
    /// with a non-empty tree means all hashes are verified.
    pub fn verified_leaf_hashes(&self, file: i32) -> Vec<bool> {
        // SAFETY: the view borrows the params object during the call.
        bitfield_to_bools(unsafe { sys::ct_atp_verified_leaf_hashes(self.as_ptr(), file) })
    }

    /// Resizes the merkle tree, mask and verified-leaf vectors to `num` files.
    pub fn set_num_merkle_trees(&mut self, num: usize) -> &mut Self {
        // SAFETY: self.ptr is a valid handle.
        unsafe { sys::ct_atp_set_num_merkle_trees(self.ptr.as_ptr(), num) };
        self
    }

    pub fn set_merkle_tree(&mut self, file: i32, hashes: &[Sha256Hash]) -> &mut Self {
        let bytes: Vec<u8> = hashes.iter().flat_map(|h| h.0).collect();
        let span = sys::ct_span {
            ptr: bytes.as_ptr(),
            len: bytes.len(),
        };
        // SAFETY: the span is only read during the call.
        unsafe { sys::ct_atp_set_merkle_tree(self.ptr.as_ptr(), file, span) };
        self
    }

    pub fn set_merkle_tree_mask(&mut self, file: i32, mask: &[bool]) -> &mut Self {
        let (bytes, bits) = bools_to_bitfield(mask);
        // SAFETY: the byte buffer is only read during the call.
        unsafe { sys::ct_atp_set_merkle_tree_mask(self.ptr.as_ptr(), file, bytes.as_ptr(), bits) };
        self
    }

    pub fn set_verified_leaf_hashes(&mut self, file: i32, verified: &[bool]) -> &mut Self {
        let (bytes, bits) = bools_to_bitfield(verified);
        // SAFETY: the byte buffer is only read during the call.
        unsafe {
            sys::ct_atp_set_verified_leaf_hashes(self.ptr.as_ptr(), file, bytes.as_ptr(), bits)
        };
        self
    }

    // ---- renamed files ---------------------------------------------------------------------------

    /// File renames applied when the torrent is added, as `(file index, new name)`
    /// in file-index order.
    pub fn renamed_files(&self) -> Vec<(i32, String)> {
        // SAFETY: indices bounds-checked; views converted before any mutation.
        unsafe {
            let num = sys::ct_atp_num_renamed_files(self.as_ptr());
            (0..num)
                .filter_map(|i| {
                    let mut file = 0i32;
                    let mut name = sys::ct_str_view {
                        ptr: std::ptr::null(),
                        len: 0,
                    };
                    sys::ct_atp_renamed_file(self.as_ptr(), i, &mut file, &mut name)
                        .then(|| (file, view_to_cow(name).into_owned()))
                })
                .collect()
        }
    }

    /// Adds a file rename. The index must be non-negative and, when
    /// metadata is present, within the torrent's file count.
    pub fn add_renamed_file(&mut self, file: i32, name: &str) -> Result<&mut Self> {
        if file < 0 {
            return Err(Error::binding(&format!("file index {file} is negative")));
        }
        if let Some(ti) = self.ti() {
            let count = ti.num_files();
            if file >= count {
                return Err(Error::binding(&format!(
                    "file index {file} is outside 0..{count}"
                )));
            }
        }
        // SAFETY: the view is only read during the call.
        unsafe { sys::ct_atp_add_renamed_file(self.ptr.as_ptr(), file, str_view(name)) };
        Ok(self)
    }

    pub fn clear_renamed_files(&mut self) -> &mut Self {
        // SAFETY: self.ptr is a valid handle.
        unsafe { sys::ct_atp_clear_renamed_files(self.ptr.as_ptr()) };
        self
    }

    // ---- peers ------------------------------------------------------------------------------------

    /// Peers to try connecting to when the torrent is added.
    pub fn peers(&self) -> Vec<SocketAddr> {
        // SAFETY: indices are bounds-checked by the shim.
        unsafe {
            let num = sys::ct_atp_num_peers(self.as_ptr());
            (0..num)
                .map(|i| socket_addr_from_ct(&sys::ct_atp_peer(self.as_ptr(), i)))
                .collect()
        }
    }

    pub fn add_peer(&mut self, peer: SocketAddr) -> &mut Self {
        let ep = socket_addr_to_ct(peer);
        // SAFETY: ep is only read during the call.
        unsafe { sys::ct_atp_add_peer(self.ptr.as_ptr(), &ep) };
        self
    }

    pub fn clear_peers(&mut self) -> &mut Self {
        // SAFETY: self.ptr is a valid handle.
        unsafe { sys::ct_atp_clear_peers(self.ptr.as_ptr()) };
        self
    }

    /// Peers banned from this torrent.
    pub fn banned_peers(&self) -> Vec<SocketAddr> {
        // SAFETY: indices are bounds-checked by the shim.
        unsafe {
            let num = sys::ct_atp_num_banned_peers(self.as_ptr());
            (0..num)
                .map(|i| socket_addr_from_ct(&sys::ct_atp_banned_peer(self.as_ptr(), i)))
                .collect()
        }
    }

    pub fn add_banned_peer(&mut self, peer: SocketAddr) -> &mut Self {
        let ep = socket_addr_to_ct(peer);
        // SAFETY: ep is only read during the call.
        unsafe { sys::ct_atp_add_banned_peer(self.ptr.as_ptr(), &ep) };
        self
    }

    pub fn clear_banned_peers(&mut self) -> &mut Self {
        // SAFETY: self.ptr is a valid handle.
        unsafe { sys::ct_atp_clear_banned_peers(self.ptr.as_ptr()) };
        self
    }
}

impl Default for AddTorrentParams {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for AddTorrentParams {
    fn clone(&self) -> Self {
        // SAFETY: self.ptr is a valid handle.
        let ptr = unsafe { sys::ct_atp_clone(self.as_ptr()) };
        AddTorrentParams {
            ptr: NonNull::new(ptr).expect("out of memory"),
        }
    }
}

impl Drop for AddTorrentParams {
    fn drop(&mut self) {
        // SAFETY: self.ptr is owned and dropped exactly once.
        unsafe { sys::ct_atp_free(self.ptr.as_ptr()) }
    }
}

impl std::fmt::Debug for AddTorrentParams {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AddTorrentParams")
            .field("name", &self.name())
            .field("info_hashes", &self.info_hashes())
            .field("save_path", &self.save_path())
            .field("flags", &self.flags())
            .finish_non_exhaustive()
    }
}

/// Unpacks an lt::bitfield view (MSB-first within bytes) into bools.
fn bitfield_to_bools(view: sys::ct_bitfield_view) -> Vec<bool> {
    if view.bytes.is_null() || view.num_bits <= 0 {
        return Vec::new();
    }
    let num_bits = view.num_bits as usize;
    // SAFETY: the shim guarantees bytes covers ceil(num_bits / 8) bytes.
    let bytes = unsafe { std::slice::from_raw_parts(view.bytes, num_bits.div_ceil(8)) };
    (0..num_bits)
        .map(|i| bytes[i / 8] & (0x80 >> (i % 8)) != 0)
        .collect()
}

/// Packs bools into lt::bitfield byte layout (MSB-first within bytes).
fn bools_to_bitfield(bits: &[bool]) -> (Vec<u8>, i32) {
    let mut bytes = vec![0u8; bits.len().div_ceil(8)];
    for (i, &bit) in bits.iter().enumerate() {
        if bit {
            bytes[i / 8] |= 0x80 >> (i % 8);
        }
    }
    (bytes, bits.len() as i32)
}
