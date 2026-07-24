// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

import { endpointIdentity } from './endpoint';

/**
 * Bearer-token persistence. "Stay signed in" stores the token in
 * localStorage (survives restarts); otherwise sessionStorage (cleared with
 * the tab). When Web Storage is unavailable (private mode, quota) a
 * volatile in-memory copy keeps the session alive until reload. The token
 * is sent as `Authorization: Bearer` on HTTP and inside the graphql-ws
 * `connection_init` payload — never in URLs.
 *
 * Tokens are namespaced by the normalized endpoint they were entered for:
 * a token remembered for one daemon must never be sent to another (e.g.
 * after an `#url=…` endpoint override pointed the UI elsewhere).
 */
const TOKEN_KEY = 'rsbtd.token';

/** In-memory fallback, scoped to the endpoint it was entered for. */
let memoryToken: { key: string; token: string } | null = null;

/** The Web Storage key holding the current endpoint's token. */
export function tokenStorageKey(): string {
  return `${TOKEN_KEY}:${endpointIdentity()}`;
}

export function getToken(): string | null {
  const key = tokenStorageKey();
  try {
    const stored = sessionStorage.getItem(key) ?? localStorage.getItem(key);
    if (stored !== null) return stored;
  } catch {
    // fall through to the in-memory copy
  }
  return memoryToken !== null && memoryToken.key === key ? memoryToken.token : null;
}

export function setToken(token: string, remember: boolean): void {
  clearToken();
  memoryToken = { key: tokenStorageKey(), token };
  try {
    (remember ? localStorage : sessionStorage).setItem(tokenStorageKey(), token);
  } catch {
    // Storage unavailable: the in-memory copy carries the session.
  }
}

export function clearToken(): void {
  memoryToken = null;
  try {
    const key = tokenStorageKey();
    sessionStorage.removeItem(key);
    localStorage.removeItem(key);
    // Drop any legacy un-namespaced token too: its intended endpoint is
    // unknowable, so it must not outlive a sign-out.
    sessionStorage.removeItem(TOKEN_KEY);
    localStorage.removeItem(TOKEN_KEY);
  } catch {
    // ignore
  }
}
