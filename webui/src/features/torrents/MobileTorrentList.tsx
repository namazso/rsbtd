// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

import { ArrowDown, ArrowUp, Check } from 'lucide-react';
import { useVirtualizer } from '@tanstack/react-virtual';
import { memo, useLayoutEffect, useRef } from 'react';
import { cn } from '@/lib/cn';
import { useLongPress } from '@/lib/hooks';
import { formatBytes, formatRate } from '@/lib/format';
import type { TorrentRow } from '@/store/torrents';
import { useUi } from '@/store/ui';
import { StatusIcon } from './TorrentTable';
import { statusStyle, uiStatus } from './status';

/**
 * Mobile torrent list (spec): double-height rows; status icon left; first
 * row the name; second row size (left) and ↓/↑ speeds (right); progress
 * rendered as the item's translucent background fill, hued by status.
 */
export interface MobileListCallbacks {
  onTap: (row: TorrentRow) => void;
  onLongPress: (row: TorrentRow) => void;
}

const ROW_HEIGHT = 64;
const LIST_KEY = 'mobileTorrents';

export function MobileTorrentList({
  rows,
  selectionMode,
  selected,
  callbacks,
  emptyText,
}: {
  rows: readonly TorrentRow[];
  selectionMode: boolean;
  selected: ReadonlySet<string>;
  callbacks: MobileListCallbacks;
  emptyText: string;
}) {
  const scrollRef = useRef<HTMLDivElement>(null);
  const initialOffset = useRef(useUi.getState().listOffsets[LIST_KEY] ?? 0);
  const virtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => ROW_HEIGHT,
    overscan: 12,
    initialOffset: initialOffset.current,
    onChange: (v) => useUi.getState().setListOffset(LIST_KEY, v.scrollOffset ?? 0),
  });

  useLayoutEffect(() => {
    scrollRef.current?.scrollTo({ top: initialOffset.current });
  }, []);

  if (rows.length === 0) {
    return (
      <div className="flex flex-1 items-center justify-center px-8 text-center text-sm text-muted-foreground">
        {emptyText}
      </div>
    );
  }

  return (
    <div ref={scrollRef} className="min-h-0 flex-1 overflow-y-auto overscroll-contain">
      <div className="relative" style={{ height: virtualizer.getTotalSize() }}>
        {virtualizer.getVirtualItems().map((vi) => {
          const row = rows[vi.index];
          if (row === undefined) return null;
          return (
            <MemoMobileRow
              key={row.infoHash}
              row={row}
              top={vi.start}
              selectionMode={selectionMode}
              isSelected={selected.has(row.infoHash)}
              callbacks={callbacks}
            />
          );
        })}
      </div>
    </div>
  );
}

function MobileRow({
  row,
  top,
  selectionMode,
  isSelected,
  callbacks,
}: {
  row: TorrentRow;
  top: number;
  selectionMode: boolean;
  isSelected: boolean;
  callbacks: MobileListCallbacks;
}) {
  const { handlers, consumedClick } = useLongPress(() => callbacks.onLongPress(row));
  const style = statusStyle(uiStatus(row));
  return (
    <button
      type="button"
      {...handlers}
      onClick={() => {
        if (!consumedClick()) callbacks.onTap(row);
      }}
      aria-pressed={selectionMode ? isSelected : undefined}
      className={cn(
        'absolute left-0 block w-full border-b border-border/50 text-left select-none',
        '[-webkit-touch-callout:none]',
        isSelected && 'bg-selected',
      )}
      style={{ transform: `translateY(${top}px)`, height: ROW_HEIGHT }}
    >
      {/* progress backdrop */}
      {!isSelected && (
        <span
          aria-hidden
          className={cn('absolute inset-y-0 left-0', style.bg, 'opacity-15')}
          style={{ width: `${row.progressPpm / 10_000}%` }}
        />
      )}
      <span className="relative flex h-full items-center gap-3 px-3">
        {selectionMode && (
          <span
            aria-hidden
            className={cn(
              'flex size-4 shrink-0 items-center justify-center rounded-sm border border-border',
              isSelected && 'border-primary bg-primary text-primary-foreground',
            )}
          >
            {isSelected && <Check className="size-3" strokeWidth={3} />}
          </span>
        )}
        <StatusIcon row={row} className="size-5" />
        <span className="min-w-0 flex-1">
          <span className="block truncate text-sm font-medium">{row.name}</span>
          <span className="mt-0.5 flex items-center justify-between text-xs text-muted-foreground">
            <span>{formatBytes(row.totalWanted)}</span>
            <span className="flex items-center gap-2 tabular-nums">
              <span className="flex items-center gap-0.5">
                <ArrowDown className="size-3 text-st-download" />
                {formatRate(row.downloadPayloadRate)}
              </span>
              <span className="flex items-center gap-0.5">
                <ArrowUp className="size-3 text-st-seed" />
                {formatRate(row.uploadPayloadRate)}
              </span>
            </span>
          </span>
        </span>
      </span>
    </button>
  );
}

const MemoMobileRow = memo(MobileRow);
