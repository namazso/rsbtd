// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The GraphQL mutation root; async-effect semantics are documented on
//! [`MutationRoot`].

use std::net::IpAddr;
use std::sync::Arc;

use async_graphql::{Context, Object};
use rbtorrent::{AddTorrentParams, DownloadPriority, SettingsPack, TorrentFlags, TorrentHandle};

use super::scalars::Base64Bytes;
use super::settings::{Settings, SettingsInput};
use super::types::{
    AddTorrentInput, CreateJob, CreateTorrentInput, IpFilterRuleInput, MoveMode, ScrapeResult,
    Torrent, TorrentFlag, TrackerInput, flags_to_bits, flags_to_list,
};
use crate::engine::registry::TorrentEntry;
use crate::engine::{Engine, EngineError};

pub struct MutationRoot;

/// Resolves the torrent entry for a uuid or fails with NOT_FOUND.
fn lookup(engine: &Engine, uuid: &uuid::Uuid) -> Result<Arc<TorrentEntry>, EngineError> {
    engine.registry().find(uuid).ok_or(EngineError::NotFound)
}

/// Validates priorities into libtorrent's 0..=7 range.
fn validate_priorities(values: &[i32]) -> Result<Vec<DownloadPriority>, EngineError> {
    values
        .iter()
        .map(|&p| {
            u8::try_from(p)
                .ok()
                .and_then(DownloadPriority::new)
                .ok_or_else(|| EngineError::Invalid(format!("priority {p} is outside 0..=7")))
        })
        .collect()
}

/// Requires metadata and validates a piece index against the torrent's
/// piece count: libtorrent silently ignores or asserts out-of-range
/// piece indexes depending on the call, while the mutation would still
/// report success.
fn check_piece_index(handle: &TorrentHandle<'_>, piece: i32) -> Result<(), EngineError> {
    let info = handle
        .torrent_file()
        .map_err(EngineError::from)?
        .ok_or_else(|| EngineError::Invalid("torrent metadata is not available yet".to_owned()))?;
    let count = info.num_pieces();
    if piece < 0 || piece >= count {
        return Err(EngineError::Invalid(format!(
            "piece index {piece} is outside 0..{count}"
        )));
    }
    Ok(())
}

