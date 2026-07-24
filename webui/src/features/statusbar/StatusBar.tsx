// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

import { ArrowDown, ArrowUp } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { Tooltip } from '@/components/ui/tooltip';
import { formatNumber, formatRate } from '@/lib/format';
import { useConnection } from '@/store/connection';
import { useStatusStats } from '@/store/statusStats';
import { useSession, useVersionInfo } from './useSession';

export function StatusBar({
  totalDownRate,
  totalUpRate,
  torrentCount,
}: {
  totalDownRate: number;
  totalUpRate: number;
  torrentCount: number;
}) {
  const { t } = useTranslation();
  const state = useConnection((s) => s.state);
  const sessionError = useConnection((s) => s.sessionError);
  const dhtNodes = useStatusStats((s) => s.dhtNodes);
  const session = useSession();
  const version = useVersionInfo();

  return (
    <footer className="flex items-center gap-4 border-t border-border px-3 py-1 text-xs text-muted-foreground">
      <span>{t(`connection.${state}`)}</span>
      {sessionError !== null && (
        <span role="alert" className="truncate text-st-error">
          {sessionError}
        </span>
      )}
      {session.data?.isPaused === true && (
        <span className="font-medium text-st-error">{t('statusbar.sessionPaused')}</span>
      )}
      <span className="ml-auto" />
      {version.data && (
        <span className="hidden sm:inline">
          {t('statusbar.version', {
            daemon: version.data.daemon,
            libtorrent: version.data.libtorrent,
          })}
        </span>
      )}
      {session.data && (
        <Tooltip
          content={
            session.data.isListening
              ? t('statusbar.listening', { port: session.data.listenPort })
              : t('statusbar.notListening')
          }
        >
          <span className={session.data.isListening ? '' : 'text-st-error'}>
            {session.data.isListening
              ? t('statusbar.listening', { port: session.data.listenPort })
              : t('statusbar.notListening')}
          </span>
        </Tooltip>
      )}
      <span>{t('statusbar.torrents', { count: torrentCount })}</span>
      {dhtNodes !== null && (
        <span className="tabular-nums">
          {t('statusbar.dht', { count: formatNumber(dhtNodes) })}
        </span>
      )}
      <span className="flex items-center gap-0.5 tabular-nums">
        <ArrowDown className="size-3 text-st-download" />
        {formatRate(totalDownRate)}
      </span>
      <span className="flex items-center gap-0.5 tabular-nums">
        <ArrowUp className="size-3 text-st-seed" />
        {formatRate(totalUpRate)}
      </span>
    </footer>
  );
}
