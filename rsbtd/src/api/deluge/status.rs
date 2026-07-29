// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Torrent-status construction, `filter_dict` matching and the sidebar
//! filter tree, with the read-only methods built on them.
//!
//! Every built-in status key is served; those without engine backing
//! read as their Deluge defaults, and unknown requested keys are skipped
//! silently. The `diff` parameter is ignored: a full status is a safe
//! superset for clients that merge diffs. `web.update_ui` omits its
//! `free_space` and `external_ip` stats, and its rate stats sum
//! per-torrent rates rather than sampling session counters — close, but
//! blind to traffic like DHT chatter.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::{Arc, Mutex};

use rbtorrent::{
    PeerFlags, TorrentFlags, TorrentHandle, TorrentInfo, TorrentState as LtState, TorrentStatus,
};
use serde_json::{Map, Value, json};
use uuid::Uuid;

use super::DelugeState;
use super::config::config_value;
use super::proto::RpcError;
use super::registry::{Access, Ctx, HandlerResult, Registry, Scope, ok, positional};
use super::session::metric_value;
use super::torrents::{all_entries, lookup};
use super::values::{peer_limit_out, rate_out};
use crate::engine::events::TrackerInfo;
use crate::engine::registry::TorrentEntry;
use crate::engine::{Engine, EngineError, PeerSnapshot};

const STATE_NAMES: [&str; 8] = [
    "Allocating",
    "Checking",
    "Downloading",
    "Seeding",
    "Paused",
    "Error",
    "Queued",
    "Moving",
];

const SUPPORTED_KEYS: [&str; 77] = [
    "active_time",
    "all_time_download",
    "auto_managed",
    "comment",
    "completed_time",
    "creator",
    "distributed_copies",
    "download_location",
    "download_payload_rate",
    "eta",
    "file_priorities",
    "file_progress",
    "files",
    "finished_time",
    "hash",
    "is_auto_managed",
    "is_finished",
    "is_seed",
    "last_seen_complete",
    "max_connections",
    "max_download_speed",
    "max_upload_slots",
    "max_upload_speed",
    "message",
    "move_completed",
    "move_completed_path",
    "move_on_completed",
    "move_on_completed_path",
    "name",
    "next_announce",
    "num_files",
    "num_peers",
    "num_pieces",
    "num_seeds",
    "orig_files",
    "owner",
    "paused",
    "peers",
    "piece_length",
    "pieces",
    "prioritize_first_last",
    "prioritize_first_last_pieces",
    "private",
    "progress",
    "queue",
    "ratio",
    "remove_at_ratio",
    "save_path",
    "seed_mode",
    "seed_rank",
    "seeding_time",
    "seeds_peers_ratio",
    "sequential_download",
    "shared",
    "state",
    "stop_at_ratio",
    "stop_ratio",
    "storage_mode",
    "super_seeding",
    "time_added",
    "time_since_download",
    "time_since_transfer",
    "time_since_upload",
    "total_done",
    "total_payload_download",
    "total_payload_upload",
    "total_peers",
    "total_remaining",
    "total_seeds",
    "total_size",
    "total_uploaded",
    "total_wanted",
    "tracker",
    "tracker_host",
    "tracker_status",
    "trackers",
    "upload_payload_rate",
];

pub(super) fn register(r: &mut Registry) {
    use Access::Normal;
    use Scope::{Daemon, WebLocal};
    r.add(
        "core.get_torrent_status",
        Daemon,
        Normal,
        core_get_torrent_status,
    );
    r.add(
        "core.get_torrents_status",
        Daemon,
        Normal,
        get_torrents_status,
    );
    r.add("core.get_filter_tree", Daemon, Normal, get_filter_tree);
    r.add("web.update_ui", WebLocal, Normal, update_ui);
    r.add(
        "web.get_torrent_status",
        WebLocal,
        Normal,
        web_get_torrent_status,
    );
}

/// The requested key set; an empty list means all built-ins.
pub(super) enum KeySet {
    All,
    Named(HashSet<String>),
}

impl KeySet {
    fn parse(value: &Value) -> Result<KeySet, RpcError> {
        let list = value
            .as_array()
            .ok_or_else(|| RpcError::call_error("keys must be a list of strings"))?;
        if list.is_empty() {
            return Ok(KeySet::All);
        }
        let mut set = HashSet::with_capacity(list.len());
        for key in list {
            set.insert(
                key.as_str()
                    .ok_or_else(|| RpcError::call_error("keys must be a list of strings"))?
                    .to_owned(),
            );
        }
        Ok(KeySet::Named(set))
    }

    pub(super) fn from_names(names: &[&str]) -> KeySet {
        KeySet::Named(names.iter().map(|n| (*n).to_owned()).collect())
    }

