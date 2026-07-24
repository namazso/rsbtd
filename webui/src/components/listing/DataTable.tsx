// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

import { useVirtualizer } from '@tanstack/react-virtual';
import { ChevronDown, ChevronUp } from 'lucide-react';
import {
  memo,
  useCallback,
  useEffect,
  useId,
  useMemo,
  useRef,
  useState,
  type KeyboardEvent,
  type MouseEvent,
  type ReactNode,
} from 'react';
import { useTranslation } from 'react-i18next';
import {
  ContextMenu,
  ContextMenuCheckboxItem,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuSub,
  ContextMenuSubContent,
  ContextMenuSubTrigger,
  ContextMenuTrigger,
} from '@/components/ui/menu';
import { cn } from '@/lib/cn';
import { usePrefs } from '@/store/prefs';

/**
 * Desktop listing table (torrents, files, trackers, peers):
 * virtualized rows, click-to-sort headers, header context menu with column
 * visibility / move left-right / reset, drag-resize, per-table layout
 * persistence. Sorting is *controlled* (the caller sorts its data — for
 * torrents that happens in the same memoized pass as filtering).
 */
export interface DataTableColumn<T> {
  id: string;
  header: string;
  width: number;
  align?: 'right';
  sortable: boolean;
  cell: (row: T) => ReactNode;
}

export interface DataTableSort {
  key: string | null;
  desc: boolean;
}

export interface DataTableProps<T> {
  tableId: string;
  columns: readonly DataTableColumn<T>[];
  defaultVisible: readonly string[];
  data: readonly T[];
  rowKey: (row: T) => string;
  rowHeight?: number;
  sort: DataTableSort;
  onSortChange: (sort: DataTableSort) => void;
  selected?: ReadonlySet<string>;
  focusKey?: string | null;
  onRowMouseDown?: (row: T, e: MouseEvent) => void;
  onRowDoubleClick?: (row: T) => void;
  onRowContextMenu?: (row: T) => void;
  /** Menu items rendered in the row right-click menu. */
  rowContextContent?: ReactNode;
  /** Arrow-key navigation: called with the new focus row. */
  onNavigate?: (row: T, extend: boolean) => void;
  onActivate?: (row: T) => void;
  emptyText: string;
}

const MIN_COL_WIDTH = 36;

