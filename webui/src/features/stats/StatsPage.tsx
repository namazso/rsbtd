// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

import { useQuery } from '@tanstack/react-query';
import { ChevronLeft, Search } from 'lucide-react';
import { useEffect, useMemo, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useNavigate } from 'react-router';
import { request } from '@/api/client';
import { SessionStatsQuery, SessionStatsStreamSubscription } from '@/api/operations/stats';
import { subscribeRetrying } from '@/api/ws';
import { BottomNav } from '@/components/BottomNav';
import { Button } from '@/components/ui/button';
import { CheckboxField } from '@/components/ui/checkbox';
import { Input } from '@/components/ui/input';
import { formatNumber, formatRate } from '@/lib/format';
import { useIsMobile } from '@/lib/platform';
import { connection, useConnection } from '@/store/connection';
import { MetricSampler, type StatSample } from './sampler';
import { UPlotChart, type ChartSeries } from './UPlotChart';

/**
 * Stats dashboard: preset live charts over dynamically discovered metric
 * names (missing series are simply hidden — names vary by libtorrent
 * version) plus a metric browser feeding a custom chart.
 */
interface ChartDef {
  id: string;
  /** Candidate metric names; only those discovered are plotted. */
  candidates: string[];
  bytes: boolean;
}

const PRESET_CHARTS: ChartDef[] = [
  {
    id: 'transfer',
    candidates: [
      'net.recv_payload_bytes',
      'net.sent_payload_bytes',
      'net.recv_bytes',
      'net.sent_bytes',
    ],
    bytes: true,
  },
  {
    id: 'dht',
    candidates: ['dht.dht_nodes', 'dht.dht_node_cache', 'dht.dht_peers'],
    bytes: false,
  },
  {
    id: 'peers',
    candidates: [
      'peer.num_peers_connected',
      'peer.num_peers_half_open',
      'peer.num_peers_up_unchoked',
      'peer.connection_attempts',
    ],
    bytes: false,
  },
  {
    id: 'disk',
    candidates: [
      'disk.queued_write_bytes',
      'disk.num_blocks_written',
      'disk.num_blocks_read',
      'disk.num_write_ops',
      'disk.num_read_ops',
    ],
    bytes: false,
  },
];

const SERIES_COLORS = [
  'oklch(0.55 0.15 250)',
  'oklch(0.56 0.13 150)',
  'oklch(0.62 0.12 85)',
  'oklch(0.55 0.19 27)',
  'oklch(0.55 0.15 300)',
];

