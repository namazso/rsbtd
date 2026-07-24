// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

import { describe, expect, it } from 'vitest';
import { extractMagnets } from './magnet';

const V1_HEX = `magnet:?xt=urn:btih:${'a'.repeat(40)}`;
const V1_B32 = `magnet:?xt=urn:btih:${'A'.repeat(32)}`;
const V2 = `magnet:?xt=urn:btmh:1220${'0'.repeat(64)}`;

describe('extractMagnets', () => {
  it('accepts plain v1/v2 magnets', () => {
    expect(extractMagnets(`${V1_HEX}\n${V1_B32}\n${V2}`)).toEqual([V1_HEX, V1_B32, V2]);
  });

  it('keeps extra parameters', () => {
    const uri = `${V1_HEX}&dn=Some+Name&tr=udp%3A%2F%2Ftracker.example%3A6969`;
    expect(extractMagnets(uri)).toEqual([uri]);
  });

  it('extracts magnets wrapped in prose', () => {
    const text = `hey, check this out: ${V1_HEX} (seed it please)\nsecond one ${V2}.`;
    expect(extractMagnets(text)).toEqual([V1_HEX, V2]);
  });

  it('strips trailing punctuation', () => {
    expect(extractMagnets(`Get it at ${V1_HEX}.`)).toEqual([V1_HEX]);
    expect(extractMagnets(`(${V1_HEX})`)).toEqual([V1_HEX]);
  });

  it('rejects text without a valid info-hash', () => {
    expect(extractMagnets('no magnets here')).toEqual([]);
    expect(extractMagnets('magnet:?xt=urn:btih:tooshort')).toEqual([]);
    expect(extractMagnets(`magnet:?xt=urn:btmh:1220${'0'.repeat(63)}`)).toEqual([]);
    expect(extractMagnets('magnet:?dn=Name+Only&tr=udp%3A%2F%2Ft')).toEqual([]);
  });

  it('deduplicates repeated URIs', () => {
    expect(extractMagnets(`${V1_HEX}\n${V1_HEX}`)).toEqual([V1_HEX]);
  });

  it('accepts any casing of scheme, parameter names, and urn prefix', () => {
    const upper = `MAGNET:?XT=URN:BTIH:${'A'.repeat(40)}`;
    const mixed = `Magnet:?Xt=urn:BTih:${'b'.repeat(40)}`;
    const upperKeyOnly = `magnet:?XT=urn:btih:${'c'.repeat(40)}`;
    expect(extractMagnets(upper)).toEqual([upper]);
    expect(extractMagnets(mixed)).toEqual([mixed]);
    expect(extractMagnets(upperKeyOnly)).toEqual([upperKeyOnly]);
  });
});
