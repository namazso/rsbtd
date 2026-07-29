// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Everything that acts on a torrent: the add and remove paths, the
//! pause/resume/queue/recheck controls, storage moves and renames, and
//! the `TorrentOptions` writes, plus the `web.add_torrents` batch.
//! Torrent-id resolution (`lookup`, `all_entries`) lives here too.
//!
//! Per-torrent options with no engine backing (`stop_at_ratio`,
//! `move_completed`, `owner`, …) parse to nothing and apply as no-ops,
//! reading back as their defaults from [`status`](super::status).
//! Storing none of them is also why `core.add_torrent_*` requires
//! `options.download_location` and why resume leaves a torrent detached
//! from auto-management (see [`resume_entries`]).

use std::sync::Arc;

use rbtorrent::{
    AddTorrentParams, DownloadPriority, StorageMode, TorrentFlags, TorrentHandle, TorrentInfo,
};
use serde_json::{Value, json};
use uuid::Uuid;

use super::DelugeState;
use super::proto::RpcError;
use super::registry::{Access, Ctx, HandlerResult, Registry, Scope, ok, positional};
use super::values::{
    bool_of, connection_limit_of, priorities_of, str_of, torrent_rate_of, upload_slots_of,
};
use crate::engine::registry::TorrentEntry;
use crate::engine::{Engine, EngineError};

/// Sugar over `core.set_torrent_options`.
const DEPRECATED_SETTERS: [(&str, &str, &str); 12] = [
    (
        "core.set_torrent_auto_managed",
        "set_torrent_auto_managed",
        "auto_managed",
    ),
    (
        "core.set_torrent_file_priorities",
        "set_torrent_file_priorities",
        "file_priorities",
    ),
    (
        "core.set_torrent_max_connections",
        "set_torrent_max_connections",
        "max_connections",
    ),
    (
        "core.set_torrent_max_download_speed",
        "set_torrent_max_download_speed",
        "max_download_speed",
    ),
    (
        "core.set_torrent_max_upload_slots",
        "set_torrent_max_upload_slots",
        "max_upload_slots",
    ),
    (
        "core.set_torrent_max_upload_speed",
        "set_torrent_max_upload_speed",
        "max_upload_speed",
    ),
    (
        "core.set_torrent_move_completed",
        "set_torrent_move_completed",
        "move_completed",
    ),
    (
        "core.set_torrent_move_completed_path",
        "set_torrent_move_completed_path",
        "move_completed_path",
    ),
    (
        "core.set_torrent_prioritize_first_last",
        "set_torrent_prioritize_first_last",
        "prioritize_first_last_pieces",
    ),
    (
        "core.set_torrent_remove_at_ratio",
        "set_torrent_remove_at_ratio",
        "remove_at_ratio",
    ),
    (
        "core.set_torrent_stop_at_ratio",
        "set_torrent_stop_at_ratio",
        "stop_at_ratio",
    ),
    (
        "core.set_torrent_stop_ratio",
        "set_torrent_stop_ratio",
        "stop_ratio",
    ),
];

pub(super) fn register(r: &mut Registry) {
    use Access::Normal;
    use Scope::{Daemon, WebLocal};
    r.add("core.add_torrent_file", Daemon, Normal, add_torrent_file);
    r.add(
        "core.add_torrent_file_async",
        Daemon,
        Normal,
        add_torrent_file_async,
    );
    r.add("core.add_torrent_files", Daemon, Normal, add_torrent_files);
    r.add(
        "core.add_torrent_magnet",
        Daemon,
        Normal,
        add_torrent_magnet,
    );
    r.add("core.add_torrent_url", Daemon, Normal, add_torrent_url);
    r.add("core.create_torrent", Daemon, Normal, create_torrent);
    r.add(
        "core.prefetch_magnet_metadata",
        Daemon,
        Normal,
        prefetch_magnet_metadata,
    );
    r.add("core.remove_torrent", Daemon, Normal, remove_torrent);
    r.add("core.remove_torrents", Daemon, Normal, remove_torrents);
    r.add("web.add_torrents", WebLocal, Normal, add_torrents);
    r.add(
        "web.download_torrent_from_url",
        WebLocal,
        Normal,
        download_torrent_from_url,
    );
    r.add("core.pause_torrent", Daemon, Normal, pause_torrent);
    r.add("core.pause_torrents", Daemon, Normal, pause_torrents);
    r.add("core.resume_torrent", Daemon, Normal, resume_torrent);
    r.add("core.resume_torrents", Daemon, Normal, resume_torrents);
    r.add("core.force_reannounce", Daemon, Normal, force_reannounce);
    r.add("core.force_recheck", Daemon, Normal, force_recheck);
    r.add("core.connect_peer", Daemon, Normal, connect_peer);
    r.add("core.queue_top", Daemon, Normal, queue_top);
    r.add("core.queue_up", Daemon, Normal, queue_up);
    r.add("core.queue_down", Daemon, Normal, queue_down);
    r.add("core.queue_bottom", Daemon, Normal, queue_bottom);
    r.add("core.move_storage", Daemon, Normal, move_storage);
    r.add("core.rename_files", Daemon, Normal, rename_files);
    r.add("core.rename_folder", Daemon, Normal, rename_folder);
    r.add("core.get_magnet_uri", Daemon, Normal, get_magnet_uri);
    r.add(
        "core.set_torrent_trackers",
        Daemon,
        Normal,
        set_torrent_trackers,
    );
    r.add(
        "core.set_torrent_options",
        Daemon,
        Normal,
        set_torrent_options,
    );
    for (name, leaf, key) in DEPRECATED_SETTERS {
        r.add(name, Daemon, Normal, move |ctx| {
            set_single_option(ctx, leaf, key)
        });
    }
}

