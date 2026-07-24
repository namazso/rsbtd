// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

import { ArrowDown, ArrowUp, ChevronLeft } from 'lucide-react';
import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useNavigate } from 'react-router';
import { SHEET_MENU_KIT, SheetActionScope } from '@/components/actions/sheetKit';
import { Button } from '@/components/ui/button';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/switch-tabs';
import { cn } from '@/lib/cn';
import { formatBytes, formatEta, formatPercentPpm, formatRate } from '@/lib/format';
import { useSynced } from '@/api/live';
import { useTorrents } from '@/store/torrents';
import { FilesTab } from '@/features/details/FilesTab';
import { GeneralTab } from '@/features/details/GeneralTab';
import { OptionsTab } from '@/features/details/OptionsTab';
import { PeersTab } from '@/features/details/PeersTab';
import { TrackersTab } from '@/features/details/TrackersTab';
import { TorrentActionItems } from './actions';
import { fieldLabel, formatFieldValue, torrentEta, TORRENT_FIELDS } from './fields';
import { statusStyle, uiStatus } from './status';
import { StatusIcon } from './TorrentTable';

type MobileTab = 'actions' | 'general' | 'files' | 'trackers' | 'peers' | 'options';
const MOBILE_TABS: MobileTab[] = ['actions', 'general', 'files', 'trackers', 'peers', 'options'];

/**
 * Mobile full-screen torrent page ("Properties"): summary + the complete
 * action set (parity with the desktop context menu), plus the detail
 * tabs (files/trackers/peers/options) shared with the desktop panel.
 */
export function MobileTorrentPage({ hash }: { hash: string }) {
  const { t } = useTranslation(['torrents', 'common', 'details']);
  const navigate = useNavigate();
  const [tab, setTab] = useState<MobileTab>('actions');
  const listVersion = useTorrents((s) => s.listVersion);
  void listVersion; // re-render on ticks
  const synced = useSynced((s) => s.synced);
  const store = useTorrents.getState();
  const canonical = store.resolve(hash) ?? hash;
  const row = store.byHash.get(canonical);

  if (row === undefined) {
    // Before the first full snapshot the store is empty and a deep link
    // would misreport every torrent as missing; wait like the desktop
    // deep-link route does.
    if (!synced) {
      return (
        <div className="flex h-dvh flex-col">
          <PageHeader title={t('details:loading')} onBack={() => void navigate('/')} />
        </div>
      );
    }
    return (
      <div className="flex h-dvh flex-col">
        <PageHeader title={t('common:notFound.title')} onBack={() => void navigate('/')} />
        <p className="p-4 text-sm text-muted-foreground">
          {t('common:notFound.torrent', { hash: hash.slice(0, 12) })}
        </p>
      </div>
    );
  }

  const style = statusStyle(uiStatus(row));
  const summaryFields = [
    'state',
    'totalWanted',
    'totalWantedDone',
    'numSeeds',
    'numPeers',
    'ratio',
    'addedTime',
    'savePath',
    'currentTracker',
  ] as const;

  return (
    <div className="flex h-dvh flex-col">
      <PageHeader title={row.name} onBack={() => void navigate(-1)} />
      <div className="min-h-0 flex-1 overflow-y-auto pb-[env(safe-area-inset-bottom)]">
        <div className="border-b border-border p-4">
          <div className="mb-2 flex items-center gap-2">
            <StatusIcon row={row} className="size-5" />
            <span className="text-sm font-medium">{t(`status.${uiStatus(row)}`)}</span>
            <span className="ml-auto text-sm font-semibold tabular-nums">
              {formatPercentPpm(row.progressPpm)}
            </span>
          </div>
          <div className="relative h-2 overflow-hidden rounded-full bg-muted">
            <div
              className={cn('absolute inset-y-0 left-0', style.bg)}
              style={{ width: `${row.progressPpm / 10_000}%` }}
            />
          </div>
          <div className="mt-2 flex justify-between text-xs text-muted-foreground tabular-nums">
            <span className="flex items-center gap-1.5">
              <span className="flex items-center gap-0.5">
                <ArrowDown aria-hidden className="size-3 text-st-download" />
                {formatRate(row.downloadPayloadRate)}
              </span>
              <span className="flex items-center gap-0.5">
                <ArrowUp aria-hidden className="size-3 text-st-seed" />
                {formatRate(row.uploadPayloadRate)}
              </span>
            </span>
            <span>
              {t('mobile.progressOf', {
                done: formatBytes(row.totalWantedDone),
                total: formatBytes(row.totalWanted),
                eta: formatEta(torrentEta(row)),
              })}
            </span>
          </div>
          {row.error != null && (
            <p role="alert" className="mt-2 text-xs text-st-error">
              {row.error.message}
            </p>
          )}
        </div>

        <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-1.5 border-b border-border p-4 text-sm">
          {summaryFields.map((key) => {
            const def = TORRENT_FIELDS.find((f) => f.key === key);
            if (!def) return null;
            return (
              <div key={key} className="contents">
                <dt className="text-muted-foreground">{fieldLabel(key)}</dt>
                <dd className="truncate text-right tabular-nums">{formatFieldValue(def, row)}</dd>
              </div>
            );
          })}
        </dl>

        <Tabs value={tab} onValueChange={(v) => setTab(v as MobileTab)}>
          <TabsList className="sticky top-0 z-10 bg-background">
            {MOBILE_TABS.map((id) => (
              <TabsTrigger key={id} value={id}>
                {t(`details:tabs.${id}`)}
              </TabsTrigger>
            ))}
          </TabsList>
          <TabsContent value="actions">
            <SheetActionScope close={() => {}}>
              <div className="py-1">
                <TorrentActionItems kit={SHEET_MENU_KIT} rows={[row]} />
              </div>
            </SheetActionScope>
          </TabsContent>
          <TabsContent value="general">
            <GeneralTab row={row} hash={row.infoHash} visible={tab === 'general'} />
          </TabsContent>
          <TabsContent value="files">
            <FilesTab hash={row.infoHash} visible={tab === 'files'} />
          </TabsContent>
          <TabsContent value="trackers">
            <TrackersTab hash={row.infoHash} visible={tab === 'trackers'} />
          </TabsContent>
          <TabsContent value="peers">
            <PeersTab hash={row.infoHash} visible={tab === 'peers'} />
          </TabsContent>
          <TabsContent value="options">
            <OptionsTab row={row} />
          </TabsContent>
        </Tabs>
      </div>
    </div>
  );
}

function PageHeader({ title, onBack }: { title: string; onBack: () => void }) {
  const { t } = useTranslation();
  return (
    <header className="flex h-[calc(3rem+env(safe-area-inset-top))] shrink-0 items-center gap-1 border-b border-border px-1 pt-[env(safe-area-inset-top)]">
      <Button variant="ghost" size="icon" aria-label={t('actions.close')} onClick={onBack}>
        <ChevronLeft />
      </Button>
      <h1 className="min-w-0 flex-1 truncate text-sm font-semibold">{title}</h1>
    </header>
  );
}
