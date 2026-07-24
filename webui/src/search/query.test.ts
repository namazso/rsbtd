// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

import { describe, expect, it } from 'vitest';
import '@/i18n';
import { makeTorrentRow } from '../../test/fixtures';
import type { TorrentRow } from '@/store/torrents';
import { compileQuery, parseQuery, tokenize } from './query';
import { parseDate, parseDurationSeconds, parsePercentPpm, parseRate, parseSize } from './units';

const HASH = 'a'.repeat(40);

function matches(query: string, row: TorrentRow): boolean {
  const pred = compileQuery(parseQuery(query));
  return pred === null ? true : pred(row);
}

describe('tokenize', () => {
  it('splits on whitespace and strips quotes', () => {
    expect(tokenize('a b  c').tokens).toEqual([
      { text: 'a', negated: false },
      { text: 'b', negated: false },
      { text: 'c', negated: false },
    ]);
    expect(tokenize('"a b" c').tokens).toEqual([
      { text: 'a b', negated: false },
      { text: 'c', negated: false },
    ]);
    // quoted whole component (spec example)
    expect(tokenize('"name:*ntu 12*"').tokens).toEqual([{ text: 'name:*ntu 12*', negated: false }]);
    // quoted value only
    expect(tokenize('name:"a b"').tokens).toEqual([{ text: 'name:a b', negated: false }]);
  });

  it('handles negation and unterminated quotes', () => {
    expect(tokenize('-isPrivate:true').tokens).toEqual([{ text: 'isPrivate:true', negated: true }]);
    expect(tokenize('-"a b"').tokens).toEqual([{ text: 'a b', negated: true }]);
    const result = tokenize('"unclosed rest');
    expect(result.unterminated).toBe(true);
    expect(result.tokens).toEqual([{ text: 'unclosed rest', negated: false }]);
    expect(tokenize('').tokens).toEqual([]);
    expect(tokenize('   ').tokens).toEqual([]);
  });
});

describe('units', () => {
  it('parses sizes with decimal and binary units', () => {
    expect(parseSize('100')).toBe(100);
    expect(parseSize('1k')).toBe(1000);
    expect(parseSize('1.5gb')).toBe(1.5e9);
    expect(parseSize('1GiB')).toBe(1024 ** 3);
    expect(parseSize('2 MiB')).toBe(2 * 1024 ** 2);
    expect(parseSize('1TB')).toBe(1e12);
    expect(parseSize('garbage')).toBeNull();
    expect(parseSize('1x')).toBeNull();
  });

  it('parses rates, percent, durations', () => {
    expect(parseRate('1mb/s')).toBe(1e6);
    expect(parseRate('512kib')).toBe(512 * 1024);
    expect(parsePercentPpm('50')).toBe(500_000);
    expect(parsePercentPpm('12.5%')).toBe(125_000);
    expect(parseDurationSeconds('90')).toBe(90);
    expect(parseDurationSeconds('5m')).toBe(300);
    expect(parseDurationSeconds('2h')).toBe(7200);
    expect(parseDurationSeconds('1d')).toBe(86_400);
  });

  it('parses dates as local day ranges', () => {
    const d = parseDate('2025-06-15');
    expect(d).not.toBeNull();
    // Local midnight to next local midnight — not a fixed 86400 s, so
    // the expectation holds in time zones where this day has a DST
    // transition.
    const dayStart = new Date(2025, 5, 15);
    const nextDayStart = new Date(2025, 5, 16);
    expect(d!.startSec).toBe(Math.floor(dayStart.getTime() / 1000));
    expect(d!.endSec - d!.startSec).toBe((nextDayStart.getTime() - dayStart.getTime()) / 1000);
    const withTime = parseDate('2025-06-15T10:30');
    expect(withTime!.endSec - withTime!.startSec).toBe(60);
    expect(parseDate('junk')).toBeNull();
  });

  it('rejects rolled-over date components', () => {
    expect(parseDate('2026-02-31')).toBeNull();
    expect(parseDate('2025-13-01')).toBeNull();
    expect(parseDate('2025-06-15T25:00')).toBeNull();
    expect(parseDate('2025-06-15T10:61')).toBeNull();
    expect(parseDate('2025-06-15T10:30:61')).toBeNull();
    expect(parseDate('2024-02-29')).not.toBeNull();
  });
});