// ---- torrent lookup ---------------------------------------------------------

/// Resolves a torrent-id value (an rsbtd uuid string).
pub(super) fn lookup(state: &DelugeState, id: &Value) -> Result<Arc<TorrentEntry>, RpcError> {
    let id = id
        .as_str()
        .ok_or_else(|| RpcError::call_error("torrent_id must be a string"))?;
    let uuid = Uuid::parse_str(id)
        .map_err(|_| RpcError::call_error(format!("invalid torrent_id: {id}")))?;
    state
        .engine
        .registry()
        .find(&uuid)
        .ok_or_else(|| EngineError::NotFound.into())
}

pub(super) fn all_entries(engine: &Engine) -> Vec<Arc<TorrentEntry>> {
    let mut entries = engine.registry().list();
    entries.sort_by_key(|e| (e.added_at, e.id));
    entries
}

/// One id string or a list of ids. A missing value is an error, never
/// "every torrent": only pause/resume document that meaning
/// ([`entries_or_all`]).
fn entries_param(state: &DelugeState, ids: &Value) -> Result<Vec<Arc<TorrentEntry>>, RpcError> {
    match ids {
        Value::String(_) => Ok(vec![lookup(state, ids)?]),
        Value::Array(list) => list.iter().map(|id| lookup(state, id)).collect(),
        _ => Err(RpcError::call_error(
            "torrent_ids must be a string or a list of strings",
        )),
    }
}

/// [`entries_param`], with a missing or empty list meaning every
/// torrent — the semantics upstream's pause/resume default arguments
/// document, and that their singular forms inherit by delegation.
fn entries_or_all(state: &DelugeState, ids: &Value) -> Result<Vec<Arc<TorrentEntry>>, RpcError> {
    match ids {
        Value::Null => Ok(all_entries(&state.engine)),
        Value::Array(list) if list.is_empty() => Ok(all_entries(&state.engine)),
        _ => entries_param(state, ids),
    }
}

// ---- torrent options --------------------------------------------------------

/// The subset of `TorrentOptions` rsbtd can honor.
#[derive(Default)]
struct Options {
    download_location: Option<String>,
    add_paused: Option<bool>,
    auto_managed: Option<bool>,
    sequential_download: Option<bool>,
    super_seeding: Option<bool>,
    seed_mode: Option<bool>,
    pre_allocate_storage: Option<bool>,
    file_priorities: Option<Vec<DownloadPriority>>,
    /// Add-time renames as `(file index, sanitized path)`.
    mapped_files: Option<Vec<(i32, String)>>,
    /// bytes/s, already validated.
    max_download_speed: Option<i32>,
    max_upload_speed: Option<i32>,
    max_connections: Option<i32>,
    max_upload_slots: Option<i32>,
}

impl Options {
    fn parse(options: &Value) -> Result<Options, RpcError> {
        let map = match options {
            Value::Null => return Ok(Options::default()),
            Value::Object(map) => map,
            _ => return Err(RpcError::call_error("options must be an object")),
        };
        let mut o = Options::default();
        for (key, value) in map {
            match key.as_str() {
                "download_location" => o.download_location = Some(str_of(key, value)?),
                "add_paused" => o.add_paused = Some(bool_of(key, value)?),
                "auto_managed" => o.auto_managed = Some(bool_of(key, value)?),
                "sequential_download" => o.sequential_download = Some(bool_of(key, value)?),
                "super_seeding" => o.super_seeding = Some(bool_of(key, value)?),
                "seed_mode" => o.seed_mode = Some(bool_of(key, value)?),
                "pre_allocate_storage" => o.pre_allocate_storage = Some(bool_of(key, value)?),
                "file_priorities" => o.file_priorities = Some(priorities_of(key, value)?),
                "mapped_files" => o.mapped_files = Some(mapped_files_of(key, value)?),
                "max_download_speed" => o.max_download_speed = Some(torrent_rate_of(key, value)?),
                "max_upload_speed" => o.max_upload_speed = Some(torrent_rate_of(key, value)?),
                "max_connections" => o.max_connections = Some(connection_limit_of(key, value)?),
                "max_upload_slots" => o.max_upload_slots = Some(upload_slots_of(key, value)?),
                // name, owner, shared, move_completed*, stop_*,
                // remove_at_ratio, … — no engine backing.
                _ => {}
            }
        }
        Ok(o)
    }
}

/// `{index: path}`, the object keys standing in for Deluge's integer
/// file indices, each path sanitized like a rename target.
fn mapped_files_of(key: &str, value: &Value) -> Result<Vec<(i32, String)>, RpcError> {
    let map = value
        .as_object()
        .ok_or_else(|| RpcError::call_error(format!("{key} must be an object of index -> path")))?;
    map.iter()
        .map(|(index, path)| {
            let parsed = index.parse().ok();
            let sanitized = path
                .as_str()
                .and_then(sanitize_path)
                .filter(|path| !path.is_empty());
            match (parsed, sanitized) {
                (Some(index), Some(path)) => Ok((index, path)),
                _ => Err(RpcError::call_error(format!(
                    "invalid {key} entry {index}: {path}"
                ))),
            }
        })
        .collect()
}

