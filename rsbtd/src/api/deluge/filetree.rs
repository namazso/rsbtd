// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The `web.*` methods that answer with a Deluge file tree: the files
//! tab (`web.get_torrent_files`, FileTree2 over the file status keys)
//! and the add-dialog previews of a `.torrent` on disk
//! (`web.get_torrent_info`) or a magnet link (`web.get_magnet_info`).
//! rsbtd serves no upload endpoint, so `get_torrent_info`'s path must
//! already exist on this daemon's filesystem.

use std::collections::{BTreeMap, HashMap};

use rbtorrent::{AddTorrentParams, InfoHash, TorrentInfo};
use serde_json::{Map, Value, json};

use super::registry::{Access, Ctx, HandlerResult, Registry, Scope, ok, positional};
use super::status::{KeySet, TrackerCache, build_status};
use super::torrents::{lookup, read_torrent_file};

pub(super) fn register(r: &mut Registry) {
    use Access::Normal;
    use Scope::WebLocal;
    r.add("web.get_torrent_files", WebLocal, Normal, get_torrent_files);
    r.add("web.get_torrent_info", WebLocal, Normal, get_torrent_info);
    r.add("web.get_magnet_info", WebLocal, Normal, get_magnet_info);
}

async fn get_torrent_files(ctx: Ctx) -> HandlerResult {
    let ([torrent_id], []) = positional("get_torrent_files", ctx.params, ["torrent_id"], [])?;
    let entry = lookup(&ctx.state, &torrent_id)?;
    let keys = KeySet::from_names(&["files", "file_progress", "file_priorities"]);
    let session_paused = ctx.state.engine.is_session_paused()?;
    let trackers = TrackerCache::default();
    let status = build_status(&ctx.state.engine, &entry, &keys, session_paused, &trackers).await?;
    ok(files_tree(&status))
}

/// Any failure is the literal `false`, the only signal the add dialog
/// knows.
async fn get_torrent_info(ctx: Ctx) -> HandlerResult {
    let ([filename], []) = positional("get_torrent_info", ctx.params, ["filename"], [])?;
    let Some(path) = filename.as_str() else {
        return ok(json!(false));
    };
    let Ok(bytes) = read_torrent_file(path.trim()).await else {
        return ok(json!(false));
    };
    let Ok(atp) = AddTorrentParams::from_torrent_buffer(&bytes) else {
        return ok(json!(false));
    };
    let Some(info) = atp.ti() else {
        return ok(json!(false));
    };
    let Some(hash) = info_hash_hex(&info.info_hashes()) else {
        return ok(json!(false));
    };
    ok(json!({
        "name": info.name(),
        "info_hash": hash,
        "files_tree": info_files_tree(&info),
    }))
}

/// An empty object for anything unparseable. The `files_tree` really is
/// the empty string: a magnet has no file list.
async fn get_magnet_info(ctx: Ctx) -> HandlerResult {
    let ([uri], []) = positional("get_magnet_info", ctx.params, ["uri"], [])?;
    let atp = match uri.as_str().map(AddTorrentParams::from_magnet_uri) {
        Some(Ok(atp)) => atp,
        _ => return ok(json!({})),
    };
    let Some(hash) = info_hash_hex(&atp.info_hashes()) else {
        return ok(json!({}));
    };
    let name = match atp.name() {
        name if name.is_empty() => hash.clone(),
        name => name.into_owned(),
    };
    let trackers: Map<String, Value> = atp
        .trackers()
        .map(|(url, tier)| (url.into_owned(), json!(tier)))
        .collect();
    ok(json!({
        "name": name,
        "info_hash": hash,
        "files_tree": "",
        "trackers": trackers,
    }))
}

/// v1 hex when present, else the v2 hex — the same preference the
/// `hash` status key uses, where Deluge would SHA-1 the info dict.
fn info_hash_hex(hashes: &InfoHash) -> Option<String> {
    match (hashes.v1(), hashes.v2()) {
        (Some(v1), _) => Some(v1.to_string()),
        (_, Some(v2)) => Some(v2.to_string()),
        _ => None,
    }
}

/// A file tree under construction, keyed by path component.
enum Node {
    File(Map<String, Value>),
    Dir(BTreeMap<String, Node>),
}

impl Node {
    fn insert(&mut self, components: &[&str], leaf: Map<String, Value>) {
        let Node::Dir(children) = self else { return };
        match components {
            [] => {}
            [name] => {
                children.insert((*name).to_owned(), Node::File(leaf));
            }
            [name, rest @ ..] => children
                .entry((*name).to_owned())
                .or_insert_with(|| Node::Dir(BTreeMap::new()))
                .insert(rest, leaf),
        }
    }

    /// `dirs` carries per-directory aggregate fields keyed by the
    /// directory's full path; the root (empty path) never has any.
    fn into_value(self, path: &str, dirs: &HashMap<String, Map<String, Value>>) -> Value {
        match self {
            Node::File(leaf) => Value::Object(leaf),
            Node::Dir(children) => {
                let mut map = Map::new();
                map.insert("type".to_owned(), json!("dir"));
                if let Some(extra) = dirs.get(path) {
                    for (key, value) in extra {
                        map.insert(key.clone(), value.clone());
                    }
                }
                let mut contents = Map::new();
                for (name, child) in children {
                    let child_path = if path.is_empty() {
                        name.clone()
                    } else {
                        format!("{path}/{name}")
                    };
                    contents.insert(name, child.into_value(&child_path, dirs));
                }
                map.insert("contents".to_owned(), Value::Object(contents));
                Value::Object(map)
            }
        }
    }
}

