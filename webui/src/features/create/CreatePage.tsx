// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

import { useQuery, useQueryClient } from '@tanstack/react-query';
import { ChevronLeft, Download, Plus, X } from 'lucide-react';
import { useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useNavigate } from 'react-router';
import { toast } from 'sonner';
import { request } from '@/api/client';
import {
  CancelCreateJobMutation,
  CreateJobProgressSubscription,
  CreateJobTorrentDataQuery,
  CreateJobsQuery,
  StartCreateTorrentMutation,
} from '@/api/operations/create';
import { subscribeRetrying } from '@/api/ws';
import { BottomNav } from '@/components/BottomNav';
import { Button } from '@/components/ui/button';
import { CheckboxField } from '@/components/ui/checkbox';
import { Input, Label, NativeSelect, Textarea } from '@/components/ui/input';
import { cn } from '@/lib/cn';
import { downloadBase64 } from '@/lib/download';
import { formatBytes } from '@/lib/format';
import { tDynamic } from '@/lib/i18nDynamic';
import { useIsMobile } from '@/lib/platform';
import { connection, useConnection } from '@/store/connection';
import { useUi } from '@/store/ui';
import type { CreateFlag, CreateJobFieldsFragment } from '@/gen/gql/graphql';

type Job = CreateJobFieldsFragment;

/** Piece sizes: powers of two, 16 KiB … 128 MiB. */
const PIECE_SIZES = Array.from({ length: 14 }, (_, i) => 16 * 1024 * 2 ** i);
const OPTION_FLAGS: CreateFlag[] = [
  'MODIFICATION_TIME',
  'SYMLINKS',
  'CANONICAL_FILES',
  'CANONICAL_FILES_NO_TAIL_PADDING',
  'NO_ATTRIBUTES',
];
const TERMINAL_STATES = new Set(['FINISHED', 'FAILED', 'CANCELLED']);

export default function CreatePage() {
  const { t } = useTranslation(['create', 'common']);
  const navigate = useNavigate();
  const isMobile = useIsMobile();

  return (
    <div className="flex h-dvh flex-col">
      <header className="flex h-[calc(3rem+env(safe-area-inset-top))] shrink-0 items-center gap-2 border-b border-border px-2 pt-[env(safe-area-inset-top)]">
        <Button
          variant="ghost"
          size="icon"
          aria-label={t('back')}
          onClick={() => void navigate('/')}
        >
          <ChevronLeft />
        </Button>
        <h1 className="text-base font-semibold">{t('title')}</h1>
      </header>
      <div className="min-h-0 flex-1 overflow-y-auto">
        <div className="mx-auto grid max-w-5xl gap-8 p-4 lg:grid-cols-2">
          <CreateForm />
          <JobsList />
        </div>
      </div>
      {isMobile && <BottomNav />}
    </div>
  );
}

