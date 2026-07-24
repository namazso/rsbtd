// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

import { SETTINGS_CATALOG } from '@/gen/settings-catalog';

/**
 * Curated settings pages: hand-organized common fields with translated
 * labels; every remaining catalog field lands in the auto-generated
 * "Advanced" section (coverage-tested). Section ids are route segments
 * (#/settings/:section) and i18n keys (settings:sections.<id>).
 */
export interface CuratedSection {
  id: string;
  fields: string[];
}

export const CURATED_SECTIONS: CuratedSection[] = [
  {
    id: 'speed',
    fields: [
      'uploadRateLimit',
      'downloadRateLimit',
      'dhtUploadRateLimit',
      'rateLimitIpOverhead',
      'mixedModeAlgorithm',
    ],
  },
  {
    id: 'connection',
    fields: [
      'connectionsLimit',
      'connectionsSlack',
      'peerTransports',
      'outgoingInterfaces',
      'outgoingPortRange',
      'anonymousMode',
      'maxFailcount',
      'minReconnectTime',
      'peerConnectTimeout',
      'connectionSpeed',
      'enableIpNotifier',
    ],
  },
  {
    id: 'network',
    fields: [
      'listenInterfaces',
      'listenSystemPortFallback',
      'maxRetryPortBind',
      'enableUpnp',
      'enableNatpmp',
      'natpmpGateway',
      'upnpIgnoreNonrouters',
      'upnpLeaseDuration',
      'natpmpLeaseDuration',
      'announceIp',
      'announcePort',
    ],
  },
  {
    id: 'bittorrent',
    fields: [
      'enableDht',
      'enableLsd',
      'useDhtAsFallback',
      'dhtBootstrapNodes',
      'dhtAnnounceInterval',
      'encryption',
      'chokingAlgorithm',
      'seedChokingAlgorithm',
      'unchokeSlotsLimit',
      'optimisticUnchokeInterval',
      'numOptimisticUnchokeSlots',
      'suggestMode',
      'maxPexPeers',
      'userAgent',
      'handshakeClientVersion',
      'peerFingerprint',
      'alwaysSendUserAgent',
    ],
  },
  {
    id: 'queueing',
    fields: [
      'activeDownloads',
      'activeSeeds',
      'activeChecking',
      'activeLimit',
      'activeDhtLimit',
      'activeTrackerLimit',
      'activeLsdLimit',
      'dontCountSlowTorrents',
      'inactiveDownRate',
      'inactiveUpRate',
      'autoManageInterval',
      'autoManageStartup',
      'autoManagePreferSeeds',
      'incomingStartsQueuedTorrents',
      'seedTimeLimit',
      'seedTimeRatioLimit',
      'shareRatioLimit',
    ],
  },
  {
    id: 'trackers',
    fields: [
      'announceToAllTiers',
      'announceToAllTrackers',
      'preferUdpTrackers',
      'maxConcurrentHttpAnnounces',
      'trackerCompletionTimeout',
      'trackerReceiveTimeout',
      'stopTrackerTimeout',
      'trackerBackoff',
      'minAnnounceInterval',
      'autoScrapeInterval',
      'autoScrapeMinInterval',
      'validateHttpsTrackers',
      'applyIpFilterToTrackers',
    ],
  },
  {
    id: 'proxy',
    fields: ['proxy', 'i2p'],
  },
  {
    id: 'disk',
    fields: [
      'diskIoCache',
      'mmapWriteMode',
      'mmapFileSizeCutoff',
      'hashingThreads',
      'aioThreads',
      'checkingMemUsage',
      'maxQueuedDiskBytes',
      'filePoolSize',
      'closeFileInterval',
      'noAtimeStorage',
      'diskDisableCopyOnWrite',
      'optimisticDiskRetry',
    ],
  },
];

const curatedSet = new Set(CURATED_SECTIONS.flatMap((s) => s.fields));

/** Everything not curated, alphabetical — the "Advanced" section. */
export const ADVANCED_FIELDS: string[] = SETTINGS_CATALOG.map((e) => e.name)
  .filter((name) => !curatedSet.has(name))
  .sort((a, b) => a.localeCompare(b));

export const SECTION_IDS = [...CURATED_SECTIONS.map((s) => s.id), 'advanced', 'session'] as const;
