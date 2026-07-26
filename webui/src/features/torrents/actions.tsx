// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

import {
  ArrowDown,
  ArrowDownToLine,
  ArrowUp,
  ArrowUpToLine,
  ClipboardCopy,
  FolderInput,
  Gauge,
  ListOrdered,
  Megaphone,
  MoreHorizontal,
  Pause,
  Play,
  RefreshCw,
  Trash2,
  Zap,
} from 'lucide-react';
import { toast } from 'sonner';
import { useTranslation } from 'react-i18next';
import { bulk, mutations } from '@/api/mutations';
import { copyText, copyTextFrom } from '@/lib/clipboard';
import type { MenuKit } from '@/components/actions/menuKit';
import { tDynamic } from '@/lib/i18nDynamic';
import { useTorrents, type TorrentRow } from '@/store/torrents';
import { useUi } from '@/store/ui';

/**
 * Torrent actions, rendered identically in the row context menu, the
 * toolbar overflow menu, and the mobile action sheet.
 *
 * Auto-management semantics: pause detaches from the queue so
 * the torrent stays paused; plain Resume re-attaches (`AUTO_MANAGED`) so
 * queue logic takes over; Force resume resumes *without* re-attaching.
 */
async function bulkToast(uuids: readonly string[], fn: (uuid: string) => Promise<unknown>) {
  const { errors } = await bulk(uuids, fn);
  if (errors.length > 0) {
    toast.error(
      tDynamic('torrents:bulk.failed', { count: errors.length }) + `: ${errors[0]?.message ?? ''}`,
    );
  }
}

export const torrentCommands = {
  resume: (uuids: readonly string[]) =>
    bulkToast(uuids, async (h) => {
      await mutations.setFlags(h, ['AUTO_MANAGED'], []);
      await mutations.resume(h);
    }),
  forceResume: (uuids: readonly string[]) =>
    bulkToast(uuids, async (h) => {
      await mutations.setFlags(h, [], ['AUTO_MANAGED']);
      await mutations.resume(h);
    }),
  pause: (uuids: readonly string[], graceful = false) =>
    bulkToast(uuids, (h) => mutations.pause(h, graceful)),
  recheck: (uuids: readonly string[]) => bulkToast(uuids, (h) => mutations.recheck(h)),
  reannounce: (uuids: readonly string[]) => bulkToast(uuids, (h) => mutations.reannounce(h)),
  dhtAnnounce: (uuids: readonly string[]) => bulkToast(uuids, (h) => mutations.dhtAnnounce(h)),
  clearError: (uuids: readonly string[]) => bulkToast(uuids, (h) => mutations.clearError(h)),
  flushCache: (uuids: readonly string[]) => bulkToast(uuids, (h) => mutations.flushCache(h)),
  saveResumeData: (uuids: readonly string[]) =>
    bulkToast(uuids, (h) => mutations.saveResumeData(h)),
  queue: (uuids: readonly string[], op: 'top' | 'up' | 'down' | 'bottom') => {
    // The moves are applied one at a time, so order decides the result:
    // an adjacent multi-selection moved "up" in arbitrary order leapfrogs
    // itself, and "top" applied first-to-last reverses the selection.
    // Order by the current queue position so relative order is preserved:
    // ascending for up (the highest row moves first), descending for
    // down and top (top pushes later rows above earlier ones), ascending
    // for bottom.
    const store = useTorrents.getState();
    const position = (uuid: string) =>
      store.byUuid.get(uuid)?.queuePosition ?? Number.MAX_SAFE_INTEGER;
    const ordered = [...uuids].sort((a, b) =>
      op === 'up' || op === 'bottom' ? position(a) - position(b) : position(b) - position(a),
    );
    return bulkToast(ordered, (h) =>
      op === 'top'
        ? mutations.queueTop(h)
        : op === 'up'
          ? mutations.queueUp(h)
          : op === 'down'
            ? mutations.queueDown(h)
            : mutations.queueBottom(h),
    );
  },
  setFlag: (
    rows: readonly TorrentRow[],
    flag: 'SEQUENTIAL_DOWNLOAD' | 'SUPER_SEEDING' | 'AUTO_MANAGED',
    target: boolean,
  ) =>
    bulkToast(
      rows.filter((r) => r.flags.includes(flag) !== target).map((r) => r.uuid),
      (h) => mutations.setFlags(h, target ? [flag] : [], target ? [] : [flag]),
    ),
};

function copyMagnets(rows: readonly TorrentRow[]): Promise<void> {
  return copyTextFrom(async () => {
    const uris: string[] = [];
    for (const row of rows) {
      const uri = await mutations.magnetUri(row.uuid);
      if (uri !== null) uris.push(uri);
    }
    return uris.join('\n');
  });
}

export interface TorrentActionItemsProps {
  kit: MenuKit;
  rows: readonly TorrentRow[];
}

