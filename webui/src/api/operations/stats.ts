// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

import { graphql } from '@/gen/gql';

/**
 * Session statistics. Metric names and units are version-dependent and are
 * discovered at runtime — never hardcode the list; unknown names
 * in a subscription are silently omitted, which makes preset charts safe.
 */
export const SessionStatsQuery = graphql(`
  query SessionStats($names: [String!]) {
    sessionStats(names: $names) {
      name
      kind
      value
    }
  }
`);

export const SessionStatsStreamSubscription = graphql(`
  subscription SessionStatsStream($intervalMs: Int!, $names: [String!]) {
    sessionStats(intervalMs: $intervalMs, names: $names) {
      name
      kind
      value
    }
  }
`);