/// Values were validated at parse time and a priority list that does
/// not cover every file exactly is skipped (upstream keeps the current
/// priorities in that case), so a delta either fails before any of it
/// is applied or applies wholly. The one remaining fallible call goes
/// first to keep it that way.
fn apply_options(engine: &Engine, entry: &TorrentEntry, o: &Options) -> Result<(), RpcError> {
    engine
        .with_handle(entry, |h| {
            if let Some(p) = &o.file_priorities {
                let files = h.torrent_file()?.map_or(0, |info| info.num_files());
                if !p.is_empty() && p.len() == usize::try_from(files).unwrap_or(0) {
                    h.prioritize_files(p)?;
                }
            }
            if let Some(v) = o.max_upload_speed {
                h.set_upload_limit(v)?;
            }
            if let Some(v) = o.max_download_speed {
                h.set_download_limit(v)?;
            }
            if let Some(v) = o.max_upload_slots {
                h.set_max_uploads(v)?;
            }
            if let Some(v) = o.max_connections {
                h.set_max_connections(v)?;
            }
            for (flag, value) in [
                (TorrentFlags::AUTO_MANAGED, o.auto_managed),
                (TorrentFlags::SEQUENTIAL_DOWNLOAD, o.sequential_download),
                (TorrentFlags::SUPER_SEEDING, o.super_seeding),
            ] {
                match value {
                    Some(true) => h.set_flags(flag.bits(), flag.bits()),
                    Some(false) => h.unset_flags(flag.bits()),
                    None => {}
                }
            }
            // `download_location` applies at add time only: it never
            // moves data (that is `core.move_storage`), and there is no
            // stored per-torrent option to update, so it is ignored.
            Ok::<_, rbtorrent::Error>(())
        })?
        .map_err(EngineError::from)?;
    Ok(())
}

async fn set_torrent_options(ctx: Ctx) -> HandlerResult {
    let ([torrent_ids, options], []) = positional(
        "set_torrent_options",
        ctx.params,
        ["torrent_ids", "options"],
        [],
    )?;
    let parsed = Options::parse(&options)?;
    for entry in entries_param(&ctx.state, &torrent_ids)? {
        apply_options(&ctx.state.engine, &entry, &parsed)?;
    }
    ok(Value::Null)
}

async fn set_single_option(ctx: Ctx, leaf: &'static str, key: &'static str) -> HandlerResult {
    let ([torrent_id, value], []) = positional(leaf, ctx.params, ["torrent_id", "value"], [])?;
    let parsed = Options::parse(&json!({ key: value }))?;
    for entry in entries_param(&ctx.state, &torrent_id)? {
        apply_options(&ctx.state.engine, &entry, &parsed)?;
    }
    ok(Value::Null)
}

// ---- adding and removing ----------------------------------------------------

/// The most `.torrent` bytes read from this daemon's filesystem.
const TORRENT_FILE_LIMIT: u64 = 32 * 1024 * 1024;

/// A bounded read, so an oversized or non-terminating file (a device
/// node, say) cannot exhaust the daemon's memory.
pub(super) async fn read_torrent_file(path: &str) -> std::io::Result<Vec<u8>> {
    use tokio::io::AsyncReadExt;
    let file = tokio::fs::File::open(path).await?;
    let mut bytes = Vec::new();
    file.take(TORRENT_FILE_LIMIT + 1)
        .read_to_end(&mut bytes)
        .await?;
    if bytes.len() as u64 > TORRENT_FILE_LIMIT {
        return Err(std::io::Error::other(format!(
            "file is larger than {TORRENT_FILE_LIMIT} bytes"
        )));
    }
    Ok(bytes)
}

fn atp_from_dump(filedump: &Value) -> Result<AddTorrentParams, RpcError> {
    use base64::Engine as _;
    let dump = filedump
        .as_str()
        .ok_or_else(|| RpcError::call_error("filedump must be a base64 string"))?;
    // Embedded newlines are tolerated, as Python's b64decode does.
    let compact: String = dump.chars().filter(|c| !c.is_whitespace()).collect();
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(compact)
        .map_err(|e| RpcError::call_error(format!("invalid base64 filedump: {e}")))?;
    AddTorrentParams::from_torrent_buffer(&bytes)
        .map_err(|e| RpcError::call_error(format!("cannot load torrent: {}", e.message())))
}

/// The real file indices of a torrent's payload files. The add dialog
/// numbers those contiguously, leaving out the pads libtorrent
/// synthesizes ([`filetree`](super::filetree)), and sends that numbering
/// back in `file_priorities` and `mapped_files`.
fn payload_files(info: &TorrentInfo) -> Vec<i32> {
    info.files()
        .filter(|f| !f.flags().is_pad_file())
        .map(|f| f.index())
        .collect()
}