    fn fields(&self) -> Vec<&'static str> {
        match self {
            KeySet::All => SUPPORTED_KEYS.to_vec(),
            KeySet::Named(set) => SUPPORTED_KEYS
                .iter()
                .copied()
                .filter(|k| set.contains(*k))
                .collect(),
        }
    }
}

// ---- status construction ----------------------------------------------------

/// One call's memo of the tracker list, which costs a live round trip
/// per torrent: `trackers`, the `tracker_host` fallback, and the sidebar
/// tree all read it, and `web.update_ui` walks the whole session more
/// than once. Nothing outlives the call — a later one queries afresh.
#[derive(Default)]
pub(super) struct TrackerCache(Mutex<HashMap<Uuid, Arc<[TrackerInfo]>>>);

impl TrackerCache {
    async fn get(
        &self,
        engine: &Engine,
        entry: &TorrentEntry,
    ) -> Result<Arc<[TrackerInfo]>, EngineError> {
        let cached = self.0.lock().unwrap().get(&entry.uuid).cloned();
        if let Some(trackers) = cached {
            return Ok(trackers);
        }
        let trackers: Arc<[TrackerInfo]> = engine.trackers(entry).await?.into();
        self.0
            .lock()
            .unwrap()
            .insert(entry.uuid, Arc::clone(&trackers));
        Ok(trackers)
    }
}

/// Gathered in one handle scope plus the requested live round trips;
/// everything past `flags` only when a requested key needs it.
struct Snapshot {
    status: TorrentStatus,
    flags: u64,
    info: Option<TorrentInfo>,
    /// Live (renamed) paths.
    paths: Vec<String>,
    priorities: Option<Vec<i64>>,
    trackers: Option<Arc<[TrackerInfo]>>,
    peers: Option<Vec<PeerSnapshot>>,
    /// Downloaded bytes per file.
    file_progress: Option<Vec<i64>>,
}

pub(super) async fn build_status(
    engine: &Engine,
    entry: &Arc<TorrentEntry>,
    keys: &KeySet,
    session_paused: bool,
    trackers: &TrackerCache,
) -> Result<Map<String, Value>, EngineError> {
    let fields = keys.fields();
    let needs = |wanted: &[&str]| fields.iter().any(|f| wanted.contains(f));
    let want_info = needs(&[
        "comment",
        "creator",
        "num_files",
        "num_pieces",
        "piece_length",
        "private",
        "total_size",
        "files",
        "orig_files",
        "file_progress",
        "file_priorities",
    ]);
    let want_paths = needs(&["files"]);
    let want_priorities = needs(&["file_priorities"]);
    let want_peers = needs(&["peers"]);
    let want_file_progress = needs(&["file_progress"]);

    // Each optional (and possibly expensive) status query runs only for
    // the fields that read it; the accurate-counter one covers
    // everything derived from total_done / total_wanted_done.
    let mut query = 0;
    for (flag, wanted) in [
        (TorrentHandle::QUERY_NAME, &["name"][..]),
        (
            TorrentHandle::QUERY_SAVE_PATH,
            &["save_path", "download_location"][..],
        ),
        (
            TorrentHandle::QUERY_DISTRIBUTED_COPIES,
            &["distributed_copies"][..],
        ),
        (
            TorrentHandle::QUERY_ACCURATE_DOWNLOAD_COUNTERS,
            &["progress", "eta", "ratio", "total_done", "total_remaining"][..],
        ),
        (
            TorrentHandle::QUERY_LAST_SEEN_COMPLETE,
            &["last_seen_complete"][..],
        ),
        (TorrentHandle::QUERY_PIECES, &["pieces"][..]),
    ] {
        if needs(wanted) {
            query |= flag;
        }
    }
    let mut snap = engine
        .with_handle(entry, |h| {
            let status = h.status(query)?;
            let flags = h.flags();
            let info = if want_info { h.torrent_file()? } else { None };
            let paths = if want_paths {
                h.file_paths()?.unwrap_or_default()
            } else {
                Vec::new()
            };
            let priorities = if want_priorities {
                info.as_ref().map(|info| {
                    (0..info.num_files())
                        .map(|f| i64::from(h.file_priority(f).value()))
                        .collect()
                })
            } else {
                None
            };
            Ok::<_, rbtorrent::Error>(Snapshot {
                status,
                flags,
                info,
                paths,
                priorities,
                trackers: None,
                peers: None,
                file_progress: None,
            })
        })?
        .map_err(EngineError::from)?;

    // `tracker_host` falls back to the first configured tracker before
    // the first announce succeeds.
    let want_trackers = needs(&["trackers"])
        || (needs(&["tracker_host"]) && snap.status.current_tracker().is_empty());
    if want_trackers {
        snap.trackers = Some(trackers.get(engine, entry).await?);
    }
    if want_peers {
        snap.peers = Some(engine.peers(entry).await?);
    }
    if want_file_progress && snap.info.is_some() {
        snap.file_progress = Some(engine.file_progress(entry).await?);
    }

    let mut map = Map::with_capacity(fields.len());
    for field in fields {
        map.insert(
            field.to_owned(),
            field_value(field, entry, &snap, session_paused),
        );
    }
    Ok(map)
}

