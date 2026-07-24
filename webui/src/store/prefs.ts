// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

import { create } from 'zustand';
import { persist } from 'zustand/middleware';
import { endpointIdentity } from '@/api/endpoint';

/**
 * Persisted UI preferences (localStorage `rsbtd.prefs`). Versioned; extend
 * `migrate` when the shape changes incompatibly.
 */
export type ThemePref = 'system' | 'light' | 'dark';

export interface TableLayout {
  order?: string[];
  hidden?: string[];
  sortKey?: string;
  sortDesc?: boolean;
  sizing?: Record<string, number>;
}

interface PrefsState {
  theme: ThemePref;
  /** Per-table column layout, keyed by tableId (torrents, files, peers…). */
  tables: Record<string, TableLayout>;
  /** Recently used save paths per daemon (endpoint identity). */
  savePathsByDaemon: Record<string, string[]>;
  /** The current daemon's save paths (most recent first; derived). */
  savePaths: string[];
  sidebarCollapsed: boolean;
  detailsPanelSize: number;
  /** Mobile list sort. */
  mobileSortKey: string;
  mobileSortDesc: boolean;

  setTheme: (theme: ThemePref) => void;
  setTableLayout: (tableId: string, layout: Partial<TableLayout>) => void;
  /** Reset order/visibility/sizing, keeping the sort. */
  resetTableLayout: (tableId: string) => void;
  addSavePath: (path: string) => void;
  /** Re-derive `savePaths` after the endpoint changed (reconnect). */
  refreshSavePaths: () => void;
  set: (partial: Partial<PrefsState>) => void;
}

export const usePrefs = create<PrefsState>()(
  persist(
    (set) => ({
      theme: 'system',
      tables: {},
      savePathsByDaemon: {},
      savePaths: [],
      sidebarCollapsed: false,
      detailsPanelSize: 320,
      mobileSortKey: 'addedTime',
      mobileSortDesc: true,

      setTheme: (theme) => set({ theme }),
      setTableLayout: (tableId, layout) =>
        set((s) => ({
          tables: { ...s.tables, [tableId]: { ...s.tables[tableId], ...layout } },
        })),
      resetTableLayout: (tableId) =>
        set((s) => {
          const prev = s.tables[tableId];
          return {
            tables: {
              ...s.tables,
              [tableId]: { sortKey: prev?.sortKey, sortDesc: prev?.sortDesc },
            },
          };
        }),
      addSavePath: (path) =>
        set((s) => {
          const daemon = endpointIdentity();
          const paths = [
            path,
            ...(s.savePathsByDaemon[daemon] ?? []).filter((p) => p !== path),
          ].slice(0, 8);
          return {
            savePathsByDaemon: { ...s.savePathsByDaemon, [daemon]: paths },
            savePaths: paths,
          };
        }),
      refreshSavePaths: () =>
        set((s) => ({ savePaths: s.savePathsByDaemon[endpointIdentity()] ?? [] })),
      set: (partial) => set(partial),
    }),
    {
      name: 'rsbtd.prefs',
      version: 2,
      migrate: (persisted) => {
        // v1 kept a single cross-daemon savePaths list; drop it.
        const state = persisted as Record<string, unknown>;
        return { ...state, savePaths: [], savePathsByDaemon: {} } as unknown as PrefsState;
      },
      merge: (persisted, current) => {
        const merged = { ...current, ...(persisted as Partial<PrefsState>) };
        merged.savePaths = merged.savePathsByDaemon[endpointIdentity()] ?? [];
        return merged;
      },
    },
  ),
);
