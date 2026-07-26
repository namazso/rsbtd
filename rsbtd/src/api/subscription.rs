// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The GraphQL subscription root (served over graphql-ws).
//!
//! All streams end when the engine shuts down (the event bus closes),
//! which terminates the WebSocket subscription server-side.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use async_graphql::futures_util::Stream;
use async_graphql::futures_util::stream::unfold;
use async_graphql::{Context, Subscription};
use tokio::sync::broadcast::error::RecvError;
use uuid::Uuid;

use super::events::TorrentEvent;
use super::query::stat_values;
use super::types::{CreateJob, StatValue, Torrent};
use crate::engine::events::EventKind;
use crate::engine::{Engine, EngineError};

pub struct SubscriptionRoot;

/// Live event streams (served over graphql-ws). Streams have no replay
/// or resume: after a reconnect, re-query current state. All streams
/// end when the daemon shuts down.
#[Subscription]
impl SubscriptionRoot {
    /// Torrent events from the engine event bus, optionally filtered to
    /// a single torrent (which excludes session-level events, including
    /// `StateUpdateEvent` batches; filtering on an unknown torrent
    /// fails at subscription start). An unfiltered subscription
    /// activates the engine's periodic `StateUpdateEvent` batches. A
    /// slow consumer that falls behind is disconnected (the stream
    /// ends), since missed events can include removals: resubscribe and
    /// take a fresh snapshot.
    async fn torrent_events(
        &self,
        ctx: &Context<'_>,
        uuid: Option<Uuid>,
    ) -> async_graphql::Result<impl Stream<Item = TorrentEvent>> {
        let engine = Arc::clone(ctx.data::<Arc<Engine>>()?);
        let filter = uuid;

        if let Some(f) = &filter
            && engine.registry().find(f).is_none()
        {
            return Err(EngineError::NotFound.into());
        }

        // Keep the engine's state ticker posting `StateUpdate` events;
        // a filtered stream skips them, so it takes no interest.
        let interest = filter.is_none().then(|| engine.state_interest());
        let rx = engine.subscribe_events();
        Ok(unfold(
            (rx, filter, engine, interest),
            move |(mut rx, filter, engine, interest)| async move {
                loop {
                    match rx.recv().await {
                        Ok(event) => {
                            // Session-level events (no torrent) are skipped
                            // when filtering by torrent.
                            if let Some(f) = &filter
                                && event.torrent != Some(*f)
                            {
                                continue;
                            }

                            if let Some(gql_event) =
                                TorrentEvent::from_engine_event(&event, &engine)
                            {
                                return Some((gql_event, (rx, filter, engine, interest)));
                            }
                        }
                        Err(RecvError::Lagged(n)) => {
                            // Missed events may include removal tombstones;
                            // end the stream so the client resnapshots
                            // instead of keeping ghost torrents forever.
                            tracing::warn!(
                                "torrentEvents subscription lagged by {n}; ending the stream"
                            );
                            return None;
                        }
                        Err(RecvError::Closed) => return None,
                    }
                }
            },
        ))
    }