describe('text terms', () => {
  const row = makeTorrentRow({ infoHash: HASH, name: 'Ubuntu 12.04 Desktop' });

  it('free text is a case-insensitive contains match on the name', () => {
    expect(matches('ubuntu', row)).toBe(true);
    expect(matches('Ubuntu 12.04', row)).toBe(true); // two terms, both match
    expect(matches('fedora', row)).toBe(false);
  });

  it('negated text', () => {
    expect(matches('-fedora', row)).toBe(true);
    expect(matches('-ubuntu', row)).toBe(false);
  });

  it('unknown property degrades to text with a diagnostic', () => {
    const parsed = parseQuery('bogus:value');
    expect(parsed.diagnostics).toContain('unknown property: bogus');
    expect(matches('bogus:value', row)).toBe(false); // name does not contain it
    const uri = makeTorrentRow({ infoHash: HASH, name: 'magnet:?xt=foo' });
    expect(matches('magnet:?xt', uri)).toBe(true);
  });
});

describe('string filters', () => {
  const row = makeTorrentRow({ infoHash: HASH, name: 'Ubuntu 12.04' });

  it('exact match unless wildcarded (spec examples)', () => {
    expect(matches('name:Ubuntu', row)).toBe(false);
    expect(matches('name:Ubuntu*', row)).toBe(true);
    expect(matches('name:*12.04', row)).toBe(true);
    expect(matches('name:*12*', row)).toBe(true);
    expect(matches('name:ubuntu 12.04', row)).toBe(false); // name:ubuntu is exact (no wildcard) and fails
    expect(matches('"name:*ntu 12*"', row)).toBe(true);
    expect(matches('name:"Ubuntu 12.04"', row)).toBe(true);
  });

  it('aliases resolve (tracker:, path:)', () => {
    const r = makeTorrentRow({ infoHash: HASH, savePath: '/data/iso' });
    expect(matches('path:/data/iso', r)).toBe(true);
    expect(matches('path:*iso', r)).toBe(true);
  });
});

describe('boolean, enum, flags filters', () => {
  it('isPrivate:true / negation (spec example)', () => {
    const priv = makeTorrentRow({ infoHash: HASH, isPrivate: true });
    const pub = makeTorrentRow({ infoHash: HASH, isPrivate: false });
    expect(matches('isPrivate:true', priv)).toBe(true);
    expect(matches('isPrivate:true', pub)).toBe(false);
    expect(matches('-isPrivate:true', pub)).toBe(true);
    expect(matches('isprivate:TRUE', priv)).toBe(true); // case-insensitive key+value
  });

  it('state and status enums are case-insensitive', () => {
    const seeding = makeTorrentRow({
      infoHash: HASH,
      state: 'SEEDING' as TorrentRow['state'],
    });
    expect(matches('state:seeding', seeding)).toBe(true);
    expect(matches('status:seeding', seeding)).toBe(true);
    expect(matches('-state:seeding', seeding)).toBe(false);
    const paused = makeTorrentRow({ infoHash: HASH, isPaused: true });
    expect(matches('status:paused', paused)).toBe(true);
  });

  it('flags membership', () => {
    const row = makeTorrentRow({
      infoHash: HASH,
      flags: ['SEQUENTIAL_DOWNLOAD'] as TorrentRow['flags'],
    });
    expect(matches('flags:sequential_download', row)).toBe(true);
    expect(matches('flags:paused', row)).toBe(false);
  });
});