fn ancestors<'a>(components: &'a [&'a str]) -> impl Iterator<Item = String> + 'a {
    (1..components.len()).map(|d| components[..d].join("/"))
}

/// File `progress` is a 0–1 fraction, and directory nodes replicate
/// Deluge's aggregates verbatim: the mixed-priority marker 9, the leaked
/// `progresses` helper list, and the `/100 … ×100` math that keeps the
/// directory total a fraction too.
fn files_tree(status: &Map<String, Value>) -> Value {
    let list = |key: &str| {
        status
            .get(key)
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
    };
    let files = list("files");
    let file_progress = list("file_progress");
    let file_priorities = list("file_priorities");

    struct DirAgg {
        size: i64,
        priority: i64,
        mixed: bool,
        progresses: Vec<f64>,
    }
    let mut root = Node::Dir(BTreeMap::new());
    let mut aggs: HashMap<String, DirAgg> = HashMap::new();
    for (i, file) in files.iter().enumerate() {
        let path = file.get("path").and_then(Value::as_str).unwrap_or_default();
        let size = file.get("size").and_then(Value::as_i64).unwrap_or(0);
        let progress = file_progress.get(i).and_then(Value::as_f64).unwrap_or(0.0);
        let priority = file_priorities.get(i).and_then(Value::as_i64).unwrap_or(4);
        let mut leaf = file.as_object().cloned().unwrap_or_default();
        leaf.insert("type".to_owned(), json!("file"));
        leaf.insert("progress".to_owned(), json!(progress));
        leaf.insert("priority".to_owned(), json!(priority));
        let components: Vec<&str> = path.split('/').filter(|c| !c.is_empty()).collect();
        for dir in ancestors(&components) {
            let agg = aggs.entry(dir).or_insert(DirAgg {
                size: 0,
                priority,
                mixed: false,
                progresses: Vec::new(),
            });
            agg.size += size;
            agg.mixed |= agg.priority != priority;
            agg.progresses.push(size as f64 * progress / 100.0);
        }
        root.insert(&components, leaf);
    }

    let dirs: HashMap<String, Map<String, Value>> = aggs
        .into_iter()
        .map(|(path, agg)| {
            let mut extra = Map::new();
            extra.insert("path".to_owned(), json!(path));
            extra.insert("size".to_owned(), json!(agg.size));
            extra.insert(
                "priority".to_owned(),
                json!(if agg.mixed { 9 } else { agg.priority }),
            );
            let progress = if agg.size > 0 {
                agg.progresses.iter().sum::<f64>() / agg.size as f64 * 100.0
            } else {
                100.0
            };
            extra.insert("progress".to_owned(), json!(progress));
            extra.insert("progresses".to_owned(), json!(agg.progresses));
            (path, extra)
        })
        .collect();
    root.into_value("", &dirs)
}

/// Multi-file torrents keep libtorrent's name-prefixed paths; a
/// single-file torrent gets Deluge's quirky typeless root. The pad files
/// libtorrent synthesizes for v2-only torrents are skipped, leaving the
/// payload files the raw BEP 52 tree would hold, numbered contiguously
/// over them — the numbering the add dialog sends back as
/// `file_priorities`.
fn info_files_tree(info: &TorrentInfo) -> Value {
    let files: Vec<(String, i64)> = info
        .files()
        .filter(|f| !f.flags().is_pad_file())
        .map(|f| (f.path().replace('\\', "/"), f.size()))
        .collect();
    if let [(path, size)] = files.as_slice()
        && !path.contains('/')
    {
        return json!({
            "contents": {
                path.as_str(): {
                    "type": "file",
                    "index": 0,
                    "length": size,
                    "download": true,
                },
            },
        });
    }

    let mut root = Node::Dir(BTreeMap::new());
    let mut lengths: HashMap<String, i64> = HashMap::new();
    for (index, (path, size)) in files.iter().enumerate() {
        let mut leaf = Map::new();
        leaf.insert("type".to_owned(), json!("file"));
        leaf.insert("path".to_owned(), json!(path));
        leaf.insert("index".to_owned(), json!(index));
        leaf.insert("length".to_owned(), json!(size));
        leaf.insert("download".to_owned(), json!(true));
        let components: Vec<&str> = path.split('/').filter(|c| !c.is_empty()).collect();
        for dir in ancestors(&components) {
            *lengths.entry(dir).or_default() += size;
        }
        root.insert(&components, leaf);
    }
    let dirs: HashMap<String, Map<String, Value>> = lengths
        .into_iter()
        .map(|(path, length)| {
            let mut extra = Map::new();
            extra.insert("length".to_owned(), json!(length));
            extra.insert("download".to_owned(), json!(true));
            (path, extra)
        })
        .collect();
    root.into_value("", &dirs)
}