/// The write surface. Most Boolean mutations post work asynchronously:
/// `true` means the command was accepted, not that its effect is
/// already visible. `addTorrent`, `removeTorrent`, `moveStorage`,
/// `renameFile`, `readPiece`, `scrapeTracker`, `saveResumeData`, and
/// `applySettings` wait for the outcome instead; `setTorrentFlags`
/// applies immediately and returns the resulting flags.
#[Object]
impl MutationRoot {
    /// Adds a torrent from a magnet link or a .torrent file and waits
    /// until it is registered. Adding a duplicate of an existing
    /// torrent is an error.
    async fn add_torrent(
        &self,
        ctx: &Context<'_>,
        input: AddTorrentInput,
    ) -> async_graphql::Result<Torrent> {
        let engine = ctx.data::<Arc<Engine>>()?;
        let mut atp = match (&input.magnet_uri, &input.torrent_data) {
            (Some(uri), None) => {
                AddTorrentParams::from_magnet_uri(uri).map_err(EngineError::from)?
            }
            (None, Some(data)) => {
                AddTorrentParams::from_torrent_buffer(&data.0).map_err(EngineError::from)?
            }
            _ => {
                return Err(EngineError::Invalid(
                    "provide exactly one of magnetUri or torrentData".to_owned(),
                )
                .into());
            }
        };
        atp.set_save_path(&input.save_path);
        if let Some(name) = &input.name {
            atp.set_name(name);
        }

        let mut bits = atp.flags().bits();
        if let Some(flags) = &input.flags {
            bits |= flags_to_bits(flags);
        }
        if input.paused == Some(true) {
            // Detach from auto-management, else the queue resumes it.
            bits |= TorrentFlags::PAUSED.bits();
            bits &= !TorrentFlags::AUTO_MANAGED.bits();
        }
        if let Some(sequential) = input.sequential_download {
            if sequential {
                bits |= TorrentFlags::SEQUENTIAL_DOWNLOAD.bits();
            } else {
                bits &= !TorrentFlags::SEQUENTIAL_DOWNLOAD.bits();
            }
        }
        atp.set_flags(TorrentFlags::from_bits(bits));

        for url in input.trackers.iter().flatten() {
            atp.add_tracker(url, 0);
        }
        for url in input.url_seeds.iter().flatten() {
            atp.add_url_seed(url);
        }
        // Validate the whole input before applying any of it: the atp
        // setters are infallible, so nothing downstream would.
        if let Some(limit) = input.upload_limit {
            rbtorrent::check_rate_limit(limit)?;
        }
        if let Some(limit) = input.download_limit {
            rbtorrent::check_rate_limit(limit)?;
        }
        if let Some(limit) = input.max_uploads {
            rbtorrent::check_peer_limit(limit)?;
        }
        if let Some(limit) = input.max_connections {
            rbtorrent::check_peer_limit(limit)?;
        }
        if let Some(limit) = input.upload_limit {
            atp.set_upload_limit(limit);
        }
        if let Some(limit) = input.download_limit {
            atp.set_download_limit(limit);
        }
        if let Some(limit) = input.max_uploads {
            atp.set_max_uploads(limit);
        }
        if let Some(limit) = input.max_connections {
            atp.set_max_connections(limit);
        }

        let entry = engine.add_torrent(&mut atp).await?;
        Ok(Torrent::load(engine, entry)?)
    }

    /// Removes a torrent and waits until it has left the session. With
    /// `deleteFiles`, additionally waits for the disk deletion outcome:
    /// success means the files are gone, and a failed deletion is
    /// reported as an error (the torrent is still removed).
    async fn remove_torrent(
        &self,
        ctx: &Context<'_>,
        uuid: uuid::Uuid,
        #[graphql(default = false)] delete_files: bool,
    ) -> async_graphql::Result<bool> {
        let engine = ctx.data::<Arc<Engine>>()?;
        engine.remove_torrent(&uuid, delete_files).await?;
        Ok(true)
    }

    /// Pauses a torrent. With `detach` (the default), also removes it
    /// from auto-management so the queue does not resume it. With
    /// `graceful`, outstanding downloaded blocks are waited for before
    /// disconnecting peers.
    async fn pause_torrent(
        &self,
        ctx: &Context<'_>,
        uuid: uuid::Uuid,
        #[graphql(default = true)] detach: bool,
        #[graphql(default = false)] graceful: bool,
    ) -> async_graphql::Result<bool> {
        let engine = ctx.data::<Arc<Engine>>()?;
        let entry = lookup(engine, &uuid)?;
        let flags = if graceful {
            TorrentHandle::PAUSE_GRACEFUL
        } else {
            0
        };
        engine.with_handle(&entry, |h| {
            if detach {
                h.unset_flags(TorrentFlags::AUTO_MANAGED.bits());
            }
            h.pause(flags);
        })?;
        Ok(true)
    }

    /// Resumes a paused torrent. Does not restore auto-management; set
    /// the `AUTO_MANAGED` flag separately if wanted.
    async fn resume_torrent(
        &self,
        ctx: &Context<'_>,
        uuid: uuid::Uuid,
    ) -> async_graphql::Result<bool> {
        let engine = ctx.data::<Arc<Engine>>()?;
        engine.with_handle(&*lookup(engine, &uuid)?, |h| h.resume())?;
        Ok(true)
    }