    /// Non-empty batches of status snapshots of torrents that changed,
    /// about once per second — optionally restricted to one torrent
    /// (filtering on an unknown torrent fails at subscription start).
    /// Subscribing activates the engine's periodic state updates.
    async fn torrent_changed(
        &self,
        ctx: &Context<'_>,
        uuid: Option<Uuid>,
    ) -> async_graphql::Result<impl Stream<Item = Vec<Torrent>>> {
        let engine = Arc::clone(ctx.data::<Arc<Engine>>()?);
        let filter = uuid;
        if let Some(f) = &filter
            && engine.registry().find(f).is_none()
        {
            return Err(EngineError::NotFound.into());
        }
        // Keeps the engine's state ticker posting updates.
        let interest = engine.state_interest();
        let rx = engine.subscribe_events();
        Ok(unfold((rx, interest), move |(mut rx, interest)| {
            let engine = Arc::clone(&engine);
            async move {
                loop {
                    match rx.recv().await {
                        Ok(event) => {
                            let EventKind::StateUpdate(statuses) = &event.kind else {
                                continue;
                            };
                            let torrents: Vec<Torrent> = statuses
                                .iter()
                                .filter_map(|status| {
                                    let entry = engine.registry().get(status.id())?;
                                    if let Some(f) = &filter
                                        && entry.uuid != *f
                                    {
                                        return None;
                                    }
                                    Torrent::load(&engine, entry).ok()
                                })
                                .collect();
                            if torrents.is_empty() {
                                continue;
                            }
                            return Some((torrents, (rx, interest)));
                        }
                        // The batches are deltas (torrents changed since
                        // the previous update), so skipped ones are not
                        // superseded by later ones: a torrent that
                        // changed once during the gap would stay stale
                        // forever. Recover with a full snapshot.
                        Err(RecvError::Lagged(n)) => {
                            tracing::warn!(
                                "torrentChanged subscriber lagged by {n}; emitting a full snapshot"
                            );
                            let torrents: Vec<Torrent> = engine
                                .registry()
                                .list()
                                .into_iter()
                                .filter_map(|entry| {
                                    if let Some(f) = &filter
                                        && entry.uuid != *f
                                    {
                                        return None;
                                    }
                                    Torrent::load(&engine, entry).ok()
                                })
                                .collect();
                            if torrents.is_empty() {
                                continue;
                            }
                            return Some((torrents, (rx, interest)));
                        }
                        Err(RecvError::Closed) => return None,
                    }
                }
            }
        }))
    }

    /// Progress of one torrent-creation job: the current snapshot
    /// immediately, then coalesced changes, ending with one terminal
    /// snapshot. An unknown or pruned job id fails at subscription
    /// start.
    async fn create_job_progress(
        &self,
        ctx: &Context<'_>,
        id: u64,
    ) -> async_graphql::Result<impl Stream<Item = CreateJob>> {
        let engine = ctx.data::<Arc<Engine>>()?;
        let rx = engine
            .jobs()
            .watch(id)
            .ok_or_else(|| async_graphql::Error::new(format!("unknown creation job {id}")))?;

        enum St {
            Yield(tokio::sync::watch::Receiver<crate::engine::jobs::JobSnapshot>),
            Wait(tokio::sync::watch::Receiver<crate::engine::jobs::JobSnapshot>),
            Done,
        }
        Ok(unfold(St::Yield(rx), |state| async move {
            let mut rx = match state {
                St::Yield(rx) => rx,
                St::Wait(mut rx) => {
                    // Ends when the job is pruned (sender dropped).
                    rx.changed().await.ok()?;
                    rx
                }
                St::Done => return None,
            };
            let snapshot = rx.borrow_and_update().clone();
            let item = CreateJob::from(&snapshot);
            let next = if snapshot.state.is_terminal() {
                St::Done
            } else {
                St::Wait(rx)
            };
            Some((item, next))
        }))
    }

    /// Session statistics counters: one sample immediately, then one
    /// per interval (all metrics, or the named subset; unknown names
    /// are silently omitted). `intervalMs` is clamped to at least 100;
    /// missed ticks are skipped.
    async fn session_stats(
        &self,
        ctx: &Context<'_>,
        #[graphql(default = 1000)] interval_ms: u64,
        names: Option<Vec<String>>,
    ) -> async_graphql::Result<impl Stream<Item = Vec<StatValue>>> {
        let engine = Arc::clone(ctx.data::<Arc<Engine>>()?);
        let filter: Option<HashSet<String>> = names.map(|ns| ns.into_iter().collect());
        let mut interval = tokio::time::interval(Duration::from_millis(interval_ms.max(100)));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        Ok(unfold(
            (interval, engine, filter),
            |(mut interval, engine, filter)| async move {
                interval.tick().await;
                match engine.session_stats().await {
                    Ok(counters) => {
                        let values = stat_values(&counters, filter.as_ref());
                        Some((values, (interval, engine, filter)))
                    }
                    // Shutting down: end the stream.
                    Err(_) => None,
                }
            },
        ))
    }
}
