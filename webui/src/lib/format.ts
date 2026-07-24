// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

import i18next from 'i18next';

/**
 * Locale-aware display formatters (torrent conventions: binary byte units).
 * All numeric formatting goes through Intl with the active i18next language.
 */
const BYTE_UNITS = ['B', 'KiB', 'MiB', 'GiB', 'TiB', 'PiB'] as const;

const nfCache = new Map<string, Intl.NumberFormat>();
function nf(digits: number): Intl.NumberFormat {
  const key = `${i18next.language}:${digits}`;
  let fmt = nfCache.get(key);
  if (!fmt) {
    fmt = new Intl.NumberFormat(i18next.language, {
      minimumFractionDigits: digits,
      maximumFractionDigits: digits,
    });
    nfCache.set(key, fmt);
  }
  return fmt;
}

export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes)) return i18next.t('placeholder.empty');
  const negative = bytes < 0;
  let value = Math.abs(bytes);
  let unit = 0;
  while (value >= 1024 && unit < BYTE_UNITS.length - 1) {
    value /= 1024;
    unit++;
  }
  const digits = unit === 0 ? 0 : value < 10 ? 2 : value < 100 ? 1 : 0;
  return `${nf(digits).format(negative ? -value : value)} ${BYTE_UNITS[unit]}`;
}

export function formatRate(bytesPerSecond: number): string {
  return `${formatBytes(bytesPerSecond)}/s`;
}

/** progressPpm (0..=1,000,000) as a percentage. */
export function formatPercentPpm(ppm: number): string {
  const key = `${i18next.language}:pct`;
  let fmt = nfCache.get(key);
  if (!fmt) {
    fmt = new Intl.NumberFormat(i18next.language, {
      style: 'percent',
      minimumFractionDigits: 0,
      maximumFractionDigits: 1,
    });
    nfCache.set(key, fmt);
  }
  return fmt.format(ppm / 1_000_000);
}

/** Compact two-component duration: "4d 3h", "3h 12m", "45s". */
export function formatDuration(seconds: number): string {
  if (!Number.isFinite(seconds) || seconds < 0) return i18next.t('placeholder.empty');
  const s = Math.round(seconds);
  const d = Math.floor(s / 86_400);
  const h = Math.floor((s % 86_400) / 3_600);
  const m = Math.floor((s % 3_600) / 60);
  const rest = s % 60;
  const u = (k: 'd' | 'h' | 'm' | 's') => i18next.t(`units.${k}`);
  if (d > 0) return `${d}${u('d')} ${h}${u('h')}`;
  if (h > 0) return `${h}${u('h')} ${m}${u('m')}`;
  if (m > 0) return `${m}${u('m')} ${rest}${u('s')}`;
  return `${rest}${u('s')}`;
}

/** ETA: null / non-finite / negative means "never at current rate". */
export function formatEta(seconds: number | null): string {
  if (seconds === null || !Number.isFinite(seconds) || seconds < 0) {
    return i18next.t('placeholder.infinity');
  }
  return formatDuration(seconds);
}

/** Unix seconds -> localized date+time; 0 means "never". */
export function formatDateTime(unixSeconds: number): string {
  if (unixSeconds <= 0) return i18next.t('placeholder.empty');
  const key = `${i18next.language}:dt`;
  let fmt = dtCache.get(key);
  if (!fmt) {
    fmt = new Intl.DateTimeFormat(i18next.language, { dateStyle: 'medium', timeStyle: 'short' });
    dtCache.set(key, fmt);
  }
  return fmt.format(new Date(unixSeconds * 1000));
}

const dtCache = new Map<string, Intl.DateTimeFormat>();

const RELATIVE_STEPS: [Intl.RelativeTimeFormatUnit, number][] = [
  ['year', 31_536_000],
  ['month', 2_592_000],
  ['day', 86_400],
  ['hour', 3_600],
  ['minute', 60],
  ['second', 1],
];

/** Unix seconds -> "5 minutes ago"; 0 means "never". */
export function formatRelativeTime(unixSeconds: number, nowMs = Date.now()): string {
  if (unixSeconds <= 0) return i18next.t('placeholder.empty');
  const diffSeconds = unixSeconds - Math.round(nowMs / 1000);
  const abs = Math.abs(diffSeconds);
  const key = `${i18next.language}:rel`;
  let fmt = rtCache.get(key);
  if (!fmt) {
    fmt = new Intl.RelativeTimeFormat(i18next.language, { numeric: 'auto' });
    rtCache.set(key, fmt);
  }
  for (const [unit, span] of RELATIVE_STEPS) {
    if (abs >= span || unit === 'second') {
      return fmt.format(Math.trunc(diffSeconds / span), unit);
    }
  }
  return fmt.format(diffSeconds, 'second');
}

const rtCache = new Map<string, Intl.RelativeTimeFormat>();

/** Plain localized integer/decimal. */
export function formatNumber(value: number, digits = 0): string {
  if (!Number.isFinite(value)) return i18next.t('placeholder.empty');
  return nf(digits).format(value);
}