    /// Rechecks all downloaded data against the piece hashes.
    async fn force_recheck(
        &self,
        ctx: &Context<'_>,
        uuid: uuid::Uuid,
    ) -> async_graphql::Result<bool> {
        let engine = ctx.data::<Arc<Engine>>()?;
        engine.with_handle(&*lookup(engine, &uuid)?, |h| h.force_recheck())?;
        Ok(true)
    }

    /// Schedules a tracker announce `seconds` from now (0 = immediately;
    /// tracker minimum intervals are still honored). `trackerIndex` -1
    /// means all trackers, otherwise it indexes the current `trackers`
    /// list.
    async fn force_reannounce(
        &self,
        ctx: &Context<'_>,
        uuid: uuid::Uuid,
        #[graphql(default = 0)] seconds: i32,
        #[graphql(default = -1)] tracker_index: i32,
    ) -> async_graphql::Result<bool> {
        let engine = ctx.data::<Arc<Engine>>()?;
        let entry = lookup(engine, &uuid)?;
        if tracker_index != -1 {
            // libtorrent silently ignores out-of-range indexes while the
            // mutation would report success.
            let count = engine.trackers(&entry).await?.len();
            if tracker_index < 0 || usize::try_from(tracker_index).unwrap_or(usize::MAX) >= count {
                return Err(EngineError::Invalid(format!(
                    "tracker index {tracker_index} is outside 0..{count}"
                ))
                .into());
            }
        }
        engine.with_handle(&entry, |h| {
            h.force_reannounce(seconds, tracker_index, 0);
        })?;
        Ok(true)
    }

    /// Re-announces to the DHT.
    async fn force_dht_announce(
        &self,
        ctx: &Context<'_>,
        uuid: uuid::Uuid,
    ) -> async_graphql::Result<bool> {
        let engine = ctx.data::<Arc<Engine>>()?;
        engine.with_handle(&*lookup(engine, &uuid)?, |h| h.force_dht_announce())?;
        Ok(true)
    }

    /// Clears a torrent's error state so it can be resumed.
    async fn clear_error(
        &self,
        ctx: &Context<'_>,
        uuid: uuid::Uuid,
    ) -> async_graphql::Result<bool> {
        let engine = ctx.data::<Arc<Engine>>()?;
        engine.with_handle(&*lookup(engine, &uuid)?, |h| h.clear_error())?;
        Ok(true)
    }

    /// Flushes the disk cache for the torrent.
    async fn flush_cache(
        &self,
        ctx: &Context<'_>,
        uuid: uuid::Uuid,
    ) -> async_graphql::Result<bool> {
        let engine = ctx.data::<Arc<Engine>>()?;
        engine.with_handle(&*lookup(engine, &uuid)?, |h| h.flush_cache())?;
        Ok(true)
    }

    /// Generates and persists resume data for the torrent now.
    async fn save_resume_data(
        &self,
        ctx: &Context<'_>,
        uuid: uuid::Uuid,
    ) -> async_graphql::Result<bool> {
        let engine = ctx.data::<Arc<Engine>>()?;
        let entry = lookup(engine, &uuid)?;
        engine.save_resume_data(&entry).await?;
        Ok(true)
    }

    /// Moves the torrent's storage, returning the new path once the move
    /// completed (may take minutes across filesystems).
    async fn move_storage(
        &self,
        ctx: &Context<'_>,
        uuid: uuid::Uuid,
        path: String,
        #[graphql(default_with = "MoveMode::AlwaysReplaceFiles")] mode: MoveMode,
    ) -> async_graphql::Result<String> {
        let engine = ctx.data::<Arc<Engine>>()?;
        let entry = lookup(engine, &uuid)?;
        Ok(engine.move_storage(&entry, &path, mode.bits()).await?)
    }

    /// Renames a file within the torrent, returning the accepted name.
    async fn rename_file(
        &self,
        ctx: &Context<'_>,
        uuid: uuid::Uuid,
        index: i32,
        name: String,
    ) -> async_graphql::Result<String> {
        let engine = ctx.data::<Arc<Engine>>()?;
        let entry = lookup(engine, &uuid)?;
        Ok(engine.rename_file(&entry, index, &name).await?)
    }

