// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

import { ChevronDown, ChevronRight, File, Folder, Link2 } from 'lucide-react';
import { useEffect, useMemo, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { toast } from 'sonner';
import { mutations } from '@/api/mutations';
import { Button } from '@/components/ui/button';
import { CheckboxField } from '@/components/ui/checkbox';
import { Dialog, DialogContent, DialogFooter, DialogTitle } from '@/components/ui/dialog';
import { Input, Label, NativeSelect } from '@/components/ui/input';
import { cn } from '@/lib/cn';
import { formatBytes } from '@/lib/format';
import { tDynamic } from '@/lib/i18nDynamic';
import { useIsMobile } from '@/lib/platform';
import { buildFileTree, buildPriorityList, type FileTreeRow, type TorrentFile } from './fileTree';
import { useFiles } from './useDetailQueries';

/**
 * Files tab: path tree with folder rollups, per-file/folder priority
 * (0 skip / 1 low / 4 normal / 7 high), rename, progress. Pad files hidden
 * by default. Priorities always submit the complete list.
 */
const PRIORITY_CHOICES = [0, 1, 4, 7] as const;

function priorityLabel(value: number | null): string {
  if (value === null) return tDynamic('details:files.priorityMixed');
  if (value === 0 || value === 1 || value === 4 || value === 7) {
    return tDynamic(`details:files.priority${value}`);
  }
  return tDynamic('details:files.priorityCustom', { value });
}

function PrioritySelect({
  value,
  onChange,
  className,
}: {
  value: number | null;
  onChange: (priority: number) => void;
  className?: string;
}) {
  const isCustom = value !== null && !PRIORITY_CHOICES.includes(value as 0 | 1 | 4 | 7);
  return (
    <NativeSelect
      value={value === null ? 'mixed' : String(value)}
      onChange={(e) => {
        const v = Number(e.target.value);
        if (Number.isFinite(v)) onChange(v);
      }}
      className={cn('h-6 w-auto min-w-20 px-1 py-0 text-xs', className)}
      onClick={(e) => e.stopPropagation()}
    >
      {value === null && <option value="mixed">{priorityLabel(null)}</option>}
      {isCustom && <option value={String(value)}>{priorityLabel(value)}</option>}
      {PRIORITY_CHOICES.map((p) => (
        <option key={p} value={String(p)}>
          {priorityLabel(p)}
        </option>
      ))}
    </NativeSelect>
  );
}

export function FilesTab({ uuid, visible }: { uuid: string; visible: boolean }) {
  const { t } = useTranslation(['details', 'common']);
  const isMobile = useIsMobile();
  const query = useFiles(uuid, visible);
  const files = query.data?.data?.torrent?.files ?? null;
  const [showPad, setShowPad] = useState(false);
  const [collapsed, setCollapsed] = useState<ReadonlySet<string>>(new Set());
  const [renameTarget, setRenameTarget] = useState<TorrentFile | null>(null);

  // Priority edits submitted but not necessarily reflected in the files
  // snapshot yet. The mutation takes the complete vector, so a second
  // edit built from a stale snapshot would silently undo the first one;
  // every submission therefore re-applies all still-pending edits, and
  // an entry is dropped once the snapshot agrees with it.
  const pendingPriorities = useRef(new Map<number, number>());
  const filesRef = useRef(files);
  const submitChain = useRef<Promise<unknown>>(Promise.resolve());
  useEffect(() => {
    filesRef.current = files;
    if (files) {
      const pending = pendingPriorities.current;
      for (const file of files) {
        if (pending.get(file.index) === file.priority) pending.delete(file.index);
      }
    }
  }, [files]);

  const rows = useMemo(
    () => (files ? buildFileTree(files, showPad, collapsed) : []),
    [files, showPad, collapsed],
  );

  const errorText =
    query.error != null
      ? t('loadError', { message: query.error.message })
      : query.data != null && query.data.errors.length > 0
        ? t('partialError', { message: query.data.errors[0] })
        : null;

  if (files === null) {
    const noData = query.data == null;
    return (
      <p
        className={cn(
          'p-4 text-sm',
          noData && errorText != null ? 'text-st-error' : 'text-muted-foreground',
        )}
      >
        {noData ? (errorText ?? t('loading')) : t('noMetadata')}
      </p>
    );
  }

  const setPriority = (indexes: readonly number[], priority: number) => {
    const pending = pendingPriorities.current;
    for (const index of indexes) pending.set(index, priority);
    // Serialized: each submission sees the freshest snapshot plus every
    // still-pending edit, so concurrent edits merge instead of racing.
    submitChain.current = submitChain.current
      .then(() =>
        mutations.setFilePriorities(uuid, buildPriorityList(filesRef.current ?? [], pending)),
      )
      .then(() => query.refetch())
      .catch((err: unknown) => {
        pending.clear();
        toast.error(err instanceof Error ? err.message : String(err));
      });
  };

  const toggleCollapse = (id: string) => {
    setCollapsed((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  };

  return (
    <div className="flex h-full flex-col">
      <div className="flex shrink-0 items-center gap-3 border-b border-border px-3 py-1.5">
        <CheckboxField
          checked={showPad}
          onCheckedChange={(v) => setShowPad(v === true)}
          label={<span className="text-xs">{t('files.showPadFiles')}</span>}
        />
        {errorText != null && <span className="truncate text-xs text-st-error">{errorText}</span>}
      </div>

      <div className="min-h-0 flex-1 overflow-auto">
        {/* header (desktop) */}
        {!isMobile && (
          <div className="sticky top-0 z-10 flex border-b border-border bg-background px-2 text-xs font-medium text-muted-foreground">
            <span className="flex-1 py-1">{t('files.name')}</span>
            <span className="w-20 py-1 text-right">{t('files.size')}</span>
            <span className="w-20 py-1 text-right">{t('files.progress')}</span>
            <span className="w-28 py-1 pl-3">{t('files.priority')}</span>
          </div>
        )}
        {rows.map((row) => (
          <FileRow
            key={row.id}
            row={row}
            isMobile={isMobile}
            collapsed={collapsed.has(row.id)}
            onToggle={() => toggleCollapse(row.id)}
            onPriority={(p) => setPriority(row.fileIndexes, p)}
            onRename={() => row.file && setRenameTarget(row.file)}
          />
        ))}
        {rows.length === 0 && (
          <p className="p-4 text-sm text-muted-foreground">{t('files.empty')}</p>
        )}
      </div>

      <RenameDialog
        uuid={uuid}
        target={renameTarget}
        onClose={() => setRenameTarget(null)}
        onRenamed={() => void query.refetch()}
      />
    </div>
  );
}

function FileRow({
  row,
  isMobile,
  collapsed,
  onToggle,
  onPriority,
  onRename,
}: {
  row: FileTreeRow;
  isMobile: boolean;
  collapsed: boolean;
  onToggle: () => void;
  onPriority: (priority: number) => void;
  onRename: () => void;
}) {
  const { t } = useTranslation('details');
  const progress = row.size > 0 ? Math.round((row.progressBytes / row.size) * 100) : 100;
  const Icon = row.isDir ? Folder : row.file?.isSymlink === true ? Link2 : File;

  return (
    <div
      className={cn(
        'flex items-center border-b border-border/40 px-2 text-[13px] hover:bg-accent/50',
        row.priority === 0 && 'opacity-50',
      )}
      style={{ paddingLeft: 8 + row.depth * 16 }}
    >
      {row.isDir ? (
        <button
          type="button"
          onClick={onToggle}
          aria-expanded={!collapsed}
          className="mr-0.5 rounded p-0.5 hover:bg-accent"
        >
          {collapsed ? <ChevronRight className="size-3.5" /> : <ChevronDown className="size-3.5" />}
        </button>
      ) : (
        <span className="w-5" />
      )}
      <Icon className="mr-1.5 size-4 shrink-0 text-muted-foreground" />
      <span className="min-w-0 flex-1 py-1.5">
        <span className="block truncate" title={row.file?.path ?? row.name}>
          {row.name}
          {row.file?.isSymlink === true && row.file.symlinkTarget != null && (
            <span className="ml-1 text-xs text-muted-foreground">
              {t('files.symlinkTo', { target: row.file.symlinkTarget })}
            </span>
          )}
        </span>
        {isMobile && (
          <span className="block text-xs text-muted-foreground">
            {formatBytes(row.size)}
            {' · '}
            {progress}%
          </span>
        )}
      </span>
      {!isMobile && (
        <>
          <span className="w-20 text-right tabular-nums">{formatBytes(row.size)}</span>
          <span className="w-20 text-right tabular-nums">{progress}%</span>
        </>
      )}
      <span className="flex w-28 items-center gap-1 pl-3">
        <PrioritySelect value={row.priority} onChange={onPriority} />
      </span>
      {!row.isDir && (
        <Button variant="ghost" size="sm" className="ml-1 h-6 px-1.5 text-xs" onClick={onRename}>
          {t('files.rename')}
        </Button>
      )}
    </div>
  );
}

function RenameDialog({
  uuid,
  target,
  onClose,
  onRenamed,
}: {
  uuid: string;
  target: TorrentFile | null;
  onClose: () => void;
  onRenamed: () => void;
}) {
  const { t } = useTranslation(['details', 'common']);
  const [name, setName] = useState('');
  const [busy, setBusy] = useState(false);
  const open = target !== null;

  // Reset the field whenever a new target opens.
  const [lastTarget, setLastTarget] = useState<TorrentFile | null>(null);
  if (target !== lastTarget) {
    setLastTarget(target);
    setName(target?.path ?? '');
  }

  const submit = async () => {
    if (target === null || name.trim() === '') return;
    setBusy(true);
    try {
      // Waits server-side for the rename confirmation.
      const result = await mutations.renameFile(uuid, target.index, name.trim());
      toast.success(t('files.renameDone', { name: result.renameFile }));
      onRenamed();
      onClose();
    } catch (err) {
      toast.error(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={(o) => !o && onClose()}>
      <DialogContent>
        <DialogTitle>{t('files.renameTitle')}</DialogTitle>
        <Label htmlFor="rename-file-input">{t('files.renameLabel')}</Label>
        <Input
          id="rename-file-input"
          value={name}
          onChange={(e) => setName(e.target.value)}
          spellCheck={false}
          className="font-mono text-xs"
        />
        <DialogFooter>
          <Button variant="outline" onClick={onClose}>
            {t('common:actions.cancel')}
          </Button>
          <Button disabled={busy || name.trim() === ''} onClick={() => void submit()}>
            {t('common:actions.apply')}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
