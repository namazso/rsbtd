// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

import { create } from 'zustand';
import type { TorrentListFieldsFragment } from '@/gen/gql/graphql';
import { useSelection } from '@/store/selection';
import { useUi } from '@/store/ui';

/**
 * Live torrent state, patched ~1/s from torrentChanged batches.
 *
 * - Keyed by `uuid`: the daemon-minted durable identity. Info-hashes are
 *   display data only, never identity.
 * - Referential stability: a batch replaces only rows that actually changed,
 *   so memoized row components skip re-rendering untouched torrents; a
 *   no-change batch doesn't even bump `listVersion`.
 */
export type TorrentRow = TorrentListFieldsFragment;

/** Field-aware shallow equality over the TorrentListFields shape. */
export function rowEqual(a: TorrentRow, b: TorrentRow): boolean {
  for (const key of Object.keys(b) as (keyof TorrentRow)[]) {
    const va = a[key];
    const vb = b[key];
    if (Object.is(va, vb)) continue;
    if (key === 'error') {
      const ea = a.error;
      const eb = b.error;
      if (ea == null || eb == null) return false; // one side null, other not
      if (ea.message !== eb.message || ea.file !== eb.file) return false;
      continue;
    }
    if (key === 'flags') {
      const fa = a.flags;
      const fb = b.flags;
      if (fa.length !== fb.length) return false;
      let equal = true;
      for (let i = 0; i < fa.length; i++) {
        if (fa[i] !== fb[i]) {
          equal = false;
          break;
        }
      }
      if (!equal) return false;
      continue;
    }
    return false;
  }
  return true;
}

interface TorrentsState {
  byUuid: ReadonlyMap<string, TorrentRow>;
  /** Bumped once per effective change (batch, add, remove, resync). */
  listVersion: number;
  patch: (rows: readonly TorrentRow[]) => void;
  upsert: (row: TorrentRow) => void;
  remove: (uuid: string) => void;
  /** Full resync: patch + delete rows the daemon no longer has. */
  replaceAll: (rows: readonly TorrentRow[]) => void;
  clear: () => void;
}

/** Rows left the session: reconcile selection and the open-details route. */
function reconcileRemoved(uuids: readonly string[]): void {
  if (uuids.length === 0) return;
  useSelection.getState().discard(uuids);
  for (const uuid of uuids) useUi.getState().onTorrentGone(uuid);
}

/** Applies one row into a mutable map. Returns true when the map changed. */
function applyRow(map: Map<string, TorrentRow>, row: TorrentRow): boolean {
  const existing = map.get(row.uuid);
  if (existing !== undefined && rowEqual(existing, row)) return false;
  // Rows differ here; merge onto a fresh object (spread keeps unknown future fields).
  map.set(row.uuid, existing !== undefined ? { ...existing, ...row } : row);
  return true;
}

export const useTorrents = create<TorrentsState>((set, get) => ({
  byUuid: new Map(),
  listVersion: 0,

  patch: (rows) => {
    if (rows.length === 0) return;
    const next = new Map(get().byUuid);
    let changed = false;
    for (const row of rows) changed = applyRow(next, row) || changed;
    if (!changed) return; // discard the clone: zero re-renders
    set((s) => ({ byUuid: next, listVersion: s.listVersion + 1 }));
  },

  upsert: (row) => {
    get().patch([row]);
  },

  remove: (uuid) => {
    if (get().byUuid.has(uuid)) {
      const next = new Map(get().byUuid);
      next.delete(uuid);
      set((s) => ({ byUuid: next, listVersion: s.listVersion + 1 }));
    }
    reconcileRemoved([uuid]);
  },

  replaceAll: (rows) => {
    const prev = get().byUuid;
    const next = new Map<string, TorrentRow>();
    let changed = false;
    for (const row of rows) {
      const existing = prev.get(row.uuid);
      if (existing !== undefined && rowEqual(existing, row)) {
        next.set(row.uuid, existing); // keep identity
      } else {
        next.set(row.uuid, existing !== undefined ? { ...existing, ...row } : row);
        changed = true;
      }
    }
    // Deletions (daemon no longer has these) and order changes.
    if (next.size !== prev.size) changed = true;
    if (!changed) {
      // Same content; also verify same key order before discarding.
      const prevKeys = [...prev.keys()];
      const nextKeys = [...next.keys()];
      for (let i = 0; i < prevKeys.length; i++) {
        if (prevKeys[i] !== nextKeys[i]) {
          changed = true;
          break;
        }
      }
    }
    if (!changed) return;
    const removed: string[] = [];
    for (const key of prev.keys()) {
      if (!next.has(key)) removed.push(key);
    }
    set((s) => ({ byUuid: next, listVersion: s.listVersion + 1 }));
    reconcileRemoved(removed);
  },

  clear: () => {
    set((s) => ({ byUuid: new Map(), listVersion: s.listVersion + 1 }));
  },
}));
