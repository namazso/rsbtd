// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

import type { FieldType } from '@/features/torrents/fields';

/**
 * Human-value parsing for the filter language's typed right-hand sides.
 *
 * Sizes/rates: `k m g t` (optionally with `b`) are decimal (1000ⁿ);
 * `kib mib gib tib` are binary (1024ⁿ); bare numbers are bytes. Rates may
 * carry a `/s` suffix. Percent accepts `50` or `50%`. Durations accept
 * `90`, `45s`, `5m`, `2h`, `1d`. Dates are local `YYYY-MM-DD[THH:MM[:SS]]`.
 */
const DECIMAL_UNITS: Record<string, number> = {
  k: 1e3,
  kb: 1e3,
  m: 1e6,
  mb: 1e6,
  g: 1e9,
  gb: 1e9,
  t: 1e12,
  tb: 1e12,
};
const BINARY_UNITS: Record<string, number> = {
  kib: 1024,
  mib: 1024 ** 2,
  gib: 1024 ** 3,
  tib: 1024 ** 4,
};
const DURATION_UNITS: Record<string, number> = { s: 1, m: 60, h: 3600, d: 86_400 };

export function parseSize(text: string): number | null {
  const match = /^([0-9]+(?:\.[0-9]+)?)\s*([a-z]*)$/i.exec(text.trim());
  if (!match) return null;
  const value = Number(match[1]);
  const unit = (match[2] ?? '').toLowerCase();
  if (unit === '' || unit === 'b') return value;
  const factor = BINARY_UNITS[unit] ?? DECIMAL_UNITS[unit];
  return factor === undefined ? null : value * factor;
}

export function parseRate(text: string): number | null {
  return parseSize(text.trim().replace(/\/s$/i, ''));
}

/** Returns parts-per-million. */
export function parsePercentPpm(text: string): number | null {
  const match = /^([0-9]+(?:\.[0-9]+)?)\s*%?$/.exec(text.trim());
  if (!match) return null;
  return Number(match[1]) * 10_000;
}

export function parseDurationSeconds(text: string): number | null {
  const match = /^([0-9]+(?:\.[0-9]+)?)\s*([smhd]?)$/i.exec(text.trim());
  if (!match) return null;
  const value = Number(match[1]);
  const unit = (match[2] ?? '').toLowerCase();
  return unit === '' ? value : value * (DURATION_UNITS[unit] ?? 1);
}

export interface DateValue {
  /** Epoch seconds of the parsed instant (local time). */
  startSec: number;
  /** Exclusive end for date-only equality (whole-day match). */
  endSec: number;
}

export function parseDate(text: string): DateValue | null {
  const match = /^(\d{4})-(\d{2})-(\d{2})(?:[T ](\d{2}):(\d{2})(?::(\d{2}))?)?$/.exec(text.trim());
  if (!match) return null;
  const [, y, mo, d, h, mi, s] = match;
  const hasTime = h !== undefined;
  const year = Number(y);
  const month = Number(mo);
  const day = Number(d);
  const hour = Number(h ?? 0);
  const minute = Number(mi ?? 0);
  const second = Number(s ?? 0);
  const date = new Date(year, month - 1, day, hour, minute, second);
  // Round-trip the given components to reject rolled-over inputs (Feb 31).
  if (date.getFullYear() !== year || date.getMonth() !== month - 1 || date.getDate() !== day) {
    return null;
  }
  if (hasTime && (date.getHours() !== hour || date.getMinutes() !== minute)) return null;
  if (s !== undefined && date.getSeconds() !== second) return null;
  const startSec = Math.floor(date.getTime() / 1000);
  const endSec = hasTime
    ? startSec + (s !== undefined ? 1 : 60)
    : Math.floor(new Date(year, month - 1, day + 1).getTime() / 1000);
  return { startSec, endSec };
}

export function parsePlainNumber(text: string): number | null {
  const trimmed = text.trim();
  if (trimmed === '' || !/^-?[0-9]+(\.[0-9]+)?$/.test(trimmed)) return null;
  return Number(trimmed);
}

/** Parse a rhs value for a field type; null = unparseable. */
export function parseTypedValue(type: FieldType, text: string): number | DateValue | null {
  switch (type) {
    case 'bytes':
      return parseSize(text);
    case 'rate':
      return parseRate(text);
    case 'percentPpm':
      return parsePercentPpm(text);
    case 'durationSecs':
    case 'etaSecs':
      return parseDurationSeconds(text);
    case 'date':
      return parseDate(text);
    case 'number':
    case 'float':
      return parsePlainNumber(text);
    default:
      return null;
  }
}