/// The priority list to add with, or [`None`] for one to ignore: as
/// upstream, only a list covering the whole torrent applies — every real
/// file, or exactly the payload files, which is spread back over the
/// real indices with the pads left unwanted. A magnet has no file list
/// to measure against, so its list is taken as it is and libtorrent
/// applies it once the metadata arrives.
fn add_priorities(
    info: Option<&TorrentInfo>,
    priorities: &[DownloadPriority],
) -> Option<Vec<DownloadPriority>> {
    if priorities.is_empty() {
        return None;
    }
    let Some(info) = info else {
        return Some(priorities.to_vec());
    };
    let files = usize::try_from(info.num_files()).unwrap_or(0);
    if priorities.len() == files {
        return Some(priorities.to_vec());
    }
    let payload = payload_files(info);
    if priorities.len() != payload.len() {
        return None;
    }
    let mut spread = vec![DownloadPriority::DONT_DOWNLOAD; files];
    for (index, priority) in payload.into_iter().zip(priorities) {
        spread[index as usize] = *priority;
    }
    Some(spread)
}

/// The shared add path; returns the new torrent's id.
async fn add_with_options(
    engine: &Engine,
    mut atp: AddTorrentParams,
    options: &Value,
) -> Result<String, RpcError> {
    let o = Options::parse(options)?;
    let Some(location) = &o.download_location else {
        return Err(RpcError::call_error(
            "options.download_location is required (rsbtd has no default download location)",
        ));
    };
    atp.set_save_path(location);
    let mut flags = atp.flags().bits();
    for (flag, value) in [
        (TorrentFlags::AUTO_MANAGED, o.auto_managed),
        (TorrentFlags::SEQUENTIAL_DOWNLOAD, o.sequential_download),
        (TorrentFlags::SUPER_SEEDING, o.super_seeding),
        (TorrentFlags::SEED_MODE, o.seed_mode),
    ] {
        match value {
            Some(true) => flags |= flag.bits(),
            Some(false) => flags &= !flag.bits(),
            None => {}
        }
    }
    if o.add_paused == Some(true) {
        // Detached from auto-management too, else the queue would
        // immediately resume it.
        flags = (flags | TorrentFlags::PAUSED.bits()) & !TorrentFlags::AUTO_MANAGED.bits();
    } else {
        // Fresh params default to PAUSED | AUTO_MANAGED, and with
        // `auto_managed: false` no queue would ever clear PAUSED.
        flags &= !TorrentFlags::PAUSED.bits();
    }
    atp.set_flags(TorrentFlags::from_bits(flags));
    if o.pre_allocate_storage == Some(true) {
        atp.set_storage_mode(StorageMode::Allocate);
    }
    // Renames ride in with the add, as upstream's pre-add rename of the
    // torrent_info does, over the add dialog's payload numbering.
    let payload = atp.ti().map(|info| payload_files(&info));
    for (index, path) in o.mapped_files.iter().flatten() {
        let mapped = match &payload {
            Some(files) => usize::try_from(*index)
                .ok()
                .and_then(|i| files.get(i).copied()),
            None => Some(*index),
        };
        let Some(mapped) = mapped else {
            return Err(RpcError::call_error(format!(
                "mapped_files: no file at index {index}"
            )));
        };
        atp.add_renamed_file(mapped, path)
            .map_err(|e| RpcError::call_error(format!("mapped_files: {}", e.message())))?;
    }
    let priorities = o
        .file_priorities
        .as_deref()
        .and_then(|p| add_priorities(atp.ti().as_ref(), p));
    if let Some(priorities) = priorities {
        atp.set_file_priorities(&priorities);
    }
    if let Some(v) = o.max_upload_speed {
        atp.set_upload_limit(v);
    }
    if let Some(v) = o.max_download_speed {
        atp.set_download_limit(v);
    }
    if let Some(v) = o.max_upload_slots {
        atp.set_max_uploads(v);
    }
    if let Some(v) = o.max_connections {
        atp.set_max_connections(v);
    }
    let entry = engine.add_torrent(&mut atp).await?;
    Ok(entry.uuid.to_string())
}

async fn add_torrent_file(ctx: Ctx) -> HandlerResult {
    let ([_filename, filedump, options], []) = positional(
        "add_torrent_file",
        ctx.params,
        ["filename", "filedump", "options"],
        [],
    )?;
    let atp = atp_from_dump(&filedump)?;
    let id = add_with_options(&ctx.state.engine, atp, &options).await?;
    ok(json!(id))
}

/// Same as [`add_torrent_file`]; rsbtd persists on every add, so
/// `save_state` is meaningless.
async fn add_torrent_file_async(ctx: Ctx) -> HandlerResult {
    let ([_filename, filedump, options], [_save_state]) = positional(
        "add_torrent_file_async",
        ctx.params,
        ["filename", "filedump", "options"],
        [("save_state", json!(true))],
    )?;
    let atp = atp_from_dump(&filedump)?;
    let id = add_with_options(&ctx.state.engine, atp, &options).await?;
    ok(json!(id))
}

/// Errors-only: each failing `[filename, filedump, options]` tuple
/// contributes a message and does not stop the rest.
async fn add_torrent_files(ctx: Ctx) -> HandlerResult {
    let ([torrent_files], []) = positional("add_torrent_files", ctx.params, ["torrent_files"], [])?;
    let list = torrent_files
        .as_array()
        .ok_or_else(|| RpcError::call_error("torrent_files must be a list"))?;
    let mut errors = Vec::new();
    for tuple in list {
        let parts = tuple.as_array();
        let dump = parts.and_then(|p| p.get(1));
        let options = parts.and_then(|p| p.get(2)).unwrap_or(&Value::Null);
        let added = match dump.map(atp_from_dump) {
            Some(Ok(atp)) => add_with_options(&ctx.state.engine, atp, options)
                .await
                .map(|_| ()),
            Some(Err(e)) => Err(e),
            None => Err(RpcError::call_error(
                "torrent_files entries must be [filename, filedump, options] tuples",
            )),
        };
        if let Err(e) = added {
            errors.push(json!(e.message));
        }
    }
    ok(json!(errors))
}