function CreateForm() {
  const { t } = useTranslation(['create', 'common']);
  const queryClient = useQueryClient();
  const generation = useConnection((s) => s.generation);
  const [sourcePath, setSourcePath] = useState('');
  const [pieceSize, setPieceSize] = useState('auto');
  const [format, setFormat] = useState<'hybrid' | 'v1' | 'v2'>('hybrid');
  const [flags, setFlags] = useState<Set<CreateFlag>>(new Set());
  const [isPrivate, setIsPrivate] = useState(false);
  const [comment, setComment] = useState('');
  const [creator, setCreator] = useState('');
  const [trackers, setTrackers] = useState('');
  const [urlSeeds, setUrlSeeds] = useState('');
  const [outputPath, setOutputPath] = useState('');
  const [busy, setBusy] = useState(false);

  const submit = async () => {
    if (sourcePath.trim() === '') {
      toast.error(t('form.sourceRequired'));
      return;
    }
    const allFlags: CreateFlag[] = [...flags];
    if (format === 'v1') allFlags.push('V1_ONLY');
    if (format === 'v2') allFlags.push('V2_ONLY');

    const trackerInputs = trackers
      .split('\n')
      .map((l) => l.trim())
      .filter((l) => l !== '')
      .map((line) => {
        const match = /^(\d+)\s+(.+)$/.exec(line);
        return match ? { tier: Number(match[1]), url: match[2]! } : { tier: 0, url: line };
      });
    const seeds = urlSeeds
      .split('\n')
      .map((l) => l.trim())
      .filter((l) => l !== '');

    setBusy(true);
    try {
      const { startCreateTorrent: job } = await request(StartCreateTorrentMutation, {
        input: {
          sourcePath: sourcePath.trim(),
          pieceSize: pieceSize === 'auto' ? undefined : Number(pieceSize),
          flags: allFlags.length > 0 ? allFlags : undefined,
          trackers: trackerInputs.length > 0 ? trackerInputs : undefined,
          urlSeeds: seeds.length > 0 ? seeds : undefined,
          comment: comment.trim() === '' ? undefined : comment.trim(),
          creator: creator.trim() === '' ? undefined : creator.trim(),
          private: isPrivate,
          outputPath: outputPath.trim() === '' ? undefined : outputPath.trim(),
        },
      });
      // The jobs query neither polls nor subscribes while it only sees
      // terminal jobs; insert the new job so it renders (and gets its
      // progress subscription) without waiting for a refetch.
      queryClient.setQueryData<{ createJobs: Job[] }>(['createJobs', generation], (prev) => ({
        createJobs: [...(prev?.createJobs ?? []).filter((j) => j.id !== job.id), job],
      }));
    } catch (err) {
      toast.error(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  };

  return (
    <section>
      <div className="space-y-3">
        <div>
          <Label htmlFor="create-source">{t('form.sourcePath')}</Label>
          <Input
            id="create-source"
            value={sourcePath}
            onChange={(e) => setSourcePath(e.target.value)}
            spellCheck={false}
            className="font-mono text-xs"
          />
        </div>

        <div className="flex flex-wrap gap-4">
          <div>
            <Label htmlFor="create-piece">{t('form.pieceSize')}</Label>
            <NativeSelect
              id="create-piece"
              value={pieceSize}
              onChange={(e) => setPieceSize(e.target.value)}
              className="w-40"
            >
              <option value="auto">{t('form.pieceAuto')}</option>
              {PIECE_SIZES.map((size) => (
                <option key={size} value={String(size)}>
                  {formatBytes(size)}
                </option>
              ))}
            </NativeSelect>
          </div>
          <div>
            <Label htmlFor="create-format">{t('form.format')}</Label>
            <NativeSelect
              id="create-format"
              value={format}
              onChange={(e) => setFormat(e.target.value as 'hybrid' | 'v1' | 'v2')}
              className="w-40"
            >
              <option value="hybrid">{t('form.hybrid')}</option>
              <option value="v1">{t('form.v1Only')}</option>
              <option value="v2">{t('form.v2Only')}</option>
            </NativeSelect>
          </div>
        </div>

        <div>
          <Label>{t('form.flags')}</Label>
          <div className="grid gap-1.5 sm:grid-cols-2">
            <CheckboxField
              checked={isPrivate}
              onCheckedChange={(v) => setIsPrivate(v === true)}
              label={t('form.private')}
            />
            {OPTION_FLAGS.map((flag) => (
              <CheckboxField
                key={flag}
                checked={flags.has(flag)}
                onCheckedChange={(v) =>
                  setFlags((prev) => {
                    const next = new Set(prev);
                    if (v === true) next.add(flag);
                    else next.delete(flag);
                    return next;
                  })
                }
                label={tDynamic(`create:form.flag.${flag}`)}
              />
            ))}
          </div>
        </div>

        <div className="grid gap-3 sm:grid-cols-2">
          <div>
            <Label htmlFor="create-comment">{t('form.comment')}</Label>
            <Input
              id="create-comment"
              value={comment}
              onChange={(e) => setComment(e.target.value)}
            />
          </div>
          <div>
            <Label htmlFor="create-creator">{t('form.creator')}</Label>
            <Input
              id="create-creator"
              value={creator}
              onChange={(e) => setCreator(e.target.value)}
            />
          </div>
        </div>

        <div>
          <Label htmlFor="create-trackers">{t('form.trackers')}</Label>
          <Textarea
            id="create-trackers"
            rows={3}
            value={trackers}
            onChange={(e) => setTrackers(e.target.value)}
            spellCheck={false}
            className="font-mono text-xs"
          />
        </div>
        <div>
          <Label htmlFor="create-seeds">{t('form.urlSeeds')}</Label>
          <Textarea
            id="create-seeds"
            rows={2}
            value={urlSeeds}
            onChange={(e) => setUrlSeeds(e.target.value)}
            spellCheck={false}
            className="font-mono text-xs"
          />
        </div>
        <div>
          <Label htmlFor="create-output">{t('form.outputPath')}</Label>
          <Input
            id="create-output"
            value={outputPath}
            onChange={(e) => setOutputPath(e.target.value)}
            spellCheck={false}
            className="font-mono text-xs"
          />
        </div>

        <Button disabled={busy} onClick={() => void submit()}>
          <Plus />
          {t('form.submit')}
        </Button>
      </div>
    </section>
  );
}

function JobsList() {
  const { t } = useTranslation(['create', 'common']);
  const state = useConnection((s) => s.state);
  const generation = useConnection((s) => s.generation);
  const [live, setLive] = useState<Record<number, Job>>({});
  const subscribed = useRef(new Map<number, () => void>());

  const query = useQuery({
    queryKey: ['createJobs', generation],
    queryFn: () => request(CreateJobsQuery),
    enabled: state === 'up' || state === 'degraded',
    // Terminal jobs are static (and retained for about an hour); only
    // poll while something is still running.
    refetchInterval: (q) =>
      (q.state.data?.createJobs ?? []).some((job) => !TERMINAL_STATES.has(job.state))
        ? 3_000
        : false,
    select: (d) => d.createJobs,
  });

  // A live snapshot only overrides a job the poll still considers
  // running: once the poll reports a terminal state it is at least as
  // fresh as the stream (which may have retried away mid-run and left a
  // stale non-terminal snapshot behind).
  const jobs = (query.data ?? [])
    .map((job) => (TERMINAL_STATES.has(job.state) ? job : (live[job.id] ?? job)))
    .sort((a, b) => b.id - a.id);

  // Drop all subscriptions on unmount/reconnect; the effect below
  // re-subscribes on the current client.
  useEffect(() => {
    const subs = subscribed.current;
    return () => {
      for (const dispose of subs.values()) dispose();
      subs.clear();
    };
  }, [generation]);

  // Live progress for active jobs (stream completes at the terminal state).
  useEffect(() => {
    const client = connection.client;
    if (!client) return;
    for (const job of query.data ?? []) {
      if (TERMINAL_STATES.has(job.state) || subscribed.current.has(job.id)) continue;
      const dispose = subscribeRetrying(
        client,
        CreateJobProgressSubscription,
        { id: job.id },
        {
          next: (data) => {
            const snapshot = data.createJobProgress;
            setLive((prev) => ({ ...prev, [snapshot.id]: snapshot }));
            if (TERMINAL_STATES.has(snapshot.state)) {
              dispose();
              void query.refetch();
            }
          },
        },
      );
      subscribed.current.set(job.id, dispose);
    }
  }, [query.data, query]);

  return (
    <section>
      <h2 className="mb-2 text-xs font-semibold text-muted-foreground uppercase">
        {t('jobs.title')}
      </h2>
      {jobs.length === 0 && <p className="text-sm text-muted-foreground">{t('jobs.empty')}</p>}
      <div className="space-y-3">
        {jobs.map((job) => (
          <JobCard key={job.id} job={job} onChanged={() => void query.refetch()} />
        ))}
      </div>
    </section>
  );
}

function JobCard({ job, onChanged }: { job: Job; onChanged: () => void }) {
  const { t } = useTranslation(['create', 'common']);
  const openAddDialog = useUi((s) => s.openAddDialog);
  // The payload is fetched on demand: polling it with every snapshot
  // would ship the whole .torrent over and over.
  const withTorrentData = (use: (base64: string) => void) => {
    request(CreateJobTorrentDataQuery, { id: job.id })
      .then((data) => {
        const base64 = data.createJob?.torrentData;
        if (base64 == null) {
          toast.error(t('jobs.dataGone'));
          onChanged();
          return;
        }
        use(base64);
      })
      .catch((err: unknown) => toast.error(err instanceof Error ? err.message : String(err)));
  };
  const pct =
    job.piecesTotal > 0
      ? Math.round((job.piecesDone / job.piecesTotal) * 100)
      : job.state === 'FINISHED'
        ? 100
        : 0;
  const terminal = TERMINAL_STATES.has(job.state);

  return (
    <div className="rounded-lg border border-border p-3">
      <div className="mb-1 flex items-center gap-2">
        <span className="text-sm font-medium">{t('jobs.job', { id: job.id })}</span>
        <span
          className={cn(
            'rounded-full px-2 py-0.5 text-[11px] font-medium',
            job.state === 'FINISHED' && 'bg-st-seed/15 text-st-seed',
            job.state === 'FAILED' && 'bg-st-error/15 text-st-error',
            job.state === 'CANCELLED' && 'bg-muted text-muted-foreground',
            !terminal && 'bg-st-download/15 text-st-download',
          )}
        >
          {tDynamic(`create:jobs.state.${job.state}`)}
        </span>
        <span className="ml-auto text-xs text-muted-foreground tabular-nums">
          {t('jobs.pieces', { done: job.piecesDone, total: job.piecesTotal })}
        </span>
      </div>
      <div className="relative h-2 overflow-hidden rounded-full bg-muted">
        <div
          className={cn(
            'absolute inset-y-0 left-0 transition-[width]',
            job.state === 'FAILED' ? 'bg-st-error' : 'bg-st-download',
          )}
          style={{ width: `${pct}%` }}
        />
      </div>
      {job.error != null && (
        <p role="alert" className="mt-1 text-xs text-st-error">
          {job.error}
        </p>
      )}
      {job.outputPath != null && job.state === 'FINISHED' && (
        <p className="mt-1 truncate font-mono text-xs text-muted-foreground">
          {t('jobs.writtenTo', { path: job.outputPath })}
        </p>
      )}
      <div className="mt-2 flex flex-wrap gap-1.5">
        {!terminal && (
          <Button
            size="sm"
            variant="outline"
            title={t('jobs.cancelHint')}
            onClick={() => {
              void request(CancelCreateJobMutation, { id: job.id })
                .then(onChanged)
                .catch((err: unknown) =>
                  toast.error(err instanceof Error ? err.message : String(err)),
                );
            }}
          >
            <X />
            {t('jobs.cancel')}
          </Button>
        )}
        {job.state === 'FINISHED' && job.hasTorrentData && (
          <>
            <Button
              size="sm"
              variant="outline"
              onClick={() =>
                withTorrentData((base64) => downloadBase64(`rsbtd-job-${job.id}.torrent`, base64))
              }
            >
              <Download />
              {t('jobs.download')}
            </Button>
            <Button
              size="sm"
              variant="outline"
              onClick={() =>
                withTorrentData((base64) => {
                  const bytes = Uint8Array.from(atob(base64), (c) => c.charCodeAt(0));
                  const file = new File(
                    [bytes.buffer as ArrayBuffer],
                    `rsbtd-job-${job.id}.torrent`,
                    {
                      type: 'application/x-bittorrent',
                    },
                  );
                  openAddDialog({ files: [file] });
                })
              }
            >
              <Plus />
              {t('jobs.addToSession')}
            </Button>
          </>
        )}
      </div>
    </div>
  );
}
