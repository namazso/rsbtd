// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

import { Plus } from 'lucide-react';
import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { toast } from 'sonner';
import { mutations } from '@/api/mutations';
import { Button } from '@/components/ui/button';
import { Dialog, DialogContent, DialogFooter, DialogTitle } from '@/components/ui/dialog';
import { Input, Label } from '@/components/ui/input';
import { Tooltip } from '@/components/ui/tooltip';
import { cn } from '@/lib/cn';
import { formatBytes, formatPercentPpm, formatRate } from '@/lib/format';
import { tEnum } from '@/lib/i18nDynamic';
import { useIsMobile } from '@/lib/platform';
import { usePeers } from './useDetailQueries';

/** `ip:port` / `[v6]:port` literal (hostnames rejected by the API). */
export function isValidPeerAddress(text: string): boolean {
  const v4 = /^(\d{1,3}\.){3}\d{1,3}:\d{1,5}$/;
  const v6 = /^\[[0-9a-fA-F:]+\]:\d{1,5}$/;
  return v4.test(text) || v6.test(text);
}

export function PeersTab({ uuid, visible }: { uuid: string; visible: boolean }) {
  const { t } = useTranslation(['details', 'common']);
  const isMobile = useIsMobile();
  const query = usePeers(uuid, visible);
  const peers = query.data?.data?.torrent?.peers ?? [];
  const [addOpen, setAddOpen] = useState(false);

  const errorText =
    query.error != null
      ? t('loadError', { message: query.error.message })
      : query.data != null && query.data.errors.length > 0
        ? t('partialError', { message: query.data.errors[0] })
        : null;

  return (
    <div className="flex h-full flex-col">
      <div className="flex shrink-0 items-center gap-1.5 border-b border-border px-3 py-1.5">
        <Button size="sm" variant="outline" onClick={() => setAddOpen(true)}>
          <Plus />
          {t('peers.addPeer')}
        </Button>
        {query.data != null && errorText != null && (
          <span className="truncate text-xs text-st-error">{errorText}</span>
        )}
      </div>

      <div className="min-h-0 flex-1 overflow-auto">
        {peers.length === 0 && (
          <p
            className={cn(
              'p-4 text-sm',
              query.data == null && errorText != null ? 'text-st-error' : 'text-muted-foreground',
            )}
          >
            {query.data == null ? (errorText ?? t('loading')) : t('peers.empty')}
          </p>
        )}
        {!isMobile && peers.length > 0 && (
          <div className="sticky top-0 z-10 flex border-b border-border bg-background px-3 text-xs font-medium text-muted-foreground">
            <span className="w-44 py-1">{t('peers.address')}</span>
            <span className="w-40 py-1">{t('peers.client')}</span>
            <span className="w-16 py-1 text-right">{t('peers.progress')}</span>
            <span className="w-20 py-1 text-right">{t('peers.downSpeed')}</span>
            <span className="w-20 py-1 text-right">{t('peers.upSpeed')}</span>
            <span className="w-24 py-1 text-right">{t('peers.downloaded')}</span>
            <span className="w-24 py-1 text-right">{t('peers.uploaded')}</span>
            <span className="w-14 py-1 text-right">{t('peers.rtt')}</span>
            <span className="flex-1 py-1 pl-3">{t('peers.flags')}</span>
          </div>
        )}
        {peers.map((peer, i) => {
          const flagText = peer.flags.join(' ').toLowerCase();
          const sourceText = peer.source.join(', ').toLowerCase();
          const address = peer.address ?? '—';
          return isMobile ? (
            <div
              key={`${address}-${i}`}
              className="border-b border-border/40 px-3 py-1.5 text-[13px]"
            >
              <div className="flex justify-between gap-2">
                <span className="truncate font-mono text-xs">{address}</span>
                <span className="shrink-0 tabular-nums">{formatPercentPpm(peer.progressPpm)}</span>
              </div>
              <div className="flex justify-between gap-2 text-xs text-muted-foreground">
                <span className="truncate">{peer.client}</span>
                <span className="shrink-0 tabular-nums">
                  {formatRate(peer.payloadDownSpeed)} / {formatRate(peer.payloadUpSpeed)}
                </span>
              </div>
            </div>
          ) : (
            <div
              key={`${address}-${i}`}
              className="flex border-b border-border/40 px-3 py-1 text-[13px] hover:bg-accent/50"
            >
              <span
                className="w-44 truncate font-mono text-xs leading-5"
                title={peer.localEndpoint ?? undefined}
              >
                {address}
              </span>
              <span
                className="w-40 truncate leading-5"
                title={tEnum('details:peers.connectionType', peer.connectionType)}
              >
                {peer.client}
              </span>
              <span className="w-16 text-right leading-5 tabular-nums">
                {formatPercentPpm(peer.progressPpm)}
              </span>
              <span className="w-20 text-right leading-5 tabular-nums">
                {peer.payloadDownSpeed > 0 ? formatRate(peer.payloadDownSpeed) : ''}
              </span>
              <span className="w-20 text-right leading-5 tabular-nums">
                {peer.payloadUpSpeed > 0 ? formatRate(peer.payloadUpSpeed) : ''}
              </span>
              <span className="w-24 text-right leading-5 tabular-nums">
                {formatBytes(peer.totalDownload)}
              </span>
              <span className="w-24 text-right leading-5 tabular-nums">
                {formatBytes(peer.totalUpload)}
              </span>
              <span className="w-14 text-right leading-5 tabular-nums">{peer.rtt}</span>
              <Tooltip content={`${flagText} · ${sourceText}`}>
                <span className="min-w-0 flex-1 truncate pl-3 text-xs leading-5 text-muted-foreground">
                  {flagText}
                </span>
              </Tooltip>
            </div>
          );
        })}
      </div>

      <AddPeerDialog uuid={uuid} open={addOpen} onClose={() => setAddOpen(false)} />
    </div>
  );
}

function AddPeerDialog({
  uuid,
  open,
  onClose,
}: {
  uuid: string;
  open: boolean;
  onClose: () => void;
}) {
  const { t } = useTranslation(['details', 'common']);
  const [address, setAddress] = useState('');
  const valid = isValidPeerAddress(address.trim());

  const submit = async () => {
    try {
      await mutations.connectPeer(uuid, address.trim());
      setAddress('');
      onClose();
    } catch (err) {
      toast.error(err instanceof Error ? err.message : String(err));
    }
  };

  return (
    <Dialog open={open} onOpenChange={(o) => !o && onClose()}>
      <DialogContent>
        <DialogTitle>{t('peers.addPeerTitle')}</DialogTitle>
        <Label htmlFor="peer-address">{t('peers.addPeerLabel')}</Label>
        <Input
          id="peer-address"
          value={address}
          onChange={(e) => setAddress(e.target.value)}
          spellCheck={false}
          className="font-mono text-xs"
        />
        {!valid && address.trim() !== '' && (
          <p className="mt-1 text-xs text-st-error">{t('peers.addPeerInvalid')}</p>
        )}
        <DialogFooter>
          <Button variant="outline" onClick={onClose}>
            {t('common:actions.cancel')}
          </Button>
          <Button disabled={!valid} onClick={() => void submit()}>
            {t('peers.addPeer')}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
