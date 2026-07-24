// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

import type { TypedDocumentString } from '@/gen/gql/graphql';
import { getToken } from './auth';
import { httpEndpoint } from './endpoint';

/**
 * Minimal typed GraphQL-over-HTTP client (see codegen.ts: documents are
 * TypedDocumentString, i.e. plain strings carrying result/variable types).
 *
 * 64-bit note: responses are parsed with standard JSON, so integers above
 * 2^53 would lose precision — no realistic quantity does; see lib/safeInt.ts.
 */

export class AuthError extends Error {
  constructor() {
    super('unauthorized');
    this.name = 'AuthError';
  }
}

export class NetworkError extends Error {
  constructor(message: string) {
    super(message);
    this.name = 'NetworkError';
  }
}

/** GraphQL-level errors. Messages are human-readable only (no error codes). */
export class GqlError extends Error {
  readonly messages: readonly string[];
  constructor(messages: readonly string[]) {
    super(messages.join('; '));
    this.name = 'GqlError';
    this.messages = messages;
  }
}

export interface RequestOptions {
  signal?: AbortSignal;
  /** Nested live torrent fields can take up to 30 s server-side. */
  timeoutMs?: number;
}

interface GqlResponse<T> {
  data?: T | null;
  errors?: { message: string }[];
}

/** Notified on HTTP 401 so the connection manager can require login. */
let authErrorHandler: (() => void) | null = null;
export function setAuthErrorHandler(handler: (() => void) | null): void {
  authErrorHandler = handler;
}

async function post<TResult, TVariables>(
  doc: TypedDocumentString<TResult, TVariables>,
  variables: TVariables | undefined,
  opts: RequestOptions,
): Promise<GqlResponse<TResult>> {
  const { signal, timeoutMs = 35_000 } = opts;
  const timeout = AbortSignal.timeout(timeoutMs);
  const combined = signal ? AbortSignal.any([signal, timeout]) : timeout;

  const headers: Record<string, string> = { 'content-type': 'application/json' };
  const token = getToken();
  if (token !== null) headers.authorization = `Bearer ${token}`;

  let res: Response;
  try {
    res = await fetch(httpEndpoint(), {
      method: 'POST',
      headers,
      body: JSON.stringify({ query: doc.toString(), variables }),
      signal: combined,
    });
  } catch (err) {
    if (signal?.aborted) throw err;
    throw new NetworkError(err instanceof Error ? err.message : String(err));
  }

  if (res.status === 401) {
    authErrorHandler?.();
    throw new AuthError();
  }
  if (!res.ok) throw new NetworkError(`HTTP ${res.status}`);

  try {
    return (await res.json()) as GqlResponse<TResult>;
  } catch {
    throw new NetworkError('invalid JSON response');
  }
}

/** Strict request: throws GqlError on any GraphQL error. */
export async function request<TResult, TVariables>(
  doc: TypedDocumentString<TResult, TVariables>,
  variables?: TVariables,
  opts: RequestOptions = {},
): Promise<TResult> {
  const body = await post(doc, variables, opts);
  if (body.errors?.length || body.data == null) {
    throw new GqlError((body.errors ?? [{ message: 'empty response' }]).map((e) => e.message));
  }
  return body.data;
}

/**
 * Tolerant request for details tabs: nested live fields (pieces/files/
 * trackers/peers) are separate server-side requests that can fail
 * independently, yielding partial data alongside errors. A response with
 * no data at all is a plain failure and throws — resolving it would let
 * React Query replace a good previous snapshot with nothing.
 */
export async function requestTolerant<TResult, TVariables>(
  doc: TypedDocumentString<TResult, TVariables>,
  variables?: TVariables,
  opts: RequestOptions = {},
): Promise<{ data: TResult; errors: string[] }> {
  const body = await post(doc, variables, opts);
  if (body.data == null) {
    throw new GqlError((body.errors ?? [{ message: 'empty response' }]).map((e) => e.message));
  }
  return { data: body.data, errors: (body.errors ?? []).map((e) => e.message) };
}
