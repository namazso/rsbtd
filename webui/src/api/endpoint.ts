// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

/**
 * GraphQL endpoint resolution.
 *
 * Default is same-origin `/graphql` — the daemon serves the built UI itself
 * via its `serve_root` option. When the UI is served from anywhere else, the
 * daemon's `cors` option must allow that origin, and the endpoint comes from
 * (highest priority first) the `url` hash param (see hashParams.ts), the
 * persisted override below (settable from the login screen), or — during
 * `npm run dev` only — the dev daemon URL injected by Vite.
 */
export const ENDPOINT_KEY = 'rsbtd.endpoint';
const DEFAULT_ENDPOINT = __DEV_GRAPHQL_URL__ ?? '/graphql';

export function getEndpointSetting(): string | null {
  try {
    return localStorage.getItem(ENDPOINT_KEY);
  } catch {
    return null;
  }
}

export function setEndpointSetting(value: string | null): void {
  try {
    if (value && value.trim() !== '' && value.trim() !== DEFAULT_ENDPOINT) {
      localStorage.setItem(ENDPOINT_KEY, value.trim());
    } else {
      localStorage.removeItem(ENDPOINT_KEY);
    }
  } catch {
    // Storage unavailable (private mode restrictions): fall back to default.
  }
}

/** HTTP(S) URL (or path) for POST requests. */
export function httpEndpoint(): string {
  return getEndpointSetting() ?? DEFAULT_ENDPOINT;
}

/**
 * The current endpoint's identity (origin + path), used to scope per-daemon
 * state such as stored tokens and remembered save paths.
 */
export function endpointIdentity(): string {
  const endpoint = httpEndpoint();
  try {
    const url = new URL(endpoint, window.location.href);
    return url.origin + url.pathname;
  } catch {
    return endpoint;
  }
}

/** ws(s):// URL for graphql-ws, derived from the HTTP endpoint. */
export function wsEndpoint(): string {
  const url = new URL(httpEndpoint(), window.location.href);
  url.protocol = url.protocol === 'https:' ? 'wss:' : 'ws:';
  return url.toString();
}
