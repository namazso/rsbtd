// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

import { graphql } from '@/gen/gql';
import type { AddTorrentInput, MoveMode, TorrentFlag, TrackerInput } from '@/gen/gql/graphql';
import { request } from './client';

/** Typed wrappers for torrent/session mutations (booleans mean
 * "accepted", not "already applied" — the 1/s stream shows the effect). */

const PauseTorrentMutation = graphql(`
  mutation PauseTorrent($hash: InfoHash!, $graceful: Boolean!) {
    pauseTorrent(infoHash: $hash, graceful: $graceful)
  }
`);

const ResumeTorrentMutation = graphql(`
  mutation ResumeTorrent($hash: InfoHash!) {
    resumeTorrent(infoHash: $hash)
  }
`);

const SetTorrentFlagsMutation = graphql(`
  mutation SetTorrentFlags($hash: InfoHash!, $set: [TorrentFlag!]!, $unset: [TorrentFlag!]!) {
    setTorrentFlags(infoHash: $hash, set: $set, unset: $unset)
  }
`);

const RemoveTorrentMutation = graphql(`
  mutation RemoveTorrent($hash: InfoHash!, $deleteFiles: Boolean!) {
    removeTorrent(infoHash: $hash, deleteFiles: $deleteFiles)
  }
`);

const ForceRecheckMutation = graphql(`
  mutation ForceRecheck($hash: InfoHash!) {
    forceRecheck(infoHash: $hash)
  }
`);

const ForceReannounceMutation = graphql(`
  mutation ForceReannounce($hash: InfoHash!, $seconds: Int!, $trackerIndex: Int!) {
    forceReannounce(infoHash: $hash, seconds: $seconds, trackerIndex: $trackerIndex)
  }
`);

const ForceDhtAnnounceMutation = graphql(`
  mutation ForceDhtAnnounce($hash: InfoHash!) {
    forceDhtAnnounce(infoHash: $hash)
  }
`);

const ClearErrorMutation = graphql(`
  mutation ClearError($hash: InfoHash!) {
    clearError(infoHash: $hash)
  }
`);

const FlushCacheMutation = graphql(`
  mutation FlushCache($hash: InfoHash!) {
    flushCache(infoHash: $hash)
  }
`);

const SaveResumeDataMutation = graphql(`
  mutation SaveResumeData($hash: InfoHash!) {
    saveResumeData(infoHash: $hash)
  }
`);

const MoveStorageMutation = graphql(`
  mutation MoveStorage($hash: InfoHash!, $path: String!, $mode: MoveMode!) {
    moveStorage(infoHash: $hash, path: $path, mode: $mode)
  }
`);

const QueueTopMutation = graphql(`
  mutation QueueTop($hash: InfoHash!) {
    queueTop(infoHash: $hash)
  }
`);
const QueueUpMutation = graphql(`
  mutation QueueUp($hash: InfoHash!) {
    queueUp(infoHash: $hash)
  }
`);
const QueueDownMutation = graphql(`
  mutation QueueDown($hash: InfoHash!) {
    queueDown(infoHash: $hash)
  }
`);
const QueueBottomMutation = graphql(`
  mutation QueueBottom($hash: InfoHash!) {
    queueBottom(infoHash: $hash)
  }
`);

const SetQueuePositionMutation = graphql(`
  mutation SetQueuePosition($hash: InfoHash!, $position: Int!) {
    setQueuePosition(infoHash: $hash, position: $position)
  }
`);

const SetTorrentLimitsMutation = graphql(`
  mutation SetTorrentLimits(
    $hash: InfoHash!
    $uploadLimit: Int
    $downloadLimit: Int
    $maxUploads: Int
    $maxConnections: Int
  ) {
    setTorrentLimits(
      infoHash: $hash
      uploadLimit: $uploadLimit
      downloadLimit: $downloadLimit
      maxUploads: $maxUploads
      maxConnections: $maxConnections
    )
  }
`);

const AddTorrentMutation = graphql(`
  mutation AddTorrent($input: AddTorrentInput!) {
    addTorrent(input: $input) {
      ...TorrentListFields
    }
  }
`);

const MagnetUriQuery = graphql(`
  query MagnetUri($hash: InfoHash!) {
    torrent(infoHash: $hash) {
      magnetUri
    }
  }
`);

const PauseSessionMutation = graphql(`
  mutation PauseSession {
    pauseSession
  }
`);
const ResumeSessionMutation = graphql(`
  mutation ResumeSession {
    resumeSession
  }
`);

const SetFilePrioritiesMutation = graphql(`
  mutation SetFilePriorities($hash: InfoHash!, $priorities: [Int!]!) {
    setFilePriorities(infoHash: $hash, priorities: $priorities)
  }
`);

const RenameFileMutation = graphql(`
  mutation RenameFile($hash: InfoHash!, $index: Int!, $name: String!) {
    renameFile(infoHash: $hash, index: $index, name: $name)
  }
`);

const AddTrackerMutation = graphql(`
  mutation AddTracker($hash: InfoHash!, $url: String!, $tier: Int!) {
    addTracker(infoHash: $hash, url: $url, tier: $tier)
  }
`);