/** Menu body shared by context menu / toolbar overflow / action sheet. */
export function TorrentActionItems({ kit, rows }: TorrentActionItemsProps) {
  const { t } = useTranslation(['torrents', 'common']);
  const K = kit;
  const uuids = rows.map((r) => r.uuid);
  const none = rows.length === 0;
  const anyError = rows.some((r) => r.error != null);
  const allSequential =
    rows.length > 0 && rows.every((r) => r.flags.includes('SEQUENTIAL_DOWNLOAD'));
  const allSuperSeeding = rows.length > 0 && rows.every((r) => r.flags.includes('SUPER_SEEDING'));
  const allAutoManaged = rows.length > 0 && rows.every((r) => r.flags.includes('AUTO_MANAGED'));
  const ui = useUi.getState();

  return (
    <>
      <K.Item disabled={none} onSelect={() => void torrentCommands.resume(uuids)}>
        <Play />
        {t('actions.resume')}
      </K.Item>
      <K.Item disabled={none} onSelect={() => void torrentCommands.pause(uuids)}>
        <Pause />
        {t('actions.pause')}
      </K.Item>
      <K.Sub>
        <K.SubTrigger>
          <MoreHorizontal />
          {t('actions.more')}
        </K.SubTrigger>
        <K.SubContent>
          <K.Item disabled={none} onSelect={() => void torrentCommands.forceResume(uuids)}>
            <Zap />
            {t('actions.forceResume')}
          </K.Item>
          <K.Item disabled={none} onSelect={() => void torrentCommands.pause(uuids, true)}>
            <Pause />
            {t('actions.pauseGraceful')}
          </K.Item>
          <K.Separator />
          <K.Item
            disabled={!anyError}
            onSelect={() =>
              void torrentCommands.clearError(
                rows.filter((r) => r.error != null).map((r) => r.uuid),
              )
            }
          >
            {t('actions.clearError')}
          </K.Item>
          <K.Item disabled={none} onSelect={() => void torrentCommands.flushCache(uuids)}>
            {t('actions.flushCache')}
          </K.Item>
          <K.Item disabled={none} onSelect={() => void torrentCommands.saveResumeData(uuids)}>
            {t('actions.saveResumeData')}
          </K.Item>
        </K.SubContent>
      </K.Sub>
      <K.Separator />
      <K.Item disabled={none} onSelect={() => void torrentCommands.recheck(uuids)}>
        <RefreshCw />
        {t('actions.recheck')}
      </K.Item>
      <K.Item disabled={none} onSelect={() => void torrentCommands.reannounce(uuids)}>
        <Megaphone />
        {t('actions.reannounce')}
      </K.Item>
      <K.Item disabled={none} onSelect={() => void torrentCommands.dhtAnnounce(uuids)}>
        <Megaphone />
        {t('actions.dhtAnnounce')}
      </K.Item>
      <K.Separator />
      <K.Sub>
        <K.SubTrigger>
          <ListOrdered />
          {t('actions.queue')}
        </K.SubTrigger>
        <K.SubContent>
          <K.Item disabled={none} onSelect={() => void torrentCommands.queue(uuids, 'top')}>
            <ArrowUpToLine />
            {t('actions.queueTop')}
          </K.Item>
          <K.Item disabled={none} onSelect={() => void torrentCommands.queue(uuids, 'up')}>
            <ArrowUp />
            {t('actions.queueUp')}
          </K.Item>
          <K.Item disabled={none} onSelect={() => void torrentCommands.queue(uuids, 'down')}>
            <ArrowDown />
            {t('actions.queueDown')}
          </K.Item>
          <K.Item disabled={none} onSelect={() => void torrentCommands.queue(uuids, 'bottom')}>
            <ArrowDownToLine />
            {t('actions.queueBottom')}
          </K.Item>
        </K.SubContent>
      </K.Sub>
      <K.CheckboxItem
        checked={allSequential}
        disabled={none}
        onCheckedChange={(v) => void torrentCommands.setFlag(rows, 'SEQUENTIAL_DOWNLOAD', v)}
      >
        {t('actions.sequential')}
      </K.CheckboxItem>
      <K.CheckboxItem
        checked={allSuperSeeding}
        disabled={none}
        onCheckedChange={(v) => void torrentCommands.setFlag(rows, 'SUPER_SEEDING', v)}
      >
        {t('actions.superSeeding')}
      </K.CheckboxItem>
      <K.CheckboxItem
        checked={allAutoManaged}
        disabled={none}
        onCheckedChange={(v) => void torrentCommands.setFlag(rows, 'AUTO_MANAGED', v)}
      >
        {t('actions.autoManaged')}
      </K.CheckboxItem>
      <K.Separator />
      <K.Item disabled={none} onSelect={() => ui.openMoveDialog(uuids)}>
        <FolderInput />
        {t('actions.setLocation')}
      </K.Item>
      <K.Item disabled={none} onSelect={() => ui.openLimitsDialog(uuids)}>
        <Gauge />
        {t('actions.limits')}
      </K.Item>
      <K.Separator />
      <K.Sub>
        <K.SubTrigger>
          <ClipboardCopy />
          {t('actions.copy')}
        </K.SubTrigger>
        <K.SubContent>
          <K.Item
            disabled={none}
            onSelect={() => void copyText(rows.map((r) => r.name).join('\n'))}
          >
            {t('actions.copyName')}
          </K.Item>
          <K.Item
            disabled={rows.every((r) => r.infoHashV1 == null)}
            onSelect={() =>
              void copyText(
                rows.flatMap((r) => (r.infoHashV1 != null ? [r.infoHashV1] : [])).join('\n'),
              )
            }
          >
            {t('actions.copyHashV1')}
          </K.Item>
          <K.Item
            disabled={rows.every((r) => r.infoHashV2 == null)}
            onSelect={() =>
              void copyText(
                rows.flatMap((r) => (r.infoHashV2 != null ? [r.infoHashV2] : [])).join('\n'),
              )
            }
          >
            {t('actions.copyHashV2')}
          </K.Item>
          <K.Item disabled={none} onSelect={() => void copyMagnets(rows)}>
            {t('actions.copyMagnet')}
          </K.Item>
        </K.SubContent>
      </K.Sub>
      <K.Separator />
      <K.Item
        disabled={none}
        className="text-destructive data-[highlighted]:bg-destructive/10"
        onSelect={() => ui.openRemoveDialog(uuids)}
      >
        <Trash2 className="!text-destructive" />
        {t('actions.remove')}
      </K.Item>
    </>
  );
}
