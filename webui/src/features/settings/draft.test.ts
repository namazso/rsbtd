// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

import { describe, expect, it } from 'vitest';
import '@/i18n';
import {
  buildDelta,
  deepEqual,
  stripTypename,
  useSettingsDraft,
  type SettingsSnapshot,
} from './draft';
import { validateDelta } from './validate';

const snapshot = {
  uploadRateLimit: 0,
  downloadRateLimit: 1000,
  enableDht: true,
  userAgent: 'RSBTD',
  proxy: null,
  i2p: null,
  outgoingPortRange: { __typename: 'PortRange', first: 6881, last: 6889 },
  encryption: {
    __typename: 'EncryptionSettings',
    incoming: 'ENABLED',
    outgoing: 'ENABLED',
    methods: { __typename: 'EncryptionMethods', plaintext: true, rc4: true },
    preferRc4: false,
    announceSupport: true,
  },
  listenInterfaces: [
    { __typename: 'ListenInterface', interface: '0.0.0.0', port: 6881, ssl: false, local: false },
  ],
  outgoingInterfaces: [],
} as unknown as SettingsSnapshot;

describe('stripTypename / deepEqual', () => {
  it('removes __typename recursively', () => {
    const stripped = stripTypename(snapshot.encryption);
    expect(JSON.stringify(stripped)).not.toContain('__typename');
    expect(stripped.methods.plaintext).toBe(true);
  });

  it('deepEqual compares structurally', () => {
    expect(deepEqual({ a: [1, { b: 2 }] }, { a: [1, { b: 2 }] })).toBe(true);
    expect(deepEqual({ a: 1 }, { a: 2 })).toBe(false);
    expect(deepEqual([1], [1, 2])).toBe(false);
  });
});

describe('buildDelta', () => {
  it('includes only changed fields; reverts drop out', () => {
    const delta = buildDelta(snapshot, {
      uploadRateLimit: 500_000, // changed
      downloadRateLimit: 1000, // unchanged -> dropped
      enableDht: true, // unchanged -> dropped
    });
    expect(delta).toEqual({ uploadRateLimit: 500_000 });
  });

  it('sends whole groups without __typename', () => {
    const edited = stripTypename(snapshot.encryption);
    edited.preferRc4 = true;
    const delta = buildDelta(snapshot, { encryption: edited });
    expect(delta.encryption).toEqual({
      incoming: 'ENABLED',
      outgoing: 'ENABLED',
      methods: { plaintext: true, rc4: true },
      preferRc4: true,
      announceSupport: true,
    });
  });

  it('group set to identical content is dropped', () => {
    const delta = buildDelta(snapshot, { encryption: stripTypename(snapshot.encryption) });
    expect(delta).toEqual({});
  });

  it('null disables only the nullable groups', () => {
    const delta = buildDelta(snapshot, {
      outgoingPortRange: null, // nullable-disable group, was set
      enableDht: null, // scalar: null forbidden -> dropped
      proxy: null, // already null -> unchanged -> dropped
    });
    expect(delta).toEqual({ outgoingPortRange: null });
  });

  it('replaces list groups wholesale', () => {
    const lists = [
      { interface: '0.0.0.0', port: 6881, ssl: false, local: false },
      { interface: 'eth0', port: 0, ssl: false, local: true },
    ];
    const delta = buildDelta(snapshot, { listenInterfaces: lists });
    expect(delta.listenInterfaces).toEqual(lists);
  });

  it('ignores unknown field names', () => {
    expect(buildDelta(snapshot, { bogus: 1 })).toEqual({});
  });
});

describe('draft store', () => {
  it('ensureScope keeps the draft within a scope and clears it across scopes', () => {
    const store = useSettingsDraft.getState();
    store.ensureScope('http://a/graphql');
    store.setField('uploadRateLimit', 5);
    store.ensureScope('http://a/graphql');
    expect(useSettingsDraft.getState().draft).toEqual({ uploadRateLimit: 5 });
    store.ensureScope('http://b/graphql');
    expect(useSettingsDraft.getState().draft).toEqual({});
  });

  it('pruneApplied drops only entries still equal to the submitted values', () => {
    const store = useSettingsDraft.getState();
    store.reset();
    store.setField('uploadRateLimit', 500);
    store.setField('downloadRateLimit', 800);
    store.setField('outgoingPortRange', { first: 100, last: 200 });
    store.pruneApplied({
      uploadRateLimit: 500,
      downloadRateLimit: 900,
      outgoingPortRange: { first: 100, last: 200 },
    });
    expect(useSettingsDraft.getState().draft).toEqual({ downloadRateLimit: 800 });
  });
});

