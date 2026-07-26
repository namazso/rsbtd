// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

import { graphql } from '@/gen/gql';

/**
 * Every flat, one-dimensional Torrent field: any of these can be a table
 * column, a sort key, or a search-filter property, so the list query and the
 * torrentChanged subscription share this exact selection (rows stay
 * shape-identical). Deliberately excluded:
 *  - pieces/files/trackers/peers: separate live requests, per-torrent only;
 *  - magnetUri: derived string, fetched on demand for the copy action;
 *  - progress: redundant with progressPpm (progress = ppm / 1e6).
 */
export const TorrentListFields = graphql(`
  fragment TorrentListFields on Torrent {
    uuid
    infoHashV1
    infoHashV2
    name
    state
    progressPpm
    error {
      message
      file
    }
    savePath
    currentTracker
    nextAnnounceSeconds
    totalDownload
    totalUpload
    totalPayloadDownload
    totalPayloadUpload
    totalFailedBytes
    totalRedundantBytes
    totalDone
    totalSize
    totalWantedDone
    totalWanted
    allTimeUpload
    allTimeDownload
    addedTime
    completedTime
    lastSeenComplete
    storageMode
    queuePosition
    downloadRate
    uploadRate
    downloadPayloadRate
    uploadPayloadRate
    numSeeds
    numPeers
    numComplete
    numIncomplete
    listSeeds
    listPeers
    connectCandidates
    piecesHave
    distributedCopies
    blockSize
    numUploads
    numConnections
    uploadsLimit
    connectionsLimit
    uploadLimit
    downloadLimit
    upBandwidthQueue
    downBandwidthQueue
    seedRank
    needSaveResumeData
    isSeeding
    isFinished
    isPaused
    isAutoManaged
    hasMetadata
    sizeOnDisk
    pieceLength
    isPrivate
    isI2P
    hasIncoming
    movingStorage
    announcingToTrackers
    announcingToLsd
    announcingToDht
    flags
  }
`);

export const TorrentsQuery = graphql(`
  query Torrents {
    torrents {
      ...TorrentListFields
    }
  }
`);

export const TorrentByUuidQuery = graphql(`
  query TorrentByUuid($uuid: UUID!) {
    torrent(uuid: $uuid) {
      ...TorrentListFields
    }
  }
`);

/** ~1/s batches of changed torrents; subscribing activates the ticker. */
export const TorrentChangedSubscription = graphql(`
  subscription TorrentChanged {
    torrentChanged {
      ...TorrentListFields
    }
  }
`);

/**
 * Engine event bus. Events are hints (the bus holds 4096 entries, slow
 * consumers silently skip): row truth comes from torrentChanged.
 */
export const TorrentEventsSubscription = graphql(`
  subscription TorrentEvents {
    torrentEvents {
      __typename
      ... on TorrentAddedEvent {
        torrentUuid
      }
      ... on TorrentRemovedEvent {
        torrentUuid
      }
      ... on TorrentFinishedEvent {
        torrentUuid
      }
      ... on MetadataReceivedEvent {
        torrentUuid
      }
      ... on MetadataFailedEvent {
        torrentUuid
        error
      }
      ... on TorrentErrorEvent {
        torrentUuid
        error
        filename
      }
      ... on TorrentDeletedEvent {
        torrentUuid
      }
      ... on TorrentDeleteFailedEvent {
        torrentUuid
        error
      }
      ... on ResumeDataSavedEvent {
        torrentUuid
      }
      ... on ResumeDataFailedEvent {
        torrentUuid
        error
      }
      ... on FileRenamedEvent {
        torrentUuid
        fileIndex
        newName
      }
      ... on FileRenameFailedEvent {
        torrentUuid
        fileIndex
        error
      }
      ... on StorageMovedEvent {
        torrentUuid
        path
      }
      ... on StorageMovedFailedEvent {
        torrentUuid
        error
      }
      ... on ScrapeReplyEvent {
        torrentUuid
        trackerUrl
        complete
        incomplete
      }
      ... on ScrapeFailedEvent {
        torrentUuid
        trackerUrl
        error
      }
      ... on SessionErrorEvent {
        error
      }
    }
  }
`);
