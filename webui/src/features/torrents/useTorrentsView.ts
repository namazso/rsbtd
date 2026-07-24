// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

import { useMemo } from 'react';
import { compileQuery, parseQuery } from '@/search/query';
import { usePrefs } from '@/store/prefs';
import { useTorrents, type TorrentRow } from '@/store/torrents';
import { useUi, CATEGORY_IDS, type CategoryId } from '@/store/ui';
import { sortTorrents } from './fields';
import { uiStatus } from './status';

/** Sidebar category predicates (counts are over ALL rows, search-independent). */
export function categoryPredicate(category: CategoryId): (row: TorrentRow) => boolean {
  switch (category) {
    case 'all':
      return () => true;
    case 'downloading':
      return (r) => {
        const s = uiStatus(r);
        return s === 'downloading' || s === 'metadata';
      };
    case 'seeding':
      return (r) => uiStatus(r) === 'seeding';
    case 'completed':
      return (r) => r.isFinished;
    case 'paused':
      return (r) => r.isPaused;
    case 'active':
      return (r) => r.downloadPayloadRate > 0 || r.uploadPayloadRate > 0;
    case 'inactive':
      return (r) => r.downloadPayloadRate === 0 && r.uploadPayloadRate === 0;
    case 'errored':
      return (r) => r.error != null;
    case 'checking':
      return (r) => uiStatus(r) === 'checking';
    case 'moving':
      return (r) => r.movingStorage;
    case 'metadata':
      return (r) => uiStatus(r) === 'metadata';
  }
}

export interface TorrentsView {
  /** Filtered + sorted rows for the list. */
  rows: TorrentRow[];
  /** View order (canonical hashes) for range selection. */
  order: string[];
  counts: Record<CategoryId, number>;
  total: number;
  /** Session-wide payload rates (over all rows, unfiltered). */
  totalDownRate: number;
  totalUpRate: number;
  /** Parsed filter-language predicate combined AND with the category. */
  searchApplied: boolean;
}

export function useTorrentsView(): TorrentsView {
  const listVersion = useTorrents((s) => s.listVersion);
  const category = useUi((s) => s.category);
  const searchText = useUi((s) => s.searchText);
  const layout = usePrefs((s) => s.tables.torrents);
  const sortKey = layout?.sortKey ?? null;
  const sortDesc = layout?.sortDesc ?? false;

  return useMemo(() => {
    const all = [...useTorrents.getState().byHash.values()];

    const counts = Object.fromEntries(CATEGORY_IDS.map((c) => [c, 0])) as Record<
      CategoryId,
      number
    >;
    let totalDownRate = 0;
    let totalUpRate = 0;
    const preds = CATEGORY_IDS.map((c) => [c, categoryPredicate(c)] as const);
    for (const row of all) {
      totalDownRate += row.downloadPayloadRate;
      totalUpRate += row.uploadPayloadRate;
      for (const [c, pred] of preds) if (pred(row)) counts[c]++;
    }

    const catPred = categoryPredicate(category);
    const searchPred = compileQuery(parseQuery(searchText));
    const filtered = all.filter((r) => catPred(r) && (searchPred === null || searchPred(r)));
    const rows = sortTorrents(filtered, sortKey, sortDesc);

    return {
      rows,
      order: rows.map((r) => r.infoHash),
      counts,
      total: all.length,
      totalDownRate,
      totalUpRate,
      searchApplied: searchPred !== null,
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps -- listVersion is the store's change signal
  }, [listVersion, category, searchText, sortKey, sortDesc]);
}