describe('validateDelta', () => {
  it('checks ports and ranges', () => {
    expect(validateDelta({ outgoingPortRange: { first: 0, last: 70000 } }).length).toBe(2);
    expect(validateDelta({ outgoingPortRange: { first: 200, last: 100 } }).length).toBe(1);
    expect(validateDelta({ outgoingPortRange: { first: 100, last: 200 } })).toEqual([]);
  });

  it('allows listen port 0 but rejects bad tokens / raw IPv6', () => {
    const ok = validateDelta({
      listenInterfaces: [{ interface: '[2001:db8::1]', port: 0, ssl: false, local: false }],
    });
    expect(ok).toEqual([]);
    const bad = validateDelta({
      listenInterfaces: [
        { interface: '2001:db8::1', port: 6881, ssl: false, local: false },
        { interface: 'has space', port: 6881, ssl: false, local: false },
      ],
    });
    expect(bad.length).toBe(2);
  });

  it('enforces the proxy protocol/credential matrix', () => {
    const base = {
      hostname: 'proxy.example',
      port: 1080,
      resolveHostnames: false,
      peerConnections: true,
      trackerConnections: true,
      socks5UdpSendLocalEndpoint: false,
      sendHostnameInConnect: false,
    };
    expect(
      validateDelta({ proxy: { ...base, protocol: 'SOCKS4', username: 'u', password: '' } }),
    ).toEqual([]);
    expect(
      validateDelta({ proxy: { ...base, protocol: 'SOCKS4', username: '', password: '' } }).length,
    ).toBe(1);
    expect(
      validateDelta({ proxy: { ...base, protocol: 'SOCKS5', username: 'u', password: '' } }).length,
    ).toBe(1);
    expect(
      validateDelta({
        proxy: { ...base, protocol: 'SOCKS5_PASSWORD', username: 'u', password: '' },
      }).length,
    ).toBe(1);
    expect(
      validateDelta({
        proxy: { ...base, protocol: 'HTTP_PASSWORD', username: 'u', password: 'p' },
      }),
    ).toEqual([]);
    expect(
      validateDelta({
        proxy: { ...base, protocol: 'SOCKS4', username: 'u', password: '', resolveHostnames: true },
      }).length,
    ).toBe(1);
  });

  it('encryption needs a method; preferRc4 needs both', () => {
    const enc = (plaintext: boolean, rc4: boolean, preferRc4: boolean) => ({
      incoming: 'ENABLED',
      outgoing: 'ENABLED',
      methods: { plaintext, rc4 },
      preferRc4,
      announceSupport: true,
    });
    expect(validateDelta({ encryption: enc(false, false, false) }).length).toBe(1);
    expect(validateDelta({ encryption: enc(true, false, true) }).length).toBe(1);
    expect(validateDelta({ encryption: enc(true, true, true) })).toEqual([]);
  });

  it('i2p tunnel ranges', () => {
    const i2p = (tunnels: number, hops: number, hopVariance: number) => ({
      hostname: 'sam.local',
      port: 7656,
      allowMixed: false,
      inbound: { tunnels, hops, hopVariance },
      outbound: { tunnels: 3, hops: 3, hopVariance: 0 },
    });
    expect(validateDelta({ i2p: i2p(3, 3, 0) })).toEqual([]);
    expect(validateDelta({ i2p: i2p(0, 3, 0) }).length).toBe(1);
    expect(validateDelta({ i2p: i2p(3, 8, 0) }).length).toBe(1);
    expect(validateDelta({ i2p: i2p(3, 3, 9) }).length).toBe(1);
  });

  it('rejects the read-only user agent', () => {
    expect(validateDelta({ userAgent: 'UNRECOGNIZED' }).length).toBe(1);
    expect(validateDelta({ userAgent: 'QBITTORRENT' })).toEqual([]);
  });
});
