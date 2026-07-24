// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

import { create } from 'zustand';
import { getToken, tokenStorageKey } from '@/api/auth';
import { AuthError, request, setAuthErrorHandler } from '@/api/client';
import { ENDPOINT_KEY } from '@/api/endpoint';
import { VersionQuery } from '@/api/operations/session';
import { createWsClient } from '@/api/ws';
import type { Client } from 'graphql-ws';

/**
 * Connection lifecycle:
 *
 *   idle → probing → (authRequired | connecting) → up ⇄ degraded → down
 *
 * - probing: HTTP `version` query classifies reachability/auth. (The
 *   unauthenticated /healthz route exists but is typically not forwarded by
 *   reverse proxies that only expose /graphql, so we classify with GraphQL.)
 * - up: graphql-ws acknowledged; subscriptions live.
 * - degraded: HTTP works but the WebSocket doesn't (proxy stripping
 *   upgrades, or repeated pre-ack closes without a 401) — live.ts falls
 *   back to polling while the ws client keeps retrying.
 * - down: HTTP unreachable; periodic re-probe with backoff.
 *
 * Caveat: a badly-authenticated ws init closes with a generic
 * code 1002 — never classify a ws failure by close code, always re-probe
 * over HTTP.
 */
export type ConnectionState =
  'idle' | 'probing' | 'authRequired' | 'connecting' | 'up' | 'degraded' | 'down';

interface ConnectionSnapshot {
  state: ConnectionState;
  /**
   * Identifies one usable daemon connection: bumped on every ws ack, and
   * on the first HTTP-degraded classification after an explicit
   * endpoint/token change (which may point at a different daemon even
   * though the ws never comes up). Generation-scoped caches and live.ts
   * resync when it changes.
   */
  generation: number;
  /** Last probe/connection error (display only). */
  error: string | null;
  /** The daemon rejected the presented credentials (login page alert). */
  authFailed: boolean;
  /** Fatal session-level error reported by the daemon (banner). */
  sessionError: string | null;
}

export const useConnection = create<ConnectionSnapshot>(() => ({
  state: 'idle',
  generation: 0,
  error: null,
  authFailed: false,
  sessionError: null,
}));

const setSnap = useConnection.setState;

export function setSessionError(message: string | null): void {
  setSnap({ sessionError: message });
}

class ConnectionController {
  private ws: Client | null = null;
  /** Whether the current socket ever got a connection ack. */
  private acked = false;
  private classifying = false;
  private retryTimer: ReturnType<typeof setTimeout> | null = null;
  private downBackoffMs = 2_000;
  private started = false;
  /**
   * Bumped by every teardown and connect attempt. Async continuations
   * (probes, ws callbacks) capture the epoch they belong to and bail
   * when superseded, so a socket torn down during a reconnect can never
   * classify, degrade, or tear down its replacement.
   */
  private epoch = 0;
  /** Aborts the in-flight probe of a superseded attempt. */
  private probeAbort: AbortController | null = null;
  /**
   * Set by an explicit endpoint/token change: the next usable state may
   * belong to a different daemon, so generation-scoped state must roll
   * over even if the WebSocket never acks (HTTP-degraded mode).
   */
  private pendingIdentityBump = false;

  /** Current ws client for subscriptions (valid while state is up). */
  get client(): Client | null {
    return this.ws;
  }

  start(): void {
    if (this.started) return;
    this.started = true;
    // Another tab rewrote the endpoint or this endpoint's credentials:
    // adopt the new identity with a full reconnect instead of letting
    // requests silently drift to a different daemon mid-session.
    window.addEventListener('storage', (e) => {
      if (e.key === null || e.key === ENDPOINT_KEY || e.key === tokenStorageKey()) {
        this.reconnect();
      }
    });
    void this.connect();
  }

  /** After the login form saved new credentials / endpoint. */
  reconnect(): void {
    this.pendingIdentityBump = true;
    this.teardown();
    void this.connect();
  }

  logout(): void {
    this.teardown();
    setSnap({ state: 'authRequired', error: null, authFailed: false });
  }

