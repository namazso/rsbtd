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
  mutation PauseTorrent($uuid: UUID!, $graceful: Boolean!) {
    pauseTorrent(uuid: $uuid, graceful: $graceful)
  }
`);

const ResumeTorrentMutation = graphql(`
  mutation ResumeTorrent($uuid: UUID!) {
    resumeTorrent(uuid: $uuid)
  }
`);

const SetTorrentFlagsMutation = graphql(`
  mutation SetTorrentFlags($uuid: UUID!, $set: [TorrentFlag!]!, $unset: [TorrentFlag!]!) {
    setTorrentFlags(uuid: $uuid, set: $set, unset: $unset)
  }
`);

const RemoveTorrentMutation = graphql(`
  mutation RemoveTorrent($uuid: UUID!, $deleteFiles: Boolean!) {
    removeTorrent(uuid: $uuid, deleteFiles: $deleteFiles)
  }
`);

const ForceRecheckMutation = graphql(`
  mutation ForceRecheck($uuid: UUID!) {
    forceRecheck(uuid: $uuid)
  }
`);

const ForceReannounceMutation = graphql(`
  mutation ForceReannounce($uuid: UUID!, $seconds: Int!, $trackerIndex: Int!) {
    forceReannounce(uuid: $uuid, seconds: $seconds, trackerIndex: $trackerIndex)
  }
`);

const ForceDhtAnnounceMutation = graphql(`
  mutation ForceDhtAnnounce($uuid: UUID!) {
    forceDhtAnnounce(uuid: $uuid)
  }
`);

const ClearErrorMutation = graphql(`
  mutation ClearError($uuid: UUID!) {
    clearError(uuid: $uuid)
  }
`);

const FlushCacheMutation = graphql(`
  mutation FlushCache($uuid: UUID!) {
    flushCache(uuid: $uuid)
  }
`);

const SaveResumeDataMutation = graphql(`
  mutation SaveResumeData($uuid: UUID!) {
    saveResumeData(uuid: $uuid)
  }
`);

const MoveStorageMutation = graphql(`
  mutation MoveStorage($uuid: UUID!, $path: String!, $mode: MoveMode!) {
    moveStorage(uuid: $uuid, path: $path, mode: $mode)
  }
`);

const QueueTopMutation = graphql(`
  mutation QueueTop($uuid: UUID!) {
    queueTop(uuid: $uuid)
  }
`);
const QueueUpMutation = graphql(`
  mutation QueueUp($uuid: UUID!) {
    queueUp(uuid: $uuid)
  }
`);
const QueueDownMutation = graphql(`
  mutation QueueDown($uuid: UUID!) {
    queueDown(uuid: $uuid)
  }
`);
const QueueBottomMutation = graphql(`
  mutation QueueBottom($uuid: UUID!) {
    queueBottom(uuid: $uuid)
  }
`);

const SetQueuePositionMutation = graphql(`
  mutation SetQueuePosition($uuid: UUID!, $position: Int!) {
    setQueuePosition(uuid: $uuid, position: $position)
  }