    /// Sets one file's download priority (0 = skip, 1..=7). Errors
    /// before metadata is available or for an out-of-range index.
    async fn set_file_priority(
        &self,
        ctx: &Context<'_>,
        uuid: uuid::Uuid,
        index: i32,
        priority: i32,
    ) -> async_graphql::Result<bool> {
        let engine = ctx.data::<Arc<Engine>>()?;
        let entry = lookup(engine, &uuid)?;
        let p = validate_priorities(&[priority])?[0];
        engine
            .with_handle(&entry, |h| h.set_file_priority(index, p))?
            .map_err(EngineError::from)?;
        Ok(true)
    }

    /// Sets download priorities for all files at once, in file-index
    /// order. Send one value per file: files beyond the end of a
    /// shorter list are reset to the default priority 4, and pad files
    /// always stay at 0. Errors before metadata is available or if the
    /// list is longer than the number of files.
    async fn set_file_priorities(
        &self,
        ctx: &Context<'_>,
        uuid: uuid::Uuid,
        priorities: Vec<i32>,
    ) -> async_graphql::Result<bool> {
        let engine = ctx.data::<Arc<Engine>>()?;
        let entry = lookup(engine, &uuid)?;
        let prios = validate_priorities(&priorities)?;
        engine
            .with_handle(&entry, |h| h.prioritize_files(&prios))?
            .map_err(EngineError::from)?;
        Ok(true)
    }

    /// Sets one piece's download priority (0 = skip, 1..=7).
    async fn set_piece_priority(
        &self,
        ctx: &Context<'_>,
        uuid: uuid::Uuid,
        piece: i32,
        priority: i32,
    ) -> async_graphql::Result<bool> {
        let engine = ctx.data::<Arc<Engine>>()?;
        let entry = lookup(engine, &uuid)?;
        let p = validate_priorities(&[priority])?[0];
        engine.with_handle(&entry, |h| {
            check_piece_index(h, piece)?;
            h.set_piece_priority(piece, p).map_err(EngineError::from)
        })??;
        Ok(true)
    }

    /// Sets download priorities for pieces, in piece-index order.
    /// Pieces beyond the end of the list keep their current priority.
    /// Errors before metadata is available or if the list is longer
    /// than the number of pieces; a no-op while seeding.
    async fn set_piece_priorities(
        &self,
        ctx: &Context<'_>,
        uuid: uuid::Uuid,
        priorities: Vec<i32>,
    ) -> async_graphql::Result<bool> {
        let engine = ctx.data::<Arc<Engine>>()?;
        let entry = lookup(engine, &uuid)?;
        let prios = validate_priorities(&priorities)?;
        engine
            .with_handle(&entry, |h| h.prioritize_pieces(&prios))?
            .map_err(EngineError::from)?;
        Ok(true)
    }

    /// Requests a piece to be downloaded early (`deadlineMs` from now).
    async fn set_piece_deadline(
        &self,
        ctx: &Context<'_>,
        uuid: uuid::Uuid,
        piece: i32,
        deadline_ms: i32,
    ) -> async_graphql::Result<bool> {
        let engine = ctx.data::<Arc<Engine>>()?;
        engine.with_handle(&*lookup(engine, &uuid)?, |h| {
            check_piece_index(h, piece)?;
            h.set_piece_deadline(piece, deadline_ms, 0)
                .map_err(EngineError::from)
        })??;
        Ok(true)
    }

    /// Removes a piece's deadline.
    async fn reset_piece_deadline(
        &self,
        ctx: &Context<'_>,
        uuid: uuid::Uuid,
        piece: i32,
    ) -> async_graphql::Result<bool> {
        let engine = ctx.data::<Arc<Engine>>()?;
        engine.with_handle(&*lookup(engine, &uuid)?, |h| {
            check_piece_index(h, piece)?;
            h.reset_piece_deadline(piece);
            Ok::<_, EngineError>(())
        })??;
        Ok(true)
    }

