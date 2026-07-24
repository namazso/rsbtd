// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

import { describe, expect, it } from 'vitest';
import '@/i18n';
import { suggestionsFor } from './autocomplete';

function at(input: string): { input: string; caret: number } {
  return { input, caret: input.length };
}

describe('suggestionsFor', () => {
  it('suggests property names completing up to the colon (spec: isPr -> isPrivate:)', () => {
    const { input, caret } = at('isPr');
    const result = suggestionsFor(input, caret);
    expect(result.items.some((i) => i.insert === 'isPrivate:')).toBe(true);
  });

  it('matches aliases and preserves the negation prefix', () => {
    const result = suggestionsFor('-siz', 4);
    const item = result.items.find((i) => i.insert === '-size:');
    expect(item).toBeDefined();
  });

  it('suggests enum values after the colon', () => {
    const result = suggestionsFor('state:seed', 10);
    expect(result.items.some((i) => i.insert === 'state:SEEDING')).toBe(true);
  });

  it('suggests booleans for bool props', () => {
    const result = suggestionsFor('isPrivate:t', 11);
    expect(result.items.some((i) => i.insert === 'isPrivate:true')).toBe(true);
  });

  it('suggests within the token containing the caret', () => {
    const input = 'foo stat bar';
    const result = suggestionsFor(input, 8); // caret after 'stat'
    expect(result.items.some((i) => i.insert === 'state:')).toBe(true);
    expect(result.replaceStart).toBe(4);
    expect(result.replaceEnd).toBe(8);
  });

  it('replaces through the token end when accepting mid-token', () => {
    const result = suggestionsFor('namxyz', 3);
    expect(result.items.some((i) => i.insert === 'name:')).toBe(true);
    expect(result.replaceStart).toBe(0);
    expect(result.replaceEnd).toBe(6);
  });

  it('replaces the full value token when the caret is mid-value', () => {
    const result = suggestionsFor('state:seedxyz rest', 10);
    expect(result.items.some((i) => i.insert === 'state:SEEDING')).toBe(true);
    expect(result.replaceStart).toBe(0);
    expect(result.replaceEnd).toBe(13);
  });

  it('no suggestions for exact matches or empty tokens', () => {
    expect(suggestionsFor('', 0).items).toHaveLength(0);
    expect(suggestionsFor('state:SEEDING', 13).items).toHaveLength(0);
  });
});
