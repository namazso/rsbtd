// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

import {
  BarChart3,
  FilePlus2,
  Monitor,
  Moon,
  MoreHorizontal,
  PanelLeft,
  Pause,
  Play,
  PlayCircle,
  Plus,
  Settings,
  StopCircle,
  Sun,
  Trash2,
} from 'lucide-react';
import { useMemo } from 'react';
import { useTranslation } from 'react-i18next';
import { useNavigate } from 'react-router';
import { mutations } from '@/api/mutations';
import { DROPDOWN_MENU_KIT } from '@/components/actions/menuKit';
import { Button } from '@/components/ui/button';
import { DropdownMenu, DropdownMenuContent, DropdownMenuTrigger } from '@/components/ui/menu';
import { Tooltip } from '@/components/ui/tooltip';
import { cn } from '@/lib/cn';
import { useConnection } from '@/store/connection';
import { usePrefs, type ThemePref } from '@/store/prefs';
import { useSelection } from '@/store/selection';
import { useTorrents } from '@/store/torrents';
import { useUi } from '@/store/ui';
import { useInvalidateSession, useSession } from '@/features/statusbar/useSession';
import { TorrentActionItems, torrentCommands } from './actions';
import { SearchBox } from './SearchBox';

const THEME_CYCLE: Record<ThemePref, ThemePref> = {
  system: 'light',
  light: 'dark',
  dark: 'system',
};

function ThemeButton() {
  const { t } = useTranslation();
  const theme = usePrefs((s) => s.theme);
  const setTheme = usePrefs((s) => s.setTheme);
  const Icon = theme === 'system' ? Monitor : theme === 'light' ? Sun : Moon;
  return (
    <Tooltip content={t(`theme.${theme}`)}>
      <Button
        variant="ghost"
        size="icon"
        aria-label={t(`theme.${theme}`)}
        onClick={() => setTheme(THEME_CYCLE[theme])}
      >
        <Icon />
      </Button>
    </Tooltip>
  );
}

function ConnectionDot() {
  const { t } = useTranslation();
  const state = useConnection((s) => s.state);
  const color =
    state === 'up'
      ? 'bg-st-seed'
      : state === 'degraded'
        ? 'bg-st-check'
        : state === 'down' || state === 'authRequired'
          ? 'bg-st-error'
          : 'bg-st-pause animate-pulse';
  return (
    <Tooltip content={t(`connection.${state}`)}>
      <span aria-label={t(`connection.${state}`)} role="status" className="px-1.5">
        <span className={cn('block size-2.5 rounded-full', color)} />
      </span>
    </Tooltip>
  );
}

function SessionPauseButton() {
  const { t } = useTranslation(['torrents', 'common']);
  const session = useSession();
  const invalidate = useInvalidateSession();
  if (!session.data) return null;
  const paused = session.data.isPaused;
  const label = paused ? t('toolbar.resumeSession') : t('toolbar.pauseSession');
  return (
    <Tooltip content={label}>
      <Button
        variant="ghost"
        size="icon"
        aria-label={label}
        className={paused ? 'text-st-error' : ''}
        onClick={() => {
          void (paused ? mutations.resumeSession() : mutations.pauseSession()).then(invalidate);
        }}
      >
        {paused ? <PlayCircle /> : <StopCircle />}
      </Button>
    </Tooltip>
  );
}

export function Toolbar() {
  const { t } = useTranslation(['torrents', 'common']);
  const navigate = useNavigate();
  const selected = useSelection((s) => s.selected);
  const listVersion = useTorrents((s) => s.listVersion);
  const openAddDialog = useUi((s) => s.openAddDialog);
  const openRemoveDialog = useUi((s) => s.openRemoveDialog);
  const sidebarCollapsed = usePrefs((s) => s.sidebarCollapsed);
  const setPrefs = usePrefs((s) => s.set);

  const selectedRows = useMemo(() => {
    const map = useTorrents.getState().byHash;
    return [...selected].flatMap((h) => {
      const row = map.get(h);
      return row !== undefined ? [row] : [];
    });
    // eslint-disable-next-line react-hooks/exhaustive-deps -- listVersion tracks store content
  }, [selected, listVersion]);
  const hashes = selectedRows.map((r) => r.infoHash);
  const none = hashes.length === 0;

  return (
    <div className="flex items-center gap-1 border-b border-border px-2 py-1.5">
      <Tooltip content={t('toolbar.toggleSidebar')}>
        <Button
          variant="ghost"
          size="icon"
          aria-label={t('toolbar.toggleSidebar')}
          onClick={() => setPrefs({ sidebarCollapsed: !sidebarCollapsed })}
        >
          <PanelLeft />
        </Button>
      </Tooltip>

      <Button size="md" onClick={() => openAddDialog()}>
        <Plus />
        {t('toolbar.add')}
      </Button>

      <span className="mx-1 h-5 w-px bg-border" />

      <Tooltip content={t('toolbar.resume')}>
        <Button
          variant="ghost"
          size="icon"
          aria-label={t('toolbar.resume')}
          disabled={none}
          onClick={() => void torrentCommands.resume(hashes)}
        >
          <Play />
        </Button>
      </Tooltip>
      <Tooltip content={t('toolbar.pause')}>
        <Button
          variant="ghost"
          size="icon"
          aria-label={t('toolbar.pause')}
          disabled={none}
          onClick={() => void torrentCommands.pause(hashes)}
        >
          <Pause />
        </Button>
      </Tooltip>
      <Tooltip content={t('toolbar.remove')}>
        <Button
          variant="ghost"
          size="icon"
          aria-label={t('toolbar.remove')}
          disabled={none}
          onClick={() => openRemoveDialog(hashes)}
        >
          <Trash2 />
        </Button>
      </Tooltip>
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <Button variant="ghost" size="icon" aria-label={t('toolbar.moreActions')}>
            <MoreHorizontal />
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="start">
          <TorrentActionItems kit={DROPDOWN_MENU_KIT} rows={selectedRows} />
        </DropdownMenuContent>
      </DropdownMenu>

      <SearchBox />

      <div className="ml-auto flex items-center">
        <SessionPauseButton />
        <Tooltip content={t('toolbar.create')}>
          <Button
            variant="ghost"
            size="icon"
            aria-label={t('toolbar.create')}
            onClick={() => void navigate('/create')}
          >
            <FilePlus2 />
          </Button>
        </Tooltip>
        <Tooltip content={t('toolbar.stats')}>
          <Button
            variant="ghost"
            size="icon"
            aria-label={t('toolbar.stats')}
            onClick={() => void navigate('/stats')}
          >
            <BarChart3 />
          </Button>
        </Tooltip>
        <Tooltip content={t('toolbar.settings')}>
          <Button
            variant="ghost"
            size="icon"
            aria-label={t('toolbar.settings')}
            onClick={() => void navigate('/settings')}
          >
            <Settings />
          </Button>
        </Tooltip>
        <ThemeButton />
        <ConnectionDot />
      </div>
    </div>
  );
}