    /// Removes all piece deadlines.
    async fn clear_piece_deadlines(
        &self,
        ctx: &Context<'_>,
        uuid: uuid::Uuid,
    ) -> async_graphql::Result<bool> {
        let engine = ctx.data::<Arc<Engine>>()?;
        engine.with_handle(&*lookup(engine, &uuid)?, |h| h.clear_piece_deadlines())?;
        Ok(true)
    }

    /// Sets per-torrent limits; omitted arguments are left unchanged.
    /// Rates are positive bytes/s; -1 removes the per-torrent limit
    /// (session-wide limits still apply). Max uploads/connections take -1 or values in
    /// 2..=16777214. An invalid value rejects the whole mutation without
    /// changing any field.
    async fn set_torrent_limits(
        &self,
        ctx: &Context<'_>,
        uuid: uuid::Uuid,
        upload_limit: Option<i32>,
        download_limit: Option<i32>,
        max_uploads: Option<i32>,
        max_connections: Option<i32>,
    ) -> async_graphql::Result<bool> {
        let engine = ctx.data::<Arc<Engine>>()?;
        let entry = lookup(engine, &uuid)?;
        // Validate the whole delta before applying any of it, so a bad
        // later value cannot leave earlier setters already applied.
        if let Some(limit) = upload_limit {
            rbtorrent::check_rate_limit(limit)?;
        }
        if let Some(limit) = download_limit {
            rbtorrent::check_rate_limit(limit)?;
        }
        if let Some(limit) = max_uploads {
            rbtorrent::check_peer_limit(limit)?;
        }
        if let Some(limit) = max_connections {
            rbtorrent::check_peer_limit(limit)?;
        }
        engine
            .with_handle(&entry, |h| {
                if let Some(limit) = upload_limit {
                    h.set_upload_limit(limit)?;
                }
                if let Some(limit) = download_limit {
                    h.set_download_limit(limit)?;
                }
                if let Some(limit) = max_uploads {
                    h.set_max_uploads(limit)?;
                }
                if let Some(limit) = max_connections {
                    h.set_max_connections(limit)?;
                }
                Ok::<_, rbtorrent::Error>(())
            })?
            .map_err(EngineError::from)?;
        Ok(true)
    }

    /// Atomically sets and unsets torrent flags, returning the
    /// resulting flag list. A flag in both lists ends up set. Some
    /// flags only matter at add time or are engine-managed; see
    /// `TorrentFlag`.
    async fn set_torrent_flags(
        &self,
        ctx: &Context<'_>,
        uuid: uuid::Uuid,
        #[graphql(default)] set: Vec<TorrentFlag>,
        #[graphql(default)] unset: Vec<TorrentFlag>,
    ) -> async_graphql::Result<Vec<TorrentFlag>> {
        let engine = ctx.data::<Arc<Engine>>()?;
        let entry = lookup(engine, &uuid)?;
        // libtorrent reads this flag only when the torrent is added;
        // accepting a runtime change would report success while updates
        // keep (or stop) flowing unchanged.
        if set.contains(&TorrentFlag::UpdateSubscribe)
            || unset.contains(&TorrentFlag::UpdateSubscribe)
        {
            return Err(EngineError::Invalid(
                "UPDATE_SUBSCRIBE can only be chosen when adding a torrent".to_owned(),
            )
            .into());
        }
        let set_bits = flags_to_bits(&set);
        let unset_bits = flags_to_bits(&unset);
        let flags = engine.with_handle(&entry, |h| {
            h.set_flags(set_bits, set_bits | unset_bits);
            h.flags()
        })?;
        Ok(flags_to_list(flags))
    }