export default function StatsPage() {
  const { t } = useTranslation(['stats', 'common']);
  const navigate = useNavigate();
  const isMobile = useIsMobile();
  const state = useConnection((s) => s.state);
  const generation = useConnection((s) => s.generation);
  const [custom, setCustom] = useState<ReadonlySet<string>>(new Set());
  const [filter, setFilter] = useState('');
  const [tick, setTick] = useState(0);
  const samplerRef = useRef(new MetricSampler(600));

  // Discovery: all metrics once per connection generation.
  const discovery = useQuery({
    queryKey: ['sessionStats', generation],
    queryFn: () => request(SessionStatsQuery, {}),
    enabled: state === 'up' || state === 'degraded',
    staleTime: Infinity,
    select: (d) => d.sessionStats,
  });

  const known = useMemo(() => new Set((discovery.data ?? []).map((s) => s.name)), [discovery.data]);

  const charts = useMemo(
    () =>
      PRESET_CHARTS.map((chart) => ({
        ...chart,
        names: chart.candidates.filter((n) => known.has(n)),
      })),
    [known],
  );

  const activeNames = useMemo(() => {
    const names = new Set<string>();
    for (const chart of charts) for (const n of chart.names) names.add(n);
    for (const n of custom) names.add(n);
    return [...names];
  }, [charts, custom]);

  // One subscription for the union of plotted names, paused when hidden.
  // While the WebSocket is unavailable (degraded), the same names are
  // polled over HTTP so the charts keep moving instead of freezing.
  useEffect(() => {
    const client = connection.client;
    const sampler = samplerRef.current;
    const live = state === 'up' && client != null;
    if ((!live && state !== 'degraded') || activeNames.length === 0) return;
    let disposed = false;
    let dispose: (() => void) | null = null;

    const start = () => {
      if (live && client != null) {
        dispose = subscribeRetrying(
          client,
          SessionStatsStreamSubscription,
          { intervalMs: 1_000, names: activeNames },
          {
            next: (data) => {
              sampler.push(Date.now() / 1000, data.sessionStats as StatSample[]);
              setTick((v) => v + 1);
            },
          },
        );
      } else {
        // The full-list query has no name filter; select client-side.
        const wanted = new Set(activeNames);
        let inflight = false;
        const timer = setInterval(() => {
          if (inflight) return;
          inflight = true;
          request(SessionStatsQuery, {})
            .then((d) => {
              const samples = d.sessionStats.filter((s) => wanted.has(s.name));
              sampler.push(Date.now() / 1000, samples as StatSample[]);
              setTick((v) => v + 1);
            })
            .catch(() => {})
            .finally(() => {
              inflight = false;
            });
        }, 2_000);
        dispose = () => clearInterval(timer);
      }
    };
    const onVisibility = () => {
      if (document.hidden) {
        dispose?.();
        dispose = null;
        sampler.breakContinuity();
      } else if (!disposed && dispose === null) {
        start();
      }
    };
    if (!document.hidden) start();
    document.addEventListener('visibilitychange', onVisibility);
    return () => {
      disposed = true;
      document.removeEventListener('visibilitychange', onVisibility);
      dispose?.();
      sampler.breakContinuity();
    };
  }, [state, generation, activeNames]);

  const browserRows = useMemo(() => {
    const needle = filter.trim().toLowerCase();
    return (discovery.data ?? []).filter((s) => needle === '' || s.name.includes(needle));
  }, [discovery.data, filter]);

  const sampler = samplerRef.current;
  void tick; // charts below read the sampler on every tick render

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
        <div className="mx-auto max-w-5xl space-y-6 p-4">
          {discovery.isPending ? (
            <p className="text-sm text-muted-foreground">{t('loading')}</p>
          ) : discovery.isError ? (
            <p className="text-sm text-st-error">
              {t('loadError', { message: discovery.error.message })}
            </p>
          ) : (
            <>
              {charts.map((chart) => (
                <section key={chart.id}>
                  <h2 className="mb-1 text-xs font-semibold text-muted-foreground uppercase">
                    {t(`charts.${chart.id}` as 'charts.transfer')}
                  </h2>
                  {chart.names.length === 0 ? (
                    <p className="text-sm text-muted-foreground">{t('noSeries')}</p>
                  ) : (
                    <UPlotChart
                      series={chart.names.map((name, i): ChartSeries => ({
                        name,
                        label: name,
                        color: SERIES_COLORS[i % SERIES_COLORS.length]!,
                      }))}
                      data={sampler.aligned(chart.names)}
                      formatValue={(v) =>
                        v == null ? '' : chart.bytes ? formatRate(v) : formatNumber(v)
                      }
                    />
                  )}
                </section>
              ))}

              {custom.size > 0 && (
                <section>
                  <h2 className="mb-1 text-xs font-semibold text-muted-foreground uppercase">
                    {t('charts.custom')}
                  </h2>
                  <UPlotChart
                    series={[...custom].map((name, i): ChartSeries => ({
                      name,
                      label: name,
                      color: SERIES_COLORS[i % SERIES_COLORS.length]!,
                    }))}
                    data={sampler.aligned([...custom])}
                    formatValue={(v) => (v == null ? '' : formatNumber(v))}
                  />
                </section>
              )}

              <section>
                <h2 className="mb-1 text-xs font-semibold text-muted-foreground uppercase">
                  {t('browser.title')}
                </h2>
                <p className="mb-2 text-xs text-muted-foreground">{t('browser.hint')}</p>
                <div className="relative mb-2 max-w-sm">
                  <Search className="pointer-events-none absolute top-1/2 left-2 size-3.5 -translate-y-1/2 text-muted-foreground" />
                  <Input
                    value={filter}
                    onChange={(e) => setFilter(e.target.value)}
                    placeholder={t('browser.search')}
                    className="pl-7"
                  />
                </div>
                <div className="max-h-96 overflow-y-auto rounded-md border border-border">
                  {browserRows.map((row) => (
                    <div
                      key={row.name}
                      className="flex items-center gap-3 border-b border-border/40 px-3 py-1 text-[13px] last:border-b-0"
                    >
                      <CheckboxField
                        checked={custom.has(row.name)}
                        onCheckedChange={(v) =>
                          setCustom((prev) => {
                            const next = new Set(prev);
                            if (v === true) next.add(row.name);
                            else next.delete(row.name);
                            return next;
                          })
                        }
                        label={<span className="font-mono text-xs">{row.name}</span>}
                        className="min-w-0 flex-1"
                      />
                      <span className="shrink-0 rounded-full bg-muted px-1.5 text-[10px] text-muted-foreground">
                        {row.kind === 'COUNTER' ? t('browser.counter') : t('browser.gauge')}
                      </span>
                      <span className="w-28 shrink-0 text-right text-xs tabular-nums">
                        {formatNumber(row.value)}
                      </span>
                    </div>
                  ))}
                </div>
              </section>
            </>
          )}
        </div>
      </div>
      {isMobile && <BottomNav />}
    </div>
  );
}