async fn add_torrent_magnet(ctx: Ctx) -> HandlerResult {
    let ([uri, options], []) =
        positional("add_torrent_magnet", ctx.params, ["uri", "options"], [])?;
    let uri = uri
        .as_str()
        .ok_or_else(|| RpcError::call_error("uri must be a string"))?;
    let atp = AddTorrentParams::from_magnet_uri(uri)
        .map_err(|e| RpcError::call_error(format!("invalid magnet: {}", e.message())))?;
    let id = add_with_options(&ctx.state.engine, atp, &options).await?;
    ok(json!(id))
}

/// `[{"path", "options"}, …]`, where a `magnet:` path adds by magnet and
/// anything else is a `.torrent` on this daemon's filesystem. Each entry
/// answers `[true, <id>]` or `[false, <message>]`, never aborting the
/// batch.
async fn add_torrents(ctx: Ctx) -> HandlerResult {
    let ([torrents], []) = positional("add_torrents", ctx.params, ["torrents"], [])?;
    let list = torrents
        .as_array()
        .ok_or_else(|| RpcError::call_error("torrents must be a list"))?;
    let mut results = Vec::with_capacity(list.len());
    for entry in list {
        results.push(match add_one(&ctx.state, entry).await {
            Ok(id) => json!([true, id]),
            Err(e) => json!([false, e.message]),
        });
    }
    ok(json!(results))
}

async fn add_one(state: &DelugeState, entry: &Value) -> Result<String, RpcError> {
    let path = entry
        .get("path")
        .and_then(Value::as_str)
        .ok_or_else(|| RpcError::call_error("torrent entry must have a path string"))?;
    let options = entry.get("options").unwrap_or(&Value::Null);
    let atp = if path.starts_with("magnet:") {
        AddTorrentParams::from_magnet_uri(path)
            .map_err(|e| RpcError::call_error(format!("invalid magnet: {}", e.message())))?
    } else {
        let bytes = read_torrent_file(path)
            .await
            .map_err(|e| RpcError::call_error(format!("cannot read {path}: {e}")))?;
        AddTorrentParams::from_torrent_buffer(&bytes)
            .map_err(|e| RpcError::call_error(format!("cannot load torrent: {}", e.message())))?
    };
    add_with_options(&state.engine, atp, options).await
}

/// There is no HTTP client to fetch a remote .torrent with.
async fn add_torrent_url(ctx: Ctx) -> HandlerResult {
    let _ = ctx;
    Err(RpcError::call_error(
        "add_torrent_url is not supported by rsbtd; add by file or magnet instead",
    ))
}

/// Fails like [`add_torrent_url`], for the same reason.
async fn download_torrent_from_url(ctx: Ctx) -> HandlerResult {
    positional(
        "download_torrent_from_url",
        ctx.params,
        ["url"],
        [("cookie", Value::Null)],
    )?;
    Err(RpcError::call_error(
        "download_torrent_from_url is not supported by rsbtd; add by file or magnet instead",
    ))
}

/// libtorrent has no fetch-metadata-without-adding mode here.
async fn prefetch_magnet_metadata(ctx: Ctx) -> HandlerResult {
    let _ = ctx;
    Err(RpcError::call_error(
        "prefetch_magnet_metadata is not supported by rsbtd",
    ))
}

/// Torrent creation is exposed via the GraphQL API only.
async fn create_torrent(ctx: Ctx) -> HandlerResult {
    let _ = ctx;
    Err(RpcError::call_error(
        "create_torrent is not supported over the Deluge API",
    ))
}

async fn remove_torrent(ctx: Ctx) -> HandlerResult {
    let ([torrent_id, remove_data], []) = positional(
        "remove_torrent",
        ctx.params,
        ["torrent_id", "remove_data"],
        [],
    )?;
    let entry = lookup(&ctx.state, &torrent_id)?;
    let remove_data = remove_data.as_bool().unwrap_or(false);
    ctx.state
        .engine
        .remove_torrent(&entry.uuid, remove_data)
        .await?;
    ok(json!(true))
}

/// Returns `[[id, message], …]` for the failures.
async fn remove_torrents(ctx: Ctx) -> HandlerResult {
    let ([torrent_ids, remove_data], []) = positional(
        "remove_torrents",
        ctx.params,
        ["torrent_ids", "remove_data"],
        [],
    )?;
    let ids = torrent_ids
        .as_array()
        .ok_or_else(|| RpcError::call_error("torrent_ids must be a list"))?;
    let remove_data = remove_data.as_bool().unwrap_or(false);
    let mut failures = Vec::new();
    for id in ids {
        let removed = match lookup(&ctx.state, id) {
            Ok(entry) => ctx
                .state
                .engine
                .remove_torrent(&entry.uuid, remove_data)
                .await
                .map_err(RpcError::from),
            Err(e) => Err(e),
        };
        if let Err(e) = removed {
            failures.push(json!([id, e.message]));
        }
    }
    ok(json!(failures))
}

// ---- controls ---------------------------------------------------------------

