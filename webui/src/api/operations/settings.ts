// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

import { graphql } from '@/gen/gql';

/**
 * Complete Settings selection — every one of the 191 fields. Shared by the
 * read query and applySettings (whose response carries the full effective
 * settings after library normalization). A tripwire test asserts this
 * fragment covers every settings-catalog entry.
 */
export const SettingsValues = graphql(`
  fragment SettingsValues on Settings {
    announceIp
    handshakeClientVersion
    peerFingerprint
    natpmpGateway
    allowMultipleConnectionsPerIp
    sendRedundantHave
    useDhtAsFallback
    upnpIgnoreNonrouters
    useParoleMode
    autoManagePreferSeeds
    dontCountSlowTorrents
    closeRedundantConnections
    prioritizePartialPieces
    rateLimitIpOverhead
    announceToAllTiers
    announceToAllTrackers
    preferUdpTrackers
    noAtimeStorage
    incomingStartsQueuedTorrents
    reportTrueDownloaded
    strictEndGameMode
    noRecheckIncompleteResume
    anonymousMode
    reportWebSeedDownloads
    seedingOutgoingConnections
    noConnectPrivilegedPorts
    smoothConnects
    alwaysSendUserAgent
    applyIpFilterToTrackers
    banWebSeeds
    supportShareMode
    reportRedundantBytes
    listenSystemPortFallback
    enableUpnp
    enableNatpmp
    enableLsd
    enableDht
    autoSequential
    enableIpNotifier
    dhtPreferVerifiedNodeIds
    dhtRestrictRoutingIps
    dhtRestrictSearchIps
    dhtExtendedRoutingTable
    dhtAggressiveLookups
    dhtPrivacyLookups
    dhtEnforceNodeId
    dhtIgnoreDarkInternet
    dhtReadOnly
    pieceExtentAffinity
    validateHttpsTrackers
    ssrfMitigation
    allowIdna
    diskDisableCopyOnWrite
    allowMultipleConnectionsPerPid
    trackerCompletionTimeout
    trackerReceiveTimeout
    stopTrackerTimeout
    pieceTimeout
    requestTimeout
    requestQueueTime
    maxAllowedInRequestQueue
    maxOutRequestQueue
    wholePiecesThreshold
    peerTimeout
    urlseedTimeout
    urlseedWaitRetry
    filePoolSize
    maxFailcount
    minReconnectTime
    peerConnectTimeout
    connectionSpeed
    inactivityTimeout
    unchokeInterval
    optimisticUnchokeInterval
    numWant
    initialPickerThreshold
    allowedFastSetSize
    maxQueuedDiskBytes
    handshakeTimeout
    sendBufferLowWatermark
    sendBufferWatermark
    sendBufferWatermarkFactor
    peerDscp
    activeDownloads
    activeSeeds
    activeChecking
    activeDhtLimit
    activeTrackerLimit
    activeLsdLimit
    activeLimit
    autoManageInterval
    seedTimeLimit
    autoScrapeInterval
    autoScrapeMinInterval
    maxPeerlistSize
    maxPausedPeerlistSize
    minAnnounceInterval
    autoManageStartup
    seedingPieceQuota
    recvSocketBufferSize
    sendSocketBufferSize
    maxPeerRecvBufferSize
    optimisticDiskRetry
    maxSuggestPieces
    localServiceAnnounceInterval
    dhtAnnounceInterval
    udpTrackerTokenExpiry
    numOptimisticUnchokeSlots
    maxPexPeers
    tickInterval
    shareModeTarget
    uploadRateLimit
    downloadRateLimit
    dhtUploadRateLimit
    unchokeSlotsLimit
    connectionsLimit
    connectionsSlack
    utpTargetDelay
    utpGainFactor
    utpMinTimeout
    utpSynResends
    utpFinResends
    utpNumResends
    utpConnectTimeout
    utpLossMultiplier
    listenQueueSize
    torrentConnectBoost
    maxMetadataSize
    hashingThreads
    checkingMemUsage
    predictivePieceAnnounce
    aioThreads
    trackerBackoff
    shareRatioLimit
    seedTimeRatioLimit
    peerTurnover
    peerTurnoverCutoff
    peerTurnoverInterval
    connectSeedEveryNDownload
    maxHttpRecvBufferSize
    maxRetryPortBind
    inactiveDownRate
    inactiveUpRate
    urlseedMaxRequestBytes
    webSeedNameLookupRetry
    closeFileInterval
    utpCwndReduceTimer
    maxWebSeedConnections
    resolverCacheTimeout
    sendNotSentLowWatermark
    rateChokerInitialThreshold
    upnpLeaseDuration
    maxConcurrentHttpAnnounces
    dhtMaxPeersReply
    dhtSearchBranching
    dhtMaxFailCount
    dhtMaxTorrents
    dhtMaxDhtItems
    dhtMaxPeers
    dhtBlockTimeout
    dhtBlockRatelimit
    dhtItemLifetime
    dhtSampleInfohashesInterval
    dhtMaxInfohashesSampleCount
    maxPieceCount
    metadataTokenLimit
    mmapFileSizeCutoff
    announcePort
    natpmpLeaseDuration
    userAgent
    proxy {
      protocol
      hostname
      port
      username
      password
      resolveHostnames
      peerConnections
      trackerConnections
      socks5UdpSendLocalEndpoint
      sendHostnameInConnect
    }
    i2p {
      hostname
      port
      allowMixed
      inbound {
        tunnels
        hops
        hopVariance
      }
      outbound {
        tunnels
        hops
        hopVariance
      }
    }
    encryption {
      incoming
      outgoing
      methods {
        plaintext
        rc4
      }
      preferRc4
      announceSupport
    }
    peerTransports {
      tcp {
        incoming
        outgoing
      }
      utp {
        incoming
        outgoing
      }
    }
    outgoingPortRange {
      first
      last
    }
    diskIoCache {
      read
      write
    }
    mmapWriteMode
    suggestMode
    chokingAlgorithm
    seedChokingAlgorithm
    mixedModeAlgorithm
    outgoingInterfaces
    listenInterfaces {
      interface
      port
      ssl
      local
    }
    dhtBootstrapNodes {
      hostname
      port
    }
  }
`);

export const AllSettingsQuery = graphql(`
  query AllSettings {
    settings {
      ...SettingsValues
    }
  }
`);

export const ApplySettingsMutation = graphql(`
  mutation ApplySettings($input: SettingsInput!) {
    applySettings(input: $input) {
      ...SettingsValues
    }
  }
`);

export const ReopenNetworkSocketsMutation = graphql(`
  mutation ReopenNetworkSockets($mapPorts: Boolean!) {
    reopenNetworkSockets(mapPorts: $mapPorts)
  }
`);
