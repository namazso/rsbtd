// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

import uPlot, { type AlignedData } from 'uplot';
import 'uplot/dist/uPlot.min.css';
import { useEffect, useRef, useState } from 'react';
import { usePrefs } from '@/store/prefs';

export interface ChartSeries {
  name: string;
  label: string;
  color: string;
}

/**
 * Thin imperative uPlot wrapper: created on mount (and theme change for
 * axis colors), resized via ResizeObserver, fed with setData — React never
 * re-renders per sample.
 */
export function UPlotChart({
  series,
  data,
  formatValue,
  height = 180,
}: {
  series: ChartSeries[];
  data: AlignedData;
  formatValue: (value: number | null) => string;
  height?: number;
}) {
  const containerRef = useRef<HTMLDivElement>(null);
  const plotRef = useRef<uPlot | null>(null);
  const theme = usePrefs((s) => s.theme);
  const [systemDark, setSystemDark] = useState(
    () => window.matchMedia('(prefers-color-scheme: dark)').matches,
  );
  const dark = theme === 'dark' || (theme === 'system' && systemDark);
  const seriesKey = series.map((s) => s.name).join(',');

  useEffect(() => {
    const mql = window.matchMedia('(prefers-color-scheme: dark)');
    const onChange = () => setSystemDark(mql.matches);
    mql.addEventListener('change', onChange);
    return () => mql.removeEventListener('change', onChange);
  }, []);

  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;
    const styles = getComputedStyle(container);
    const axisColor = styles.getPropertyValue('--muted-foreground').trim();
    const gridColor = styles.getPropertyValue('--border').trim();

    const plot = new uPlot(
      {
        width: container.clientWidth || 300,
        height,
        legend: { show: true },
        cursor: { points: { size: 5 } },
        series: [
          {},
          ...series.map((s) => ({
            label: s.label,
            stroke: s.color,
            width: 1.5,
            spanGaps: false,
            value: (_u: uPlot, v: number | null) => formatValue(v),
          })),
        ],
        axes: [
          { stroke: axisColor, grid: { stroke: gridColor, width: 1 }, ticks: { show: false } },
          {
            stroke: axisColor,
            grid: { stroke: gridColor, width: 1 },
            ticks: { show: false },
            size: 60,
            values: (_u: uPlot, ticks: number[]) => ticks.map((v) => formatValue(v)),
          },
        ],
      },
      data,
      container,
    );
    plotRef.current = plot;

    const observer = new ResizeObserver(() => {
      plot.setSize({ width: container.clientWidth || 300, height });
    });
    observer.observe(container);
    return () => {
      observer.disconnect();
      plot.destroy();
      plotRef.current = null;
    };
    // Recreate on series set or resolved-theme change (axis colors are baked in).
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [seriesKey, dark, height]);

  useEffect(() => {
    plotRef.current?.setData(data);
  }, [data]);

  return <div ref={containerRef} className="w-full" />;
}