/// Pausing detaches from auto-management first, so the queue does not
/// immediately resume the torrent.
fn pause_entries(state: &DelugeState, ids: &Value) -> Result<(), RpcError> {
    for entry in entries_or_all(state, ids)? {
        state.engine.with_handle(&entry, |h| {
            h.unset_flags(TorrentFlags::AUTO_MANAGED.bits());
            h.pause(0);
        })?;
    }
    Ok(())
}

/// Clears a stopped-on-error state, but cannot restore auto-management:
/// with no per-torrent options stored there is no remembered
/// `auto_managed` to re-apply, so a paused-then-resumed torrent stays
/// detached from the queue. Reattach with `core.set_torrent_options`.
fn resume_entries(state: &DelugeState, ids: &Value) -> Result<(), RpcError> {
    for entry in entries_or_all(state, ids)? {
        state.engine.with_handle(&entry, |h| {
            h.clear_error();
            h.resume();
        })?;
    }
    Ok(())
}

async fn pause_torrent(ctx: Ctx) -> HandlerResult {
    let ([torrent_id], []) = positional("pause_torrent", ctx.params, ["torrent_id"], [])?;
    pause_entries(&ctx.state, &torrent_id)?;
    ok(Value::Null)
}

async fn pause_torrents(ctx: Ctx) -> HandlerResult {
    let ([], [torrent_ids]) = positional(
        "pause_torrents",
        ctx.params,
        [],
        [("torrent_ids", Value::Null)],
    )?;
    pause_entries(&ctx.state, &torrent_ids)?;
    ok(Value::Null)
}

async fn resume_torrent(ctx: Ctx) -> HandlerResult {
    let ([torrent_id], []) = positional("resume_torrent", ctx.params, ["torrent_id"], [])?;
    resume_entries(&ctx.state, &torrent_id)?;
    ok(Value::Null)
}

async fn resume_torrents(ctx: Ctx) -> HandlerResult {
    let ([], [torrent_ids]) = positional(
        "resume_torrents",
        ctx.params,
        [],
        [("torrent_ids", Value::Null)],
    )?;
    resume_entries(&ctx.state, &torrent_ids)?;
    ok(Value::Null)
}

async fn force_reannounce(ctx: Ctx) -> HandlerResult {
    let ([torrent_ids], []) = positional("force_reannounce", ctx.params, ["torrent_ids"], [])?;
    for entry in entries_param(&ctx.state, &torrent_ids)? {
        ctx.state
            .engine
            .with_handle(&entry, |h| h.force_reannounce(0, -1, 0))?;
    }
    ok(Value::Null)
}

async fn force_recheck(ctx: Ctx) -> HandlerResult {
    let ([torrent_ids], []) = positional("force_recheck", ctx.params, ["torrent_ids"], [])?;
    for entry in entries_param(&ctx.state, &torrent_ids)? {
        ctx.state.engine.with_handle(&entry, |h| {
            h.force_recheck();
            // libtorrent checks files only while unpaused; upstream
            // resumes alongside for the same reason.
            h.resume();
        })?;
    }
    ok(Value::Null)
}

async fn connect_peer(ctx: Ctx) -> HandlerResult {
    let ([torrent_id, ip, port], []) =
        positional("connect_peer", ctx.params, ["torrent_id", "ip", "port"], [])?;
    let entry = lookup(&ctx.state, &torrent_id)?;
    let ip: std::net::IpAddr = ip
        .as_str()
        .and_then(|ip| ip.parse().ok())
        .ok_or_else(|| RpcError::call_error("ip must be an IP address literal"))?;
    let port = port
        .as_u64()
        .and_then(|p| u16::try_from(p).ok())
        .ok_or_else(|| RpcError::call_error("port must be an integer in 0..=65535"))?;
    ctx.state
        .engine
        .with_handle(&entry, |h| {
            h.connect_peer(std::net::SocketAddr::new(ip, port))
        })?
        .map_err(EngineError::from)?;
    ok(Value::Null)
}

/// Batch queue operations order their moves by current position so that
/// a multi-selection keeps its relative order; caller order would rotate
/// the selection instead.
fn queued_selection(
    state: &DelugeState,
    ids: &Value,
) -> Result<Vec<(i32, Arc<TorrentEntry>)>, RpcError> {
    let mut selection = Vec::new();
    for entry in entries_param(state, ids)? {
        let position = state.engine.with_handle(&entry, |h| h.queue_position())?;
        selection.push((position, entry));
    }
    Ok(selection)
}

/// Seeds and finished torrents sit outside the queue at -1.
fn last_queue_position(engine: &Engine) -> i32 {
    let mut last = -1;
    for entry in all_entries(engine) {
        if let Ok(position) = engine.with_handle(&entry, |h| h.queue_position()) {
            last = last.max(position);
        }
    }
    last
}

async fn queue_top(ctx: Ctx) -> HandlerResult {
    let ([ids], []) = positional("queue_top", ctx.params, ["torrent_ids"], [])?;
    let mut selection = queued_selection(&ctx.state, &ids)?;
    selection.sort_by_key(|(position, _)| std::cmp::Reverse(*position));
    for (_, entry) in selection {
        ctx.state
            .engine
            .with_handle(&entry, |h| h.queue_position_top())?;
    }
    ok(Value::Null)
}

