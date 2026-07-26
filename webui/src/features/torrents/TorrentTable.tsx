// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

import { useMemo } from 'react';
import { useTranslation } from 'react-i18next';
import { DataTable, type DataTableColumn } from '@/components/listing/DataTable';
import { CONTEXT_MENU_KIT } from '@/components/actions/menuKit';
import { cn } from '@/lib/cn';
import { formatPercentPpm } from '@/lib/format';
import { usePrefs } from '@/store/prefs';
import { useSelection } from '@/store/selection';
import { useTorrents, type TorrentRow } from '@/store/torrents';
import { useUi } from '@/store/ui';
import { TorrentActionItems } from './actions';
import { fieldLabel, formatFieldValue, TORRENT_FIELDS } from './fields';
import { statusStyle, uiStatus } from './status';
import type { TorrentsView } from './useTorrentsView';

export function StatusIcon({ row, className }: { row: TorrentRow; className?: string }) {
  const { t } = useTranslation('torrents');
  const status = uiStatus(row);
  const style = statusStyle(status);
  const Icon = style.icon;
  const label = t(`status.${status}`);
  return <Icon aria-label={label} className={cn('size-4 shrink-0', style.fg, className)} />;
}

function NameCell({ row }: { row: TorrentRow }) {
  return (
    <span className="flex h-full items-center gap-1.5">
      <StatusIcon row={row} />
      <span className="truncate" title={row.error != null ? row.error.message : row.name}>
        {row.name}
      </span>
    </span>
  );
}

function ProgressCell({ row }: { row: TorrentRow }) {
  const style = statusStyle(uiStatus(row));
  const ppm = row.progressPpm;
  return (
    <span className="flex h-full items-center">
      <span className="relative block h-4 w-full overflow-hidden rounded-sm bg-muted">
        <span
          className={cn('absolute inset-y-0 left-0 opacity-75', style.bg)}
          style={{ width: `${ppm / 10_000}%` }}
        />
        <span className="absolute inset-0 text-center text-[10px] leading-4 font-medium">
          {formatPercentPpm(ppm)}
        </span>
      </span>
    </span>
  );
}

const TORRENT_COLUMNS: DataTableColumn<TorrentRow>[] = TORRENT_FIELDS.map((def) => ({
  id: def.key,
  header: '', // filled per-render (i18n) below
  width: def.column?.width ?? 100,
  align: def.column?.align,
  sortable: def.sortable !== false,
  cell:
    def.key === 'name'
      ? (row: TorrentRow) => <NameCell row={row} />
      : def.key === 'progressPpm'
        ? (row: TorrentRow) => <ProgressCell row={row} />
        : (row: TorrentRow) => formatFieldValue(def, row),
}));

const DEFAULT_VISIBLE = TORRENT_FIELDS.filter((f) => f.column?.defaultVisible).map((f) => f.key);

export function TorrentTable({ view }: { view: TorrentsView }) {
  const { t } = useTranslation('torrents');
  const layout = usePrefs((s) => s.tables.torrents);
  const setTableLayout = usePrefs((s) => s.setTableLayout);
  const selected = useSelection((s) => s.selected);
  const focus = useSelection((s) => s.focus);
  const listVersion = useTorrents((s) => s.listVersion);

  const columns = useMemo(
    () => TORRENT_COLUMNS.map((c) => ({ ...c, header: fieldLabel(c.id) })),
    [],
  );

  const selectedRows = useMemo(() => {
    const map = useTorrents.getState().byUuid;
    return [...selected].flatMap((u) => {
      const row = map.get(u);
      return row !== undefined ? [row] : [];
    });
    // eslint-disable-next-line react-hooks/exhaustive-deps -- listVersion tracks store content
  }, [selected, listVersion]);

  const sel = useSelection.getState();
  const ui = useUi.getState();

  return (
    <DataTable
      tableId="torrents"
      columns={columns}
      defaultVisible={DEFAULT_VISIBLE}
      data={view.rows}
      rowKey={(r) => r.uuid}
      sort={{ key: layout?.sortKey ?? null, desc: layout?.sortDesc ?? false }}
      onSortChange={(s) =>
        setTableLayout('torrents', { sortKey: s.key ?? undefined, sortDesc: s.desc })
      }
      selected={selected}
      focusKey={focus}
      onRowMouseDown={(row, e) => {
        if (e.button !== 0) return;
        sel.click(row.uuid, { ctrl: e.ctrlKey || e.metaKey, shift: e.shiftKey }, view.order);
      }}
      onRowDoubleClick={(row) => ui.setDetailsUuid(row.uuid)}
      onRowContextMenu={(row) => sel.contextSelect(row.uuid)}
      rowContextContent={<TorrentActionItems kit={CONTEXT_MENU_KIT} rows={selectedRows} />}
      onNavigate={(row, extend) => sel.keyMove(row.uuid, extend, view.order)}
      onActivate={(row) => ui.setDetailsUuid(row.uuid)}
      emptyText={view.total === 0 ? t('empty.noTorrents') : t('empty.noMatch')}
    />
  );
}
