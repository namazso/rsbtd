// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

import {
  ArrowDownUp,
  ArrowDown,
  ArrowUp,
  MoreVertical,
  Pause,
  Play,
  Plus,
  Trash2,
  X,
} from 'lucide-react';
import { useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useNavigate } from 'react-router';
import { SHEET_MENU_KIT, SheetActionScope } from '@/components/actions/sheetKit';
import { BottomNav } from '@/components/BottomNav';
import { Button } from '@/components/ui/button';
import { BottomSheet } from '@/components/ui/sheet';
import { cn } from '@/lib/cn';
import { usePrefs } from '@/store/prefs';
import { useSelection } from '@/store/selection';
import { useTorrents } from '@/store/torrents';
import { useUi, CATEGORY_IDS } from '@/store/ui';
import { TorrentActionItems, torrentCommands } from './actions';
import { fieldLabel, TORRENT_FIELDS } from './fields';
import { MobileTorrentList } from './MobileTorrentList';
import { SearchBox } from './SearchBox';
import type { TorrentsView } from './useTorrentsView';

/**
 * Mobile main screen (spec): top bar, separate search bar, status filter
 * chips, simple sortable list, FAB add, bottom nav; long-press enters
 * multi-select with a contextual action bar; every context-menu action is
 * reachable through the bottom action sheet.
 */
export function MobileTorrents({ view }: { view: TorrentsView }) {
  const { t } = useTranslation(['torrents', 'common']);
  const navigate = useNavigate();
  const category = useUi((s) => s.category);
  const setCategory = useUi((s) => s.setCategory);
  const selectionMode = useUi((s) => s.selectionMode);
  const setSelectionMode = useUi((s) => s.setSelectionMode);
  const openAddDialog = useUi((s) => s.openAddDialog);
  const openRemoveDialog = useUi((s) => s.openRemoveDialog);
  const selected = useSelection((s) => s.selected);
  const listVersion = useTorrents((s) => s.listVersion);
  const [sortOpen, setSortOpen] = useState(false);
  const [actionsOpen, setActionsOpen] = useState(false);

  const selectedRows = useMemo(() => {
    const map = useTorrents.getState().byHash;
    return [...selected].flatMap((h) => {
      const row = map.get(h);
      return row !== undefined ? [row] : [];
    });
    // eslint-disable-next-line react-hooks/exhaustive-deps -- listVersion tracks store content
  }, [selected, listVersion]);
  const hashes = selectedRows.map((r) => r.infoHash);

  const exitSelection = () => {
    setSelectionMode(false);
    useSelection.getState().clear();
  };

  const closeActions = () => {
    setActionsOpen(false);
    exitSelection();
  };

  return (
    <div className="flex h-dvh flex-col">
      {selectionMode ? (
        <header className="flex h-[calc(3rem+env(safe-area-inset-top))] shrink-0 items-center gap-1 border-b border-border px-2 pt-[env(safe-area-inset-top)]">
          <Button
            variant="ghost"
            size="icon"
            aria-label={t('mobile.exitSelection')}
            onClick={exitSelection}
          >
            <X />
          </Button>
          <span className="text-sm font-medium">
            {t('mobile.selected', { count: selected.size })}
          </span>
          <span className="ml-auto flex">
            <Button
              variant="ghost"
              size="icon"
              aria-label={t('toolbar.resume')}
              disabled={hashes.length === 0}
              onClick={() => void torrentCommands.resume(hashes)}
            >
              <Play />
            </Button>
            <Button
              variant="ghost"
              size="icon"
              aria-label={t('toolbar.pause')}
              disabled={hashes.length === 0}
              onClick={() => void torrentCommands.pause(hashes)}
            >
              <Pause />
            </Button>
            <Button
              variant="ghost"
              size="icon"
              aria-label={t('toolbar.remove')}
              disabled={hashes.length === 0}
              onClick={() => {
                openRemoveDialog(hashes);
                exitSelection();
              }}
            >
              <Trash2 />
            </Button>
            <Button
              variant="ghost"
              size="icon"
              aria-label={t('mobile.properties')}
              disabled={hashes.length === 0}
              onClick={() => setActionsOpen(true)}
            >
              <MoreVertical />
            </Button>
          </span>
        </header>
      ) : (
        <header className="flex h-[calc(3rem+env(safe-area-inset-top))] shrink-0 items-center gap-2 border-b border-border px-3 pt-[env(safe-area-inset-top)]">
          <h1 className="text-base font-semibold">{t('common:app.name')}</h1>
          <span className="ml-auto flex items-center gap-1">
            <Button
              variant="ghost"
              size="icon"
              aria-label={t('mobile.sort')}
              onClick={() => setSortOpen(true)}
            >
              <ArrowDownUp />
            </Button>
          </span>
        </header>
      )}

      <div className="flex shrink-0 py-1.5 pr-3 pl-1">
        <SearchBox />
      </div>

      <div className="scrollbar-none flex shrink-0 gap-1.5 overflow-x-auto px-3 pb-2">
        {CATEGORY_IDS.map((id) => (
          <button
            key={id}
            type="button"
            onClick={() => setCategory(id)}
            className={cn(
              'shrink-0 rounded-full border border-border px-2.5 py-1 text-xs whitespace-nowrap',
              category === id
                ? 'border-primary bg-primary text-primary-foreground'
                : 'text-muted-foreground',
            )}
          >
            {t(`categories.${id}`)}
            <span className="ml-1 opacity-70">{view.counts[id]}</span>
          </button>
        ))}
      </div>

      <MobileTorrentList
        rows={view.rows}
        selectionMode={selectionMode}
        selected={selected}
        callbacks={{
          onTap: (row) => {
            if (selectionMode) useSelection.getState().toggle(row.infoHash);
            else void navigate(`/torrent/${row.infoHash}`);
          },
          onLongPress: (row) => {
            if (!selectionMode) setSelectionMode(true);
            useSelection.getState().toggle(row.infoHash);
          },
        }}
        emptyText={view.total === 0 ? t('empty.noTorrents') : t('empty.noMatch')}
      />

      {!selectionMode && (
        <Button
          size="icon"
          aria-label={t('toolbar.add')}
          onClick={() => openAddDialog()}
          className="absolute right-4 bottom-[calc(3.5rem+env(safe-area-inset-bottom)+1rem)] z-30 size-13 rounded-full shadow-lg"
        >
          <Plus className="!size-6" />
        </Button>
      )}

      <BottomNav />

      <SortSheet open={sortOpen} onOpenChange={setSortOpen} />

      <BottomSheet
        open={actionsOpen}
        onOpenChange={(open) => {
          if (open) setActionsOpen(true);
          else closeActions();
        }}
        title={t('mobile.properties')}
      >
        <SheetActionScope close={closeActions}>
          <TorrentActionItems kit={SHEET_MENU_KIT} rows={selectedRows} />
        </SheetActionScope>
      </BottomSheet>
    </div>
  );
}