/// Sentinels: `ratio`/`seeds_peers_ratio` -1.0 for "no data", `eta` -1
/// for "over a year", `time_since_*` -1 for "never", `pieces` null when
/// seeding or metadata-less.
fn field_value(field: &str, entry: &TorrentEntry, snap: &Snapshot, session_paused: bool) -> Value {
    let s = &snap.status;
    let flag = |f: TorrentFlags| snap.flags & f.bits() != 0;
    match field {
        // No engine backing: constants at their defaults.
        "active_time" | "seeding_time" | "finished_time" => json!(0),
        "time_since_download" | "time_since_upload" | "time_since_transfer" => json!(-1),
        "owner" => json!("localclient"),
        "shared"
        | "move_completed"
        | "move_on_completed"
        | "stop_at_ratio"
        | "remove_at_ratio"
        | "prioritize_first_last"
        | "prioritize_first_last_pieces" => json!(false),
        "move_completed_path" | "move_on_completed_path" | "tracker_status" => json!(""),
        "stop_ratio" => json!(2.0),
        "comment" | "creator" => json!(""),

        // The real info-hash, not the uuid the torrent is keyed by.
        "hash" => {
            let hashes = s.info_hashes();
            match (hashes.v1(), hashes.v2()) {
                (Some(v1), _) => json!(v1.to_string()),
                (_, Some(v2)) => json!(v2.to_string()),
                _ => json!(""),
            }
        }
        "name" => {
            let name = s.name();
            if name.is_empty() {
                json!(entry.uuid.to_string())
            } else {
                json!(name)
            }
        }
        "save_path" | "download_location" => json!(s.save_path()),
        "state" => json!(deluge_state(s, snap.flags, session_paused)),
        "paused" => json!(flag(TorrentFlags::PAUSED)),
        "auto_managed" | "is_auto_managed" => json!(flag(TorrentFlags::AUTO_MANAGED)),
        "sequential_download" => json!(flag(TorrentFlags::SEQUENTIAL_DOWNLOAD)),
        "super_seeding" => json!(flag(TorrentFlags::SUPER_SEEDING)),
        "seed_mode" => json!(flag(TorrentFlags::SEED_MODE)),
        "message" => match s.error() {
            Some(e) => json!(e.to_string()),
            None => json!("OK"),
        },
        "progress" => {
            if s.error().is_some() {
                json!(100.0)
            } else {
                json!(f64::from(s.progress()) * 100.0)
            }
        }
        "eta" => json!(eta(s)),
        "ratio" => json!(ratio(s)),
        "seeds_peers_ratio" => {
            if s.num_incomplete() <= 0 {
                json!(-1.0)
            } else {
                json!(f64::from(s.num_complete()) / f64::from(s.num_incomplete()))
            }
        }
        "is_finished" => json!(s.is_finished()),
        "is_seed" => json!(s.is_seeding()),
        "queue" => json!(s.queue_position()),
        "seed_rank" => json!(s.seed_rank()),
        "distributed_copies" => json!(f64::from(s.distributed_copies()).max(0.0)),
        "storage_mode" => json!(match s.storage_mode() {
            rbtorrent::StatusStorageMode::Allocate => "allocate",
            _ => "sparse",
        }),
        "time_added" => json!(s.added_time()),
        "completed_time" => json!(s.completed_time()),
        "last_seen_complete" => json!(s.last_seen_complete()),
        "next_announce" => json!(s.next_announce_seconds()),
        "all_time_download" => json!(s.all_time_download()),
        "total_uploaded" => json!(s.all_time_upload()),
        "total_done" => json!(s.total_done()),
        // The metadata's full size, pad bytes included
        // (`TorrentStatus::total` excludes pads).
        "total_size" => json!(snap.info.as_ref().map_or(0, TorrentInfo::total_size)),
        "total_wanted" => json!(s.total_wanted()),
        "total_remaining" => json!(s.total_wanted() - s.total_wanted_done()),
        "total_payload_download" => json!(s.total_payload_download()),
        "total_payload_upload" => json!(s.total_payload_upload()),
        "download_payload_rate" => json!(s.download_payload_rate()),
        "upload_payload_rate" => json!(s.upload_payload_rate()),
        "num_seeds" => json!(s.num_seeds()),
        "num_peers" => json!(s.num_peers() - s.num_seeds()),
        "total_seeds" => json!(s.num_complete()),
        "total_peers" => json!(s.num_incomplete()),
        // The metadata's fixed piece count, not the number downloaded.
        "num_pieces" => json!(snap.info.as_ref().map_or(0, TorrentInfo::num_pieces)),
        "max_download_speed" => json!(rate_out(i64::from(s.download_limit()))),
        "max_upload_speed" => json!(rate_out(i64::from(s.upload_limit()))),
        "max_connections" => json!(peer_limit_out(s.connections_limit())),
        "max_upload_slots" => json!(peer_limit_out(s.uploads_limit())),
        "tracker" => json!(s.current_tracker()),
        "tracker_host" => json!(tracker_host(
            &s.current_tracker(),
            snap.trackers.as_deref().unwrap_or_default(),
        )),
        "trackers" => trackers_value(snap.trackers.as_deref().unwrap_or_default()),
        "peers" => peers_value(snap.peers.as_deref().unwrap_or_default()),
        "pieces" => pieces_value(s),
        "num_files" => json!(snap.info.as_ref().map_or(0, TorrentInfo::num_files)),
        "piece_length" => json!(snap.info.as_ref().map_or(0, TorrentInfo::piece_length)),
        "private" => json!(snap.info.as_ref().is_some_and(TorrentInfo::is_private)),
        "files" => files_value(snap.info.as_ref(), Some(&snap.paths)),
        "orig_files" => files_value(snap.info.as_ref(), None),
        "file_priorities" => json!(snap.priorities.clone().unwrap_or_default()),
        "file_progress" => file_progress_value(
            snap.info.as_ref(),
            snap.file_progress.as_deref().unwrap_or_default(),
        ),
        _ => Value::Null,
    }
}

