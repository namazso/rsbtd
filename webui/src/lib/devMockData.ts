// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

import type { TorrentRow } from '@/store/torrents';

const STATES = ['DOWNLOADING', 'SEEDING', 'FINISHED', 'CHECKING_FILES'] as const;

/** Deterministic synthetic row for the dev perf fixture. */
export function makeTorrentRowLike(i: number): TorrentRow {
  const hash = i.toString(16).padStart(40, '0');
  const size = 1024 * 1024 * (64 + (i % 4096));
  const done = Math.floor(size * ((i % 100) / 100));
  return {
    id: i,
    infoHash: hash,
    infoHashV1: hash,
    infoHashV2: null,
    name: `synthetic-${i.toString(36)}-linux-distro-${i}.iso`,
    state: STATES[i % STATES.length]!,
    progressPpm: Math.floor(((i % 100) / 100) * 1_000_000),
    error: null,
    savePath: '/downloads',
    currentTracker: i % 3 === 0 ? 'https://tracker.example/announce' : null,
    nextAnnounceSeconds: (i % 1800) + 10,
    totalDownload: done,
    totalUpload: Math.floor(done / 2),
    totalPayloadDownload: done,
    totalPayloadUpload: Math.floor(done / 2),
    totalFailedBytes: 0,
    totalRedundantBytes: 0,
    totalDone: done,
    totalSize: size,
    totalWantedDone: done,
    totalWanted: size,
    allTimeUpload: Math.floor(done / 2),
    allTimeDownload: done,
    addedTime: 1_700_000_000 + i * 60,
    completedTime: i % 5 === 0 ? 1_700_100_000 + i * 60 : null,
    lastSeenComplete: null,
    storageMode: 'SPARSE',
    queuePosition: i % 7 === 0 ? null : i,
    downloadRate: (i % 50) * 10_240,
    uploadRate: (i % 20) * 10_240,
    downloadPayloadRate: (i % 50) * 10_000,
    uploadPayloadRate: (i % 20) * 10_000,
    numSeeds: i % 12,
    numPeers: i % 40,
    numComplete: null,
    numIncomplete: null,
    listSeeds: i % 60,
    listPeers: i % 200,
    connectCandidates: i % 30,
    piecesHave: i % 900,
    distributedCopies: (i % 50) / 10,
    blockSize: 16_384,
    numUploads: i % 4,
    numConnections: i % 45,
    uploadsLimit: -1,
    connectionsLimit: -1,
    uploadLimit: -1,
    downloadLimit: -1,
    upBandwidthQueue: 0,
    downBandwidthQueue: 0,
    seedRank: i % 100,
    needSaveResumeData: false,
    isSeeding: i % 4 === 1,
    isFinished: i % 4 === 1 || i % 4 === 2,
    isPaused: i % 9 === 0,
    isAutoManaged: i % 9 !== 0,
    hasMetadata: true,
    sizeOnDisk: size,
    pieceLength: 1_048_576,
    isPrivate: i % 6 === 0,
    isI2P: false,
    hasIncoming: i % 2 === 0,
    movingStorage: false,
    announcingToTrackers: true,
    announcingToLsd: true,
    announcingToDht: true,
    flags: i % 9 === 0 ? ['PAUSED'] : ['AUTO_MANAGED'],
  };
}
