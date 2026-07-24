// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

import { setToken } from './auth';
import { setEndpointSetting } from './endpoint';

/**
 * One-shot startup configuration via hash params: `#url=…&token=…`, or
 * combined with a route as `#/settings?url=…&token=…`. Lets a copy of the
 * UI served anywhere be pointed at a daemon (whose `cors` option must allow
 * the origin) without touching the login screen.
 *
 * `url` is the daemon's GraphQL endpoint (a bare daemon URL like
 * `http://host:3928` gets `/graphql` appended) and is persisted like the
 * login screen's endpoint override. `token` is kept for this tab only
 * (sessionStorage). Consumed params are stripped from the URL and history
 * so the token does not linger in the address bar.
 */
export function applyHashParams(): void {
  const hash = window.location.hash.replace(/^#/, '');
  if (hash === '') return;

  // `#/route?params` carries params after `?`; a hash not starting with
  // `/` is treated as params-only (`#url=…`). Anything else is a route.
  const queryStart = hash.indexOf('?');
  const route = queryStart >= 0 ? hash.slice(0, queryStart) : hash.startsWith('/') ? hash : '';
  const query = queryStart >= 0 ? hash.slice(queryStart + 1) : hash.startsWith('/') ? '' : hash;
  if (query === '') return;

  const params = new URLSearchParams(query);
  const url = params.get('url');
  const token = params.get('token');
  if (url === null && token === null) return;
  params.delete('url');
  params.delete('token');

  if (url !== null && url.trim() !== '') {
    setEndpointSetting(normalizeEndpoint(url.trim()));
  }
  if (token !== null && token !== '') {
    setToken(token, false);
  }

  const rest = params.toString();
  const newHash = route + (rest !== '' ? `?${rest}` : '');
  window.history.replaceState(
    null,
    '',
    window.location.pathname + window.location.search + (newHash !== '' ? `#${newHash}` : ''),
  );
}

/** A URL without a path is the daemon itself; the API lives on /graphql. */
function normalizeEndpoint(raw: string): string {
  try {
    const url = new URL(raw, window.location.href);
    if (url.pathname === '' || url.pathname === '/') url.pathname = '/graphql';
    return url.toString();
  } catch {
    return raw;
  }
}