/// Resolution order: forced error, then moving, then the pause/queue
/// distinction, then the libtorrent state map.
fn deluge_state(status: &TorrentStatus, flags: u64, session_paused: bool) -> &'static str {
    if status.error().is_some() {
        return "Error";
    }
    if status.moving_storage() {
        return "Moving";
    }
    let paused = flags & TorrentFlags::PAUSED.bits() != 0;
    let auto_managed = flags & TorrentFlags::AUTO_MANAGED.bits() != 0;
    if !session_paused && paused && auto_managed {
        return "Queued";
    }
    if session_paused || paused {
        return "Paused";
    }
    match status.state() {
        LtState::CheckingFiles | LtState::CheckingResumeData => "Checking",
        LtState::Finished | LtState::Seeding => "Seeding",
        _ => "Downloading",
    }
}

fn eta(status: &TorrentStatus) -> i64 {
    let left = status.total_wanted() - status.total_wanted_done();
    let rate = i64::from(status.download_payload_rate());
    if left <= 0 || rate <= 0 {
        return 0;
    }
    let eta = left / rate;
    // Anything past a year (365.25 days, inclusive) is the -1 sentinel.
    if eta >= 31_557_600 { -1 } else { eta }
}

fn ratio(status: &TorrentStatus) -> f64 {
    let done = status.total_done();
    if done == 0 {
        -1.0
    } else {
        status.all_time_upload() as f64 / done as f64
    }
}

/// The announce host cut to its registrable-ish suffix; IPs verbatim,
/// `DHT` for a tracker without a hostname, empty without any tracker.
fn tracker_host(current: &str, trackers: &[TrackerInfo]) -> String {
    let url = if current.is_empty() {
        match trackers.first() {
            Some(t) => t.url.as_str(),
            None => return String::new(),
        }
    } else {
        current
    };
    match host_of(url) {
        Some(host) => shorten_host(host),
        None => "DHT".to_owned(),
    }
}

fn host_of(url: &str) -> Option<&str> {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    let rest = rest.split(['/', '?']).next().unwrap_or(rest);
    let rest = rest.rsplit_once('@').map_or(rest, |(_, rest)| rest);
    let host = match rest.strip_prefix('[') {
        Some(v6) => v6.split(']').next().unwrap_or(v6),
        None => rest.split(':').next().unwrap_or(rest),
    };
    (!host.is_empty()).then_some(host)
}

fn shorten_host(host: &str) -> String {
    if host.parse::<std::net::IpAddr>().is_ok() {
        return host.to_owned();
    }
    let parts: Vec<&str> = host.split('.').collect();
    if parts.len() > 2 {
        // Keep three labels for second-level registries and any .uk
        // domain, two otherwise.
        let keep = if ["co", "com", "net", "org"].contains(&parts[parts.len() - 2])
            || parts[parts.len() - 1] == "uk"
        {
            3
        } else {
            2
        };
        return parts[parts.len().saturating_sub(keep)..].join(".");
    }
    host.to_owned()
}

fn trackers_value(trackers: &[TrackerInfo]) -> Value {
    json!(
        trackers
            .iter()
            .map(|t| {
                json!({
                    "url": t.url,
                    "tier": t.tier,
                    "trackerid": t.trackerid,
                    "fail_limit": t.fail_limit,
                    "source": t.source,
                    "verified": t.verified,
                })
            })
            .collect::<Vec<_>>()
    )
}

