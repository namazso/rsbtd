// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { getToken, setToken } from './auth';
import { getEndpointSetting } from './endpoint';
import { applyHashParams } from './hashParams';

function setHash(hash: string) {
  window.history.replaceState(null, '', `${window.location.pathname}${hash}`);
}

describe('applyHashParams', () => {
  beforeEach(() => {
    localStorage.clear();
    sessionStorage.clear();
    setHash('');
  });
  afterEach(() => {
    localStorage.clear();
    sessionStorage.clear();
    setHash('');
  });

  it('does nothing without params', () => {
    setHash('#/settings/network');
    applyHashParams();
    expect(getToken()).toBeNull();
    expect(getEndpointSetting()).toBeNull();
    expect(window.location.hash).toBe('#/settings/network');
  });

  it('consumes bare params and strips them from the URL', () => {
    setHash('#url=http%3A%2F%2F127.0.0.1%3A3928%2Fgraphql&token=s3cret');
    applyHashParams();
    expect(getEndpointSetting()).toBe('http://127.0.0.1:3928/graphql');
    expect(getToken()).toBe('s3cret');
    // Session-only: the token must not survive the tab.
    expect(Object.keys(localStorage).some((k) => k.startsWith('rsbtd.token'))).toBe(false);
    expect(window.location.hash).toBe('');
  });

  it('does not carry a remembered token to a different endpoint', () => {
    // Sign in against the default endpoint with "stay signed in".
    setToken('s3cret', true);
    expect(getToken()).toBe('s3cret');

    // An #url override pointing the UI at another daemon must not send
    // the remembered token there.
    setHash('#url=https%3A%2F%2Fattacker.example%2Fgraphql');
    applyHashParams();
    expect(getEndpointSetting()).toBe('https://attacker.example/graphql');
    expect(getToken()).toBeNull();
  });

  it('appends /graphql to a bare daemon URL', () => {
    setHash('#url=http://127.0.0.1:3928');
    applyHashParams();
    expect(getEndpointSetting()).toBe('http://127.0.0.1:3928/graphql');
  });

  it('preserves the route and unrelated params', () => {
    setHash('#/torrent/abc?token=t0k&foo=bar');
    applyHashParams();
    expect(getToken()).toBe('t0k');
    expect(window.location.hash).toBe('#/torrent/abc?foo=bar');
  });

  it('takes token alone without touching the endpoint', () => {
    setHash('#token=only');
    applyHashParams();
    expect(getToken()).toBe('only');
    expect(getEndpointSetting()).toBeNull();
    expect(window.location.hash).toBe('');
  });
});
