// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

import { describe, expect, it } from 'vitest';
import '@/i18n';
import { formatBytes, formatDuration, formatEta, formatPercentPpm, formatRate } from './format';

describe('formatBytes', () => {
  it('uses binary units with adaptive precision', () => {
    expect(formatBytes(0)).toBe('0 B');
    expect(formatBytes(1023)).toBe('1,023 B');
    expect(formatBytes(1024)).toBe('1.00 KiB');
    expect(formatBytes(1536)).toBe('1.50 KiB');
    expect(formatBytes(10 * 1024)).toBe('10.0 KiB');
    expect(formatBytes(100 * 1024)).toBe('100 KiB');
    expect(formatBytes(5 * 1024 ** 3)).toBe('5.00 GiB');
  });

  it('handles non-finite input', () => {
    expect(formatBytes(Number.NaN)).toBe('—');
  });
});

describe('formatRate', () => {
  it('appends /s', () => {
    expect(formatRate(2048)).toBe('2.00 KiB/s');
  });
});

describe('formatPercentPpm', () => {
  it('formats ppm as percent', () => {
    expect(formatPercentPpm(0)).toBe('0%');
    expect(formatPercentPpm(455_000)).toBe('45.5%');
    expect(formatPercentPpm(1_000_000)).toBe('100%');
  });
});

describe('formatDuration / formatEta', () => {
  it('renders two components', () => {
    expect(formatDuration(45)).toBe('45s');
    expect(formatDuration(3 * 3600 + 12 * 60)).toBe('3h 12m');
    expect(formatDuration(4 * 86400 + 3 * 3600)).toBe('4d 3h');
  });

  it('eta sentinel handling', () => {
    expect(formatEta(null)).toBe('∞');
    expect(formatEta(Number.POSITIVE_INFINITY)).toBe('∞');
    expect(formatEta(-1)).toBe('∞');
    expect(formatEta(90)).toBe('1m 30s');
  });
});
