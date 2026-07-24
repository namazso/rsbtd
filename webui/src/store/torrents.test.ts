// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

import { beforeEach, describe, expect, it, vi } from 'vitest';
import { makeTorrentRow } from '../../test/fixtures';
import { useSelection } from './selection';
import { onTorrentRekey, rowEqual, useTorrents } from './torrents';
import { useUi } from './ui';

const HASH_A = 'a'.repeat(40);
const HASH_B = 'b'.repeat(40);
const HASH_V2 = 'c'.repeat(64);

function store() {
  return useTorrents.getState();
}

beforeEach(() => {
  store().clear();
  useSelection.getState().clear();
  useUi.getState().setDetailsHash(null);
});

describe('rowEqual', () => {
  it('is true for identical content', () => {
    const a = makeTorrentRow({ infoHash: HASH_A });
    const b = makeTorrentRow({ infoHash: HASH_A });
    expect(rowEqual(a, b)).toBe(true);
  });

  it('detects scalar, error and flags changes', () => {
    const a = makeTorrentRow({ infoHash: HASH_A });
    expect(rowEqual(a, makeTorrentRow({ infoHash: HASH_A, downloadRate: 1 }))).toBe(false);
    expect(
      rowEqual(a, makeTorrentRow({ infoHash: HASH_A, error: { message: 'x', file: -1 } })),
    ).toBe(false);
    expect(
      rowEqual(a, makeTorrentRow({ infoHash: HASH_A, flags: ['PAUSED'] as TorrentRowFlags })),
    ).toBe(false);
  });

  it('compares equal error objects and flag arrays by value', () => {
    const a = makeTorrentRow({
      infoHash: HASH_A,
      error: { message: 'x', file: -1 },
      flags: ['PAUSED'] as TorrentRowFlags,
    });
    const b = makeTorrentRow({
      infoHash: HASH_A,
      error: { message: 'x', file: -1 },
      flags: ['PAUSED'] as TorrentRowFlags,
    });
    expect(rowEqual(a, b)).toBe(true);
  });
});

type TorrentRowFlags = ReturnType<typeof makeTorrentRow>['flags'];

describe('torrents store patch', () => {
  it('inserts new rows and bumps listVersion', () => {
    const v0 = store().listVersion;
    store().patch([makeTorrentRow({ infoHash: HASH_A })]);
    expect(store().byHash.size).toBe(1);
    expect(store().listVersion).toBe(v0 + 1);
  });

  it('keeps identity of unchanged rows and skips no-op batches', () => {
    store().patch([makeTorrentRow({ infoHash: HASH_A }), makeTorrentRow({ infoHash: HASH_B })]);
    const v1 = store().listVersion;
    const rowA = store().byHash.get(HASH_A);

    // Identical batch: nothing changes, no version bump, same references.
    store().patch([makeTorrentRow({ infoHash: HASH_A }), makeTorrentRow({ infoHash: HASH_B })]);
    expect(store().listVersion).toBe(v1);
    expect(store().byHash.get(HASH_A)).toBe(rowA);

    // One row changes: version bumps, unchanged row keeps identity.
    store().patch([makeTorrentRow({ infoHash: HASH_B, downloadRate: 999 })]);
    expect(store().listVersion).toBe(v1 + 1);
    expect(store().byHash.get(HASH_A)).toBe(rowA);
    expect(store().byHash.get(HASH_B)?.downloadRate).toBe(999);
  });

  it('re-keys when a hybrid magnet gains its preferred v1 hash', () => {
    const rekeys: [string, string][] = [];
    const off = onTorrentRekey((oldHash, newHash) => rekeys.push([oldHash, newHash]));

    // Added by v2 hash only (magnet before metadata).
    store().patch([makeTorrentRow({ infoHash: HASH_V2, infoHashV1: null, infoHashV2: HASH_V2 })]);
    expect(store().byHash.has(HASH_V2)).toBe(true);

    // Metadata arrives: daemon now reports v1-preferred infoHash.
    store().patch([makeTorrentRow({ infoHash: HASH_A, infoHashV1: HASH_A, infoHashV2: HASH_V2 })]);
    expect(store().byHash.has(HASH_V2)).toBe(false);
    expect(store().byHash.has(HASH_A)).toBe(true);
    expect(store().byHash.size).toBe(1);
    expect(rekeys).toEqual([[HASH_V2, HASH_A]]);
    // Both hashes resolve to the canonical key.
    expect(store().resolve(HASH_V2)).toBe(HASH_A);
    expect(store().resolve(HASH_A)).toBe(HASH_A);
    off();
  });

  it('remove works via either hash and drops aliases', () => {
    store().patch([makeTorrentRow({ infoHash: HASH_A, infoHashV1: HASH_A, infoHashV2: HASH_V2 })]);
    store().remove(HASH_V2);
    expect(store().byHash.size).toBe(0);
    expect(store().resolve(HASH_A)).toBeUndefined();
  });
});

