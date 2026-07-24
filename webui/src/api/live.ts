// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

import { toast } from 'sonner';
import i18next from 'i18next';
import { create } from 'zustand';
import { request } from '@/api/client';
import {
  TorrentByHashQuery,
  TorrentChangedSubscription,
  TorrentEventsSubscription,
  TorrentsQuery,
} from '@/api/operations/torrents';
import { SessionStatsStreamSubscription } from '@/api/operations/stats';
import { subscribeRetrying } from '@/api/ws';
import { connection, setSessionError, useConnection } from '@/store/connection';
import { usePrefs } from '@/store/prefs';
import { onTorrentRekey, useTorrents } from '@/store/torrents';
import { useSelection } from '@/store/selection';
import { useStatusStats } from '@/store/statusStats';
import { useUi } from '@/store/ui';
import type { ResultOf } from '@graphql-typed-document-node/core';

/**
 * App-level live data wiring:
 *  - exactly one `torrentChanged` subscription (it activates the daemon's
 *    ~1/s ticker) feeding the torrent store;
 *  - exactly one `torrentEvents` subscription for adds/removals/toasts;
 *  - full resync (torrents query) on every (re)connect — the ticker is
 *    row truth;
 *  - fallback HTTP polling while the WebSocket is unavailable (`degraded`).
 */
type TorrentEvent = ResultOf<typeof TorrentEventsSubscription>['torrentEvents'];

let started = false;
let subDisposers: (() => void)[] = [];
let pollTimer: ReturnType<typeof setTimeout> | null = null;
let polling = false;
/** Identifies the active poll chain so a stale one cannot reschedule. */
let pollChain = 0;

/** True once a full snapshot has been applied (deep links wait on this). */
export const useSynced = create<{ synced: boolean }>(() => ({ synced: false }));

interface InflightResync {
  abort: AbortController;
  generation: number;
  /** Live updates queued while the snapshot is in flight (ordering barrier). */
  queued: (() => void)[];
}

let inflight: InflightResync | null = null;

/** Apply a live update now, or after the in-flight snapshot has landed. */
function applyLive(apply: () => void): void {
  if (inflight !== null) inflight.queued.push(apply);
  else apply();
}

export function startLive(): void {
  if (started) return;
  started = true;

  if (import.meta.env.DEV) {
    // Perf fixture: rsbtdMock(5000) in the console seeds synthetic rows.
    void import('@/lib/devMock').then((m) => m.installDevMock());
  }

  // Keep selection and the open-details route in step with the store.
  onTorrentRekey((oldHash, newHash) => {
    useSelection.getState().migrate(oldHash, newHash);
    useUi.getState().onTorrentRekeyed(oldHash, newHash);
  });

  let prev = useConnection.getState();
  useConnection.subscribe((snap) => {
    if (snap.generation !== prev.generation && snap.state === 'up') onUp();
    // An endpoint change that only reaches HTTP-degraded mode still
    // switches daemons: invalidate the synced flag and per-daemon prefs;
    // the poll loop delivers the new snapshot.
    if (snap.generation !== prev.generation && snap.state === 'degraded') {
      useSynced.setState({ synced: false });
      usePrefs.getState().refreshSavePaths();
    }
    if (snap.state !== prev.state) {
      if (snap.state === 'degraded') startPolling();
      else stopPolling();
      if (snap.state === 'authRequired' || snap.state === 'down') stopSubs();
    }
    prev = snap;
  });

  connection.start();
}

async function resync(): Promise<void> {
  const generation = useConnection.getState().generation;
  const superseded = inflight;
  const current: InflightResync = {
    abort: new AbortController(),
    generation,
    queued: superseded !== null && superseded.generation === generation ? superseded.queued : [],
  };
  inflight = current;
  superseded?.abort.abort();
  try {
    const data = await request(TorrentsQuery, undefined, { signal: current.abort.signal });
    if (inflight !== current || useConnection.getState().generation !== generation) return;
    useTorrents.getState().replaceAll(data.torrents);
    useSynced.setState({ synced: true });
  } catch {
    // Reachability problems surface through the connection layer, but in
    // `up` mode nothing else re-triggers a snapshot: without a retry a
    // transient failure would leave the UI unsynced (blank deep links,
    // ghost rows) until the next reconnect. Degraded mode has its own
    // poll cadence.
    if (inflight === current && useConnection.getState().generation === generation) {
      setTimeout(() => {
        const snap = useConnection.getState();
        if (snap.generation === generation && snap.state === 'up' && inflight === null) {
          void resync();
        }
      }, 3_000);
    }
  } finally {
    if (inflight === current) {
      inflight = null;
      for (const apply of current.queued) apply();
    }
  }
}

