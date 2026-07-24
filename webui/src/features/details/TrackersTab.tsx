// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

import { BadgeCheck, ClipboardCopy, Megaphone, Plus, Radar, Trash2 } from 'lucide-react';
import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { toast } from 'sonner';
import { mutations } from '@/api/mutations';
import { Button } from '@/components/ui/button';
import { Dialog, DialogContent, DialogFooter, DialogTitle } from '@/components/ui/dialog';
import { Input, Label } from '@/components/ui/input';
import { Tooltip } from '@/components/ui/tooltip';
import { cn } from '@/lib/cn';
import { tEnum } from '@/lib/i18nDynamic';
import { useTorrents } from '@/store/torrents';

/**
 * Trackers tab. `trackerIndex` refers to the daemon's current tracker
 * order, so indexes come from the query order. Removal replaces the full
 * list, keyed by URL (unique within libtorrent's announce list).
 */
import { useTrackers } from './useDetailQueries';

export function TrackersTab({ hash, visible }: { hash: string; visible: boolean }) {
  const { t } = useTranslation(['details', 'common']);
  const query = useTrackers(hash, visible);
  const trackers = query.data?.data?.torrent?.trackers ?? [];
  const urlSeeds = query.data?.data?.torrent?.urlSeeds ?? [];
  const currentTracker = useTorrents((s) => {
    const canonical = s.resolve(hash) ?? hash;
    return s.byHash.get(canonical)?.currentTracker ?? null;
  });
  const [addOpen, setAddOpen] = useState(false);
  const [seedUrl, setSeedUrl] = useState('');

  const errorText =
    query.error != null
      ? t('loadError', { message: query.error.message })
      : query.data != null && query.data.errors.length > 0
        ? t('partialError', { message: query.data.errors[0] })
        : null;

  const scrape = (index: number) => {
    void mutations
      .scrapeTracker(hash, index)
      .then((r) =>
        toast.success(
          t('trackers.scrapeResult', {
            url: r.scrapeTracker.trackerUrl ?? '?',
            seeds: r.scrapeTracker.complete,
            leeches: r.scrapeTracker.incomplete,
          }),
        ),
      )
      .catch((err: unknown) => toast.error(err instanceof Error ? err.message : String(err)));
  };

  const reannounce = (index: number) => {
    void mutations.reannounce(hash, 0, index).catch((err: unknown) => {
      toast.error(err instanceof Error ? err.message : String(err));
    });
  };

  const removeTracker = (url: string) => {
    const remaining = trackers
      .filter((tracker) => tracker.url !== url)
      .map((tracker) => ({ url: tracker.url, tier: tracker.tier }));
    void mutations
      .replaceTrackers(hash, remaining)
      .then(() => query.refetch())
      .catch((err: unknown) => toast.error(err instanceof Error ? err.message : String(err)));
  };

  return (
    <div className="flex h-full flex-col">
      <div className="flex shrink-0 flex-wrap items-center gap-1.5 border-b border-border px-3 py-1.5">
        <Button size="sm" variant="outline" onClick={() => setAddOpen(true)}>
          <Plus />
          {t('trackers.add')}
        </Button>
        <Button size="sm" variant="outline" onClick={() => reannounce(-1)}>
          <Megaphone />
          {t('trackers.reannounceAll')}
        </Button>
        <Button size="sm" variant="outline" onClick={() => scrape(-1)}>
          <Radar />
          {t('trackers.scrape')}
        </Button>
        {query.data != null && errorText != null && (
          <span className="truncate text-xs text-st-error">{errorText}</span>
        )}
      </div>

      <div className="min-h-0 flex-1 overflow-y-auto">
        {trackers.length === 0 && (
          <p
            className={cn(
              'p-4 text-sm',
              query.data == null && errorText != null ? 'text-st-error' : 'text-muted-foreground',
            )}
          >
            {query.data == null ? (errorText ?? t('loading')) : t('trackers.empty')}
          </p>
        )}
        {trackers.map((tracker, index) => (
          <div
            key={`${tracker.url}-${index}`}
            className={cn(
              'flex items-center gap-2 border-b border-border/40 px-3 py-1.5 text-[13px]',
              currentTracker === tracker.url && 'bg-selected/40',
            )}
          >
            <span className="w-8 shrink-0 text-xs text-muted-foreground tabular-nums">
              {tracker.tier}
            </span>
            <span className="min-w-0 flex-1">
              <span className="block truncate font-mono text-xs" title={tracker.url}>
                {tracker.url}
              </span>
              <span className="flex gap-2 text-[11px] text-muted-foreground">
                {tracker.verified && (
                  <span className="flex items-center gap-0.5 text-st-seed">
                    <BadgeCheck className="size-3" />
                    {t('trackers.verified')}
                  </span>
                )}
                {currentTracker === tracker.url && <span>{t('trackers.current')}</span>}
                <span>
                  {tracker.source.map((s) => tEnum('torrents:trackerSource', s)).join(', ')}
                </span>
              </span>
            </span>
            <Tooltip content={t('trackers.reannounceThis')}>
              <Button
                variant="ghost"
                size="iconSm"
                aria-label={t('trackers.reannounceThis')}
                onClick={() => reannounce(index)}
              >
                <Megaphone />
              </Button>
            </Tooltip>
            <Tooltip content={t('trackers.scrapeThis')}>
              <Button
                variant="ghost"
                size="iconSm"
                aria-label={t('trackers.scrapeThis')}
                onClick={() => scrape(index)}
              >
                <Radar />
              </Button>
            </Tooltip>
            <Tooltip content={t('trackers.copyUrl')}>
              <Button
                variant="ghost"
                size="iconSm"
                aria-label={t('trackers.copyUrl')}
                onClick={() => void navigator.clipboard.writeText(tracker.url)}
              >
                <ClipboardCopy />
              </Button>
            </Tooltip>
            <Tooltip content={t('trackers.remove')}>
              <Button
                variant="ghost"
                size="iconSm"
                aria-label={t('trackers.remove')}
                onClick={() => removeTracker(tracker.url)}
              >
                <Trash2 />
              </Button>
            </Tooltip>
          </div>
        ))}

        <div className="border-t border-border p-3">
          <h3 className="mb-1 text-xs font-semibold text-muted-foreground uppercase">
            {t('trackers.webSeeds')}
          </h3>
          <p className="mb-2 text-xs text-muted-foreground">{t('trackers.webSeedsHint')}</p>
          <div className="flex max-w-xl gap-1.5">
            <Input
              value={seedUrl}
              onChange={(e) => setSeedUrl(e.target.value)}
              placeholder={t('trackers.webSeedUrl')}
              spellCheck={false}
              className="font-mono text-xs"
            />
            <Button
              size="sm"
              variant="outline"
              disabled={seedUrl.trim() === ''}
              onClick={() => {
                void mutations
                  .addUrlSeed(hash, seedUrl.trim())
                  .then(() => {
                    setSeedUrl('');
                    return query.refetch();
                  })
                  .catch((err: unknown) =>
                    toast.error(err instanceof Error ? err.message : String(err)),
                  );
              }}
            >
              {t('trackers.addSeed')}
            </Button>
          </div>
          {urlSeeds.length > 0 && (
            <ul className="mt-2 max-w-xl space-y-0.5">
              {urlSeeds.map((url) => (
                <li key={url} className="flex items-center gap-1.5">
                  <span className="min-w-0 flex-1 truncate font-mono text-xs" title={url}>
                    {url}
                  </span>
                  <Tooltip content={t('trackers.copyUrl')}>
                    <Button
                      variant="ghost"
                      size="iconSm"
                      aria-label={t('trackers.copyUrl')}
                      onClick={() => void navigator.clipboard.writeText(url)}
                    >
                      <ClipboardCopy />
                    </Button>
                  </Tooltip>
                  <Tooltip content={t('trackers.removeSeed')}>
                    <Button
                      variant="ghost"
                      size="iconSm"
                      aria-label={t('trackers.removeSeed')}
                      onClick={() => {
                        void mutations
                          .removeUrlSeed(hash, url)
                          .then(() => query.refetch())
                          .catch((err: unknown) =>
                            toast.error(err instanceof Error ? err.message : String(err)),
                          );
                      }}
                    >
                      <Trash2 />
                    </Button>
                  </Tooltip>
                </li>
              ))}
            </ul>
          )}
        </div>
      </div>

      <AddTrackerDialog
        hash={hash}
        open={addOpen}
        onClose={() => setAddOpen(false)}
        onAdded={() => void query.refetch()}
      />
    </div>
  );
}

