// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

import { graphql } from '@/gen/gql';

/**
 * Per-torrent nested fields — each is a separate live request server-side
 * that can take up to 30 s and fail independently, so each tab
 * has its own document, is polled only while visible, and is fetched with
 * the tolerant client (partial data + errors).
 */
export const TorrentPiecesQuery = graphql(`
  query TorrentPieces($uuid: UUID!) {
    torrent(uuid: $uuid) {
      pieces(includeBitfield: true) {
        total
        have
        bitfield
      }
    }
  }
`);

export const TorrentFilesQuery = graphql(`
  query TorrentFiles($uuid: UUID!) {
    torrent(uuid: $uuid) {
      files {
        index
        path
        size
        offset
        priority
        progressBytes
        isPadFile
        isSymlink
        symlinkTarget
        isExecutable
        isHidden
      }
    }
  }
`);

export const TorrentTrackersQuery = graphql(`
  query TorrentTrackers($uuid: UUID!) {
    torrent(uuid: $uuid) {
      trackers {
        url
        trackerId
        tier
        failLimit
        verified
        source
      }
      urlSeeds
    }
  }
`);

export const TorrentPeersQuery = graphql(`
  query TorrentPeers($uuid: UUID!) {
    torrent(uuid: $uuid) {
      peers {
        address
        localEndpoint
        peerId
        client
        connectionType
        flags
        source
        progressPpm
        downSpeed
        upSpeed
        payloadDownSpeed
        payloadUpSpeed
        totalDownload
        totalUpload
        lastActiveUs
        lastRequestUs
        numHashfails
        failcount
        downloadRatePeak
        uploadRatePeak
        numPieces
        rtt
      }
    }
  }
`);