    /// Moves the torrent to the top of the download queue.
    async fn queue_top(&self, ctx: &Context<'_>, uuid: uuid::Uuid) -> async_graphql::Result<bool> {
        let engine = ctx.data::<Arc<Engine>>()?;
        engine.with_handle(&*lookup(engine, &uuid)?, |h| h.queue_position_top())?;
        Ok(true)
    }

    /// Moves the torrent one step up the download queue.
    async fn queue_up(&self, ctx: &Context<'_>, uuid: uuid::Uuid) -> async_graphql::Result<bool> {
        let engine = ctx.data::<Arc<Engine>>()?;
        engine.with_handle(&*lookup(engine, &uuid)?, |h| h.queue_position_up())?;
        Ok(true)
    }

    /// Moves the torrent one step down the download queue.
    async fn queue_down(&self, ctx: &Context<'_>, uuid: uuid::Uuid) -> async_graphql::Result<bool> {
        let engine = ctx.data::<Arc<Engine>>()?;
        engine.with_handle(&*lookup(engine, &uuid)?, |h| h.queue_position_down())?;
        Ok(true)
    }

    /// Moves the torrent to the bottom of the download queue.
    async fn queue_bottom(
        &self,
        ctx: &Context<'_>,
        uuid: uuid::Uuid,
    ) -> async_graphql::Result<bool> {
        let engine = ctx.data::<Arc<Engine>>()?;
        engine.with_handle(&*lookup(engine, &uuid)?, |h| h.queue_position_bottom())?;
        Ok(true)
    }

    /// Sets the torrent's queue position (0-based; clamped to the end
    /// of the queue).
    async fn set_queue_position(
        &self,
        ctx: &Context<'_>,
        uuid: uuid::Uuid,
        position: i32,
    ) -> async_graphql::Result<bool> {
        if position < 0 {
            return Err(
                EngineError::Invalid(format!("queue position {position} is negative")).into(),
            );
        }
        let engine = ctx.data::<Arc<Engine>>()?;
        engine
            .with_handle(&*lookup(engine, &uuid)?, |h| h.set_queue_position(position))?
            .map_err(EngineError::from)?;
        Ok(true)
    }

    /// Adds a tracker to the torrent. `tier` must be within 0..=255
    /// (tiers are stored as 8-bit values). The change reaches the resume
    /// file with the periodic sweep or the shutdown checkpoint.
    async fn add_tracker(
        &self,
        ctx: &Context<'_>,
        uuid: uuid::Uuid,
        url: String,
        #[graphql(default = 0)] tier: i32,
    ) -> async_graphql::Result<bool> {
        let engine = ctx.data::<Arc<Engine>>()?;
        let tier = u8::try_from(tier)
            .map_err(|_| EngineError::Invalid(format!("tracker tier {tier} is outside 0..=255")))?;
        engine.with_handle(&*lookup(engine, &uuid)?, |h| h.add_tracker(&url, tier))?;
        Ok(true)
    }

    /// Replaces the torrent's full tracker list (an empty list removes
    /// all trackers), then durably persists the torrent's resume data so
    /// removals survive a restart. Tiers must be within 0..=255.
    async fn replace_trackers(
        &self,
        ctx: &Context<'_>,
        uuid: uuid::Uuid,
        trackers: Vec<TrackerInput>,
    ) -> async_graphql::Result<bool> {
        let engine = ctx.data::<Arc<Engine>>()?;
        let entry = lookup(engine, &uuid)?;
        let mut list = Vec::with_capacity(trackers.len());
        for tracker in &trackers {
            let tier = u8::try_from(tracker.tier).map_err(|_| {
                EngineError::Invalid(format!("tracker tier {} is outside 0..=255", tracker.tier))
            })?;
            list.push((tracker.url.as_str(), tier));
        }
        engine.with_handle(&entry, |h| h.replace_trackers(&list))?;
        engine.save_resume_data(&entry).await?;
        Ok(true)
    }

