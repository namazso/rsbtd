// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

import { useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { toast } from 'sonner';
import { mutations } from '@/api/mutations';
import { Button } from '@/components/ui/button';
import { CheckboxField } from '@/components/ui/checkbox';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogTitle,
} from '@/components/ui/dialog';
import { Input, Label, Textarea } from '@/components/ui/input';
import { arrayBufferToBase64, MAX_TORRENT_FILE_BYTES } from '@/lib/base64';
import { parseRateKiB } from '@/lib/rateLimit';
import { usePrefs } from '@/store/prefs';
import { useUi } from '@/store/ui';
import { extractMagnets } from './magnet';
import type { AddTorrentInput, TorrentFlag } from '@/gen/gql/graphql';

/**
 * Add torrents from magnet links and/or .torrent files. Multiple sources
 * are added sequentially; per-item failures (e.g. duplicates) are reported
 * without aborting the rest. There is no server-side file browser or
 * default download dir in the API — the save path is free text with
 * locally remembered suggestions.
 */
export function AddTorrentDialog() {
  const { t } = useTranslation(['torrents', 'common']);
  const init = useUi((s) => s.addDialog);
  const close = useUi((s) => s.closeDialogs);
  const savePaths = usePrefs((s) => s.savePaths);
  const addSavePath = usePrefs((s) => s.addSavePath);

  const [magnets, setMagnets] = useState('');
  const [files, setFiles] = useState<File[]>([]);
  const [savePath, setSavePath] = useState('');
  const [rename, setRename] = useState('');
  const [paused, setPaused] = useState(false);
  const [sequential, setSequential] = useState(false);
  const [skipContent, setSkipContent] = useState(false);
  const [showMore, setShowMore] = useState(false);
  const [trackers, setTrackers] = useState('');
  const [urlSeeds, setUrlSeeds] = useState('');
  const [upLimit, setUpLimit] = useState('');
  const [downLimit, setDownLimit] = useState('');
  const [busy, setBusy] = useState(false);
  const fileInputRef = useRef<HTMLInputElement>(null);
  const seenInit = useRef<object | null>(null);
  const invocation = useRef(0);

  const open = init !== null;

  // Merge newly dropped/pasted content into the open dialog.
  useEffect(() => {
    if (init === null) {
      seenInit.current = null;
      invocation.current++;
      return;
    }
    if (seenInit.current === init) return;
    const first = seenInit.current === null;
    seenInit.current = init;
    if (first) {
      setMagnets(init.magnet ?? '');
      setFiles(init.files ?? []);
      setRename('');
      setPaused(false);
      setSequential(false);
      setSkipContent(false);
      setShowMore(false);
      setTrackers('');
      setUrlSeeds('');
      setUpLimit('');
      setDownLimit('');
      setBusy(false);
      setSavePath((prev) => (prev !== '' ? prev : (usePrefs.getState().savePaths[0] ?? '')));
    } else {
      if (init.magnet !== undefined && init.magnet !== '') {
        setMagnets((prev) => (prev === '' ? init.magnet! : `${prev}\n${init.magnet!}`));
      }
      if (init.files?.length) setFiles((prev) => [...prev, ...init.files!]);
    }
  }, [init]);

  const upLimitInvalid = parseRateKiB(upLimit) === null;
  const downLimitInvalid = parseRateKiB(downLimit) === null;

  const submit = async () => {
    if (upLimitInvalid || downLimitInvalid) return;
    const magnetList = extractMagnets(magnets);
    if (magnetList.length === 0 && files.length === 0) {
      toast.error(t('add.nothingToAdd'));
      return;
    }
    const path = savePath.trim();
    if (path === '') {
      toast.error(t('add.savePathRequired'));
      return;
    }

    const token = invocation.current;
    const initAtSubmit = seenInit.current;
    setBusy(true);
    const flags: TorrentFlag[] = skipContent ? ['DEFAULT_DONT_DOWNLOAD'] : [];
    const common: Omit<AddTorrentInput, 'magnetUri' | 'torrentData'> = {
      savePath: path,
      paused: paused || undefined,
      sequentialDownload: sequential || undefined,
      flags: flags.length > 0 ? flags : undefined,
      trackers: listOf(trackers),
      urlSeeds: listOf(urlSeeds),
      uploadLimit: parseRateKiB(upLimit) ?? undefined,
      downloadLimit: parseRateKiB(downLimit) ?? undefined,
    };

    let added = 0;
    const failures: string[] = [];
    const addedMagnets = new Set<string>();
    const addedFiles = new Set<File>();
    for (const magnetUri of magnetList) {
      try {
        await mutations.addTorrent({
          ...common,
          magnetUri,
          name: rename.trim() === '' ? undefined : rename.trim(),
        });
        added++;
        addedMagnets.add(magnetUri);
      } catch (err) {
        failures.push(t('add.failed', { name: shorten(magnetUri), message: messageOf(err) }));
      }
    }
    for (const file of files) {
      try {
        if (file.size > MAX_TORRENT_FILE_BYTES) {
          throw new Error(t('add.fileTooLarge', { name: file.name }));
        }
        const torrentData = arrayBufferToBase64(await file.arrayBuffer());
        await mutations.addTorrent({ ...common, torrentData });
        added++;
        addedFiles.add(file);
      } catch (err) {
        failures.push(t('add.failed', { name: file.name, message: messageOf(err) }));
      }
    }

    if (added > 0) {
      addSavePath(path);
      toast.success(t('add.added', { count: added }));
    }
    for (const failure of failures) toast.error(failure);
    if (invocation.current !== token) return;
    setBusy(false);
    if (failures.length === 0 && seenInit.current === initAtSubmit) {
      setMagnets('');
      setFiles([]);
      close();
    } else {
      setMagnets((prev) =>
        prev
          .split('\n')
          .filter((line) => {
            const found = extractMagnets(line);
            return found.length === 0 || found.some((m) => !addedMagnets.has(m));
          })
          .join('\n'),
      );
      setFiles((prev) => prev.filter((f) => !addedFiles.has(f)));
    }
  };

  return (
    <Dialog open={open} onOpenChange={(o) => !o && close()}>
      <DialogContent wide>
        <DialogTitle>{t('add.title')}</DialogTitle>
        <DialogDescription className="mb-2" />

        <Label htmlFor="add-magnets">{t('add.magnetLabel')}</Label>
        <Textarea
          id="add-magnets"
          rows={3}
          value={magnets}
          onChange={(e) => setMagnets(e.target.value)}
          placeholder={t('add.magnetPlaceholder')}
          spellCheck={false}
          className="mb-3 font-mono text-xs"
        />

        <Label>{t('add.filesLabel')}</Label>
        <div className="mb-3">
          <input
            ref={fileInputRef}
            type="file"
            multiple
            accept=".torrent,application/x-bittorrent"
            className="hidden"
            onChange={(e) => {
              const list = e.target.files;
              if (list) setFiles((prev) => [...prev, ...Array.from(list)]);
              e.target.value = '';
            }}
          />
          <Button variant="outline" size="sm" onClick={() => fileInputRef.current?.click()}>
            {t('add.browse')}
          </Button>
          {files.length > 0 && (
            <ul className="mt-2 space-y-1 text-xs">
              {files.map((f, i) => (
                <li key={`${f.name}-${i}`} className="flex items-center gap-2">
                  <span className="truncate">{f.name}</span>
                  <button
                    type="button"
                    className="text-muted-foreground hover:text-destructive"
                    aria-label={t('common:actions.close')}
                    onClick={() => setFiles((prev) => prev.filter((_, j) => j !== i))}
                  >
                    ×
                  </button>
                </li>
              ))}
            </ul>
          )}
        </div>

        <Label htmlFor="add-savepath">{t('add.savePathLabel')}</Label>
        <Input
          id="add-savepath"
          list="add-savepath-list"
          value={savePath}
          onChange={(e) => setSavePath(e.target.value)}
          placeholder={t('add.savePathPlaceholder')}
          spellCheck={false}
          className="mb-3 font-mono text-xs"
        />
        <datalist id="add-savepath-list">
          {savePaths.map((p) => (
            <option key={p} value={p} />
          ))}
        </datalist>

        <div className="mb-3 grid grid-cols-1 gap-2 sm:grid-cols-3">
          <CheckboxField
            checked={paused}
            onCheckedChange={(v) => setPaused(v === true)}
            label={t('add.startPaused')}
          />
          <CheckboxField
            checked={sequential}
            onCheckedChange={(v) => setSequential(v === true)}
            label={t('add.sequential')}
          />
          <CheckboxField
            checked={skipContent}
            onCheckedChange={(v) => setSkipContent(v === true)}
            label={t('add.skipContent')}
          />
        </div>

        <button
          type="button"
          aria-expanded={showMore}
          aria-controls={showMore ? 'add-more-options' : undefined}
          className="mb-2 text-sm text-muted-foreground underline-offset-2 hover:underline"
          onClick={() => setShowMore((v) => !v)}
        >
          {t('add.moreOptions')}
        </button>
        {showMore && (
          <div id="add-more-options" className="mb-2 space-y-3">
            <div>
              <Label htmlFor="add-rename">{t('add.renameLabel')}</Label>
              <Input id="add-rename" value={rename} onChange={(e) => setRename(e.target.value)} />
            </div>
            <div className="grid grid-cols-2 gap-3">
              <div>
                <Label htmlFor="add-uplimit">{t('add.upLimitLabel')}</Label>
                <Input
                  id="add-uplimit"
                  type="number"
                  value={upLimit}
                  onChange={(e) => setUpLimit(e.target.value)}
                  aria-invalid={upLimitInvalid || undefined}
                />
                {upLimitInvalid && (
                  <p className="mt-1 text-xs text-st-error">{t('limitsDialog.invalidRate')}</p>
                )}
              </div>
              <div>
                <Label htmlFor="add-downlimit">{t('add.downLimitLabel')}</Label>
                <Input
                  id="add-downlimit"
                  type="number"
                  value={downLimit}
                  onChange={(e) => setDownLimit(e.target.value)}
                  aria-invalid={downLimitInvalid || undefined}
                />
                {downLimitInvalid && (
                  <p className="mt-1 text-xs text-st-error">{t('limitsDialog.invalidRate')}</p>
                )}
              </div>
            </div>
            <div>
              <Label htmlFor="add-trackers">{t('add.trackersLabel')}</Label>
              <Textarea
                id="add-trackers"
                rows={2}
                value={trackers}
                onChange={(e) => setTrackers(e.target.value)}
                spellCheck={false}
                className="font-mono text-xs"
              />
            </div>
            <div>
              <Label htmlFor="add-urlseeds">{t('add.urlSeedsLabel')}</Label>
              <Textarea
                id="add-urlseeds"
                rows={2}
                value={urlSeeds}
                onChange={(e) => setUrlSeeds(e.target.value)}
                spellCheck={false}
                className="font-mono text-xs"
              />
            </div>
          </div>
        )}

        <DialogFooter>
          <Button variant="outline" onClick={close}>
            {t('common:actions.cancel')}
          </Button>
          <Button
            disabled={busy || upLimitInvalid || downLimitInvalid}
            onClick={() => void submit()}
          >
            {busy ? t('add.submitBusy') : t('add.submit')}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function listOf(text: string): string[] | undefined {
  const items = text
    .split('\n')
    .map((l) => l.trim())
    .filter((l) => l !== '');
  return items.length > 0 ? items : undefined;
}

function shorten(uri: string): string {
  return uri.length > 40 ? `${uri.slice(0, 40)}…` : uri;
}

function messageOf(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}
