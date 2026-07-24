// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

import { describe, expect, it } from 'vitest';
import { formatRateKiB, parseRateKiB } from './rateLimit';

describe('formatRateKiB', () => {
  it('shows unlimited and unset as -1', () => {
    expect(formatRateKiB(undefined)).toBe('-1');
    expect(formatRateKiB(-1)).toBe('-1');
  });

  it('preserves exact values, including sub-KiB rates', () => {
    expect(formatRateKiB(524_288)).toBe('512');
    expect(formatRateKiB(512)).toBe('0.5');
    expect(parseRateKiB(formatRateKiB(511))).toBe(511);
    expect(parseRateKiB(formatRateKiB(1))).toBe(1);
  });
});

describe('parseRateKiB', () => {
  it('treats blank as leave-unchanged', () => {
    expect(parseRateKiB('')).toBeUndefined();
    expect(parseRateKiB('  ')).toBeUndefined();
  });

  it('accepts exactly -1 and positive rates', () => {
    expect(parseRateKiB('-1')).toBe(-1);
    expect(parseRateKiB('0.5')).toBe(512);
    expect(parseRateKiB('100')).toBe(102_400);
  });

  it('rejects zero, other negatives, sub-byte rates, and non-numbers', () => {
    expect(parseRateKiB('0')).toBeNull();
    expect(parseRateKiB('-2')).toBeNull();
    expect(parseRateKiB('-0.5')).toBeNull();
    expect(parseRateKiB('1e-9')).toBeNull();
    expect(parseRateKiB('abc')).toBeNull();
  });
});
