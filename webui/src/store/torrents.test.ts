// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

import { beforeEach, describe, expect, it } from 'vitest';
import { makeTorrentRow } from '../../test/fixtures';
import { useSelection } from './selection';
import { rowEqual, useTorrents } from './torrents';
import { useUi } from './ui';

const UUID_A = 'aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa';
const UUID_B = 'bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb';

function store() {
  return useTorrents.getState();
}

beforeEach(() => {
  store().clear();
  useSelection.getState().clear();
  useUi.getState().setDetailsUuid(null);
});

describe('rowEqual', () => {
  it('is true for identical content', () => {
    const a = makeTorrentRow({ uuid: UUID_A });
    const b = makeTorrentRow({ uuid: UUID_A });
    expect(rowEqual(a, b)).toBe(true);
  });

  it('detects scalar, error and flags changes', () => {
    const a = makeTorrentRow({ uuid: UUID_A });
    expect(rowEqual(a, makeTorrentRow({ uuid: UUID_A, downloadRate: 1 }))).toBe(false);
    expect(rowEqual(a, makeTorrentRow({ uuid: UUID_A, error: { message: 'x', file: -1 } }))).toBe(
      false,
    );
    expect(
      rowEqual(a, makeTorrentRow({ uuid: UUID_A, flags: ['PAUSED'] as TorrentRowFlags })),
    ).toBe(false);
  });

  it('compares equal error objects and flag arrays by value', () => {
    const a = makeTorrentRow({
      uuid: UUID_A,
      error: { message: 'x', file: -1 },
      flags: ['PAUSED'] as TorrentRowFlags,
    });
    const b = makeTorrentRow({
      uuid: UUID_A,
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
    store().patch([makeTorrentRow({ uuid: UUID_A })]);
    expect(store().byUuid.size).toBe(1);
    expect(store().listVersion).toBe(v0 + 1);
  });

  it('keeps identity of unchanged rows and skips no-op batches', () => {
    store().patch([makeTorrentRow({ uuid: UUID_A }), makeTorrentRow({ uuid: UUID_B })]);
    const v1 = store().listVersion;
    const rowA = store().byUuid.get(UUID_A);

    // Identical batch: nothing changes, no version bump, same references.
    store().patch([makeTorrentRow({ uuid: UUID_A }), makeTorrentRow({ uuid: UUID_B })]);
    expect(store().listVersion).toBe(v1);
    expect(store().byUuid.get(UUID_A)).toBe(rowA);

    // One row changes: version bumps, unchanged row keeps identity.
    store().patch([makeTorrentRow({ uuid: UUID_B, downloadRate: 999 })]);
    expect(store().listVersion).toBe(v1 + 1);
    expect(store().byUuid.get(UUID_A)).toBe(rowA);
    expect(store().byUuid.get(UUID_B)?.downloadRate).toBe(999);
  });

  it('keys by uuid even when the info-hashes change (metadata arrival)', () => {
    // Added by v2-only magnet: no v1 hash yet.
    store().patch([makeTorrentRow({ uuid: UUID_A, infoHashV1: null, infoHashV2: 'c'.repeat(64) })]);
    // Metadata arrives with the v1 hash: same uuid, same entry, no rekey.
    store().patch([
      makeTorrentRow({ uuid: UUID_A, infoHashV1: 'a'.repeat(40), infoHashV2: 'c'.repeat(64) }),
    ]);
    expect(store().byUuid.size).toBe(1);
    expect(store().byUuid.get(UUID_A)?.infoHashV1).toBe('a'.repeat(40));
  });
});

describe('torrents store replaceAll', () => {
  it('deletes missing rows and keeps identity of unchanged rows', () => {
    store().patch([makeTorrentRow({ uuid: UUID_A }), makeTorrentRow({ uuid: UUID_B })]);
    const rowA = store().byUuid.get(UUID_A);
    const v1 = store().listVersion;

    store().replaceAll([makeTorrentRow({ uuid: UUID_A })]);
    expect(store().byUuid.size).toBe(1);
    expect(store().byUuid.get(UUID_A)).toBe(rowA);
    expect(store().listVersion).toBe(v1 + 1);
    expect(store().byUuid.has(UUID_B)).toBe(false);
  });

  it('is a no-op when content and order are identical', () => {
    store().replaceAll([makeTorrentRow({ uuid: UUID_A }), makeTorrentRow({ uuid: UUID_B })]);
    const v1 = store().listVersion;
    store().replaceAll([makeTorrentRow({ uuid: UUID_A }), makeTorrentRow({ uuid: UUID_B })]);
    expect(store().listVersion).toBe(v1);
  });
});

describe('removal reconciliation', () => {
  it('remove drops the uuid from selection and closes its details', () => {
    store().patch([makeTorrentRow({ uuid: UUID_A })]);
    useSelection.getState().click(UUID_A, { ctrl: false, shift: false }, [UUID_A]);
    useUi.getState().setDetailsUuid(UUID_A);
    store().remove(UUID_A);
    expect(useSelection.getState().selected.has(UUID_A)).toBe(false);
    expect(useUi.getState().detailsUuid).toBeNull();
  });

  it('replaceAll reconciles resync-dropped rows like removal events', () => {
    store().patch([makeTorrentRow({ uuid: UUID_A }), makeTorrentRow({ uuid: UUID_B })]);
    useSelection.getState().click(UUID_A, { ctrl: false, shift: false }, [UUID_A, UUID_B]);
    useSelection.getState().click(UUID_B, { ctrl: true, shift: false }, [UUID_A, UUID_B]);
    useUi.getState().setDetailsUuid(UUID_B);

    store().replaceAll([makeTorrentRow({ uuid: UUID_A })]);
    expect(useSelection.getState().selected.has(UUID_B)).toBe(false);
    expect(useSelection.getState().selected.has(UUID_A)).toBe(true);
    expect(useUi.getState().detailsUuid).toBeNull();
  });
});

describe('performance envelope', () => {
  it('patches a 10% batch of 5k torrents well under the tick budget', () => {
    const rows = Array.from({ length: 5_000 }, (_, i) =>
      makeTorrentRow({
        uuid: `00000000-0000-4000-8000-${String(i).padStart(12, '0')}`,
        downloadRate: i,
      }),
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
    expect(store().byUuid.size).toBe(5_000);
  });
});