function onUp(): void {
  stopSubs();
  useSynced.setState({ synced: false });
  usePrefs.getState().refreshSavePaths();
  void resync();

  const client = connection.client;
  if (!client) return;

  subDisposers.push(
    subscribeRetrying(client, TorrentChangedSubscription, undefined, {
      next: (data) => applyLive(() => useTorrents.getState().patch(data.torrentChanged)),
    }),
    subscribeRetrying(client, TorrentEventsSubscription, undefined, {
      next: (data) => applyLive(() => handleEvent(data.torrentEvents)),
      // A gap in the event stream may have skipped removals; resnapshot.
      onRetry: () => void resync(),
    }),
    // Status bar: DHT node count (unknown names are silently omitted by
    // the daemon, so this is safe across libtorrent versions).
    subscribeRetrying(
      client,
      SessionStatsStreamSubscription,
      { intervalMs: 2_000, names: ['dht.dht_nodes'] },
      {
        next: (data) => {
          const nodes = data.sessionStats.find((s) => s.name === 'dht.dht_nodes');
          useStatusStats.setState({ dhtNodes: nodes?.value ?? null });
        },
      },
    ),
  );
}

function stopSubs(): void {
  for (const dispose of subDisposers) dispose();
  subDisposers = [];
  useStatusStats.setState({ dhtNodes: null });
}

/**
 * Polls sequentially: the next poll is scheduled only after the current
 * snapshot settles, so a query slower than the poll interval still
 * completes instead of being aborted by its successor forever.
 */
function startPolling(): void {
  if (polling) return;
  polling = true;
  pollChain += 1;
  const chain = pollChain;
  const tick = async (): Promise<void> => {
    await resync();
    if (!polling || chain !== pollChain) return;
    pollTimer = setTimeout(() => void tick(), 3_000);
  };
  void tick();
}

function stopPolling(): void {
  polling = false;
  if (pollTimer !== null) {
    clearTimeout(pollTimer);
    pollTimer = null;
  }
}

async function fetchRow(hash: string): Promise<void> {
  const generation = useConnection.getState().generation;
  try {
    const data = await request(TorrentByHashQuery, { hash });
    const row = data.torrent;
    if (row == null || useConnection.getState().generation !== generation) return;
    applyLive(() => useTorrents.getState().upsert(row));
  } catch {
    // Row will arrive via the ticker or the next resync.
  }
}

/** Refresh one torrent's flat row on demand (flag writebacks etc.). */
export const refreshTorrent = fetchRow;

function torrentName(hash: string): string {
  const store = useTorrents.getState();
  const canonical = store.resolve(hash) ?? hash;
  return store.byHash.get(canonical)?.name ?? `${hash.slice(0, 8)}…`;
}

function handleEvent(event: TorrentEvent): void {
  switch (event.__typename) {
    case 'TorrentAddedEvent':
    case 'MetadataReceivedEvent':
      void fetchRow(event.infoHash);
      break;
    case 'TorrentRemovedEvent': {
      const canonical = useTorrents.getState().resolve(event.infoHash) ?? event.infoHash;
      useTorrents.getState().remove(event.infoHash);
      useSelection.getState().discard([canonical]);
      useUi.getState().onTorrentGone(canonical);
      break;
    }
    case 'TorrentFinishedEvent':
      toast.success(i18next.t('events.finished', { name: torrentName(event.infoHash) }));
      break;
    case 'TorrentErrorEvent':
      toast.error(
        i18next.t('events.torrentError', {
          name: torrentName(event.infoHash),
          message: event.error ?? event.filename ?? '',
        }),
      );
      void fetchRow(event.infoHash);
      break;
    case 'MetadataFailedEvent':
      toast.error(
        i18next.t('events.metadataFailed', {
          name: torrentName(event.infoHash),
          message: event.error ?? '',
        }),
      );
      break;
    case 'TorrentDeletedEvent':
      toast.success(i18next.t('events.deleted', { name: torrentName(event.infoHash) }));
      break;
    case 'TorrentDeleteFailedEvent':
      toast.error(
        i18next.t('events.deleteFailed', {
          name: torrentName(event.infoHash),
          message: event.error ?? '',
        }),
      );
      break;
    case 'StorageMovedFailedEvent':
      toast.error(
        i18next.t('events.storageMoveFailed', {
          name: torrentName(event.infoHash),
          message: event.error ?? '',
        }),
      );
      break;
    case 'SessionErrorEvent':
      setSessionError(event.error ?? 'session error');
      break;
    default:
      // ResumeData*/FileRename*/StorageMoved/Scrape* are consumed by the
      // correlation registry; other events are hints we don't need.
      break;
  }
}
