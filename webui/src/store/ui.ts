// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

import { create } from 'zustand';

/**
 * Ephemeral UI state for the torrents screen: category filter, search text,
 * details selection, and dialog visibility. Not persisted.
 */
export const CATEGORY_IDS = [
  'all',
  'downloading',
  'seeding',
  'completed',
  'paused',
  'active',
  'inactive',
  'errored',
  'checking',
  'moving',
  'metadata',
] as const;
export type CategoryId = (typeof CATEGORY_IDS)[number];

export interface AddDialogInit {
  magnet?: string;
  files?: File[];
}

interface UiState {
  category: CategoryId;
  searchText: string;
  /** Torrent shown in the details panel/page (uuid). */
  detailsUuid: string | null;
  /** Mobile long-press multi-select mode. */
  selectionMode: boolean;
  /** Virtual-list scroll offsets, keyed by view, restored on remount. */
  listOffsets: Record<string, number>;

  addDialog: AddDialogInit | null;
  removeDialog: { uuids: string[] } | null;
  moveDialog: { uuids: string[] } | null;
  limitsDialog: { uuids: string[] } | null;

  setCategory: (category: CategoryId) => void;
  setSearchText: (text: string) => void;
  setDetailsUuid: (uuid: string | null) => void;
  setSelectionMode: (on: boolean) => void;
  setListOffset: (key: string, offset: number) => void;
  openAddDialog: (init?: AddDialogInit) => void;
  openRemoveDialog: (uuids: string[]) => void;
  openMoveDialog: (uuids: string[]) => void;
  openLimitsDialog: (uuids: string[]) => void;
  closeDialogs: () => void;
  /** A torrent left the session. */
  onTorrentGone: (uuid: string) => void;
}

export const useUi = create<UiState>((set) => ({
  category: 'all',
  searchText: '',
  detailsUuid: null,
  selectionMode: false,
  listOffsets: {},
  addDialog: null,
  removeDialog: null,
  moveDialog: null,
  limitsDialog: null,

  setCategory: (category) => set({ category }),
  setSearchText: (searchText) => set({ searchText }),
  setDetailsUuid: (detailsUuid) => set({ detailsUuid }),
  setSelectionMode: (selectionMode) => set({ selectionMode }),
  setListOffset: (key, offset) =>
    set((s) => ({ listOffsets: { ...s.listOffsets, [key]: offset } })),
  openAddDialog: (init) => set({ addDialog: init ?? {} }),
  openRemoveDialog: (uuids) => set({ removeDialog: { uuids } }),
  openMoveDialog: (uuids) => set({ moveDialog: { uuids } }),
  openLimitsDialog: (uuids) => set({ limitsDialog: { uuids } }),
  closeDialogs: () =>
    set({ addDialog: null, removeDialog: null, moveDialog: null, limitsDialog: null }),
  onTorrentGone: (uuid) => set((s) => (s.detailsUuid === uuid ? { detailsUuid: null } : {})),
}));