describe('torrents store replaceAll', () => {
  it('deletes missing rows and keeps identity of unchanged rows', () => {
    store().patch([makeTorrentRow({ infoHash: HASH_A }), makeTorrentRow({ infoHash: HASH_B })]);
    const rowA = store().byHash.get(HASH_A);
    const v1 = store().listVersion;

    store().replaceAll([makeTorrentRow({ infoHash: HASH_A })]);
    expect(store().byHash.size).toBe(1);
    expect(store().byHash.get(HASH_A)).toBe(rowA);
    expect(store().listVersion).toBe(v1 + 1);
    expect(store().resolve(HASH_B)).toBeUndefined();
  });

  it('is a no-op when content and order are identical', () => {
    store().replaceAll([
      makeTorrentRow({ infoHash: HASH_A }),
      makeTorrentRow({ infoHash: HASH_B }),
    ]);
    const v1 = store().listVersion;
    store().replaceAll([
      makeTorrentRow({ infoHash: HASH_A }),
      makeTorrentRow({ infoHash: HASH_B }),
    ]);
    expect(store().listVersion).toBe(v1);
  });
});

describe('removal reconciliation', () => {
  it('remove drops the hash from selection and closes its details', () => {
    store().patch([makeTorrentRow({ infoHash: HASH_A })]);
    useSelection.getState().click(HASH_A, { ctrl: false, shift: false }, [HASH_A]);
    useUi.getState().setDetailsHash(HASH_A);
    store().remove(HASH_A);
    expect(useSelection.getState().selected.has(HASH_A)).toBe(false);
    expect(useUi.getState().detailsHash).toBeNull();
  });

  it('replaceAll reconciles resync-dropped rows like removal events', () => {
    store().patch([makeTorrentRow({ infoHash: HASH_A }), makeTorrentRow({ infoHash: HASH_B })]);
    useSelection.getState().click(HASH_A, { ctrl: false, shift: false }, [HASH_A, HASH_B]);
    useSelection.getState().click(HASH_B, { ctrl: true, shift: false }, [HASH_A, HASH_B]);
    useUi.getState().setDetailsHash(HASH_B);

    store().replaceAll([makeTorrentRow({ infoHash: HASH_A })]);
    expect(useSelection.getState().selected.has(HASH_B)).toBe(false);
    expect(useSelection.getState().selected.has(HASH_A)).toBe(true);
    expect(useUi.getState().detailsHash).toBeNull();
  });
});

describe('unsubscribed rekey listener', () => {
  it('does not fire after unsubscribe', () => {
    const spy = vi.fn();
    const off = onTorrentRekey(spy);
    off();
    store().patch([makeTorrentRow({ infoHash: HASH_V2, infoHashV1: null, infoHashV2: HASH_V2 })]);
    store().patch([makeTorrentRow({ infoHash: HASH_A, infoHashV1: HASH_A, infoHashV2: HASH_V2 })]);
    expect(spy).not.toHaveBeenCalled();
  });
});

describe('performance envelope', () => {
  it('patches a 10% batch of 5k torrents well under the tick budget', () => {
    const rows = Array.from({ length: 5_000 }, (_, i) =>
      makeTorrentRow({ infoHash: String(i).padStart(40, '0'), id: i, downloadRate: i }),
    );
    store().replaceAll(rows);

    const changed = rows
      .filter((_, i) => i % 10 === 0)
      .map((r) => ({ ...r, downloadPayloadRate: r.downloadPayloadRate + 1 }));
    const start = performance.now();
    store().patch(changed);
    const patchMs = performance.now() - start;

    // Generous CI bound; typical is single-digit milliseconds.
    expect(patchMs).toBeLessThan(150);
    expect(store().byHash.size).toBe(5_000);
  });
});
