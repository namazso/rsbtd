// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

import { createClient, type Client } from 'graphql-ws';
import type { TypedDocumentString } from '@/gen/gql/graphql';
import { getToken } from './auth';
import { wsEndpoint } from './endpoint';

/**
 * graphql-ws socket factory. Auth rides in the connection_init payload
 * (browsers cannot set headers on WebSocket upgrades). Note the daemon
 * closes a badly-authenticated init with a generic code 1002 — the
 * connection manager classifies that via an HTTP probe, never via the
 * close code alone.
 */
export interface WsCallbacks {
  onConnected: () => void;
  onClosed: (event: unknown) => void;
}

/** How long an unanswered keep-alive ping may go without a pong. */
const PONG_TIMEOUT_MS = 10_000;

export function createWsClient(callbacks: WsCallbacks): Client {
  // Keep-alive pings only help if an unanswered one terminates the
  // connection: without this watchdog a half-open socket (or an upgrade
  // that stalls before the ack) would freeze live data forever instead
  // of degrading to polling.
  let activeSocket: WebSocket | null = null;
  let pongWait: ReturnType<typeof setTimeout> | null = null;
  const clearPongWait = () => {
    if (pongWait !== null) {
      clearTimeout(pongWait);
      pongWait = null;
    }
  };
  return createClient({
    url: wsEndpoint,
    lazy: false,
    keepAlive: 15_000,
    // A server that accepts the upgrade but never acks must not hang the
    // connection attempt indefinitely.
    connectionAckWaitTimeout: 10_000,
    retryAttempts: Infinity,
    shouldRetry: () => true,
    retryWait: async (retries) => {
      // Exponential backoff with jitter, capped at 30 s.
      const base = Math.min(1_000 * 2 ** retries, 30_000);
      const jittered = base * (0.7 + Math.random() * 0.6);
      await new Promise((resolve) => setTimeout(resolve, jittered));
    },
    connectionParams: () => {
      const token = getToken();
      return token !== null ? { token } : {};
    },
    on: {
      connected: (socket) => {
        activeSocket = socket as WebSocket;
        callbacks.onConnected();
      },
      ping: (received) => {
        if (received) return; // the peer's ping; we pong automatically
        clearPongWait();
        pongWait = setTimeout(() => {
          if (activeSocket !== null && activeSocket.readyState === WebSocket.OPEN) {
            // 4408 Request Timeout; the client's retry loop takes over.
            activeSocket.close(4408, 'Request Timeout');
          }
        }, PONG_TIMEOUT_MS);
      },
      pong: (received) => {
        if (received) clearPongWait();
      },
      closed: (event) => {
        clearPongWait();
        callbacks.onClosed(event);
      },
    },
  });
}

export interface SubscriptionHandlers<TResult> {
  next: (data: TResult) => void;
  /** Operation-level GraphQL errors (e.g. filtering an absent torrent). */
  onOperationError?: (errors: readonly { message: string }[]) => void;
  /**
   * Called when the operation ended (completed or errored) and a retry is
   * scheduled. The stream may have missed data — the daemon ends
   * subscriptions it could not keep up with.
   */
  onRetry?: () => void;
}

/**
 * Subscribe and keep the operation alive: graphql-ws re-establishes the
 * socket itself, but a completed/errored *operation* (engine shutdown ends
 * all streams; transient server errors) must be restarted by us. There is
 * no replay — the connection manager's resync covers any gap.
 */
export function subscribeRetrying<TResult, TVariables>(
  client: Client,
  doc: TypedDocumentString<TResult, TVariables>,
  variables: TVariables | undefined,
  handlers: SubscriptionHandlers<TResult>,
): () => void {
  let disposed = false;
  let unsubscribe: (() => void) | null = null;
  let retryTimer: ReturnType<typeof setTimeout> | null = null;

  const scheduleRetry = () => {
    if (disposed) return;
    retryTimer = setTimeout(run, 2_000);
  };

  const run = () => {
    if (disposed) return;
    unsubscribe = client.subscribe<TResult>(
      { query: doc.toString(), variables: variables as Record<string, unknown> | undefined },
      {
        next: (msg) => {
          if (msg.errors?.length) handlers.onOperationError?.(msg.errors);
          if (msg.data != null) handlers.next(msg.data);
        },
        error: () => {
          handlers.onRetry?.();
          scheduleRetry();
        },
        complete: () => {
          handlers.onRetry?.();
          scheduleRetry();
        },
      },
    );
  };

  run();

  return () => {
    disposed = true;
    if (retryTimer !== null) clearTimeout(retryTimer);
    unsubscribe?.();
  };
}
