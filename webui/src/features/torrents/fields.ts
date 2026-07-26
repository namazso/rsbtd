// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

import type { ReactNode } from 'react';
import {
  formatBytes,
  formatDateTime,
  formatDuration,
  formatEta,
  formatNumber,
  formatPercentPpm,
  formatRate,
} from '@/lib/format';
import { tDynamic, tEnum } from '@/lib/i18nDynamic';
import type { TorrentRow } from '@/store/torrents';
import { UI_STATUSES, uiStatus } from './status';

/**
 * THE torrent field registry: one entry per flat/derived field. Drives the
 * desktop column set (visibility menu, widths, cells), the mobile sort
 * sheet, search-filter properties + autocomplete, and sorting comparators.
 */
export type FieldType =
  | 'string'
  | 'bytes'
  | 'rate'
  | 'number'
  | 'float'
  | 'percentPpm'
  | 'bool'
  | 'enum'
  | 'date'
  | 'durationSecs'
  | 'etaSecs'
  | 'flags';

export interface TorrentFieldDef {
  key: string;
  type: FieldType;
  get: (row: TorrentRow) => unknown;
  /** Search aliases (`size:` for totalWanted…). */
  aliases?: readonly string[];
  enumValues?: readonly string[];
  /** -1 is a valid "unlimited" value (per-torrent limits). */
  unlimitedSentinel?: boolean;
  sortable?: boolean; // default true
  filterable?: boolean; // default true
  /** Custom text formatting (sentinels like -1 = unlimited). */
  format?: (row: TorrentRow) => string;
  column?: {
    defaultVisible?: boolean;
    width: number;
    align?: 'right';
  };
}

const TORRENT_STATES = [
  'CHECKING_FILES',
  'DOWNLOADING_METADATA',
  'DOWNLOADING',
  'FINISHED',
  'SEEDING',
  'CHECKING_RESUME_DATA',
  'UNKNOWN',
] as const;

export function torrentRatio(row: TorrentRow): number {
  if (row.allTimeDownload > 0) return row.allTimeUpload / row.allTimeDownload;
  return row.allTimeUpload > 0 ? Number.POSITIVE_INFINITY : 0;
}

/** Seconds until selected content completes, or null (∞). */
export function torrentEta(row: TorrentRow): number | null {
  if (row.isFinished || row.isPaused) return null;
  const remaining = row.totalWanted - row.totalWantedDone;
  if (remaining <= 0) return null;
  return row.downloadPayloadRate > 0 ? remaining / row.downloadPayloadRate : null;
}

function formatRatio(row: TorrentRow): string {
  const ratio = torrentRatio(row);
  return Number.isFinite(ratio) ? formatNumber(ratio, 2) : tDynamic('common:placeholder.infinity');
}

const dash = () => tDynamic('common:placeholder.empty');
/** -1 = unlimited, 0 = session default (per-torrent limits). */
function formatLimit(value: number): string {
  if (value < 0) return tDynamic('common:placeholder.infinity');
  if (value === 0) return tDynamic('torrents:sentinel.sessionDefault');
  return formatRate(value);
}
function formatCountLimit(value: number): string {
  if (value < 0) return tDynamic('common:placeholder.infinity');
  if (value === 0) return tDynamic('torrents:sentinel.sessionDefault');
  return formatNumber(value);
}

function field(def: TorrentFieldDef): TorrentFieldDef {
  return def;
}