export function DataTable<T>({
  tableId,
  columns,
  defaultVisible,
  data,
  rowKey,
  rowHeight = 28,
  sort,
  onSortChange,
  selected,
  focusKey,
  onRowMouseDown,
  onRowDoubleClick,
  onRowContextMenu,
  rowContextContent,
  onNavigate,
  onActivate,
  emptyText,
}: DataTableProps<T>) {
  const { t } = useTranslation();
  const layout = usePrefs((s) => s.tables[tableId]);
  const setTableLayout = usePrefs((s) => s.setTableLayout);
  const resetTableLayout = usePrefs((s) => s.resetTableLayout);
  const scrollRef = useRef<HTMLDivElement>(null);
  // Which column header was right-clicked (state, so the menu content
  // re-renders before Radix opens it).
  const [headerMenuTarget, setHeaderMenuTarget] = useState<string | null>(null);
  // Live-resize widths without hammering the persisted store.
  const [liveSizing, setLiveSizing] = useState<Record<string, number> | null>(null);

  const hidden = useMemo(() => {
    if (layout?.hidden) return new Set(layout.hidden);
    return new Set(columns.map((c) => c.id).filter((id) => !defaultVisible.includes(id)));
  }, [layout?.hidden, columns, defaultVisible]);

  const order = useMemo(() => {
    const ids = columns.map((c) => c.id);
    const stored = (layout?.order ?? []).filter((id) => ids.includes(id));
    return [...stored, ...ids.filter((id) => !stored.includes(id))];
  }, [layout?.order, columns]);

  const sizing = liveSizing ?? layout?.sizing ?? {};

  const byId = useMemo(() => new Map(columns.map((c) => [c.id, c])), [columns]);
  const visibleCols = useMemo(
    () =>
      order
        .filter((id) => !hidden.has(id))
        .map((id) => byId.get(id))
        .filter((c): c is DataTableColumn<T> => c !== undefined),
    [order, hidden, byId],
  );
  const widths = visibleCols.map((c) => Math.max(MIN_COL_WIDTH, sizing[c.id] ?? c.width));
  const totalWidth = widths.reduce((a, b) => a + b, 0);

  const virtualizer = useVirtualizer({
    count: data.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => rowHeight,
    overscan: 10,
  });

  const focusIndex = useMemo(() => {
    if (focusKey == null) return -1;
    return data.findIndex((row) => rowKey(row) === focusKey);
  }, [data, focusKey, rowKey]);
  // Focus stays on the grid container; the keyboard-active row is
  // exposed to assistive tech via aria-activedescendant, which needs a
  // DOM id per row. Ids are per-position: the focused row is always
  // scrolled into the virtual window when it changes.
  const gridId = useId();

  useEffect(() => {
    if (focusIndex >= 0) virtualizer.scrollToIndex(focusIndex);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [focusIndex]);

  const cycleSort = useCallback(
    (colId: string) => {
      const col = byId.get(colId);
      if (!col?.sortable) return;
      if (sort.key !== colId) onSortChange({ key: colId, desc: false });
      else if (!sort.desc) onSortChange({ key: colId, desc: true });
      else onSortChange({ key: null, desc: false });
    },
    [byId, sort, onSortChange],
  );

  const moveColumn = useCallback(
    (colId: string, dir: -1 | 1) => {
      const visibleIds = visibleCols.map((c) => c.id);
      const vi = visibleIds.indexOf(colId);
      const neighbor = visibleIds[vi + dir];
      if (neighbor === undefined) return;
      const next = [...order];
      const from = next.indexOf(colId);
      const to = next.indexOf(neighbor);
      next.splice(from, 1);
      next.splice(to, 0, colId);
      setTableLayout(tableId, { order: next });
    },
    [visibleCols, order, setTableLayout, tableId],
  );

  const setHidden = useCallback(
    (colId: string, hide: boolean) => {
      const next = new Set(hidden);
      if (hide) next.add(colId);
      else next.delete(colId);
      if (next.size >= columns.length) return; // keep at least one column
      setTableLayout(tableId, { hidden: [...next] });
    },
    [hidden, columns.length, setTableLayout, tableId],
  );

  const startResize = useCallback(
    (colId: string, startX: number, startWidth: number) => {
      const onMove = (e: PointerEvent) => {
        const width = Math.max(MIN_COL_WIDTH, startWidth + (e.clientX - startX));
        setLiveSizing((prev) => ({ ...(prev ?? layout?.sizing ?? {}), [colId]: width }));
      };
      const onUp = (e: PointerEvent) => {
        window.removeEventListener('pointermove', onMove);
        window.removeEventListener('pointerup', onUp);
        const width = Math.max(MIN_COL_WIDTH, startWidth + (e.clientX - startX));
        setLiveSizing(null);
        setTableLayout(tableId, { sizing: { ...layout?.sizing, [colId]: width } });
      };
      window.addEventListener('pointermove', onMove);
      window.addEventListener('pointerup', onUp);
    },
    [layout?.sizing, setTableLayout, tableId],
  );

  const handleKeyDown = useCallback(
    (e: KeyboardEvent<HTMLDivElement>) => {
      if (e.target !== e.currentTarget) return;
      if (data.length === 0) return;
      const current = focusIndex;
      let next: number;
      if (e.key === 'ArrowDown') next = Math.min(current + 1, data.length - 1);
      else if (e.key === 'ArrowUp') next = Math.max(current - 1, 0);
      else if (e.key === 'Home') next = 0;
      else if (e.key === 'End') next = data.length - 1;
      else if (e.key === 'Enter' && current >= 0) {
        const row = data[current];
        if (row !== undefined) onActivate?.(row);
        e.preventDefault();
        return;
      } else return;
      if (next >= 0) {
        const row = data[next];
        if (row !== undefined) onNavigate?.(row, e.shiftKey);
        e.preventDefault();
      }
    },
    [data, focusIndex, onNavigate, onActivate],
  );

  return (
    <div
      ref={scrollRef}
      role="grid"
      aria-rowcount={data.length + 1}
      aria-activedescendant={focusIndex >= 0 ? `${gridId}-r${focusIndex + 2}` : undefined}
      tabIndex={0}
      onKeyDown={handleKeyDown}
      className="relative min-h-0 flex-1 overflow-auto outline-none focus-visible:ring-1 focus-visible:ring-ring"
    >
      {/* header */}
      <ContextMenu>
        <ContextMenuTrigger asChild>
          <div
            role="row"
            aria-rowindex={1}
            className="sticky top-0 z-10 flex border-b border-border bg-background text-xs font-medium text-muted-foreground select-none"
            style={{ width: totalWidth, minWidth: '100%' }}
          >
            {visibleCols.map((col, i) => {
              const sortIcon =
                sort.key === col.id ? (
                  sort.desc ? (
                    <ChevronDown className="size-3 shrink-0" />
                  ) : (
                    <ChevronUp className="size-3 shrink-0" />
                  )
                ) : null;
              return (
                <div
                  key={col.id}
                  role="columnheader"
                  aria-sort={
                    sort.key === col.id
                      ? sort.desc
                        ? 'descending'
                        : 'ascending'
                      : col.sortable
                        ? 'none'
                        : undefined
                  }
                  className="relative flex h-7 items-center border-r border-border/50"
                  style={{ width: widths[i] }}
                  onContextMenu={() => setHeaderMenuTarget(col.id)}
                >
                  {col.sortable ? (
                    <button
                      type="button"
                      onClick={() => cycleSort(col.id)}
                      className={cn(
                        'flex h-full min-w-0 flex-1 cursor-pointer items-center gap-0.5 px-2 outline-none hover:bg-accent focus-visible:ring-1 focus-visible:ring-ring focus-visible:ring-inset',
                        col.align === 'right' && 'justify-end',
                      )}
                    >
                      <span className="truncate">{col.header}</span>
                      {sortIcon}
                    </button>
                  ) : (
                    <span
                      className={cn(
                        'flex h-full min-w-0 flex-1 items-center px-2',
                        col.align === 'right' && 'justify-end',
                      )}
                    >
                      <span className="truncate">{col.header}</span>
                    </span>
                  )}
                  <div
                    className="absolute top-0 -right-0.75 z-10 h-full w-1.5 cursor-col-resize"
                    onClick={(e) => e.stopPropagation()}
                    onPointerDown={(e) => {
                      e.preventDefault();
                      e.stopPropagation();
                      startResize(col.id, e.clientX, widths[i] ?? col.width);
                    }}
                  />
                </div>
              );
            })}
          </div>
        </ContextMenuTrigger>
        <ContextMenuContent>
          {headerMenuTarget !== null && byId.get(headerMenuTarget)?.sortable && (
            <>
              <ContextMenuItem
                onSelect={() => onSortChange({ key: headerMenuTarget, desc: false })}
              >
                {t('columnsMenu.sortAsc')}
              </ContextMenuItem>
              <ContextMenuItem onSelect={() => onSortChange({ key: headerMenuTarget, desc: true })}>
                {t('columnsMenu.sortDesc')}
              </ContextMenuItem>
              <ContextMenuItem onSelect={() => onSortChange({ key: null, desc: false })}>
                {t('columnsMenu.sortClear')}
              </ContextMenuItem>
              <ContextMenuSeparator />
            </>
          )}
          {headerMenuTarget !== null && (
            <>
              <ContextMenuItem onSelect={() => moveColumn(headerMenuTarget, -1)}>
                {t('columnsMenu.moveLeft')}
              </ContextMenuItem>
              <ContextMenuItem onSelect={() => moveColumn(headerMenuTarget, 1)}>
                {t('columnsMenu.moveRight')}
              </ContextMenuItem>
              <ContextMenuItem onSelect={() => setHidden(headerMenuTarget, true)}>
                {t('columnsMenu.hide')}
              </ContextMenuItem>
              <ContextMenuSeparator />
            </>
          )}
          <ContextMenuSub>
            <ContextMenuSubTrigger>{t('columnsMenu.columns')}</ContextMenuSubTrigger>
            <ContextMenuSubContent>
              {order.map((id) => {
                const col = byId.get(id);
                if (!col) return null;
                return (
                  <ContextMenuCheckboxItem
                    key={id}
                    checked={!hidden.has(id)}
                    onSelect={(e) => e.preventDefault()}
                    onCheckedChange={(checked) => setHidden(id, !checked)}
                  >
                    {col.header}
                  </ContextMenuCheckboxItem>
                );
              })}
            </ContextMenuSubContent>
          </ContextMenuSub>
          <ContextMenuItem onSelect={() => resetTableLayout(tableId)}>
            {t('columnsMenu.reset')}
          </ContextMenuItem>
        </ContextMenuContent>
      </ContextMenu>

      {/* body */}
      {data.length === 0 ? (
        <div className="flex h-40 items-center justify-center text-sm text-muted-foreground">
          {emptyText}
        </div>
      ) : (
        <ContextMenu>
          <ContextMenuTrigger asChild>
            <div
              className="relative"
              style={{ height: virtualizer.getTotalSize(), width: totalWidth, minWidth: '100%' }}
            >
              {virtualizer.getVirtualItems().map((vi) => {
                const row = data[vi.index];
                if (row === undefined) return null;
                const key = rowKey(row);
                return (
                  <MemoRow
                    key={key}
                    row={row}
                    cols={visibleCols}
                    widths={widths}
                    top={vi.start}
                    height={rowHeight}
                    ariaRowIndex={vi.index + 2}
                    domId={`${gridId}-r${vi.index + 2}`}
                    isSelected={selected?.has(key) ?? false}
                    isFocused={focusKey === key}
                    onMouseDown={onRowMouseDown}
                    onDoubleClick={onRowDoubleClick}
                    onContextMenu={onRowContextMenu}
                  />
                );
              })}
            </div>
          </ContextMenuTrigger>
          {rowContextContent !== undefined && (
            <ContextMenuContent>{rowContextContent}</ContextMenuContent>
          )}
        </ContextMenu>
      )}
    </div>
  );
}

interface RowProps<T> {
  row: T;
  cols: readonly DataTableColumn<T>[];
  widths: readonly number[];
  top: number;
  height: number;
  ariaRowIndex: number;
  domId: string;
  isSelected: boolean;
  isFocused: boolean;
  onMouseDown?: (row: T, e: MouseEvent) => void;
  onDoubleClick?: (row: T) => void;
  onContextMenu?: (row: T) => void;
}

function Row<T>({
  row,
  cols,
  widths,
  top,
  height,
  ariaRowIndex,
  domId,
  isSelected,
  isFocused,
  onMouseDown,
  onDoubleClick,
  onContextMenu,
}: RowProps<T>) {
  return (
    <div
      id={domId}
      role="row"
      aria-rowindex={ariaRowIndex}
      aria-selected={isSelected}
      className={cn(
        'absolute left-0 flex w-full items-center border-b border-border/40 text-[13px]',
        isSelected ? 'bg-selected' : 'hover:bg-accent/60',
        isFocused && 'ring-1 ring-ring ring-inset',
      )}
      style={{ transform: `translateY(${top}px)`, height }}
      onMouseDown={(e) => onMouseDown?.(row, e)}
      onDoubleClick={() => onDoubleClick?.(row)}
      onContextMenu={() => onContextMenu?.(row)}
    >
      {cols.map((col, i) => (
        <div
          key={col.id}
          role="gridcell"
          className={cn(
            'h-full overflow-hidden px-2 leading-[inherit] whitespace-nowrap',
            col.align === 'right' && 'text-right',
          )}
          style={{ width: widths[i], lineHeight: `${height}px` }}
        >
          {col.cell(row)}
        </div>
      ))}
    </div>
  );
}

const MemoRow = memo(Row) as typeof Row;