/// Lowest position first; a torrent below a selected one that could not
/// move stays put instead of rotating through it.
async fn queue_up(ctx: Ctx) -> HandlerResult {
    let ([ids], []) = positional("queue_up", ctx.params, ["torrent_ids"], [])?;
    let mut selection = queued_selection(&ctx.state, &ids)?;
    selection.sort_by_key(|(position, _)| *position);
    let mut moved = true;
    let mut prev_position: Option<i32> = None;
    for (position, entry) in selection {
        let blocked = !moved && prev_position.is_some_and(|prev| position - prev <= 1);
        moved = if !blocked && position != 0 {
            ctx.state
                .engine
                .with_handle(&entry, |h| h.queue_position_up())?;
            true
        } else {
            false
        };
        prev_position = Some(position);
    }
    ok(Value::Null)
}

/// Mirror of [`queue_up`], blocked at the queue bottom.
async fn queue_down(ctx: Ctx) -> HandlerResult {
    let ([ids], []) = positional("queue_down", ctx.params, ["torrent_ids"], [])?;
    let mut selection = queued_selection(&ctx.state, &ids)?;
    selection.sort_by_key(|(position, _)| std::cmp::Reverse(*position));
    let bottom = last_queue_position(&ctx.state.engine);
    let mut moved = true;
    let mut prev_position: Option<i32> = None;
    for (position, entry) in selection {
        let blocked = !moved && prev_position.is_some_and(|prev| prev - position <= 1);
        moved = if !blocked && position != bottom {
            ctx.state
                .engine
                .with_handle(&entry, |h| h.queue_position_down())?;
            true
        } else {
            false
        };
        prev_position = Some(position);
    }
    ok(Value::Null)
}

async fn queue_bottom(ctx: Ctx) -> HandlerResult {
    let ([ids], []) = positional("queue_bottom", ctx.params, ["torrent_ids"], [])?;
    let mut selection = queued_selection(&ctx.state, &ids)?;
    selection.sort_by_key(|(position, _)| *position);
    for (_, entry) in selection {
        ctx.state
            .engine
            .with_handle(&entry, |h| h.queue_position_bottom())?;
    }
    ok(Value::Null)
}

// ---- storage, metadata ------------------------------------------------------

/// Deluge's `sanitize_filepath`: backslashes become separators and each
/// component is trimmed, with empty and dots-only components (`.`, `..`)
/// dropped — a UNC root included, since its separators leave nothing
/// behind. The result is therefore a relative path that cannot climb
/// out of the save root, which libtorrent's `rename_file` does not
/// enforce by itself; the one form that would survive as an absolute
/// rename, a drive root, is rejected instead ([`None`]).
fn sanitize_path(path: &str) -> Option<String> {
    let path = path.replace('\\', "/");
    let components: Vec<&str> = path
        .split('/')
        .map(str::trim)
        .filter(|c| !c.is_empty() && !c.chars().all(|ch| ch == '.'))
        .collect();
    let drive_rooted = components
        .first()
        .and_then(|c| c.split_once(':'))
        .is_some_and(|(drive, _)| {
            !drive.is_empty() && drive.chars().all(|c| c.is_ascii_alphabetic())
        });
    (!drive_rooted).then(|| components.join("/"))
}

/// Fire-and-forget like upstream: each move runs in the background
/// through the engine's serialized move path (completion alerts carry
/// no request key, so concurrent raw moves would steal each other's
/// outcome), the `Moving` state showing meanwhile. Its turn is taken
/// before this answers, so an accepted move cannot be overtaken by a
/// later one. `dont_replace` keeps files already at the destination.
async fn move_storage(ctx: Ctx) -> HandlerResult {
    let ([torrent_ids, dest], []) =
        positional("move_storage", ctx.params, ["torrent_ids", "dest"], [])?;
    let dest = dest
        .as_str()
        .ok_or_else(|| RpcError::call_error("dest must be a string"))?;
    if dest.is_empty() {
        // libtorrent would resolve "" to the daemon's working directory.
        return Err(RpcError::call_error("dest must not be empty"));
    }
    for entry in entries_param(&ctx.state, &torrent_ids)? {
        ctx.state
            .engine
            .move_storage_detached(entry, dest.to_owned(), TorrentHandle::MOVE_DONT_REPLACE)
            .await;
    }
    ok(Value::Null)
}

async fn rename_files(ctx: Ctx) -> HandlerResult {
    let ([torrent_id, filenames], []) =
        positional("rename_files", ctx.params, ["torrent_id", "filenames"], [])?;
    let entry = lookup(&ctx.state, &torrent_id)?;
    let pairs = filenames
        .as_array()
        .ok_or_else(|| RpcError::call_error("filenames must be a list of [index, name]"))?;
    let mut renames = Vec::with_capacity(pairs.len());
    for pair in pairs {
        let index = pair
            .get(0)
            .and_then(Value::as_i64)
            .and_then(|i| i32::try_from(i).ok());
        let name = pair.get(1).and_then(Value::as_str);
        match (index, name) {
            (Some(index), Some(name)) => {
                let sanitized = sanitize_path(name).filter(|path| !path.is_empty());
                let Some(sanitized) = sanitized else {
                    return Err(RpcError::call_error(format!("invalid file name: {name}")));
                };
                renames.push((index, sanitized));
            }
            _ => {
                return Err(RpcError::call_error(
                    "filenames entries must be [index, name] pairs",
                ));
            }
        }
    }
    for (index, name) in &renames {
        ctx.state.engine.rename_file(&entry, *index, name).await?;
    }
    ok(Value::Null)
}