export const TORRENT_FIELDS: readonly TorrentFieldDef[] = [
  field({
    key: 'status',
    type: 'enum',
    enumValues: UI_STATUSES,
    get: (r) => uiStatus(r),
    format: (r) => tEnum('torrents:status', uiStatus(r)),
    column: { defaultVisible: true, width: 110 },
  }),
  field({
    key: 'queuePosition',
    type: 'number',
    aliases: ['queue'],
    // One-based like the displayed value, so queue:1 matches the row
    // shown as #1 (the store keeps the daemon's zero-based position).
    get: (r) => (r.queuePosition == null ? null : r.queuePosition + 1),
    format: (r) => (r.queuePosition == null ? dash() : formatNumber(r.queuePosition + 1)),
    column: { defaultVisible: true, width: 44, align: 'right' },
  }),
  field({
    key: 'name',
    type: 'string',
    get: (r) => r.name,
    column: { defaultVisible: true, width: 320 },
  }),
  field({
    key: 'totalWanted',
    type: 'bytes',
    aliases: ['size'],
    get: (r) => r.totalWanted,
    column: { defaultVisible: true, width: 80, align: 'right' },
  }),
  field({
    key: 'progressPpm',
    type: 'percentPpm',
    aliases: ['progress'],
    get: (r) => r.progressPpm,
    column: { defaultVisible: true, width: 130 },
  }),
  field({
    key: 'numSeeds',
    type: 'number',
    aliases: ['seeds'],
    get: (r) => r.numSeeds,
    format: (r) => `${formatNumber(r.numSeeds)} (${formatNumber(r.listSeeds)})`,
    column: { defaultVisible: true, width: 70, align: 'right' },
  }),
  field({
    key: 'numPeers',
    type: 'number',
    aliases: ['peers'],
    get: (r) => r.numPeers,
    format: (r) => `${formatNumber(r.numPeers)} (${formatNumber(r.listPeers)})`,
    column: { defaultVisible: true, width: 70, align: 'right' },
  }),
  field({
    key: 'downloadPayloadRate',
    type: 'rate',
    aliases: ['downSpeed', 'down'],
    get: (r) => r.downloadPayloadRate,
    format: (r) => (r.downloadPayloadRate > 0 ? formatRate(r.downloadPayloadRate) : dash()),
    column: { defaultVisible: true, width: 90, align: 'right' },
  }),
  field({
    key: 'uploadPayloadRate',
    type: 'rate',
    aliases: ['upSpeed', 'up'],
    get: (r) => r.uploadPayloadRate,
    format: (r) => (r.uploadPayloadRate > 0 ? formatRate(r.uploadPayloadRate) : dash()),
    column: { defaultVisible: true, width: 90, align: 'right' },
  }),
  field({
    key: 'eta',
    type: 'etaSecs',
    get: (r) => torrentEta(r),
    format: (r) => formatEta(torrentEta(r)),
    column: { defaultVisible: true, width: 80, align: 'right' },
  }),
  field({
    key: 'ratio',
    type: 'float',
    get: (r) => torrentRatio(r),
    format: formatRatio,
    column: { defaultVisible: true, width: 60, align: 'right' },
  }),
  field({
    key: 'addedTime',
    type: 'date',
    aliases: ['added'],
    get: (r) => r.addedTime,
    column: { defaultVisible: true, width: 150 },
  }),
  field({
    key: 'state',
    type: 'enum',
    enumValues: TORRENT_STATES,
    get: (r) => r.state,
    format: (r) => tEnum('torrents:state', r.state),
    column: { width: 150 },
  }),
  field({
    key: 'totalSize',
    type: 'bytes',
    get: (r) => r.totalSize,
    column: { width: 80, align: 'right' },
  }),
  field({
    key: 'totalWantedDone',
    type: 'bytes',
    aliases: ['done'],
    get: (r) => r.totalWantedDone,
    column: { width: 80, align: 'right' },
  }),
  field({
    key: 'totalDone',
    type: 'bytes',
    get: (r) => r.totalDone,
    column: { width: 80, align: 'right' },
  }),
  field({
    key: 'sizeOnDisk',
    type: 'bytes',
    get: (r) => r.sizeOnDisk,
    format: (r) => (r.sizeOnDisk == null ? dash() : formatBytes(r.sizeOnDisk)),
    column: { width: 90, align: 'right' },
  }),
  field({
    key: 'downloadRate',
    type: 'rate',
    get: (r) => r.downloadRate,
    format: (r) => (r.downloadRate > 0 ? formatRate(r.downloadRate) : dash()),
    column: { width: 90, align: 'right' },
  }),
  field({
    key: 'uploadRate',
    type: 'rate',
    get: (r) => r.uploadRate,
    format: (r) => (r.uploadRate > 0 ? formatRate(r.uploadRate) : dash()),
    column: { width: 90, align: 'right' },
  }),
  field({
    key: 'numComplete',
    type: 'number',
    get: (r) => r.numComplete,
    column: { width: 80, align: 'right' },
  }),
  field({
    key: 'numIncomplete',
    type: 'number',
    get: (r) => r.numIncomplete,
    column: { width: 80, align: 'right' },
  }),
  field({
    key: 'listSeeds',
    type: 'number',
    get: (r) => r.listSeeds,
    column: { width: 70, align: 'right' },
  }),
  field({
    key: 'listPeers',
    type: 'number',
    get: (r) => r.listPeers,
    column: { width: 70, align: 'right' },
  }),
  field({
    key: 'connectCandidates',
    type: 'number',
    get: (r) => r.connectCandidates,
    column: { width: 80, align: 'right' },
  }),
  field({
    key: 'completedTime',
    type: 'date',
    aliases: ['completed'],
    get: (r) => r.completedTime,
    column: { width: 150 },
  }),
  field({
    key: 'lastSeenComplete',
    type: 'date',
    get: (r) => r.lastSeenComplete,
    column: { width: 150 },
  }),
  field({
    key: 'savePath',
    type: 'string',
    aliases: ['path'],
    get: (r) => r.savePath,
    column: { width: 220 },
  }),
  field({
    key: 'currentTracker',
    type: 'string',
    aliases: ['tracker'],
    get: (r) => r.currentTracker ?? '',
    format: (r) => r.currentTracker ?? dash(),
    column: { width: 220 },
  }),
  field({
    key: 'nextAnnounceSeconds',
    type: 'durationSecs',
    get: (r) => r.nextAnnounceSeconds,
    format: (r) => (r.nextAnnounceSeconds > 0 ? formatDuration(r.nextAnnounceSeconds) : dash()),
    column: { width: 90, align: 'right' },
  }),
  field({
    key: 'allTimeDownload',
    type: 'bytes',
    aliases: ['downloaded'],
    get: (r) => r.allTimeDownload,
    column: { width: 90, align: 'right' },
  }),
  field({
    key: 'allTimeUpload',
    type: 'bytes',
    aliases: ['uploaded'],
    get: (r) => r.allTimeUpload,
    column: { width: 90, align: 'right' },
  }),
  field({
    key: 'totalDownload',
    type: 'bytes',
    get: (r) => r.totalDownload,
    column: { width: 100, align: 'right' },
  }),
  field({
    key: 'totalUpload',
    type: 'bytes',
    get: (r) => r.totalUpload,
    column: { width: 100, align: 'right' },
  }),
  field({
    key: 'totalPayloadDownload',
    type: 'bytes',
    get: (r) => r.totalPayloadDownload,
    column: { width: 100, align: 'right' },
  }),
  field({
    key: 'totalPayloadUpload',
    type: 'bytes',
    get: (r) => r.totalPayloadUpload,
    column: { width: 100, align: 'right' },
  }),
  field({
    key: 'totalFailedBytes',
    type: 'bytes',
    aliases: ['wasted'],
    get: (r) => r.totalFailedBytes,
    column: { width: 90, align: 'right' },
  }),
  field({
    key: 'totalRedundantBytes',
    type: 'bytes',
    get: (r) => r.totalRedundantBytes,
    column: { width: 90, align: 'right' },
  }),
  field({
    key: 'downloadLimit',
    type: 'rate',
    unlimitedSentinel: true,
    get: (r) => r.downloadLimit,
    format: (r) => formatLimit(r.downloadLimit),
    column: { width: 90, align: 'right' },
  }),
  field({
    key: 'uploadLimit',
    type: 'rate',
    unlimitedSentinel: true,
    get: (r) => r.uploadLimit,
    format: (r) => formatLimit(r.uploadLimit),
    column: { width: 90, align: 'right' },
  }),
  field({
    key: 'uploadsLimit',
    type: 'number',
    unlimitedSentinel: true,
    get: (r) => r.uploadsLimit,
    format: (r) => formatCountLimit(r.uploadsLimit),
    column: { width: 80, align: 'right' },
  }),
  field({
    key: 'connectionsLimit',
    type: 'number',
    unlimitedSentinel: true,
    get: (r) => r.connectionsLimit,
    format: (r) => formatCountLimit(r.connectionsLimit),
    column: { width: 90, align: 'right' },
  }),
  field({
    key: 'numUploads',
    type: 'number',
    get: (r) => r.numUploads,
    column: { width: 70, align: 'right' },
  }),
  field({
    key: 'numConnections',
    type: 'number',
    get: (r) => r.numConnections,
    column: { width: 90, align: 'right' },
  }),
  field({
    key: 'distributedCopies',
    type: 'float',
    aliases: ['availability'],
    get: (r) => r.distributedCopies,
    column: { width: 90, align: 'right' },
  }),
  field({
    key: 'seedRank',
    type: 'number',
    get: (r) => r.seedRank,
    column: { width: 80, align: 'right' },
  }),
  field({
    key: 'blockSize',
    type: 'bytes',
    get: (r) => r.blockSize,
    column: { width: 80, align: 'right' },
  }),
  field({
    key: 'pieceLength',
    type: 'bytes',
    get: (r) => r.pieceLength,
    format: (r) => (r.pieceLength == null ? dash() : formatBytes(r.pieceLength)),
    column: { width: 90, align: 'right' },
  }),
  field({
    key: 'piecesHave',
    type: 'number',
    get: (r) => r.piecesHave,
    column: { width: 90, align: 'right' },
  }),
  field({
    key: 'upBandwidthQueue',
    type: 'number',
    get: (r) => r.upBandwidthQueue,
    column: { width: 70, align: 'right' },
  }),
  field({
    key: 'downBandwidthQueue',
    type: 'number',
    get: (r) => r.downBandwidthQueue,
    column: { width: 70, align: 'right' },
  }),
  field({
    key: 'storageMode',
    type: 'enum',
    enumValues: ['ALLOCATE', 'SPARSE', 'UNKNOWN'],
    get: (r) => r.storageMode,
    format: (r) => tEnum('torrents:storageMode', r.storageMode),
    column: { width: 90 },
  }),
  field({
    key: 'isPrivate',
    type: 'bool',
    aliases: ['private'],
    get: (r) => r.isPrivate,
    column: { width: 70 },
  }),
  field({ key: 'isPaused', type: 'bool', get: (r) => r.isPaused, column: { width: 70 } }),
  field({
    key: 'isAutoManaged',
    type: 'bool',
    get: (r) => r.isAutoManaged,
    column: { width: 90 },
  }),
  field({ key: 'isFinished', type: 'bool', get: (r) => r.isFinished, column: { width: 80 } }),
  field({ key: 'isSeeding', type: 'bool', get: (r) => r.isSeeding, column: { width: 80 } }),
  field({ key: 'hasMetadata', type: 'bool', get: (r) => r.hasMetadata, column: { width: 80 } }),
  field({ key: 'hasIncoming', type: 'bool', get: (r) => r.hasIncoming, column: { width: 80 } }),
  field({
    key: 'movingStorage',
    type: 'bool',
    get: (r) => r.movingStorage,
    column: { width: 80 },
  }),
  field({
    key: 'needSaveResumeData',
    type: 'bool',
    get: (r) => r.needSaveResumeData,
    column: { width: 80 },
  }),
  field({
    key: 'announcingToTrackers',
    type: 'bool',
    get: (r) => r.announcingToTrackers,
    column: { width: 80 },
  }),
  field({
    key: 'announcingToLsd',
    type: 'bool',
    get: (r) => r.announcingToLsd,
    column: { width: 80 },
  }),
  field({
    key: 'announcingToDht',
    type: 'bool',
    get: (r) => r.announcingToDht,
    column: { width: 80 },
  }),
  field({ key: 'isI2P', type: 'bool', get: (r) => r.isI2P, column: { width: 60 } }),
  field({
    key: 'error',
    type: 'string',
    get: (r) => r.error?.message ?? '',
    format: (r) => r.error?.message ?? dash(),
    column: { width: 220 },
  }),
  field({
    key: 'infoHashV1',
    type: 'string',
    aliases: ['hash', 'hashv1'],
    get: (r) => r.infoHashV1,
    format: (r) => r.infoHashV1 ?? dash(),
    column: { width: 300 },
  }),
  field({
    key: 'infoHashV2',
    type: 'string',
    aliases: ['hashv2'],
    get: (r) => r.infoHashV2,
    format: (r) => r.infoHashV2 ?? dash(),
    column: { width: 460 },
  }),
  field({
    key: 'flags',
    type: 'flags',
    get: (r) => r.flags,
    format: (r) => r.flags.join(' '),
    sortable: false,
    column: { width: 220 },
  }),
];

