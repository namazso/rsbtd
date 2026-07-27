// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The session-wide `core.*` methods: what the daemon as a whole holds
//! and is doing — the torrent list, the listen port, the paused state,
//! libtorrent's counters — plus the daemon-side queries rsbtd cannot
//! answer, stubbed at the "nothing found" values.

use serde_json::{Map, Value, json};

use super::proto::RpcError;
use super::registry::{Access, Ctx, HandlerResult, Registry, Scope, ok, positional};
use super::torrents::all_entries;
use crate::api::query::metrics_table;

/// Deprecated aliases still accepted by `get_session_status`.
const DEPRECATED_KEYS: [(&str, &str); 23] = [
    ("allowed_upload_slots", "ses.num_unchoke_slots"),
    ("dht_node_cache", "dht.dht_node_cache"),
    ("dht_nodes", "dht.dht_nodes"),
    ("dht_torrents", "dht.dht_torrents"),
    ("down_bandwidth_bytes_queue", "net.limiter_down_bytes"),
    ("down_bandwidth_queue", "net.limiter_down_queue"),
    ("has_incoming_connections", "net.has_incoming_connections"),
    ("num_peers", "peer.num_peers_connected"),
    ("num_unchoked", "peer.num_peers_up_unchoked"),
    ("total_dht_download", "dht.dht_bytes_in"),
    ("total_dht_upload", "dht.dht_bytes_out"),
    ("total_download", "net.recv_bytes"),
    ("total_failed_bytes", "net.recv_failed_bytes"),
    ("total_ip_overhead_download", "net.recv_ip_overhead_bytes"),
    ("total_ip_overhead_upload", "net.sent_ip_overhead_bytes"),
    ("total_payload_download", "net.recv_payload_bytes"),
    ("total_payload_upload", "net.sent_payload_bytes"),
    ("total_redundant_bytes", "net.recv_redundant_bytes"),
    ("total_tracker_download", "net.recv_tracker_bytes"),
    ("total_tracker_upload", "net.sent_tracker_bytes"),
    ("total_upload", "net.sent_bytes"),
    ("up_bandwidth_bytes_queue", "net.limiter_up_bytes"),
    ("up_bandwidth_queue", "net.limiter_up_queue"),
];

pub(super) fn register(r: &mut Registry) {
    use Access::Normal;
    use Scope::Daemon;
    r.add("core.get_session_state", Daemon, Normal, get_session_state);
    r.add(
        "core.get_session_status",
        Daemon,
        Normal,
        get_session_status,
    );
    r.add("core.is_session_paused", Daemon, Normal, is_session_paused);
    r.add("core.pause_session", Daemon, Normal, pause_session);
    r.add("core.resume_session", Daemon, Normal, resume_session);
    r.add("core.get_listen_port", Daemon, Normal, get_listen_port);
    r.add(
        "core.get_libtorrent_version",
        Daemon,
        Normal,
        get_libtorrent_version,
    );
    r.add(
        "core.get_auth_levels_mappings",
        Daemon,
        Normal,
        get_auth_levels_mappings,
    );
    r.add("core.get_external_ip", Daemon, Normal, get_external_ip);
    r.add("core.test_listen_port", Daemon, Normal, test_listen_port);
    r.add("core.get_free_space", Daemon, Normal, get_free_space);
    r.add("core.get_path_size", Daemon, Normal, get_path_size);
    r.add("core.glob", Daemon, Normal, glob);
    r.add(
        "core.get_completion_paths",
        Daemon,
        Normal,
        get_completion_paths,
    );
}

async fn get_session_state(ctx: Ctx) -> HandlerResult {
    positional("get_session_state", ctx.params, [], [])?;
    let ids: Vec<String> = all_entries(&ctx.state.engine)
        .iter()
        .map(|e| e.uuid.to_string())
        .collect();
    ok(json!(ids))
}

async fn is_session_paused(ctx: Ctx) -> HandlerResult {
    positional("is_session_paused", ctx.params, [], [])?;
    ok(json!(ctx.state.engine.is_session_paused()?))
}

async fn pause_session(ctx: Ctx) -> HandlerResult {
    positional("pause_session", ctx.params, [], [])?;
    ctx.state.engine.pause_session()?;
    ok(Value::Null)
}

async fn resume_session(ctx: Ctx) -> HandlerResult {
    positional("resume_session", ctx.params, [], [])?;
    ctx.state.engine.resume_session()?;
    ok(Value::Null)
}

async fn get_listen_port(ctx: Ctx) -> HandlerResult {
    positional("get_listen_port", ctx.params, [], [])?;
    ok(json!(ctx.state.engine.listen_port()?))
}

async fn get_libtorrent_version(ctx: Ctx) -> HandlerResult {
    positional("get_libtorrent_version", ctx.params, [], [])?;
    ok(json!(rbtorrent::libtorrent_version()))
}

