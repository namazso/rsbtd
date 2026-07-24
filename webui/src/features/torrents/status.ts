// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

import {
  ArrowDown,
  ArrowUp,
  Check,
  CircleHelp,
  FileSearch,
  FolderInput,
  Pause,
  ScanSearch,
  TriangleAlert,
  type LucideIcon,
} from 'lucide-react';
import type { TorrentRow } from '@/store/torrents';

/**
 * UI-level status derived from state + isPaused + error (+ movingStorage),
 * used for the row icon, the progress hue, the sidebar categories, and the
 * `status:` search filter.
 */
export const UI_STATUSES = [
  'error',
  'moving',
  'checking',
  'metadata',
  'paused',
  'downloading',
  'finished',
  'seeding',
  'unknown',
] as const;
export type UiStatus = (typeof UI_STATUSES)[number];

export function uiStatus(row: TorrentRow): UiStatus {
  if (row.error != null) return 'error';
  if (row.movingStorage) return 'moving';
  if (row.state === 'CHECKING_FILES' || row.state === 'CHECKING_RESUME_DATA') return 'checking';
  if (row.state === 'DOWNLOADING_METADATA') return row.isPaused ? 'paused' : 'metadata';
  if (row.isPaused) return 'paused';
  switch (row.state) {
    case 'DOWNLOADING':
      return 'downloading';
    case 'FINISHED':
      return 'finished';
    case 'SEEDING':
      return 'seeding';
    default:
      return 'unknown';
  }
}

interface StatusStyle {
  icon: LucideIcon;
  /** text color utility */
  fg: string;
  /** background utility for progress fills */
  bg: string;
}

const STATUS_STYLE: Record<UiStatus, StatusStyle> = {
  error: { icon: TriangleAlert, fg: 'text-st-error', bg: 'bg-st-error' },
  moving: { icon: FolderInput, fg: 'text-st-check', bg: 'bg-st-check' },
  checking: { icon: ScanSearch, fg: 'text-st-check', bg: 'bg-st-check' },
  metadata: { icon: FileSearch, fg: 'text-st-download', bg: 'bg-st-download' },
  paused: { icon: Pause, fg: 'text-st-pause', bg: 'bg-st-pause' },
  downloading: { icon: ArrowDown, fg: 'text-st-download', bg: 'bg-st-download' },
  finished: { icon: Check, fg: 'text-st-seed', bg: 'bg-st-seed' },
  seeding: { icon: ArrowUp, fg: 'text-st-seed', bg: 'bg-st-seed' },
  unknown: { icon: CircleHelp, fg: 'text-st-pause', bg: 'bg-st-pause' },
};

export function statusStyle(status: UiStatus): StatusStyle {
  return STATUS_STYLE[status];
}

/** Rank for sorting by status (errors first, then activity). */
export function statusRank(status: UiStatus): number {
  return UI_STATUSES.indexOf(status);
}
