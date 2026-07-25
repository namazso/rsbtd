// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The GraphQL query root (read surface).

use std::collections::HashSet;
use std::sync::{Arc, OnceLock};

use async_graphql::{Context, Object};
use rbtorrent::{MetricKind, StatsMetric};
use uuid::Uuid;

use super::settings::Settings;
use super::types::{
    CreateJob, IpFilterRule, SessionInfo, StatKind, StatValue, Torrent, TorrentState, VersionInfo,
};
use crate::engine::{Engine, EngineError};

pub struct QueryRoot;

/// The linked libtorrent's stats-metrics table (static per build).
fn metrics_table() -> &'static [StatsMetric] {
    static TABLE: OnceLock<Vec<StatsMetric>> = OnceLock::new();
    TABLE.get_or_init(|| {
        rbtorrent::session_stats_metrics().expect("cannot enumerate libtorrent's stats metrics")
    })
}

/// Maps a stats-counter snapshot to named values, optionally filtered
/// (shared by the `sessionStats` query and subscription).
pub(super) fn stat_values(counters: &[i64], filter: Option<&HashSet<String>>) -> Vec<StatValue> {
    metrics_table()
        .iter()
        .filter(|m| filter.is_none_or(|f| f.contains(&m.name)))
        .map(|m| StatValue {
            name: m.name.clone(),
            kind: match m.kind {
                MetricKind::Counter => StatKind::Counter,
                MetricKind::Gauge => StatKind::Gauge,
            },
            value: counters.get(m.value_index as usize).copied().unwrap_or(0),
        })
        .collect()
}

/// The read surface.
#[Object]
impl QueryRoot {
    /// Versions of the daemon and its embedded BitTorrent engine.
    async fn version(&self) -> VersionInfo {
        VersionInfo {
            daemon: env!("CARGO_PKG_VERSION").to_owned(),
            libtorrent: rbtorrent::libtorrent_version().to_owned(),
        }
    }

    /// Session-level state.
    async fn session(&self, ctx: &Context<'_>) -> async_graphql::Result<SessionInfo> {
        let engine = ctx.data::<Arc<Engine>>()?;
        Ok(SessionInfo {
            is_paused: engine.is_session_paused()?,
            is_listening: engine.is_listening()?,
            is_dht_running: engine.is_dht_running()?,
            listen_port: i32::from(engine.listen_port()?),
            ssl_listen_port: i32::from(engine.ssl_listen_port()?),
            torrent_count: engine.registry().len() as i64,
        })
    }

    /// All torrents (ordered by add time), optionally filtered by state.
    async fn torrents(
        &self,
        ctx: &Context<'_>,
        state: Option<TorrentState>,
    ) -> async_graphql::Result<Vec<Torrent>> {
        let engine = ctx.data::<Arc<Engine>>()?;
        let mut entries = engine.registry().list();
        entries.sort_by_key(|e| (e.added_at, e.id));
        let mut torrents = Vec::with_capacity(entries.len());
        for entry in entries {
            // A torrent removed mid-iteration just drops out of the list.
            let Ok(torrent) = Torrent::load(engine, entry) else {
                continue;
            };
            if state.is_none_or(|s| torrent.state_value() == s) {
                torrents.push(torrent);
            }
        }
        Ok(torrents)
    }

    /// One torrent by its uuid, or `null` if not in the session.
    async fn torrent(
        &self,
        ctx: &Context<'_>,
        uuid: Uuid,
    ) -> async_graphql::Result<Option<Torrent>> {
        let engine = ctx.data::<Arc<Engine>>()?;
        match engine.registry().find(&uuid) {
            None => Ok(None),
            Some(entry) => Ok(Some(Torrent::load(engine, entry)?)),
        }
    }

    /// One sample of the session statistics counters (all metrics, or
    /// the named subset). Unknown names are silently omitted, and
    /// results follow the daemon's fixed metric-table order, not input
    /// order. The set of metric names depends on the daemon build;
    /// discover it by querying without `names`.
    async fn session_stats(
        &self,
        ctx: &Context<'_>,
        names: Option<Vec<String>>,
    ) -> async_graphql::Result<Vec<StatValue>> {
        let engine = ctx.data::<Arc<Engine>>()?;
        let counters = engine.session_stats().await?;
        let filter: Option<HashSet<String>> = names.map(|ns| ns.into_iter().collect());
        Ok(stat_values(&counters, filter.as_ref()))
    }

    /// The daemon's effective configuration. Every public setting is
    /// an explicit, documented field; select the fields you need.
    /// Change settings with `applySettings`.
    async fn settings(&self, ctx: &Context<'_>) -> async_graphql::Result<Settings> {
        let engine = ctx.data::<Arc<Engine>>()?;
        let pack = engine.settings()?;
        Ok(super::settings::read(&pack).map_err(EngineError::from)?)
    }

    /// All torrent-creation jobs that have not been pruned (terminal jobs
    /// are kept for an hour).
    async fn create_jobs(&self, ctx: &Context<'_>) -> async_graphql::Result<Vec<CreateJob>> {
        let engine = ctx.data::<Arc<Engine>>()?;
        Ok(engine.jobs().list().iter().map(CreateJob::from).collect())
    }

    /// One torrent-creation job, or `null` if unknown or pruned.
    async fn create_job(
        &self,
        ctx: &Context<'_>,
        id: u64,
    ) -> async_graphql::Result<Option<CreateJob>> {
        let engine = ctx.data::<Arc<Engine>>()?;
        Ok(engine.jobs().get(id).as_ref().map(CreateJob::from))
    }

    /// The effective IP filter, exported as normalized non-overlapping
    /// ranges — not necessarily the same list that was submitted to
    /// `setIpFilter` (it can include explicit allow ranges, and
    /// adjacent rules may be merged or split).
    async fn ip_filter(&self, ctx: &Context<'_>) -> async_graphql::Result<Vec<IpFilterRule>> {
        let engine = ctx.data::<Arc<Engine>>()?;
        let filter = engine.get_ip_filter()?;
        let rules = filter.export().map_err(EngineError::from)?;
        Ok(rules
            .into_iter()
            .map(|(first, last, blocked)| IpFilterRule {
                first: first.to_string(),
                last: last.to_string(),
                blocked,
            })
            .collect())
    }
}