const ReplaceTrackersMutation = graphql(`
  mutation ReplaceTrackers($hash: InfoHash!, $trackers: [TrackerInput!]!) {
    replaceTrackers(infoHash: $hash, trackers: $trackers)
  }
`);

const ScrapeTrackerMutation = graphql(`
  mutation ScrapeTracker($hash: InfoHash!, $trackerIndex: Int!) {
    scrapeTracker(infoHash: $hash, trackerIndex: $trackerIndex) {
      trackerUrl
      complete
      incomplete
    }
  }
`);

const AddUrlSeedMutation = graphql(`
  mutation AddUrlSeed($hash: InfoHash!, $url: String!) {
    addUrlSeed(infoHash: $hash, url: $url)
  }
`);

const RemoveUrlSeedMutation = graphql(`
  mutation RemoveUrlSeed($hash: InfoHash!, $url: String!) {
    removeUrlSeed(infoHash: $hash, url: $url)
  }
`);

const ConnectPeerMutation = graphql(`
  mutation ConnectPeer($hash: InfoHash!, $address: String!) {
    connectPeer(infoHash: $hash, address: $address)
  }
`);

export const mutations = {
  pause: (hash: string, graceful = false) => request(PauseTorrentMutation, { hash, graceful }),
  resume: (hash: string) => request(ResumeTorrentMutation, { hash }),
  setFlags: (hash: string, set: TorrentFlag[], unset: TorrentFlag[]) =>
    request(SetTorrentFlagsMutation, { hash, set, unset }),
  remove: (hash: string, deleteFiles: boolean) =>
    request(RemoveTorrentMutation, { hash, deleteFiles }),
  recheck: (hash: string) => request(ForceRecheckMutation, { hash }),
  reannounce: (hash: string, seconds = 0, trackerIndex = -1) =>
    request(ForceReannounceMutation, { hash, seconds, trackerIndex }),
  dhtAnnounce: (hash: string) => request(ForceDhtAnnounceMutation, { hash }),
  clearError: (hash: string) => request(ClearErrorMutation, { hash }),
  flushCache: (hash: string) => request(FlushCacheMutation, { hash }),
  saveResumeData: (hash: string) => request(SaveResumeDataMutation, { hash }),
  moveStorage: (hash: string, path: string, mode: MoveMode) =>
    // Waits server-side for confirmation, up to 10 minutes.
    request(MoveStorageMutation, { hash, path, mode }, { timeoutMs: 610_000 }),
  queueTop: (hash: string) => request(QueueTopMutation, { hash }),
  queueUp: (hash: string) => request(QueueUpMutation, { hash }),
  queueDown: (hash: string) => request(QueueDownMutation, { hash }),
  queueBottom: (hash: string) => request(QueueBottomMutation, { hash }),
  setQueuePosition: (hash: string, position: number) =>
    request(SetQueuePositionMutation, { hash, position }),
  setLimits: (
    hash: string,
    limits: {
      uploadLimit?: number;
      downloadLimit?: number;
      maxUploads?: number;
      maxConnections?: number;
    },
  ) => request(SetTorrentLimitsMutation, { hash, ...limits }),
  addTorrent: (input: AddTorrentInput) => request(AddTorrentMutation, { input }),
  magnetUri: async (hash: string) =>
    (await request(MagnetUriQuery, { hash })).torrent?.magnetUri ?? null,
  pauseSession: () => request(PauseSessionMutation),
  resumeSession: () => request(ResumeSessionMutation),
  /** Always send the complete list: an omitted tail gets default 4. */
  setFilePriorities: (hash: string, priorities: number[]) =>
    request(SetFilePrioritiesMutation, { hash, priorities }),
  /** Waits server-side for rename confirmation (serialized per torrent). */
  renameFile: (hash: string, index: number, name: string) =>
    request(RenameFileMutation, { hash, index, name }),
  addTracker: (hash: string, url: string, tier: number) =>
    request(AddTrackerMutation, { hash, url, tier }),
  /** Replaces the full tracker list; an empty list removes all trackers. */
  replaceTrackers: (hash: string, trackers: TrackerInput[]) =>
    request(ReplaceTrackersMutation, { hash, trackers }),
  /** Waits up to 30 s for the tracker's scrape response. */
  scrapeTracker: (hash: string, trackerIndex: number) =>
    request(ScrapeTrackerMutation, { hash, trackerIndex }),
  addUrlSeed: (hash: string, url: string) => request(AddUrlSeedMutation, { hash, url }),
  removeUrlSeed: (hash: string, url: string) => request(RemoveUrlSeedMutation, { hash, url }),
  connectPeer: (hash: string, address: string) => request(ConnectPeerMutation, { hash, address }),
};

/** Sequentially apply one mutation to many torrents; collect failures. */
export async function bulk(
  hashes: readonly string[],
  fn: (hash: string) => Promise<unknown>,
): Promise<{ ok: number; errors: { hash: string; message: string }[] }> {
  let ok = 0;
  const errors: { hash: string; message: string }[] = [];
  for (const hash of hashes) {
    try {
      await fn(hash);
      ok++;
    } catch (err) {
      errors.push({ hash, message: err instanceof Error ? err.message : String(err) });
    }
  }
  return { ok, errors };
}
