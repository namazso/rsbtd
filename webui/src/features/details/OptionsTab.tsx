// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { toast } from 'sonner';
import { refreshTorrent } from '@/api/live';
import { mutations } from '@/api/mutations';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Switch } from '@/components/ui/switch-tabs';
import { tDynamic } from '@/lib/i18nDynamic';
import type { TorrentRow } from '@/store/torrents';
import { useUi } from '@/store/ui';
import type { TorrentFlag } from '@/gen/gql/graphql';

/**
 * Per-torrent options: rate/slot limits, behavior flags (applied as a
 * set/unset diff; the daemon's returned flag list is reconciled via a row
 * refresh), queue position, and maintenance actions.
 */
const TOGGLE_FLAGS: TorrentFlag[] = [
  'AUTO_MANAGED',
  'SEQUENTIAL_DOWNLOAD',
  'SUPER_SEEDING',
  'UPLOAD_MODE',
  'SHARE_MODE',
  'APPLY_IP_FILTER',
  'STOP_WHEN_READY',
  'DISABLE_DHT',
  'DISABLE_LSD',
  'DISABLE_PEX',
];

export function OptionsTab({ row }: { row: TorrentRow }) {
  const { t } = useTranslation(['details', 'torrents', 'common']);
  const hash = row.infoHash;
  const openMoveDialog = useUi((s) => s.openMoveDialog);
  const openLimitsDialog = useUi((s) => s.openLimitsDialog);
  const [queuePos, setQueuePos] = useState('');

  const toggleFlag = (flag: TorrentFlag, on: boolean) => {
    void mutations
      .setFlags(hash, on ? [flag] : [], on ? [] : [flag])
      .then(() => refreshTorrent(hash))
      .catch((err: unknown) => toast.error(err instanceof Error ? err.message : String(err)));
  };

  const run = (fn: () => Promise<unknown>) => {
    void fn().catch((err: unknown) =>
      toast.error(err instanceof Error ? err.message : String(err)),
    );
  };

  return (
    <div className="grid gap-6 p-3 md:grid-cols-2">
      <section>
        <h3 className="mb-2 text-xs font-semibold text-muted-foreground uppercase">
          {t('options.flags')}
        </h3>
        <div className="space-y-2.5">
          {TOGGLE_FLAGS.map((flag) => (
            <label key={flag} className="flex cursor-pointer items-start gap-3">
              <Switch
                checked={row.flags.includes(flag)}
                onCheckedChange={(v) => toggleFlag(flag, v)}
              />
              <span className="min-w-0">
                <span className="block text-sm leading-4">{tDynamic(`torrents:flag.${flag}`)}</span>
                <span className="block text-xs text-muted-foreground">
                  {tDynamic(`details:options.flagHelp.${flag}`)}
                </span>
              </span>
            </label>
          ))}
        </div>
      </section>

      <div className="space-y-6">
        <section>
          <h3 className="mb-2 text-xs font-semibold text-muted-foreground uppercase">
            {t('options.limits')}
          </h3>
          <Button size="sm" variant="outline" onClick={() => openLimitsDialog([hash])}>
            {t('torrents:actions.limits')}
          </Button>
        </section>

        <section>
          <h3 className="mb-2 text-xs font-semibold text-muted-foreground uppercase">
            {t('options.queue')}
          </h3>
          <div className="flex max-w-56 gap-1.5">
            <Input
              type="number"
              min={1}
              value={queuePos}
              onChange={(e) => setQueuePos(e.target.value)}
              placeholder={row.queuePosition != null ? String(row.queuePosition + 1) : '—'}
            />
            <Button
              size="sm"
              variant="outline"
              disabled={queuePos.trim() === ''}
              onClick={() => {
                const pos = Number(queuePos) - 1;
                if (Number.isFinite(pos) && pos >= 0) {
                  run(() => mutations.setQueuePosition(hash, pos).then(() => refreshTorrent(hash)));
                  setQueuePos('');
                }
              }}
            >
              {t('options.setPosition')}
            </Button>
          </div>
        </section>

        <section>
          <h3 className="mb-2 text-xs font-semibold text-muted-foreground uppercase">
            {t('options.maintenance')}
          </h3>
          <div className="flex flex-wrap gap-1.5">
            <Button size="sm" variant="outline" onClick={() => run(() => mutations.recheck(hash))}>
              {t('options.recheck')}
            </Button>
            <Button
              size="sm"
              variant="outline"
              onClick={() => run(() => mutations.saveResumeData(hash))}
            >
              {t('options.saveResume')}
            </Button>
            <Button
              size="sm"
              variant="outline"
              onClick={() => run(() => mutations.flushCache(hash))}
            >
              {t('options.flushCache')}
            </Button>
            <Button size="sm" variant="outline" onClick={() => openMoveDialog([hash])}>
              {t('options.setLocation')}
            </Button>
            <Button
              size="sm"
              variant="outline"
              onClick={() => run(() => mutations.reannounce(hash))}
            >
              {t('options.reannounce')}
            </Button>
            <Button
              size="sm"
              variant="outline"
              onClick={() => run(() => mutations.dhtAnnounce(hash))}
            >
              {t('options.dhtAnnounce')}
            </Button>
          </div>
        </section>
      </div>
    </div>
  );
}