  /** HTTP layer saw a 401 mid-session (token revoked / daemon restarted). */
  authLost(): void {
    if (useConnection.getState().state === 'authRequired') return;
    const rejected = getToken() !== null;
    this.teardown();
    setSnap({ state: 'authRequired', error: null, authFailed: rejected });
  }

  private async connect(): Promise<void> {
    this.clearRetry();
    const epoch = ++this.epoch;
    this.probeAbort?.abort();
    const abort = new AbortController();
    this.probeAbort = abort;
    setSnap({ state: 'probing', error: null, authFailed: false });
    try {
      await request(VersionQuery, undefined, { timeoutMs: 10_000, signal: abort.signal });
    } catch (err) {
      if (epoch !== this.epoch) return; // superseded while probing
      if (err instanceof AuthError) {
        setSnap({ state: 'authRequired', authFailed: getToken() !== null });
        return;
      }
      this.toDown(err);
      return;
    }
    if (epoch !== this.epoch) return;
    this.openWs(epoch);
  }

  private openWs(epoch: number): void {
    setSnap({ state: 'connecting' });
    this.acked = false;
    const client: Client = createWsClient({
      onConnected: () => {
        if (epoch !== this.epoch || client !== this.ws) return;
        this.acked = true;
        this.downBackoffMs = 2_000;
        this.pendingIdentityBump = false;
        // A new session: whatever the previous one reported is stale.
        setSnap((prev) => ({
          state: 'up',
          generation: prev.generation + 1,
          error: null,
          sessionError: null,
        }));
      },
      onClosed: () => {
        if (epoch !== this.epoch || client !== this.ws) return;
        void this.onWsClosed(epoch, client);
      },
    });
    this.ws = client;
  }

  private async onWsClosed(epoch: number, client: Client): Promise<void> {
    const state = useConnection.getState().state;
    if (state === 'authRequired' || state === 'down') return;

    if (this.acked) {
      // Was live; the client retries the socket itself. Poll meanwhile.
      this.acked = false;
      setSnap({ state: 'degraded' });
      return;
    }

    // Closed before ack (possibly bad auth, blocked upgrade, or daemon gone):
    // classify over HTTP, at most one probe in flight.
    if (this.classifying) return;
    this.classifying = true;
    try {
      await request(VersionQuery, undefined, { timeoutMs: 10_000 });
      if (epoch !== this.epoch || client !== this.ws) return;
      // HTTP fine, ws not: degraded; the ws client keeps retrying. An
      // endpoint change that lands here never gets a ws ack, so this is
      // where its generation-scoped state must roll over.
      if (useConnection.getState().state !== 'up') {
        if (this.pendingIdentityBump) {
          this.pendingIdentityBump = false;
          setSnap((prev) => ({
            state: 'degraded',
            generation: prev.generation + 1,
            sessionError: null,
          }));
        } else {
          setSnap({ state: 'degraded' });
        }
      }
    } catch (err) {
      if (epoch !== this.epoch || client !== this.ws) return;
      if (err instanceof AuthError) {
        this.teardown();
        setSnap({ state: 'authRequired', error: null, authFailed: getToken() !== null });
      } else {
        this.toDown(err);
      }
    } finally {
      this.classifying = false;
    }
  }

  private toDown(err: unknown): void {
    this.teardown();
    setSnap({
      state: 'down',
      error: err instanceof Error ? err.message : String(err),
    });
    this.retryTimer = setTimeout(() => void this.connect(), this.downBackoffMs);
    this.downBackoffMs = Math.min(this.downBackoffMs * 2, 30_000);
  }

  private teardown(): void {
    this.epoch++;
    this.clearRetry();
    this.probeAbort?.abort();
    this.probeAbort = null;
    const ws = this.ws;
    this.ws = null;
    this.acked = false;
    if (ws) void ws.dispose();
  }

  private clearRetry(): void {
    if (this.retryTimer !== null) {
      clearTimeout(this.retryTimer);
      this.retryTimer = null;
    }
  }
}

export const connection = new ConnectionController();

// HTTP 401 anywhere → require login.
setAuthErrorHandler(() => connection.authLost());
