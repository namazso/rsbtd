// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

import { graphql } from '@/gen/gql';

export const CreateJobFields = graphql(`
  fragment CreateJobFields on CreateJob {
    id
    state
    piecesDone
    piecesTotal
    error
    hasTorrentData
    outputPath
  }
`);

/** The full .torrent payload, fetched on demand (download/seed buttons). */
export const CreateJobTorrentDataQuery = graphql(`
  query CreateJobTorrentData($id: Int!) {
    createJob(id: $id) {
      torrentData
    }
  }
`);

export const StartCreateTorrentMutation = graphql(`
  mutation StartCreateTorrent($input: CreateTorrentInput!) {
    startCreateTorrent(input: $input) {
      ...CreateJobFields
    }
  }
`);

export const CreateJobsQuery = graphql(`
  query CreateJobs {
    createJobs {
      ...CreateJobFields
    }
  }
`);

/** Current snapshot immediately, then changes; completes at terminal state. */
export const CreateJobProgressSubscription = graphql(`
  subscription CreateJobProgress($id: Int!) {
    createJobProgress(id: $id) {
      ...CreateJobFields
    }
  }
`);

export const CancelCreateJobMutation = graphql(`
  mutation CancelCreateJob($id: Int!) {
    cancelCreateJob(id: $id)
  }
`);