/// Half-established connections are skipped, `seed` is the raw 1024
/// flag mask the web UI compares against, `country` empty (no GeoIP).
fn peers_value(peers: &[PeerSnapshot]) -> Value {
    json!(
        peers
            .iter()
            .filter(|p| !p
                .flags
                .intersects(PeerFlags::CONNECTING | PeerFlags::HANDSHAKE))
            .map(|p| {
                json!({
                    "client": p.client,
                    "country": "",
                    "down_speed": p.payload_down_speed,
                    // `host:port` without brackets even for IPv6; the
                    // web UI adds them.
                    "ip": p.address
                        .map(|a| format!("{}:{}", a.ip(), a.port()))
                        .unwrap_or_default(),
                    "progress": f64::from(p.progress_ppm) / 1e6,
                    "seed": if p.flags.contains(PeerFlags::SEED) { 1024 } else { 0 },
                    "up_speed": p.payload_up_speed,
                })
            })
            .collect::<Vec<_>>()
    )
}

/// Piece codes: 0 missing, 3 done. Codes 1 and 2 ("available", "being
/// downloaded") need peer bookkeeping the engine does not surface.
fn pieces_value(status: &TorrentStatus) -> Value {
    if !status.has_metadata() || status.is_seeding() {
        return Value::Null;
    }
    match status.pieces() {
        Some(bits) => json!(
            bits.iter()
                .map(|have| if have { 3u8 } else { 0u8 })
                .collect::<Vec<_>>()
        ),
        None => Value::Null,
    }
}

fn files_value(info: Option<&TorrentInfo>, live_paths: Option<&[String]>) -> Value {
    let Some(info) = info else {
        return json!([]);
    };
    json!(
        info.files()
            .map(|f| {
                let path = live_paths
                    .and_then(|paths| paths.get(f.index() as usize))
                    .cloned()
                    .unwrap_or_else(|| f.path());
                json!({
                    "index": f.index(),
                    "path": path.replace('\\', "/"),
                    "size": f.size(),
                    "offset": f.offset(),
                })
            })
            .collect::<Vec<_>>()
    )
}

fn file_progress_value(info: Option<&TorrentInfo>, progress: &[i64]) -> Value {
    let Some(info) = info else {
        return json!([]);
    };
    json!(
        info.files()
            .map(|f| {
                let done = progress.get(f.index() as usize).copied().unwrap_or(0);
                if f.size() > 0 {
                    done as f64 / f.size() as f64
                } else {
                    0.0
                }
            })
            .collect::<Vec<f64>>()
    )
}

// ---- filter_dict matching ---------------------------------------------------

/// One normalized filter: the field and its ORed accepted values.
struct Filter {
    field: String,
    values: Vec<Value>,
}

/// Fields AND, values within a field OR, string scalars auto-wrapped.
/// `id` seeds the candidate set, `Active` in `state` is the
/// nonzero-payload-rate special case, `keyword` and `name` are substring
/// matches, the rest compare for equality.
struct Filters {
    seed_ids: Option<Vec<Value>>,
    active_required: bool,
    filters: Vec<Filter>,
}

impl Filters {
    fn parse(filter_dict: &Value) -> Result<Filters, RpcError> {
        let empty = Map::new();
        let map = match filter_dict {
            Value::Null => &empty,
            Value::Object(map) => map,
            _ => return Err(RpcError::call_error("filter_dict must be an object")),
        };
        let mut parsed = Filters {
            seed_ids: None,
            active_required: false,
            filters: Vec::new(),
        };
        for (field, value) in map {
            let mut values = match value {
                Value::Array(list) => list.clone(),
                scalar => vec![scalar.clone()],
            };
            match field.as_str() {
                "id" => parsed.seed_ids = Some(values),
                "state" => {
                    // Only a list that `Active` empties drops the
                    // constraint; one that arrived empty stands, and no
                    // state satisfies it.
                    if values.iter().any(|v| v == "Active") {
                        parsed.active_required = true;
                        values.retain(|v| v != "Active");
                        if values.is_empty() {
                            continue;
                        }
                    }
                    parsed.filters.push(Filter {
                        field: "state".to_owned(),
                        values,
                    });
                }
                _ => parsed.filters.push(Filter {
                    field: field.clone(),
                    values,
                }),
            }
        }
        Ok(parsed)
    }

    fn needed_keys(&self) -> Vec<&str> {
        let mut keys = Vec::new();
        if self.active_required {
            keys.extend(["download_payload_rate", "upload_payload_rate"]);
        }
        for filter in &self.filters {
            match filter.field.as_str() {
                "keyword" => keys.extend(["name", "state", "trackers", "files"]),
                "name" => keys.push("name"),
                key => keys.push(key),
            }
        }
        keys
    }

    fn is_empty(&self) -> bool {
        !self.active_required && self.filters.is_empty()
    }

