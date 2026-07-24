// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

import { useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { toast } from 'sonner';
import { bulk, mutations } from '@/api/mutations';
import { Button } from '@/components/ui/button';
import { CheckboxField } from '@/components/ui/checkbox';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogTitle,
} from '@/components/ui/dialog';
import { Input, Label, NativeSelect } from '@/components/ui/input';
import { tDynamic } from '@/lib/i18nDynamic';
import { formatRateKiB, parseRateKiB } from '@/lib/rateLimit';
import { useTorrents } from '@/store/torrents';
import { useUi } from '@/store/ui';
import type { MoveMode } from '@/gen/gql/graphql';

export function RemoveTorrentDialog() {
  const { t } = useTranslation(['torrents', 'common']);
  const state = useUi((s) => s.removeDialog);
  const close = useUi((s) => s.closeDialogs);
  const [deleteFiles, setDeleteFiles] = useState(false);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (state !== null) {
      setDeleteFiles(false);
      setBusy(false);
    }
  }, [state]);

  const submit = async () => {
    if (state === null) return;
    setBusy(true);
    const { errors } = await bulk(state.hashes, (h) => mutations.remove(h, deleteFiles));
    for (const e of errors) toast.error(e.message);
    if (useUi.getState().removeDialog !== state) return;
    setBusy(false);
    close();
  };

  const count = state?.hashes.length ?? 0;
  return (
    <Dialog open={state !== null} onOpenChange={(o) => !o && close()}>
      <DialogContent role="alertdialog" onOpenAutoFocus={(e) => e.preventDefault()}>
        <DialogTitle>{t('removeDialog.title', { count })}</DialogTitle>
        <DialogDescription>{t('removeDialog.body')}</DialogDescription>
        <CheckboxField
          checked={deleteFiles}
          onCheckedChange={(v) => setDeleteFiles(v === true)}
          label={<span className="text-destructive">{t('removeDialog.deleteFiles')}</span>}
        />
        <DialogFooter>
          <Button autoFocus variant="outline" onClick={close}>
            {t('common:actions.cancel')}
          </Button>
          <Button variant="destructive" disabled={busy} onClick={() => void submit()}>
            {t('removeDialog.confirm')}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

const MOVE_MODES: MoveMode[] = [
  'ALWAYS_REPLACE_FILES',
  'FAIL_IF_EXIST',
  'DONT_REPLACE',
  'RESET_SAVE_PATH',
  'RESET_SAVE_PATH_UNCHECKED',
];

export function MoveStorageDialog() {
  const { t } = useTranslation(['torrents', 'common']);
  const state = useUi((s) => s.moveDialog);
  const close = useUi((s) => s.closeDialogs);
  const [path, setPath] = useState('');
  const [mode, setMode] = useState<MoveMode>('ALWAYS_REPLACE_FILES');
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (state !== null) {
      const first = useTorrents.getState().byHash.get(state.hashes[0] ?? '');
      setPath(first?.savePath ?? '');
      setMode('ALWAYS_REPLACE_FILES');
      setBusy(false);
    }
  }, [state]);

  const submit = async () => {
    if (state === null || path.trim() === '') return;
    setBusy(true);
    // Waits server-side for the storage_moved confirmation (up to 10 min);
    // per-torrent moves are serialized by the daemon.
    const { errors } = await bulk(state.hashes, async (h) => {
      const result = await mutations.moveStorage(h, path.trim(), mode);
      const name = useTorrents.getState().byHash.get(h)?.name ?? h.slice(0, 8);
      toast.success(t('moveDialog.moved', { name, path: result.moveStorage }));
    });
    for (const e of errors) toast.error(e.message);
    if (useUi.getState().moveDialog !== state) return;
    setBusy(false);
    close();
  };

  return (
    <Dialog open={state !== null} onOpenChange={(o) => !o && close()}>
      <DialogContent>
        <DialogTitle>{t('moveDialog.title')}</DialogTitle>
        <DialogDescription>{busy ? t('moveDialog.moving') : null}</DialogDescription>
        <Label htmlFor="move-path">{t('moveDialog.pathLabel')}</Label>
        <Input
          id="move-path"
          value={path}
          onChange={(e) => setPath(e.target.value)}
          spellCheck={false}
          className="mb-3 font-mono text-xs"
        />
        <Label htmlFor="move-mode">{t('moveDialog.modeLabel')}</Label>
        <NativeSelect
          id="move-mode"
          value={mode}
          onChange={(e) => setMode(e.target.value as MoveMode)}
        >
          {MOVE_MODES.map((m) => (
            <option key={m} value={m}>
              {tDynamic(`torrents:moveDialog.mode.${m}`)}
            </option>
          ))}
        </NativeSelect>
        <DialogFooter>
          <Button variant="outline" onClick={close}>
            {t('common:actions.cancel')}
          </Button>
          <Button disabled={busy || path.trim() === ''} onClick={() => void submit()}>
            {t('moveDialog.confirm')}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

export function TorrentLimitsDialog() {
  const { t } = useTranslation(['torrents', 'common']);
  const state = useUi((s) => s.limitsDialog);
  const close = useUi((s) => s.closeDialogs);
  const [up, setUp] = useState('');
  const [down, setDown] = useState('');
  const [maxUploads, setMaxUploads] = useState('');
  const [maxConnections, setMaxConnections] = useState('');
  const [busy, setBusy] = useState(false);
  const initial = useRef({ up: '', down: '', maxUploads: '', maxConnections: '' });

  useEffect(() => {
    if (state !== null) {
      const first = useTorrents.getState().byHash.get(state.hashes[0] ?? '');
      const countText = (v: number | undefined) => (v === undefined ? '' : String(v));
      initial.current =
        state.hashes.length === 1 && first
          ? {
              up: formatRateKiB(first.uploadLimit),
              down: formatRateKiB(first.downloadLimit),
              maxUploads: countText(first.uploadsLimit),
              maxConnections: countText(first.connectionsLimit),
            }
          : { up: '', down: '', maxUploads: '', maxConnections: '' };
      setUp(initial.current.up);
      setDown(initial.current.down);
      setMaxUploads(initial.current.maxUploads);
      setMaxConnections(initial.current.maxConnections);
      setBusy(false);
    }
  }, [state]);

  const parseCount = (text: string): number | undefined => {
    const trimmed = text.trim();
    if (trimmed === '') return undefined;
    const value = Number(trimmed);
    return Number.isFinite(value) ? Math.trunc(value) : undefined;
  };
  // Mirrors the daemon's domains (blank = leave unchanged): rates are -1
  // or positive; counts are -1 or 2..=16777214.
  const rateInvalid = (text: string): boolean => parseRateKiB(text) === null;
  const countInvalid = (text: string): boolean => {
    const trimmed = text.trim();
    if (trimmed === '') return false;
    const value = Number(trimmed);
    if (!Number.isInteger(value)) return true;
    return value !== -1 && (value < 2 || value > 16_777_214);
  };
  const invalid =
    rateInvalid(up) ||
    rateInvalid(down) ||
    countInvalid(maxUploads) ||
    countInvalid(maxConnections);

  const submit = async () => {
    if (state === null || invalid) return;
    setBusy(true);
    const untouched = (text: string, key: keyof typeof initial.current) =>
      text.trim() === initial.current[key];
    const limits = {
      uploadLimit: untouched(up, 'up') ? undefined : (parseRateKiB(up) ?? undefined),
      downloadLimit: untouched(down, 'down') ? undefined : (parseRateKiB(down) ?? undefined),
      maxUploads: untouched(maxUploads, 'maxUploads') ? undefined : parseCount(maxUploads),
      maxConnections: untouched(maxConnections, 'maxConnections')
        ? undefined
        : parseCount(maxConnections),
    };
    const { errors } = await bulk(state.hashes, (h) => mutations.setLimits(h, limits));
    if (useUi.getState().limitsDialog !== state) return;
    setBusy(false);
    if (errors.length > 0) {
      // Keep the dialog open so the rejected values can be corrected.
      for (const e of errors) toast.error(e.message);
      return;
    }
    close();
  };

  const fields = [
    {
      id: 'limit-up',
      label: t('limitsDialog.upLabel'),
      value: up,
      set: setUp,
      error: rateInvalid(up) ? t('limitsDialog.invalidRate') : null,
    },
    {
      id: 'limit-down',
      label: t('limitsDialog.downLabel'),
      value: down,
      set: setDown,
      error: rateInvalid(down) ? t('limitsDialog.invalidRate') : null,
    },
    {
      id: 'limit-uploads',
      label: t('limitsDialog.maxUploadsLabel'),
      value: maxUploads,
      set: setMaxUploads,
      error: countInvalid(maxUploads) ? t('limitsDialog.invalidCount') : null,
    },
    {
      id: 'limit-conns',
      label: t('limitsDialog.maxConnectionsLabel'),
      value: maxConnections,
      set: setMaxConnections,
      error: countInvalid(maxConnections) ? t('limitsDialog.invalidCount') : null,
    },
  ];

  return (
    <Dialog open={state !== null} onOpenChange={(o) => !o && close()}>
      <DialogContent>
        <DialogTitle>{t('limitsDialog.title')}</DialogTitle>
        <DialogDescription>{t('limitsDialog.hint')}</DialogDescription>
        <div className="grid grid-cols-2 gap-3">
          {fields.map((f) => (
            <div key={f.id}>
              <Label htmlFor={f.id}>{f.label}</Label>
              <Input
                id={f.id}
                type="number"
                value={f.value}
                onChange={(e) => f.set(e.target.value)}
                aria-invalid={f.error !== null}
              />
              {f.error !== null && <p className="mt-1 text-xs text-st-error">{f.error}</p>}
            </div>
          ))}
        </div>
        <DialogFooter>
          <Button variant="outline" onClick={close}>
            {t('common:actions.cancel')}
          </Button>
          <Button disabled={busy || invalid} onClick={() => void submit()}>
            {t('limitsDialog.confirm')}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