export const TORRENT_FIELD_MAP: ReadonlyMap<string, TorrentFieldDef> = new Map(
  TORRENT_FIELDS.map((f) => [f.key.toLowerCase(), f]),
);

/** Resolve a search-filter property name (case-insensitive, aliases). */
export function lookupTorrentField(name: string): TorrentFieldDef | undefined {
  const lower = name.toLowerCase();
  const direct = TORRENT_FIELD_MAP.get(lower);
  if (direct) return direct;
  return TORRENT_FIELDS.find((f) => f.aliases?.some((a) => a.toLowerCase() === lower));
}

export function fieldLabel(key: string): string {
  return tDynamic(`torrents:fields.${key}`);
}

/** Default text rendering for a field (custom cells may override). */
export function formatFieldValue(def: TorrentFieldDef, row: TorrentRow): string {
  if (def.format) return def.format(row);
  const value = def.get(row);
  if (value == null) return tDynamic('common:placeholder.empty');
  switch (def.type) {
    case 'bytes':
      return formatBytes(value as number);
    case 'rate':
      return formatRate(value as number);
    case 'percentPpm':
      return formatPercentPpm(value as number);
    case 'number':
      return formatNumber(value as number);
    case 'float':
      return formatNumber(value as number, 2);
    case 'date':
      return formatDateTime(value as number);
    case 'durationSecs':
      return formatDuration(value as number);
    case 'etaSecs':
      return formatEta(value as number | null);
    case 'bool':
      return tDynamic(value ? 'common:boolean.yes' : 'common:boolean.no');
    case 'enum':
    case 'string':
      return String(value);
    case 'flags':
      return (value as string[]).join(' ');
  }
}