function SortSheet({
  open,
  onOpenChange,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const { t } = useTranslation(['torrents', 'common']);
  const layout = usePrefs((s) => s.tables.torrents);
  const setTableLayout = usePrefs((s) => s.setTableLayout);
  const sortKey = layout?.sortKey ?? null;
  const sortDesc = layout?.sortDesc ?? false;

  return (
    <BottomSheet open={open} onOpenChange={onOpenChange} title={t('mobile.sortBy')}>
      <div className="mb-1 flex gap-2 px-4 py-1.5">
        <Button
          variant={sortDesc ? 'outline' : 'default'}
          size="sm"
          onClick={() => setTableLayout('torrents', { sortDesc: false })}
        >
          <ArrowUp />
          {t('mobile.asc')}
        </Button>
        <Button
          variant={sortDesc ? 'default' : 'outline'}
          size="sm"
          onClick={() => setTableLayout('torrents', { sortDesc: true })}
        >
          <ArrowDown />
          {t('mobile.desc')}
        </Button>
      </div>
      {TORRENT_FIELDS.filter((f) => f.sortable !== false).map((f) => (
        <button
          key={f.key}
          type="button"
          className={cn(
            'flex w-full items-center px-4 py-2 text-left text-sm active:bg-accent',
            sortKey === f.key && 'font-semibold text-primary',
          )}
          onClick={() => {
            setTableLayout('torrents', { sortKey: f.key });
            onOpenChange(false);
          }}
        >
          {fieldLabel(f.key)}
        </button>
      ))}
    </BottomSheet>
  );
}
