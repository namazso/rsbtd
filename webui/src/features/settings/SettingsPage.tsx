// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { ChevronLeft, Search } from 'lucide-react';
import { useEffect, useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import i18next from 'i18next';
import { useNavigate, useParams } from 'react-router';
import { toast } from 'sonner';
import { request } from '@/api/client';
import { clearToken } from '@/api/auth';
import { httpEndpoint } from '@/api/endpoint';
import { mutations } from '@/api/mutations';
import {
  AllSettingsQuery,
  ApplySettingsMutation,
  ReopenNetworkSocketsMutation,
} from '@/api/operations/settings';
import { BottomNav } from '@/components/BottomNav';
import { Button } from '@/components/ui/button';
import { CheckboxField } from '@/components/ui/checkbox';
import { Input, NativeSelect } from '@/components/ui/input';
import { cn } from '@/lib/cn';
import { useIsMobile } from '@/lib/platform';
import { tDynamic } from '@/lib/i18nDynamic';
import { connection, useConnection } from '@/store/connection';
import { usePrefs, type ThemePref } from '@/store/prefs';
import { useInvalidateSession, useSession, useVersionInfo } from '@/features/statusbar/useSession';
import type { SettingsInput } from '@/gen/gql/graphql';
import { CATALOG_BY_NAME, buildDelta, useSettingsDraft, type SettingsSnapshot } from './draft';
import { SettingRow, settingHelp, settingLabel } from './editors';
import { ADVANCED_FIELDS, CURATED_SECTIONS, SECTION_IDS } from './manifest';
import { validateDelta } from './validate';

type SectionId = (typeof SECTION_IDS)[number];

/**
 * Settings editor: curated sections + auto-generated Advanced (100% of the
 * 191 fields), settings-wide search, delta apply with client pre-validation
 * and atomic server rejection (draft preserved).
 */
export default function SettingsPage() {
  const { t } = useTranslation(['settings', 'common']);
  const navigate = useNavigate();
  const isMobile = useIsMobile();
  const params = useParams();
  const section = (
    SECTION_IDS.includes(params.section as SectionId) ? params.section : 'speed'
  ) as SectionId;
  const [search, setSearch] = useState('');
  const generation = useConnection((s) => s.generation);

  const endpoint = httpEndpoint();
  const ensureScope = useSettingsDraft((s) => s.ensureScope);
  useEffect(() => ensureScope(endpoint), [ensureScope, endpoint]);

  const query = useQuery({
    queryKey: ['settings', generation],
    queryFn: () => request(AllSettingsQuery),
    staleTime: Infinity,
    select: (d) => d.settings,
  });

  const snapshot = query.data;

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
        <div className="relative ml-auto w-full max-w-xs">
          <Search className="pointer-events-none absolute top-1/2 left-2 size-3.5 -translate-y-1/2 text-muted-foreground" />
          <Input
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            placeholder={t('search')}
            className="pl-7"
          />
        </div>
      </header>

      {snapshot === undefined ? (
        <p className="p-6 text-sm text-muted-foreground">
          {query.isError
            ? t('loadFailed', { message: query.error instanceof Error ? query.error.message : '' })
            : t('loading')}
        </p>
      ) : (
        <div className={cn('flex min-h-0 flex-1', isMobile && 'flex-col')}>
          <SectionNav section={section} isMobile={isMobile} />
          <div className="flex min-h-0 flex-1 flex-col">
            <div className="min-h-0 flex-1 overflow-y-auto px-4">
              {search.trim() !== '' ? (
                <SearchResults snapshot={snapshot} needle={search.trim().toLowerCase()} />
              ) : section === 'session' ? (
                <SessionSection />
              ) : section === 'advanced' ? (
                <FieldList
                  snapshot={snapshot}
                  fields={ADVANCED_FIELDS}
                  intro={tDynamic('settings:intro.advanced')}
                />
              ) : (
                <FieldList
                  snapshot={snapshot}
                  fields={CURATED_SECTIONS.find((s) => s.id === section)?.fields ?? []}
                  intro={
                    i18next.exists(`settings:intro.${section}`)
                      ? tDynamic(`settings:intro.${section}`)
                      : undefined
                  }
                />
              )}
            </div>
            <ApplyBar snapshot={snapshot} generation={generation} />
          </div>
        </div>
      )}
      {isMobile && <BottomNav />}
    </div>
  );
}