describe('numeric comparisons', () => {
  it('size with units and ranges', () => {
    const big = makeTorrentRow({ infoHash: HASH, totalWanted: 2 * 1024 ** 3 });
    const small = makeTorrentRow({ infoHash: HASH, totalWanted: 100 * 1024 ** 2 });
    expect(matches('size:>1gb', big)).toBe(true);
    expect(matches('size:>1gb', small)).toBe(false);
    expect(matches('size:<1gb', small)).toBe(true);
    expect(matches('size:>100mb size:<10gb', big)).toBe(true);
    expect(matches('size:>=2GiB', big)).toBe(true);
    expect(matches('totalWanted:>1gb', big)).toBe(true); // canonical key too
  });

  it('rates and progress', () => {
    const row = makeTorrentRow({
      infoHash: HASH,
      downloadPayloadRate: 2 * 1024 * 1024,
      progressPpm: 455_000,
    });
    expect(matches('downSpeed:>1mb', row)).toBe(true);
    expect(matches('down:<1mb', row)).toBe(false);
    expect(matches('progress:>40', row)).toBe(true);
    expect(matches('progress:<40%', row)).toBe(false);
  });

  it('-1 unlimited sentinel on limit fields', () => {
    const unlimited = makeTorrentRow({ infoHash: HASH, downloadLimit: -1 });
    const limited = makeTorrentRow({ infoHash: HASH, downloadLimit: 1000 });
    expect(parseQuery('downloadLimit:-1').diagnostics).toEqual([]);
    expect(matches('downloadLimit:-1', unlimited)).toBe(true);
    expect(matches('downloadLimit:-1', limited)).toBe(false);
    expect(matches('uploadsLimit:-1', unlimited)).toBe(true);
    // Plain rates have no sentinel semantics.
    expect(parseQuery('downSpeed:-1').diagnostics.length).toBe(1);
  });

  it('ratio infinity semantics', () => {
    const infinite = makeTorrentRow({
      infoHash: HASH,
      allTimeDownload: 0,
      allTimeUpload: 100,
    });
    expect(matches('ratio:>1', infinite)).toBe(true);
    const zero = makeTorrentRow({ infoHash: HASH, allTimeDownload: 0, allTimeUpload: 0 });
    expect(matches('ratio:>1', zero)).toBe(false);
  });

  it('eta null sorts as infinity for comparisons', () => {
    const stalled = makeTorrentRow({ infoHash: HASH, downloadPayloadRate: 0 });
    expect(matches('eta:>1d', stalled)).toBe(true);
    expect(matches('eta:<1d', stalled)).toBe(false);
  });

  it('queue filters use the displayed one-based position', () => {
    const second = makeTorrentRow({ infoHash: HASH, queuePosition: 1 });
    expect(matches('queue:2', second)).toBe(true);
    expect(matches('queue:1', second)).toBe(false);
  });

  it('other missing numeric values match no comparison', () => {
    const metadataless = makeTorrentRow({ infoHash: HASH, pieceLength: null });
    expect(matches('pieceLength:>1G', metadataless)).toBe(false);
    expect(matches('pieceLength:<1G', metadataless)).toBe(false);
    const unqueued = makeTorrentRow({ infoHash: HASH, queuePosition: null });
    expect(matches('queue:>100', unqueued)).toBe(false);
  });

  it('dates: equality is whole-day, comparisons are boundaries', () => {
    const noonJune15 = Math.floor(new Date(2025, 5, 15, 12, 0, 0).getTime() / 1000);
    const row = makeTorrentRow({ infoHash: HASH, addedTime: noonJune15 });
    expect(matches('added:2025-06-15', row)).toBe(true);
    expect(matches('added:2025-06-14', row)).toBe(false);
    expect(matches('added:>2025-06-14', row)).toBe(true);
    expect(matches('added:>2025-06-15', row)).toBe(false); // same day, > means after that day
    expect(matches('added:>=2025-06-15', row)).toBe(true);
    expect(matches('added:<2025-07-01', row)).toBe(true);
    expect(matches('addedTime:>2025-01-01', row)).toBe(true);
  });
});

describe('forgiving parsing while typing', () => {
  const row = makeTorrentRow({ infoHash: HASH, name: 'x' });

  it('empty rhs and bare comparators are no-ops', () => {
    expect(matches('name:', row)).toBe(true);
    expect(matches('size:>', row)).toBe(true);
    expect(matches('isPrivate:', row)).toBe(true);
  });

  it('unparsable values produce diagnostics, not exclusion', () => {
    const parsed = parseQuery('size:banana isPrivate:maybe state:flying');
    expect(parsed.diagnostics.length).toBe(3);
    expect(matches('size:banana', row)).toBe(true);
  });
});

describe('fuzz: parser never throws', () => {
  it('survives 10k random inputs', () => {
    const alphabet = 'abc:*-"<>=1.5 gGmMkK%/s\t';
    let seed = 42;
    const rand = () => {
      seed = (seed * 1103515245 + 12345) & 0x7fffffff;
      return seed / 0x7fffffff;
    };
    const row = makeTorrentRow({ infoHash: HASH });
    for (let i = 0; i < 10_000; i++) {
      const len = Math.floor(rand() * 24);
      let input = '';
      for (let j = 0; j < len; j++) {
        input += alphabet[Math.floor(rand() * alphabet.length)];
      }
      const parsed = parseQuery(input);
      const pred = compileQuery(parsed);
      if (pred !== null) pred(row);
    }
  });
});