    /// Scrapes a tracker and waits for its response, updating the
    /// torrent's swarm counts. `trackerIndex` -1 means the last working
    /// tracker, otherwise it indexes the current `trackers` list.
    async fn scrape_tracker(
        &self,
        ctx: &Context<'_>,
        uuid: uuid::Uuid,
        #[graphql(default = -1)] tracker_index: i32,
    ) -> async_graphql::Result<ScrapeResult> {
        let engine = ctx.data::<Arc<Engine>>()?;
        let entry = lookup(engine, &uuid)?;
        let (tracker_url, complete, incomplete) =
            engine.scrape_tracker(&entry, tracker_index).await?;
        Ok(ScrapeResult {
            tracker_url,
            complete,
            incomplete,
        })
    }

    /// Adds an HTTP/web seed.
    async fn add_url_seed(
        &self,
        ctx: &Context<'_>,
        uuid: uuid::Uuid,
        url: String,
    ) -> async_graphql::Result<bool> {
        let engine = ctx.data::<Arc<Engine>>()?;
        engine.with_handle(&*lookup(engine, &uuid)?, |h| h.add_url_seed(&url))?;
        Ok(true)
    }

    /// Removes an HTTP/web seed.
    async fn remove_url_seed(
        &self,
        ctx: &Context<'_>,
        uuid: uuid::Uuid,
        url: String,
    ) -> async_graphql::Result<bool> {
        let engine = ctx.data::<Arc<Engine>>()?;
        engine.with_handle(&*lookup(engine, &uuid)?, |h| h.remove_url_seed(&url))?;
        Ok(true)
    }

    /// Manually connects a peer. `address` is an IP literal plus port —
    /// `1.2.3.4:6881` or `[2001:db8::1]:6881`; hostnames are not
    /// accepted. Acceptance does not imply a successful handshake.
    async fn connect_peer(
        &self,
        ctx: &Context<'_>,
        uuid: uuid::Uuid,
        address: String,
    ) -> async_graphql::Result<bool> {
        let engine = ctx.data::<Arc<Engine>>()?;
        let entry = lookup(engine, &uuid)?;
        let addr = address
            .parse()
            .map_err(|_| EngineError::Invalid(format!("cannot parse {address} as ip:port")))?;
        engine
            .with_handle(&entry, |h| h.connect_peer(addr))?
            .map_err(EngineError::from)?;
        Ok(true)
    }

    /// Reads one already-downloaded piece from storage and returns its
    /// exact bytes (the final piece may be shorter than `pieceLength`).
    async fn read_piece(
        &self,
        ctx: &Context<'_>,
        uuid: uuid::Uuid,
        piece: i32,
    ) -> async_graphql::Result<Base64Bytes> {
        let engine = ctx.data::<Arc<Engine>>()?;
        let entry = lookup(engine, &uuid)?;
        Ok(Base64Bytes(engine.read_piece(&entry, piece).await?))
    }

    /// Applies a settings delta atomically and persists it, returning
    /// the full new effective settings (select the fields you need).
    /// Omitted input fields are left unchanged; `null` disables the
    /// nullable groups (`proxy`, `i2p`, `outgoingPortRange`). Any
    /// validation error rejects the whole delta.
    async fn apply_settings(
        &self,
        ctx: &Context<'_>,
        input: SettingsInput,
    ) -> async_graphql::Result<Settings> {
        let engine = ctx.data::<Arc<Engine>>()?;
        let mut pack = SettingsPack::new();
        super::settings::write(&mut pack, input).map_err(EngineError::from)?;
        engine.apply_settings(&mut pack).await?;
        let effective = engine.settings()?;
        Ok(super::settings::read(&effective).map_err(EngineError::from)?)
    }