/// The fixed two-level model; rsbtd sessions are all NORMAL.
async fn get_auth_levels_mappings(ctx: Ctx) -> HandlerResult {
    positional("get_auth_levels_mappings", ctx.params, [], [])?;
    ok(json!([
        {"NONE": 0, "READONLY": 1, "DEFAULT": 5, "NORMAL": 5, "ADMIN": 10},
        {"0": "NONE", "1": "READONLY", "5": "NORMAL", "10": "ADMIN"},
    ]))
}

/// Absent metrics read as 0.
pub(super) fn metric_value(counters: &[i64], name: &str) -> i64 {
    metrics_table()
        .iter()
        .find(|m| m.name == name)
        .and_then(|m| counters.get(m.value_index as usize))
        .copied()
        .unwrap_or(0)
}

/// The Deluge cache-hit ratios, derived as upstream does from the disk
/// counters of the same snapshot: the fraction of blocks that needed no
/// operation of their own.
fn hit_ratio(counters: &[i64], blocks: &str, ops: &str) -> f64 {
    let blocks = metric_value(counters, blocks);
    if blocks == 0 {
        return 0.0;
    }
    (blocks - metric_value(counters, ops)) as f64 / blocks as f64
}

fn write_hit_ratio(counters: &[i64]) -> f64 {
    hit_ratio(counters, "disk.num_blocks_written", "disk.num_write_ops")
}

fn read_hit_ratio(counters: &[i64]) -> f64 {
    hit_ratio(counters, "disk.num_blocks_read", "disk.num_read_ops")
}

/// Empty `keys` returns every libtorrent metric; named keys resolve
/// directly, through the deprecated-alias table, or are silently
/// omitted. The derived rate keys (`payload_download_rate`, …) are
/// omitted: they would need a previous-sample cache.
async fn get_session_status(ctx: Ctx) -> HandlerResult {
    let ([keys], []) = positional("get_session_status", ctx.params, ["keys"], [])?;
    let keys = keys
        .as_array()
        .ok_or_else(|| RpcError::call_error("keys must be a list of strings"))?;
    let counters = ctx.state.engine.session_stats().await?;

    let mut result = Map::new();
    if keys.is_empty() {
        for metric in metrics_table() {
            result.insert(
                metric.name.clone(),
                json!(metric_value(&counters, &metric.name)),
            );
        }
        result.insert(
            "write_hit_ratio".to_owned(),
            json!(write_hit_ratio(&counters)),
        );
        result.insert(
            "read_hit_ratio".to_owned(),
            json!(read_hit_ratio(&counters)),
        );
        return ok(Value::Object(result));
    }
    for key in keys {
        let Some(key) = key.as_str() else {
            return Err(RpcError::call_error("keys must be a list of strings"));
        };
        let value = if metrics_table().iter().any(|m| m.name == key) {
            Some(json!(metric_value(&counters, key)))
        } else if let Some((_, metric)) = DEPRECATED_KEYS.iter().find(|(alias, _)| *alias == key) {
            Some(json!(metric_value(&counters, metric)))
        } else if key == "write_hit_ratio" {
            Some(json!(write_hit_ratio(&counters)))
        } else if key == "read_hit_ratio" {
            Some(json!(read_hit_ratio(&counters)))
        } else {
            None
        };
        if let Some(value) = value {
            result.insert(key.to_owned(), value);
        }
    }
    ok(Value::Object(result))
}

async fn get_external_ip(ctx: Ctx) -> HandlerResult {
    positional("get_external_ip", ctx.params, [], [])?;
    ok(Value::Null)
}

/// Null reads as "could not check".
async fn test_listen_port(ctx: Ctx) -> HandlerResult {
    positional("test_listen_port", ctx.params, [], [])?;
    ok(Value::Null)
}

/// -1 is the "inaccessible path" answer.
async fn get_free_space(ctx: Ctx) -> HandlerResult {
    positional("get_free_space", ctx.params, [], [("path", Value::Null)])?;
    ok(json!(-1))
}

async fn get_path_size(ctx: Ctx) -> HandlerResult {
    positional("get_path_size", ctx.params, ["path"], [])?;
    ok(json!(-1))
}

async fn glob(ctx: Ctx) -> HandlerResult {
    positional("glob", ctx.params, ["path"], [])?;
    ok(json!([]))
}

/// The shape the path chooser expects, with no completions.
async fn get_completion_paths(ctx: Ctx) -> HandlerResult {
    let ([args], []) = positional("get_completion_paths", ctx.params, ["args"], [])?;
    let mut result = args.as_object().cloned().unwrap_or_else(Map::new);
    result.insert("paths".to_owned(), json!([]));
    ok(Value::Object(result))
}