    fn matches(&self, id: &str, status: &Map<String, Value>) -> bool {
        if self.active_required {
            let rate = |key: &str| status.get(key).and_then(Value::as_i64).unwrap_or(0);
            if rate("download_payload_rate") == 0 && rate("upload_payload_rate") == 0 {
                return false;
            }
        }
        self.filters.iter().all(|f| match f.field.as_str() {
            "keyword" => keyword_matches(&f.values, id, status),
            "name" => name_matches(&f.values, status),
            "tracker_host" => tracker_host_matches(&f.values, status),
            field => f
                .values
                .iter()
                .any(|v| status.get(field).is_some_and(|actual| value_eq(actual, v))),
        })
    }
}

/// Python's equality, where `-1 == -1.0`: numbers compare by value,
/// everything else structurally.
fn value_eq(a: &Value, b: &Value) -> bool {
    if let (Some(a), Some(b)) = (a.as_f64(), b.as_f64()) {
        return a == b;
    }
    a == b
}

/// Every comma-separated keyword must match the lowercased name, state
/// or one of the file paths, the id, or the first tracker's url — which
/// upstream compares case-sensitively, unlike the rest.
fn keyword_matches(values: &[Value], id: &str, status: &Map<String, Value>) -> bool {
    let joined = values
        .iter()
        .filter_map(Value::as_str)
        .collect::<Vec<_>>()
        .join(",");
    let lowercased = |key: &str| {
        status
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_lowercase()
    };
    let name = lowercased("name");
    let state = lowercased("state");
    let tracker = status
        .get("trackers")
        .and_then(|trackers| trackers.get(0))
        .and_then(|tracker| tracker.get("url"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let paths: Vec<String> = status
        .get("files")
        .and_then(Value::as_array)
        .map(|files| {
            files
                .iter()
                .filter_map(|file| file.get("path"))
                .filter_map(Value::as_str)
                .map(str::to_lowercase)
                .collect()
        })
        .unwrap_or_default();
    joined
        .split(',')
        .map(|k| k.trim().to_lowercase())
        .filter(|k| !k.is_empty())
        .all(|k| {
            name.contains(&k)
                || state.contains(&k)
                || tracker.contains(&k)
                || id.contains(&k)
                || paths.iter().any(|path| path.contains(&k))
        })
}

/// Substring match on the first value only; a `::match` suffix makes it
/// case-sensitive.
fn name_matches(values: &[Value], status: &Map<String, Value>) -> bool {
    let Some(search) = values.first().and_then(Value::as_str) else {
        return false;
    };
    let name = status
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or_default();
    match search.strip_suffix("::match") {
        Some(exact) => name.contains(exact),
        None => name.to_lowercase().contains(&search.to_lowercase()),
    }
}

/// The first value only; the literal `Error` selects tracker-error
/// torrents, which are never reported, so it matches none.
fn tracker_host_matches(values: &[Value], status: &Map<String, Value>) -> bool {
    let Some(wanted) = values.first().and_then(Value::as_str) else {
        return false;
    };
    if wanted == "Error" {
        return false;
    }
    status
        .get("tracker_host")
        .and_then(Value::as_str)
        .is_some_and(|host| host == wanted)
}

/// Applies `filter_dict`, returning the surviving torrents in add
/// order. `id` entries that do not resolve are dropped silently.
async fn filter_entries(
    state: &DelugeState,
    filter_dict: &Value,
    session_paused: bool,
    trackers: &TrackerCache,
) -> Result<Vec<Arc<TorrentEntry>>, RpcError> {
    let filters = Filters::parse(filter_dict)?;
    let engine = &state.engine;
    let candidates: Vec<Arc<TorrentEntry>> = match &filters.seed_ids {
        Some(ids) => ids
            .iter()
            .filter_map(|id| {
                id.as_str()
                    .and_then(|s| Uuid::parse_str(s).ok())
                    .and_then(|uuid| engine.registry().find(&uuid))
            })
            .collect(),
        None => all_entries(engine),
    };
    if filters.is_empty() {
        return Ok(candidates);
    }
    let keys = KeySet::from_names(&filters.needed_keys());
    let mut kept = Vec::new();
    for entry in candidates {
        let status = match build_status(engine, &entry, &keys, session_paused, trackers).await {
            Ok(status) => status,
            Err(EngineError::NotFound) => continue,
            Err(e) => return Err(e.into()),
        };
        if filters.matches(&entry.uuid.to_string(), &status) {
            kept.push(entry);
        }
    }
    Ok(kept)
}

// ---- filter tree ------------------------------------------------------------

/// `{field: [[value, count], …]}` over `state`, `tracker_host`, and
/// `owner`. Hidden categories are not merely dropped from the answer:
/// their per-torrent keys are never fetched, `tracker_host` in
/// particular being a live tracker round trip per torrent.
async fn build_filter_tree(
    engine: &Engine,
    show_zero_hits: bool,
    hidden: &HashSet<&str>,
    trackers: &TrackerCache,
) -> Result<Map<String, Value>, RpcError> {
    let want_state = !hidden.contains("state");
    let want_hosts = !hidden.contains("tracker_host");
    let mut names: Vec<&str> = Vec::new();
    if want_state {
        names.extend(["state", "download_payload_rate", "upload_payload_rate"]);
    }
    if want_hosts {
        names.push("tracker_host");
    }

    let mut total = 0i64;
    let mut active = 0i64;
    let mut state_counts: BTreeMap<&'static str, i64> =
        STATE_NAMES.iter().map(|s| (*s, 0)).collect();
    let mut hosts: BTreeMap<String, i64> = BTreeMap::new();
    if names.is_empty() {
        // Only totals left to count (for the owner category).
        total = all_entries(engine).len() as i64;
    } else {
        let session_paused = engine.is_session_paused()?;
        let keys = KeySet::from_names(&names);
        for entry in all_entries(engine) {
            let status = match build_status(engine, &entry, &keys, session_paused, trackers).await {
                Ok(status) => status,
                Err(EngineError::NotFound) => continue,
                Err(e) => return Err(e.into()),
            };
            total += 1;
            if want_state {
                if let Some(state) = status.get("state").and_then(Value::as_str)
                    && let Some(count) = state_counts.get_mut(state)
                {
                    *count += 1;
                }
                let rate = |key: &str| status.get(key).and_then(Value::as_i64).unwrap_or(0);
                if rate("download_payload_rate") != 0 || rate("upload_payload_rate") != 0 {
                    active += 1;
                }
            }
            if want_hosts {
                let host = status
                    .get("tracker_host")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                *hosts.entry(host.to_owned()).or_default() += 1;
            }
        }
    }

    let keep = |name: &str, count: i64| show_zero_hits || count > 0 || name == "All";
    let mut tree = Map::new();
    if want_state {
        let mut states = vec![("All", total), ("Active", active)];
        states.extend(STATE_NAMES.iter().map(|s| (*s, state_counts[s])));
        let states: Vec<Value> = states
            .into_iter()
            .filter(|(name, count)| keep(name, *count))
            .map(|(name, count)| json!([name, count]))
            .collect();
        tree.insert("state".to_owned(), json!(states));
    }
    if want_hosts {
        hosts.insert("All".to_owned(), total);
        hosts.entry("Error".to_owned()).or_insert(0);
        let hosts: Vec<Value> = hosts
            .iter()
            .filter(|(name, count)| keep(name, **count))
            .map(|(name, count)| json!([name, count]))
            .collect();
        tree.insert("tracker_host".to_owned(), json!(hosts));
    }
    if !hidden.contains("owner") {
        let owners = [("", 0), ("localclient", total)];
        let owners: Vec<Value> = owners
            .into_iter()
            .filter(|(name, count)| keep(name, *count))
            .map(|(name, count)| json!([name, count]))
            .collect();
        tree.insert("owner".to_owned(), json!(owners));
    }
    Ok(tree)
}

// ---- handlers ---------------------------------------------------------------

async fn core_get_torrent_status(ctx: Ctx) -> HandlerResult {
    let ([torrent_id, keys], [_diff]) = positional(
        "get_torrent_status",
        ctx.params,
        ["torrent_id", "keys"],
        [("diff", json!(false))],
    )?;
    let entry = lookup(&ctx.state, &torrent_id)?;
    let keys = KeySet::parse(&keys)?;
    let session_paused = ctx.state.engine.is_session_paused()?;
    let trackers = TrackerCache::default();
    let status = build_status(&ctx.state.engine, &entry, &keys, session_paused, &trackers).await?;
    ok(Value::Object(status))
}

async fn get_torrents_status(ctx: Ctx) -> HandlerResult {
    let ([filter_dict, keys], [_diff]) = positional(
        "get_torrents_status",
        ctx.params,
        ["filter_dict", "keys"],
        [("diff", json!(false))],
    )?;
    let keys = KeySet::parse(&keys)?;
    let session_paused = ctx.state.engine.is_session_paused()?;
    let trackers = TrackerCache::default();
    let entries = filter_entries(&ctx.state, &filter_dict, session_paused, &trackers).await?;
    let mut result = Map::new();
    for entry in entries {
        let status =
            match build_status(&ctx.state.engine, &entry, &keys, session_paused, &trackers).await {
                Ok(status) => status,
                Err(EngineError::NotFound) => continue,
                Err(e) => return Err(e.into()),
            };
        result.insert(entry.uuid.to_string(), Value::Object(status));
    }
    ok(Value::Object(result))
}

async fn get_filter_tree(ctx: Ctx) -> HandlerResult {
    let ([], [show_zero_hits, hide_cat]) = positional(
        "get_filter_tree",
        ctx.params,
        [],
        [("show_zero_hits", json!(true)), ("hide_cat", Value::Null)],
    )?;
    let show_zero_hits = show_zero_hits.as_bool().unwrap_or(true);
    let hidden: HashSet<&str> = hide_cat
        .as_array()
        .map(|list| list.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    let trackers = TrackerCache::default();
    let tree = build_filter_tree(&ctx.state.engine, show_zero_hits, &hidden, &trackers).await?;
    ok(Value::Object(tree))
}

/// [`core_get_torrent_status`] minus the `diff` parameter.
async fn web_get_torrent_status(ctx: Ctx) -> HandlerResult {
    let ([torrent_id, keys], []) =
        positional("get_torrent_status", ctx.params, ["torrent_id", "keys"], [])?;
    let entry = lookup(&ctx.state, &torrent_id)?;
    let keys = KeySet::parse(&keys)?;
    let session_paused = ctx.state.engine.is_session_paused()?;
    let trackers = TrackerCache::default();
    let status = build_status(&ctx.state.engine, &entry, &keys, session_paused, &trackers).await?;
    ok(Value::Object(status))
}

/// The one-call UI poll: statuses, sidebar tree, and toolbar stats. All
/// three passes share one [`TrackerCache`], which is what keeps the poll
/// to a single tracker round trip per torrent.
async fn update_ui(ctx: Ctx) -> HandlerResult {
    let ([keys, filter_dict], []) =
        positional("update_ui", ctx.params, ["keys", "filter_dict"], [])?;
    let keys = KeySet::parse(&keys)?;
    let engine = &ctx.state.engine;
    let session_paused = engine.is_session_paused()?;
    let trackers = TrackerCache::default();

    let mut torrents = Map::new();
    for entry in filter_entries(&ctx.state, &filter_dict, session_paused, &trackers).await? {
        let status = match build_status(engine, &entry, &keys, session_paused, &trackers).await {
            Ok(status) => status,
            Err(EngineError::NotFound) => continue,
            Err(e) => return Err(e.into()),
        };
        torrents.insert(entry.uuid.to_string(), Value::Object(status));
    }

    let filters = build_filter_tree(engine, true, &HashSet::new(), &trackers).await?;

    let mut stats = Map::new();
    let pack = engine.settings()?;
    for (stat, key) in [
        ("max_download", "max_download_speed"),
        ("max_upload", "max_upload_speed"),
        ("max_num_connections", "max_connections_global"),
    ] {
        stats.insert(
            stat.to_owned(),
            config_value(engine, &pack, key).unwrap_or(Value::Null),
        );
    }
    let counters = engine.session_stats().await?;
    stats.insert(
        "num_connections".to_owned(),
        json!(metric_value(&counters, "peer.num_peers_connected")),
    );
    stats.insert(
        "dht_nodes".to_owned(),
        json!(metric_value(&counters, "dht.dht_nodes")),
    );
    // The raw metric: an int gauge, not a bool.
    stats.insert(
        "has_incoming_connections".to_owned(),
        json!(metric_value(&counters, "net.has_incoming_connections")),
    );
    let (mut payload_down, mut payload_up) = (0i64, 0i64);
    let (mut proto_down, mut proto_up) = (0i64, 0i64);
    for entry in all_entries(engine) {
        let Ok(Ok(s)) = engine.with_handle(&entry, |h| h.status(0)) else {
            continue;
        };
        payload_down += i64::from(s.download_payload_rate());
        payload_up += i64::from(s.upload_payload_rate());
        proto_down += i64::from(s.download_rate() - s.download_payload_rate());
        proto_up += i64::from(s.upload_rate() - s.upload_payload_rate());
    }
    stats.insert("download_rate".to_owned(), json!(payload_down));
    stats.insert("upload_rate".to_owned(), json!(payload_up));
    stats.insert("download_protocol_rate".to_owned(), json!(proto_down));
    stats.insert("upload_protocol_rate".to_owned(), json!(proto_up));

    ok(json!({
        "connected": true,
        "torrents": torrents,
        "filters": filters,
        "stats": stats,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_status() -> Map<String, Value> {
        json!({
            "name": "Fixture",
            "state": "Seeding",
            "trackers": [{"url": "http://tracker.example.com/announce"}],
            "files": [{"path": "fixture/a.bin"}, {"path": "fixture/b.txt"}],
        })
        .as_object()
        .unwrap()
        .clone()
    }

    #[test]
    fn keywords_search_every_field_upstream_does() {
        let id = "0d5190f4-991f-4d5e-b2ae-9e34e0c11d63";
        let status = sample_status();
        let matches = |keyword: &str| keyword_matches(&[json!(keyword)], id, &status);
        for keyword in ["fixt", "seeding", "tracker.example", "b.txt", "9e34"] {
            assert!(matches(keyword), "{keyword}");
        }
        assert!(!matches("nope"));
        // Comma-separated keywords all have to match.
        assert!(matches("fixt,b.txt"));
        assert!(!matches("fixt,nope"));
    }
}