    /// Replaces the session's IP filter with the given rules and persists
    /// it. An empty list clears the filter.
    async fn set_ip_filter(
        &self,
        ctx: &Context<'_>,
        rules: Vec<IpFilterRuleInput>,
    ) -> async_graphql::Result<bool> {
        let engine = ctx.data::<Arc<Engine>>()?;
        let mut filter = rbtorrent::IpFilter::new().map_err(EngineError::from)?;
        for rule in &rules {
            let first: IpAddr = rule.first.parse().map_err(|_| {
                EngineError::Invalid(format!("cannot parse address {}", rule.first))
            })?;
            let last: IpAddr = rule
                .last
                .parse()
                .map_err(|_| EngineError::Invalid(format!("cannot parse address {}", rule.last)))?;
            filter
                .add_rule(first, last, rule.blocked)
                .map_err(EngineError::from)?;
        }
        engine.set_ip_filter(&filter).await?;
        Ok(true)
    }

    /// Starts an async torrent-creation job (hashing runs on a blocking
    /// thread; observe it via the `createJob` query or the
    /// `createJobProgress` subscription).
    async fn start_create_torrent(
        &self,
        ctx: &Context<'_>,
        input: CreateTorrentInput,
    ) -> async_graphql::Result<CreateJob> {
        let engine = ctx.data::<Arc<Engine>>()?;
        let flags = input
            .flags
            .iter()
            .flatten()
            .fold(rbtorrent::CreateFlags::empty(), |acc, f| acc | f.bits());
        if flags.contains(rbtorrent::CreateFlags::V1_ONLY | rbtorrent::CreateFlags::V2_ONLY) {
            return Err(EngineError::Invalid(
                "V1_ONLY and V2_ONLY are mutually exclusive".to_owned(),
            )
            .into());
        }
        if let Some(size) = input.piece_size
            && size != 0
            && (!size.is_power_of_two() || !(16_384..=134_217_728).contains(&size))
        {
            return Err(EngineError::Invalid(
                "pieceSize must be a power of two between 16 KiB and 128 MiB \
                 (0 or omitted = automatic)"
                    .to_owned(),
            )
            .into());
        }
        let params = crate::engine::jobs::CreateParams {
            source: input.source_path.into(),
            piece_size: input.piece_size.unwrap_or(0),
            flags,
            trackers: input
                .trackers
                .into_iter()
                .flatten()
                .map(|t| (t.url, t.tier))
                .collect(),
            url_seeds: input.url_seeds.unwrap_or_default(),
            comment: input.comment,
            creator: input.creator,
            private: input.private,
            output_path: input.output_path.map(Into::into),
        };
        let snapshot = engine.jobs().start(params).await?;
        Ok(CreateJob::from(&snapshot))
    }

    /// Cancels a running creation job; hashing stops within one piece
    /// and any partial result is discarded.
    /// Returns false if the job is unknown or already finished.
    async fn cancel_create_job(&self, ctx: &Context<'_>, id: u64) -> async_graphql::Result<bool> {
        let engine = ctx.data::<Arc<Engine>>()?;
        Ok(engine.jobs().cancel(id))
    }

    /// Pauses the whole session (all torrents).
    async fn pause_session(&self, ctx: &Context<'_>) -> async_graphql::Result<bool> {
        let engine = ctx.data::<Arc<Engine>>()?;
        engine.pause_session()?;
        Ok(true)
    }

    /// Resumes the session after `pauseSession`.
    async fn resume_session(&self, ctx: &Context<'_>) -> async_graphql::Result<bool> {
        let engine = ctx.data::<Arc<Engine>>()?;
        engine.resume_session()?;
        Ok(true)
    }

    /// Closes and reopens all listen and outgoing sockets (use after the
    /// host's network configuration changed).
    async fn reopen_network_sockets(
        &self,
        ctx: &Context<'_>,
        #[graphql(default = true)] map_ports: bool,
    ) -> async_graphql::Result<bool> {
        let engine = ctx.data::<Arc<Engine>>()?;
        engine.reopen_network_sockets(map_ports)?;
        Ok(true)
    }
}
