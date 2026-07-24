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
 * - Keyed by `infoHash` (v1-preferred); `Torrent.id` is never
 *   used as identity.
 * - `aliases` maps every known v1/v2 hash to the canonical key: a hybrid
 *   magnet can gain its second (v1, then preferred) hash when metadata
 *   arrives, which re-keys the row. Rekeys are announced to
 *   subscribers (selection, open-details route).
 * - Referential stability: a batch replaces only rows that actually changed,
 *   so memoized row components skip re-rendering untouched torrents; a
 *   no-change batch doesn't even bump `listVersion`.
 */
export type TorrentRow = TorrentListFieldsFragment;

type RekeyListener = (oldHash: string, newHash: string) => void;
const rekeyListeners = new Set<RekeyListener>();
export function onTorrentRekey(listener: RekeyListener): () => void {
  rekeyListeners.add(listener);
  return () => rekeyListeners.delete(listener);
}

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
  byHash: ReadonlyMap<string, TorrentRow>;
  /** Bumped once per effective change (batch, add, remove, resync). */
  listVersion: number;
  /** Resolve any known v1/v2 hash to the canonical key. */
  resolve: (hash: string) => string | undefined;
  patch: (rows: readonly TorrentRow[]) => void;
  upsert: (row: TorrentRow) => void;
  remove: (hash: string) => void;
  /** Full resync: patch + delete rows the daemon no longer has. */
  replaceAll: (rows: readonly TorrentRow[]) => void;
  clear: () => void;
}

/** hash (either version) -> canonical key. Module-level, not reactive. */
const aliases = new Map<string, string>();

/** Rows left the session: reconcile selection and the open-details route. */
function reconcileRemoved(hashes: readonly string[]): void {
  if (hashes.length === 0) return;
  useSelection.getState().discard(hashes);
  for (const hash of hashes) useUi.getState().onTorrentGone(hash);
}

function registerAliases(row: TorrentRow, canonical: string): void {
  aliases.set(row.infoHash, canonical);
  if (row.infoHashV1) aliases.set(row.infoHashV1, canonical);
  if (row.infoHashV2) aliases.set(row.infoHashV2, canonical);
}

function dropAliasesFor(canonical: string): void {
  for (const [hash, key] of aliases) {
    if (key === canonical) aliases.delete(hash);
  }
}

/**
 * Find the existing canonical key for an incoming row by ANY of its hashes:
 * a magnet added by v2 hash gains a v1-preferred `infoHash` after metadata,
 * so the v2 alias is what links the update to the existing entry.
 */
function lookupCanonical(row: TorrentRow): string {
  return (
    aliases.get(row.infoHash) ??
    (row.infoHashV1 != null ? aliases.get(row.infoHashV1) : undefined) ??
    (row.infoHashV2 != null ? aliases.get(row.infoHashV2) : undefined) ??
    row.infoHash
  );
}

/**
 * Applies one row into a mutable map. Returns true when the map changed.
 * Handles re-keying when a torrent's preferred hash changed.
 */
function applyRow(map: Map<string, TorrentRow>, row: TorrentRow): boolean {
  const canonical = lookupCanonical(row);
  const existing = map.get(canonical);

  if (existing !== undefined && canonical !== row.infoHash) {
    // Metadata added a preferred (v1) hash: re-key to the new canonical.
    map.delete(canonical);
    dropAliasesFor(canonical);
    registerAliases(row, row.infoHash);
    map.set(row.infoHash, row);
    for (const listener of rekeyListeners) listener(canonical, row.infoHash);
    return true;
  }

  registerAliases(row, row.infoHash);
  if (existing !== undefined && rowEqual(existing, row)) return false;
  // Rows differ here; merge onto a fresh object (spread keeps unknown future fields).
  map.set(row.infoHash, existing !== undefined ? { ...existing, ...row } : row);
  return true;
}

export const useTorrents = create<TorrentsState>((set, get) => ({
  byHash: new Map(),
  listVersion: 0,

  resolve: (hash) => aliases.get(hash),

  patch: (rows) => {
    if (rows.length === 0) return;
    const next = new Map(get().byHash);
    let changed = false;
    for (const row of rows) changed = applyRow(next, row) || changed;
    if (!changed) return; // discard the clone: zero re-renders
    set((s) => ({ byHash: next, listVersion: s.listVersion + 1 }));
  },

  upsert: (row) => {
    get().patch([row]);
  },

  remove: (hash) => {
    const canonical = aliases.get(hash) ?? hash;
    if (!get().byHash.has(canonical)) return;
    const next = new Map(get().byHash);
    next.delete(canonical);
    dropAliasesFor(canonical);
    set((s) => ({ byHash: next, listVersion: s.listVersion + 1 }));
    reconcileRemoved([canonical]);
  },

  replaceAll: (rows) => {
    const prev = get().byHash;
    const next = new Map<string, TorrentRow>();
    let changed = false;
    for (const row of rows) {
      const canonical = lookupCanonical(row);
      const existing = prev.get(canonical);
      registerAliases(row, row.infoHash);
      if (existing !== undefined && canonical !== row.infoHash) {
        dropAliasesFor(canonical);
        registerAliases(row, row.infoHash);
        next.set(row.infoHash, row);
        for (const listener of rekeyListeners) listener(canonical, row.infoHash);
        changed = true;
      } else if (existing !== undefined && rowEqual(existing, row)) {
        next.set(canonical, existing); // keep identity
      } else {
        next.set(row.infoHash, existing !== undefined ? { ...existing, ...row } : row);
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
      if (!next.has(key)) {
        dropAliasesFor(key);
        removed.push(key);
      }
    }
    set((s) => ({ byHash: next, listVersion: s.listVersion + 1 }));
    reconcileRemoved(removed);
  },

  clear: () => {
    aliases.clear();
    set((s) => ({ byHash: new Map(), listVersion: s.listVersion + 1 }));
  },
}));