/** Comparator for sorting; nulls sort last in ascending order. */
export function compareByField(def: TorrentFieldDef, a: TorrentRow, b: TorrentRow): number {
  if (def.key === 'status') {
    return statusCompare(a, b);
  }
  const va = def.get(a);
  const vb = def.get(b);
  if (va == null && vb == null) return 0;
  if (va == null) return 1;
  if (vb == null) return -1;
  switch (def.type) {
    case 'string':
    case 'enum':
      return String(va).localeCompare(String(vb), undefined, { sensitivity: 'base' });
    case 'bool':
      return Number(va) - Number(vb);
    case 'flags':
      return (va as string[]).length - (vb as string[]).length;
    default:
      return (va as number) - (vb as number);
  }
}

function statusCompare(a: TorrentRow, b: TorrentRow): number {
  const ra = UI_STATUSES.indexOf(uiStatus(a));
  const rb = UI_STATUSES.indexOf(uiStatus(b));
  return ra - rb;
}

export function sortTorrents(
  rows: TorrentRow[],
  sortKey: string | null,
  desc: boolean,
): TorrentRow[] {
  if (!sortKey) return rows;
  const def = lookupTorrentField(sortKey);
  if (!def || def.sortable === false) return rows;
  const dir = desc ? -1 : 1;
  return [...rows].sort((a, b) => {
    const aMissing = def.get(a) == null;
    const bMissing = def.get(b) == null;
    if (aMissing || bMissing) return Number(aMissing) - Number(bMissing);
    return dir * compareByField(def, a, b);
  });
}

export type { ReactNode };
