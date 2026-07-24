// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

import { useQuery, useQueryClient } from '@tanstack/react-query';
import { request } from '@/api/client';
import { SessionQuery, VersionQuery } from '@/api/operations/session';
import { useConnection } from '@/store/connection';

/** Session-level info for the status bar / session pause toggle. */
export function useSession() {
  const state = useConnection((s) => s.state);
  const generation = useConnection((s) => s.generation);
  return useQuery({
    queryKey: ['session', generation],
    queryFn: () => request(SessionQuery),
    enabled: state === 'up' || state === 'degraded',
    refetchInterval: 10_000,
    select: (d) => d.session,
  });
}

export function useVersionInfo() {
  const state = useConnection((s) => s.state);
  const generation = useConnection((s) => s.generation);
  return useQuery({
    queryKey: ['version', generation],
    queryFn: () => request(VersionQuery),
    enabled: state === 'up' || state === 'degraded',
    staleTime: Infinity,
    select: (d) => d.version,
  });
}

export function useInvalidateSession() {
  const queryClient = useQueryClient();
  return () => void queryClient.invalidateQueries({ queryKey: ['session'] });
}