/// Per-file, there being no folder object to rename; the answer is
/// upstream's `DeferredList`, one `[success, result]` entry per renamed
/// child. An empty `new_folder` flattens into the parent, leaving no
/// leading separator.
async fn rename_folder(ctx: Ctx) -> HandlerResult {
    let ([torrent_id, folder, new_folder], []) = positional(
        "rename_folder",
        ctx.params,
        ["torrent_id", "folder", "new_folder"],
        [],
    )?;
    let entry = lookup(&ctx.state, &torrent_id)?;
    let folder = folder
        .as_str()
        .ok_or_else(|| RpcError::call_error("folder must be a string"))?
        .trim_end_matches('/')
        .to_owned();
    let new_folder = new_folder
        .as_str()
        .and_then(sanitize_path)
        .ok_or_else(|| RpcError::call_error("new_folder must be a relative path string"))?;
    let prefix = format!("{folder}/");
    let paths = ctx
        .state
        .engine
        .with_handle(&entry, |h| h.file_paths())?
        .map_err(EngineError::from)?
        .unwrap_or_default();
    let mut results = Vec::new();
    for (index, path) in paths.iter().enumerate() {
        if let Some(rest) = path.replace('\\', "/").strip_prefix(&prefix) {
            let renamed = if new_folder.is_empty() {
                rest.to_owned()
            } else {
                format!("{new_folder}/{rest}")
            };
            ctx.state
                .engine
                .rename_file(&entry, index as i32, &renamed)
                .await?;
            results.push(json!([true, Value::Null]));
        }
    }
    ok(json!(results))
}

async fn set_torrent_trackers(ctx: Ctx) -> HandlerResult {
    let ([torrent_id, trackers], []) = positional(
        "set_torrent_trackers",
        ctx.params,
        ["torrent_id", "trackers"],
        [],
    )?;
    let entry = lookup(&ctx.state, &torrent_id)?;
    let list = trackers
        .as_array()
        .ok_or_else(|| RpcError::call_error("trackers must be a list of {url, tier}"))?;
    let mut parsed: Vec<(String, u8)> = Vec::with_capacity(list.len());
    for tracker in list {
        let url = tracker.get("url").and_then(Value::as_str);
        let tier = tracker
            .get("tier")
            .and_then(Value::as_u64)
            .and_then(|t| u8::try_from(t).ok());
        match (url, tier) {
            (Some(url), Some(tier)) => parsed.push((url.to_owned(), tier)),
            _ => {
                return Err(RpcError::call_error(
                    "each tracker needs a url string and a tier in 0..=255",
                ));
            }
        }
    }
    let refs: Vec<(&str, u8)> = parsed
        .iter()
        .map(|(url, tier)| (url.as_str(), *tier))
        .collect();
    ctx.state
        .engine
        .with_handle(&entry, |h| h.replace_trackers(&refs))?;
    ok(Value::Null)
}

async fn get_magnet_uri(ctx: Ctx) -> HandlerResult {
    let ([torrent_id], []) = positional("get_magnet_uri", ctx.params, ["torrent_id"], [])?;
    let entry = lookup(&ctx.state, &torrent_id)?;
    let (hashes, name, url_seeds) = ctx
        .state
        .engine
        .with_handle(&entry, |h| {
            let status = h.status(TorrentHandle::QUERY_NAME)?;
            Ok::<_, rbtorrent::Error>((status.info_hashes(), status.name(), h.url_seeds()?))
        })?
        .map_err(EngineError::from)?;
    let trackers = ctx.state.engine.trackers(&entry).await?;
    let mut uri = String::from("magnet:?");
    let mut sep = "";
    if let Some(v1) = hashes.v1() {
        uri.push_str(&format!("xt=urn:btih:{v1}"));
        sep = "&";
    }
    if let Some(v2) = hashes.v2() {
        uri.push_str(&format!("{sep}xt=urn:btmh:1220{v2}"));
        sep = "&";
    }
    if !name.is_empty() {
        uri.push_str(&format!(
            "{sep}dn={}",
            crate::api::types::percent_encode(&name)
        ));
        sep = "&";
    }
    for tracker in &trackers {
        uri.push_str(&format!(
            "{sep}tr={}",
            crate::api::types::percent_encode(&tracker.url)
        ));
        sep = "&";
    }
    // Web seeds ride along as ws=, like libtorrent's make_magnet_uri.
    for seed in &url_seeds {
        uri.push_str(&format!(
            "{sep}ws={}",
            crate::api::types::percent_encode(seed)
        ));
        sep = "&";
    }
    ok(json!(uri))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitized_paths_stay_relative() {
        assert_eq!(sanitize_path("a/b.bin").as_deref(), Some("a/b.bin"));
        assert_eq!(sanitize_path(r"a\b.bin").as_deref(), Some("a/b.bin"));
        assert_eq!(
            sanitize_path("/../a/./ b /../b.bin").as_deref(),
            Some("a/b/b.bin")
        );
        assert_eq!(
            sanitize_path(r"\\server\share\x").as_deref(),
            Some("server/share/x")
        );
        assert_eq!(sanitize_path("..").as_deref(), Some(""));
        // A drive root would rename the file outside the save path.
        assert_eq!(sanitize_path(r"C:\outside\payload.bin"), None);
        assert_eq!(sanitize_path("c:/outside"), None);
        // A colon anywhere else is an ordinary file name.
        assert_eq!(sanitize_path("a/c:b").as_deref(), Some("a/c:b"));
    }
}
