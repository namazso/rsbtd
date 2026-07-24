// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

import { graphql } from '@/gen/gql';

/** Daemon + libtorrent version; also used as the authenticated HTTP probe. */
export const VersionQuery = graphql(`
  query Version {
    version {
      daemon
      libtorrent
    }
  }
`);

/** Session-level state for the status bar and the session pause toggle. */
export const SessionQuery = graphql(`
  query Session {
    session {
      isPaused
      isListening
      isDhtRunning
      listenPort
      sslListenPort
      torrentCount
    }
  }
`);
