// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

import { X } from 'lucide-react';
import { useCallback, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Button } from '@/components/ui/button';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/switch-tabs';
import { usePrefs } from '@/store/prefs';
import { useTorrents } from '@/store/torrents';
import { useUi } from '@/store/ui';
import { StatusIcon } from '@/features/torrents/TorrentTable';
import { FilesTab } from './FilesTab';
import { GeneralTab } from './GeneralTab';
import { OptionsTab } from './OptionsTab';
import { PeersTab } from './PeersTab';
import { TrackersTab } from './TrackersTab';

type TabId = 'general' | 'files' | 'trackers' | 'peers' | 'options';
const TAB_IDS: TabId[] = ['general', 'files', 'trackers', 'peers', 'options'];

/**
 * Desktop bottom details panel (qBittorrent-style): collapsible, resizable
 * via the top drag handle, tabs polled only while visible.
 */
export function DetailsPanel() {
  const { t } = useTranslation(['details', 'common']);
  const detailsUuid = useUi((s) => s.detailsUuid);
  const setDetailsUuid = useUi((s) => s.setDetailsUuid);
  const height = usePrefs((s) => s.detailsPanelSize);
  const setPrefs = usePrefs((s) => s.set);
  const listVersion = useTorrents((s) => s.listVersion);
  void listVersion; // live row updates
  const [tab, setTab] = useState<TabId>('general');

  const startResize = useCallback(
    (e: React.PointerEvent) => {
      e.preventDefault();
      const startY = e.clientY;
      const startHeight = usePrefs.getState().detailsPanelSize;
      const onMove = (ev: PointerEvent) => {
        const next = Math.min(Math.max(startHeight + (startY - ev.clientY), 160), 640);
        setPrefs({ detailsPanelSize: next });
      };
      const onUp = () => {
        window.removeEventListener('pointermove', onMove);
        window.removeEventListener('pointerup', onUp);
      };
      window.addEventListener('pointermove', onMove);
      window.addEventListener('pointerup', onUp);
    },
    [setPrefs],
  );

  if (detailsUuid === null) return null;
  const row = useTorrents.getState().byUuid.get(detailsUuid);
  if (row === undefined) return null;

  return (
    <div className="flex shrink-0 flex-col border-t border-border" style={{ height }}>
      <div
        role="separator"
        aria-orientation="horizontal"
        className="h-1 shrink-0 cursor-row-resize bg-border/50 hover:bg-primary/40"
        onPointerDown={startResize}
      />
      <Tabs
        value={tab}
        onValueChange={(v) => setTab(v as TabId)}
        className="flex min-h-0 flex-1 flex-col"
      >
        <div className="flex shrink-0 items-center">
          <TabsList className="flex-1 border-b-0">
            {TAB_IDS.map((id) => (
              <TabsTrigger key={id} value={id}>
                {t(`tabs.${id}`)}
              </TabsTrigger>
            ))}
          </TabsList>
          <span className="flex min-w-0 items-center gap-1.5 px-2 text-xs text-muted-foreground">
            <StatusIcon row={row} />
            <span className="max-w-72 truncate">{row.name}</span>
          </span>
          <Button
            variant="ghost"
            size="iconSm"
            aria-label={t('common:actions.close')}
            onClick={() => setDetailsUuid(null)}
          >
            <X />
          </Button>
        </div>
        {/* Keyed by torrent so the tabs' local state (rename target,
            web-seed draft, queue-position text, collapsed folders) never
            leaks from one torrent to the next; the chosen tab lives
            outside and survives. */}
        <div key={row.uuid} className="min-h-0 flex-1 overflow-y-auto border-t border-border">
          <TabsContent value="general" className="h-full">
            <GeneralTab row={row} uuid={row.uuid} visible={tab === 'general'} />
          </TabsContent>
          <TabsContent value="files" className="h-full">
            <FilesTab uuid={row.uuid} visible={tab === 'files'} />
          </TabsContent>
          <TabsContent value="trackers" className="h-full">
            <TrackersTab uuid={row.uuid} visible={tab === 'trackers'} />
          </TabsContent>
          <TabsContent value="peers" className="h-full">
            <PeersTab uuid={row.uuid} visible={tab === 'peers'} />
          </TabsContent>
          <TabsContent value="options" className="h-full">
            <OptionsTab row={row} />
          </TabsContent>
        </div>
      </Tabs>
    </div>
  );
}
