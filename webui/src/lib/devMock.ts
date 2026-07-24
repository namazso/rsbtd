// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

import { makeTorrentRowLike } from './devMockData';
import { useTorrents, type TorrentRow } from '@/store/torrents';

/**
 * Dev-only perf fixture (1/s batches × thousands of rows).
 * In the browser console:
 *
 *   rsbtdMock(5000)   // seed 5k synthetic torrents + 1/s ticker of ~10%
 *   rsbtdMock(0)      // stop and clear
 */
let ticker: ReturnType<typeof setInterval> | null = null;

export function installDevMock(): void {
  (window as unknown as { rsbtdMock: (count: number) => void }).rsbtdMock = (count: number) => {
    if (ticker !== null) {
      clearInterval(ticker);
      ticker = null;
    }
    if (count <= 0) {
      useTorrents.getState().clear();
      return;
    }
    const rows: TorrentRow[] = [];
    for (let i = 0; i < count; i++) rows.push(makeTorrentRowLike(i));
    useTorrents.getState().replaceAll(rows);
    ticker = setInterval(() => {
      const changed: TorrentRow[] = [];
      const step = Math.max(1, Math.floor(count / 10));
      for (let k = 0; k < step; k++) {
        const i = Math.floor(Math.random() * count);
        const row = makeTorrentRowLike(i);
        changed.push({
          ...row,
          downloadPayloadRate: Math.floor(Math.random() * 5_000_000),
          uploadPayloadRate: Math.floor(Math.random() * 1_000_000),
          progressPpm: Math.min(1_000_000, row.progressPpm + Math.floor(Math.random() * 5_000)),
        });
      }
      const start = performance.now();
      useTorrents.getState().patch(changed);
      const ms = performance.now() - start;
      if (ms > 10) console.warn(`rsbtdMock: patch took ${ms.toFixed(1)}ms`);
    }, 1_000);
  };
}