function SectionNav({ section, isMobile }: { section: SectionId; isMobile: boolean }) {
  const navigate = useNavigate();
  if (isMobile) {
    return (
      <nav className="scrollbar-none flex shrink-0 gap-1.5 overflow-x-auto border-b border-border px-3 py-2">
        {SECTION_IDS.map((id) => (
          <button
            key={id}
            type="button"
            onClick={() => void navigate(`/settings/${id}`, { replace: true })}
            className={cn(
              'shrink-0 rounded-full border border-border px-2.5 py-1 text-xs whitespace-nowrap',
              section === id
                ? 'border-primary bg-primary text-primary-foreground'
                : 'text-muted-foreground',
            )}
          >
            {tDynamic(`settings:sections.${id}`)}
          </button>
        ))}
      </nav>
    );
  }
  return (
    <nav className="w-48 shrink-0 overflow-y-auto border-r border-border py-2">
      {SECTION_IDS.map((id) => (
        <button
          key={id}
          type="button"
          aria-current={section === id}
          onClick={() => void navigate(`/settings/${id}`, { replace: true })}
          className={cn(
            'block w-full px-4 py-1.5 text-left text-sm hover:bg-accent',
            section === id && 'bg-selected font-medium',
          )}
        >
          {tDynamic(`settings:sections.${id}`)}
        </button>
      ))}
    </nav>
  );
}

function FieldList({
  snapshot,
  fields,
  intro,
}: {
  snapshot: SettingsSnapshot;
  fields: string[];
  intro?: string;
}) {
  return (
    <div className="mx-auto max-w-3xl pb-6">
      {intro !== undefined && (
        <p className="mt-3 rounded-md border border-border bg-muted/40 px-3 py-2 text-xs leading-relaxed whitespace-pre-line text-muted-foreground">
          {intro}
        </p>
      )}
      {fields.map((name) => (
        <SettingRow key={name} snapshot={snapshot} name={name} />
      ))}
    </div>
  );
}

function SearchResults({ snapshot, needle }: { snapshot: SettingsSnapshot; needle: string }) {
  const { t } = useTranslation('settings');
  const matches = useMemo(() => {
    const out: string[] = [];
    for (const [name, entry] of CATALOG_BY_NAME) {
      if (
        name.toLowerCase().includes(needle) ||
        settingLabel(name).toLowerCase().includes(needle) ||
        settingHelp(name).toLowerCase().includes(needle) ||
        entry.description.toLowerCase().includes(needle)
      ) {
        out.push(name);
      }
      if (out.length >= 60) break;
    }
    return out;
  }, [needle]);

  return (
    <div className="mx-auto max-w-3xl pb-6">
      <p className="pt-3 text-xs font-semibold text-muted-foreground uppercase">
        {t('searchResults')}
      </p>
      <FieldList snapshot={snapshot} fields={matches} />
    </div>
  );
}

function ApplyBar({ snapshot, generation }: { snapshot: SettingsSnapshot; generation: number }) {
  const { t } = useTranslation(['settings', 'common']);
  const queryClient = useQueryClient();
  const draft = useSettingsDraft((s) => s.draft);
  const reset = useSettingsDraft((s) => s.reset);
  const pruneApplied = useSettingsDraft((s) => s.pruneApplied);
  const [serverError, setServerError] = useState<string | null>(null);

  const delta = useMemo(() => buildDelta(snapshot, draft), [snapshot, draft]);
  const issues = useMemo(() => validateDelta(delta), [delta]);
  const count = Object.keys(delta).length;

  const mutation = useMutation({
    mutationFn: (input: SettingsInput) => request(ApplySettingsMutation, { input }),
    onSuccess: (data, variables) => {
      queryClient.setQueryData(['settings', generation], { settings: data.applySettings });
      pruneApplied(variables as Record<string, unknown>);
      setServerError(null);
      toast.success(t('apply.applied'));
    },
    onError: (error) => {
      // Atomic rejection: keep the draft so nothing is lost.
      setServerError(error instanceof Error ? error.message : String(error));
    },
  });

  if (count === 0 && serverError === null) return null;

  return (
    <div className="shrink-0 border-t border-border bg-card px-4 py-2">
      {serverError !== null && (
        <p role="alert" className="mb-1 text-xs text-st-error">
          {t('apply.rejected', { message: serverError })}
        </p>
      )}
      {issues.length > 0 && (
        <ul className="mb-1 text-xs text-st-error">
          {issues.slice(0, 4).map((issue, i) => (
            <li key={i}>
              {settingLabel(issue.field)}
              {': '}
              {issue.message}
            </li>
          ))}
        </ul>
      )}
      <div className="flex items-center gap-3">
        <span className="text-sm text-muted-foreground">{t('apply.changed', { count })}</span>
        <span className="ml-auto flex gap-2">
          <Button
            variant="outline"
            size="sm"
            onClick={() => {
              reset();
              setServerError(null);
            }}
          >
            {t('apply.reset')}
          </Button>
          <Button
            size="sm"
            disabled={count === 0 || issues.length > 0 || mutation.isPending}
            onClick={() => mutation.mutate(delta as SettingsInput)}
          >
            {mutation.isPending ? t('apply.applying') : t('apply.apply')}
          </Button>
        </span>
      </div>
    </div>
  );
}