`);

const SetTorrentLimitsMutation = graphql(`
  mutation SetTorrentLimits(
    $uuid: UUID!
    $uploadLimit: Int
    $downloadLimit: Int
    $maxUploads: Int
    $maxConnections: Int
  ) {
    setTorrentLimits(
      uuid: $uuid
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
  query MagnetUri($uuid: UUID!) {
    torrent(uuid: $uuid) {
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
  mutation SetFilePriorities($uuid: UUID!, $priorities: [Int!]!) {
    setFilePriorities(uuid: $uuid, priorities: $priorities)
  }
`);

const RenameFileMutation = graphql(`
  mutation RenameFile($uuid: UUID!, $index: Int!, $name: String!) {
    renameFile(uuid: $uuid, index: $index, name: $name)
  }
`);

const AddTrackerMutation = graphql(`
  mutation AddTracker($uuid: UUID!, $url: String!, $tier: Int!) {
    addTracker(uuid: $uuid, url: $url, tier: $tier)
  }
`);

const ReplaceTrackersMutation = graphql(`
  mutation ReplaceTrackers($uuid: UUID!, $trackers: [TrackerInput!]!) {
    replaceTrackers(uuid: $uuid, trackers: $trackers)
  }
`);

const ScrapeTrackerMutation = graphql(`
  mutation ScrapeTracker($uuid: UUID!, $trackerIndex: Int!) {
    scrapeTracker(uuid: $uuid, trackerIndex: $trackerIndex) {
      trackerUrl
      complete
      incomplete
    }
  }
`);

const AddUrlSeedMutation = graphql(`
  mutation AddUrlSeed($uuid: UUID!, $url: String!) {
    addUrlSeed(uuid: $uuid, url: $url)
  }
`);

const RemoveUrlSeedMutation = graphql(`
  mutation RemoveUrlSeed($uuid: UUID!, $url: String!) {
    removeUrlSeed(uuid: $uuid, url: $url)
  }
`);

const ConnectPeerMutation = graphql(`
  mutation ConnectPeer($uuid: UUID!, $address: String!) {
    connectPeer(uuid: $uuid, address: $address)
  }
`);

export const mutations = {
  pause: (uuid: string, graceful = false) => request(PauseTorrentMutation, { uuid, graceful }),
  resume: (uuid: string) => request(ResumeTorrentMutation, { uuid }),
  setFlags: (uuid: string, set: TorrentFlag[], unset: TorrentFlag[]) =>
    request(SetTorrentFlagsMutation, { uuid, set, unset }),
  remove: (uuid: string, deleteFiles: boolean) =>
    request(RemoveTorrentMutation, { uuid, deleteFiles }),
  recheck: (uuid: string) => request(ForceRecheckMutation, { uuid }),
  reannounce: (uuid: string, seconds = 0, trackerIndex = -1) =>
    request(ForceReannounceMutation, { uuid, seconds, trackerIndex }),
  dhtAnnounce: (uuid: string) => request(ForceDhtAnnounceMutation, { uuid }),
  clearError: (uuid: string) => request(ClearErrorMutation, { uuid }),
  flushCache: (uuid: string) => request(FlushCacheMutation, { uuid }),
  saveResumeData: (uuid: string) => request(SaveResumeDataMutation, { uuid }),
  moveStorage: (uuid: string, path: string, mode: MoveMode) =>
    // Waits server-side for confirmation, up to 10 minutes.
    request(MoveStorageMutation, { uuid, path, mode }, { timeoutMs: 610_000 }),
  queueTop: (uuid: string) => request(QueueTopMutation, { uuid }),
  queueUp: (uuid: string) => request(QueueUpMutation, { uuid }),
  queueDown: (uuid: string) => request(QueueDownMutation, { uuid }),
  queueBottom: (uuid: string) => request(QueueBottomMutation, { uuid }),
  setQueuePosition: (uuid: string, position: number) =>
    request(SetQueuePositionMutation, { uuid, position }),
  setLimits: (
    uuid: string,
    limits: {
      uploadLimit?: number;
      downloadLimit?: number;
      maxUploads?: number;
      maxConnections?: number;
    },
  ) => request(SetTorrentLimitsMutation, { uuid, ...limits }),
  addTorrent: (input: AddTorrentInput) => request(AddTorrentMutation, { input }),
  magnetUri: async (uuid: string) =>
    (await request(MagnetUriQuery, { uuid })).torrent?.magnetUri ?? null,
  pauseSession: () => request(PauseSessionMutation),
  resumeSession: () => request(ResumeSessionMutation),
  /** Always send the complete list: an omitted tail gets default 4. */
  setFilePriorities: (uuid: string, priorities: number[]) =>
    request(SetFilePrioritiesMutation, { uuid, priorities }),
  /** Waits server-side for rename confirmation (serialized per torrent). */
  renameFile: (uuid: string, index: number, name: string) =>
    request(RenameFileMutation, { uuid, index, name }),
  addTracker: (uuid: string, url: string, tier: number) =>
    request(AddTrackerMutation, { uuid, url, tier }),
  /** Replaces the full tracker list; an empty list removes all trackers. */
  replaceTrackers: (uuid: string, trackers: TrackerInput[]) =>
    request(ReplaceTrackersMutation, { uuid, trackers }),
  /** Waits up to 30 s for the tracker's scrape response. */
  scrapeTracker: (uuid: string, trackerIndex: number) =>
    request(ScrapeTrackerMutation, { uuid, trackerIndex }),
  addUrlSeed: (uuid: string, url: string) => request(AddUrlSeedMutation, { uuid, url }),
  removeUrlSeed: (uuid: string, url: string) => request(RemoveUrlSeedMutation, { uuid, url }),
  connectPeer: (uuid: string, address: string) => request(ConnectPeerMutation, { uuid, address }),
};

/** Sequentially apply one mutation to many torrents; collect failures. */
export async function bulk(
  uuids: readonly string[],
  fn: (uuid: string) => Promise<unknown>,
): Promise<{ ok: number; errors: { uuid: string; message: string }[] }> {
  let ok = 0;
  const errors: { uuid: string; message: string }[] = [];
  for (const uuid of uuids) {
    try {
      await fn(uuid);
      ok++;
    } catch (err) {
      errors.push({ uuid, message: err instanceof Error ? err.message : String(err) });
    }
  }
  return { ok, errors };
}
