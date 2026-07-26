// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

import { ClipboardCopy } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { mutations } from '@/api/mutations';
import { copyTextFrom } from '@/lib/clipboard';
import { refreshTorrent } from '@/api/live';
import { Button } from '@/components/ui/button';
import { formatBytes } from '@/lib/format';
import type { TorrentRow } from '@/store/torrents';
import { fieldLabel, formatFieldValue, TORRENT_FIELDS } from '@/features/torrents/fields';
import { PieceBar } from './PieceBar';
import { usePieces } from './useDetailQueries';

const FIELD_MAP = new Map(TORRENT_FIELDS.map((f) => [f.key, f]));

function StatList({ keys, row }: { keys: readonly string[]; row: TorrentRow }) {
  return (
    <dl className="grid grid-cols-[minmax(0,auto)_minmax(0,1fr)] gap-x-4 gap-y-1 text-[13px]">
      {keys.map((key) => {
        const def = FIELD_MAP.get(key);
        if (!def) return null;
        return (
          <div key={key} className="contents">
            <dt className="whitespace-nowrap text-muted-foreground">{fieldLabel(key)}</dt>
            <dd className="truncate text-right tabular-nums" title={formatFieldValue(def, row)}>
              {formatFieldValue(def, row)}
            </dd>
          </div>
        );
      })}
    </dl>
  );
}

const TRANSFER_KEYS = [
  'totalWantedDone',
  'totalWanted',
  'totalDone',
  'allTimeDownload',
  'allTimeUpload',
  'totalDownload',
  'totalUpload',
  'totalFailedBytes',
  'totalRedundantBytes',
  'ratio',
  'downloadLimit',
  'uploadLimit',
] as const;

const SWARM_KEYS = [
  'numSeeds',
  'numPeers',
  'connectCandidates',
  'numComplete',
  'numIncomplete',
  'numConnections',
  'numUploads',
  'distributedCopies',
  'seedRank',
] as const;

const META_KEYS = [
  'addedTime',
  'completedTime',
  'lastSeenComplete',
  'nextAnnounceSeconds',
  'currentTracker',
  'savePath',
  'storageMode',
  'isPrivate',
  'pieceLength',
  'state',
] as const;

/** Total piece count from row metadata, while the pieces query (whose
 * `total` is authoritative) is still in flight. */
export function totalPieceCount(row: TorrentRow): number {
  if (row.pieceLength != null && row.pieceLength > 0 && row.totalSize != null) {
    return Math.ceil(row.totalSize / row.pieceLength);
  }
  return 0;
}

export function GeneralTab({
  row,
  uuid,
  visible,
}: {
  row: TorrentRow;
  uuid: string;
  visible: boolean;
}) {
  const { t } = useTranslation('details');
  const pieces = usePieces(uuid, visible);
  const pieceInfo = pieces.data?.data?.torrent?.pieces;
  const total = pieceInfo?.total ?? totalPieceCount(row);

  const copyMagnet = () =>
    copyTextFrom(async () => {
      const uri = await mutations.magnetUri(row.uuid);
      if (uri === null) throw new Error('magnet unavailable');
      return uri;
    });

  return (
    <div className="space-y-4 p-3">
      {row.error != null && (
        <div
          role="alert"
          className="flex items-center gap-3 rounded-md border border-st-error/40 bg-st-error/10 px-3 py-2 text-sm"
        >
          <span className="min-w-0 flex-1 truncate">{row.error.message}</span>
          <Button
            size="sm"
            variant="outline"
            onClick={() => {
              void mutations.clearError(row.uuid).then(() => refreshTorrent(row.uuid));
            }}
          >
            {t('general.clearError')}
          </Button>
        </div>
      )}

      <section>
        <div className="mb-1 flex items-baseline justify-between">
          <h3 className="text-xs font-semibold text-muted-foreground uppercase">
            {t('general.pieces')}
          </h3>
          <span className="text-xs text-muted-foreground">
            {t('general.piecesOf', {
              have: pieceInfo?.have ?? row.piecesHave,
              total,
              size: row.pieceLength != null ? formatBytes(row.pieceLength) : '…',
            })}
          </span>
        </div>
        <PieceBar
          bitfield={pieceInfo?.bitfield}
          totalPieces={total}
          className="h-3.5 w-full rounded-sm"
        />
        {pieces.error != null ? (
          <p className="mt-1 text-xs text-st-error">
            {t('loadError', { message: pieces.error.message })}
          </p>
        ) : (
          pieces.data != null &&
          pieces.data.errors.length > 0 && (
            <p className="mt-1 text-xs text-st-error">
              {t('partialError', { message: pieces.data.errors[0] })}
            </p>
          )
        )}
      </section>

      <div className="grid gap-4 md:grid-cols-3">
        <section>
          <h3 className="mb-1 text-xs font-semibold text-muted-foreground uppercase">
            {t('general.transfer')}
          </h3>
          <StatList keys={TRANSFER_KEYS} row={row} />
        </section>
        <section>
          <h3 className="mb-1 text-xs font-semibold text-muted-foreground uppercase">
            {t('general.swarm')}
          </h3>
          <StatList keys={SWARM_KEYS} row={row} />
        </section>
        <section>
          <h3 className="mb-1 text-xs font-semibold text-muted-foreground uppercase">
            {t('general.timesAndIds')}
          </h3>
          <StatList keys={META_KEYS} row={row} />
        </section>
      </div>

      <section className="space-y-1 text-xs text-muted-foreground">
        {row.infoHashV1 != null && (
          <p className="truncate font-mono" title={row.infoHashV1}>
            {row.infoHashV1}
          </p>
        )}
        {row.infoHashV2 != null && (
          <p className="truncate font-mono" title={row.infoHashV2}>
            {row.infoHashV2}
          </p>
        )}
        <Button size="sm" variant="outline" onClick={() => void copyMagnet()}>
          <ClipboardCopy />
          {t('general.copyMagnet')}
        </Button>
      </section>
    </div>
  );
}
