// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

import { useQuery } from '@tanstack/react-query';
import { requestTolerant } from '@/api/client';
import {
  TorrentFilesQuery,
  TorrentPeersQuery,
  TorrentPiecesQuery,
  TorrentTrackersQuery,
} from '@/api/operations/details';
import { useConnection } from '@/store/connection';
import type { TypedDocumentString } from '@/gen/gql/graphql';

/**
 * Visible-only polling with an in-flight overlap guard (nested live fields
 * can take up to 30 s server-side); tolerant partial data keeps stale
 * content on screen with an inline error instead of blanking the tab.
 */
function useDetailQuery<TResult>(
  part: string,
  doc: TypedDocumentString<TResult, { uuid: string }>,
  uuid: string,
  visible: boolean,
  intervalMs: number,
) {
  const state = useConnection((s) => s.state);
  const generation = useConnection((s) => s.generation);
  return useQuery({
    queryKey: ['torrent', uuid, part, generation],
    queryFn: ({ signal }) => requestTolerant(doc, { uuid }, { signal }),
    enabled: visible && (state === 'up' || state === 'degraded'),
    refetchInterval: (query) => (query.state.fetchStatus === 'fetching' ? false : intervalMs),
    // Carry data across refetches/reconnects of the SAME torrent only.
    // A blanket keepPreviousData would show torrent A's files/trackers
    // while B loads — with every action closure already bound to B's
    // uuid, sending A's row actions to B.
    placeholderData: (prev, prevQuery) =>
      prevQuery !== undefined && prevQuery.queryKey[1] === uuid ? prev : undefined,
  });
}

export function usePieces(uuid: string, visible: boolean) {
  return useDetailQuery('pieces', TorrentPiecesQuery, uuid, visible, 5_000);
}
export function useFiles(uuid: string, visible: boolean) {
  return useDetailQuery('files', TorrentFilesQuery, uuid, visible, 5_000);
}
export function useTrackers(uuid: string, visible: boolean) {
  return useDetailQuery('trackers', TorrentTrackersQuery, uuid, visible, 5_000);
}
export function usePeers(uuid: string, visible: boolean) {
  return useDetailQuery('peers', TorrentPeersQuery, uuid, visible, 4_000);
}