function SessionSection() {
  const { t } = useTranslation(['settings', 'common']);
  const version = useVersionInfo();
  const session = useSession();
  const invalidateSession = useInvalidateSession();
  const theme = usePrefs((s) => s.theme);
  const setTheme = usePrefs((s) => s.setTheme);
  const [mapPorts, setMapPorts] = useState(true);

  return (
    <div className="mx-auto max-w-3xl space-y-6 py-4">
      <section>
        <h3 className="mb-2 text-xs font-semibold text-muted-foreground uppercase">
          {t('session.about')}
        </h3>
        <dl className="grid max-w-md grid-cols-[auto_1fr] gap-x-6 gap-y-1 text-sm">
          <dt className="text-muted-foreground">{t('session.daemonVersion')}</dt>
          <dd>{version.data?.daemon ?? '…'}</dd>
          <dt className="text-muted-foreground">{t('session.libtorrentVersion')}</dt>
          <dd>{version.data?.libtorrent ?? '…'}</dd>
          <dt className="text-muted-foreground">{t('session.listenPort')}</dt>
          <dd>{session.data?.listenPort ?? '…'}</dd>
          <dt className="text-muted-foreground">{t('session.dht')}</dt>
          <dd>
            {session.data
              ? t(session.data.isDhtRunning ? 'common:boolean.yes' : 'common:boolean.no')
              : '…'}
          </dd>
          <dt className="text-muted-foreground">{t('session.torrents')}</dt>
          <dd>{session.data?.torrentCount ?? '…'}</dd>
        </dl>
      </section>

      <section>
        <h3 className="mb-2 text-xs font-semibold text-muted-foreground uppercase">
          {t('session.connection')}
        </h3>
        <div className="flex flex-wrap items-center gap-2">
          {session.data && (
            <Button
              variant="outline"
              size="sm"
              onClick={() => {
                void (session.data.isPaused ? mutations.resumeSession() : mutations.pauseSession())
                  .then(invalidateSession)
                  .catch((err: unknown) =>
                    toast.error(err instanceof Error ? err.message : String(err)),
                  );
              }}
            >
              {session.data.isPaused ? t('session.resumeSession') : t('session.pauseSession')}
            </Button>
          )}
          <Button
            variant="outline"
            size="sm"
            onClick={() => {
              void request(ReopenNetworkSocketsMutation, { mapPorts })
                .then(() => toast.success(t('session.reopened')))
                .catch((err: unknown) =>
                  toast.error(err instanceof Error ? err.message : String(err)),
                );
            }}
          >
            {t('session.reopenSockets')}
          </Button>
          <CheckboxField
            checked={mapPorts}
            onCheckedChange={(v) => setMapPorts(v === true)}
            label={<span className="text-xs">{t('session.mapPorts')}</span>}
          />
        </div>
        <p className="mt-1 max-w-xl text-xs text-muted-foreground">{t('session.reopenHint')}</p>
      </section>

      <section>
        <h3 className="mb-2 text-xs font-semibold text-muted-foreground uppercase">
          {t('session.theme')}
        </h3>
        <NativeSelect
          value={theme}
          onChange={(e) => setTheme(e.target.value as ThemePref)}
          className="max-w-48"
        >
          <option value="system">{t('common:theme.system')}</option>
          <option value="light">{t('common:theme.light')}</option>
          <option value="dark">{t('common:theme.dark')}</option>
        </NativeSelect>
      </section>

      <section>
        <Button
          variant="outline"
          size="sm"
          onClick={() => {
            clearToken();
            connection.logout();
          }}
        >
          {t('common:actions.logout')}
        </Button>
      </section>
    </div>
  );
}