function AddTrackerDialog({
  hash,
  open,
  onClose,
  onAdded,
}: {
  hash: string;
  open: boolean;
  onClose: () => void;
  onAdded: () => void;
}) {
  const { t } = useTranslation(['details', 'common']);
  const [url, setUrl] = useState('');
  const [tier, setTier] = useState('0');

  const submit = async () => {
    const tierNum = Number(tier);
    try {
      await mutations.addTracker(hash, url.trim(), Number.isFinite(tierNum) ? tierNum : 0);
      setUrl('');
      onAdded();
      onClose();
    } catch (err) {
      toast.error(err instanceof Error ? err.message : String(err));
    }
  };

  return (
    <Dialog open={open} onOpenChange={(o) => !o && onClose()}>
      <DialogContent>
        <DialogTitle>{t('trackers.addTitle')}</DialogTitle>
        <Label htmlFor="tracker-url">{t('trackers.urlLabel')}</Label>
        <Input
          id="tracker-url"
          value={url}
          onChange={(e) => setUrl(e.target.value)}
          spellCheck={false}
          className="mb-3 font-mono text-xs"
        />
        <Label htmlFor="tracker-tier">{t('trackers.tierLabel')}</Label>
        <Input
          id="tracker-tier"
          type="number"
          min={0}
          value={tier}
          onChange={(e) => setTier(e.target.value)}
          className="w-24"
        />
        <DialogFooter>
          <Button variant="outline" onClick={onClose}>
            {t('common:actions.cancel')}
          </Button>
          <Button disabled={url.trim() === ''} onClick={() => void submit()}>
            {t('trackers.add')}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
