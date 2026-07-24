// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

import { tDynamic } from '@/lib/i18nDynamic';

/**
 * Client-side pre-validation (the daemon stays authoritative — any
 * invalid field rejects the whole delta atomically).
 */
export interface SettingsIssue {
  field: string;
  message: string;
}

const msg = (key: string, options?: Record<string, unknown>) =>
  tDynamic(`settings:validation.${key}`, options);

function isBadToken(token: string): boolean {
  return token === '' || /[\s,]/.test(token);
}

/** IPv6 literals must be bracketed in interface/host tokens. */
function looksLikeUnbracketedV6(token: string): boolean {
  return token.includes(':') && !token.startsWith('[');
}

function checkPort(field: string, port: unknown, issues: SettingsIssue[], allowZero = false) {
  if (typeof port !== 'number' || !Number.isInteger(port)) {
    issues.push({ field, message: msg('port') });
    return;
  }
  if (port > 65_535 || port < (allowZero ? 0 : 1)) {
    issues.push({ field, message: msg('port') });
  }
}

function checkHostPort(field: string, value: unknown, issues: SettingsIssue[]) {
  if (value === null || typeof value !== 'object') return;
  const { hostname, port } = value as { hostname?: unknown; port?: unknown };
  if (typeof hostname !== 'string' || isBadToken(hostname) || looksLikeUnbracketedV6(hostname)) {
    issues.push({ field, message: msg('hostToken') });
  }
  checkPort(field, port, issues);
}

export function validateDelta(delta: Record<string, unknown>): SettingsIssue[] {
  const issues: SettingsIssue[] = [];

  const range = delta.outgoingPortRange as { first: number; last: number } | null | undefined;
  if (range != null) {
    checkPort('outgoingPortRange', range.first, issues);
    checkPort('outgoingPortRange', range.last, issues);
    if (range.last < range.first) {
      issues.push({ field: 'outgoingPortRange', message: msg('rangeOrder') });
    }
  }

  const listen = delta.listenInterfaces as
    { interface: string; port: number; ssl: boolean; local: boolean }[] | undefined;
  if (listen) {
    for (const entry of listen) {
      if (isBadToken(entry.interface)) {
        issues.push({ field: 'listenInterfaces', message: msg('interfaceToken') });
      } else if (looksLikeUnbracketedV6(entry.interface)) {
        issues.push({ field: 'listenInterfaces', message: msg('ipv6Brackets') });
      }
      // Port 0 asks the OS for an ephemeral port (allowed here only).
      checkPort('listenInterfaces', entry.port, issues, true);
    }
  }

  const outgoing = delta.outgoingInterfaces as string[] | undefined;
  if (outgoing) {
    for (const token of outgoing) {
      if (isBadToken(token)) {
        issues.push({ field: 'outgoingInterfaces', message: msg('interfaceToken') });
      }
    }
  }

  const nodes = delta.dhtBootstrapNodes as { hostname: string; port: number }[] | undefined;
  if (nodes) for (const node of nodes) checkHostPort('dhtBootstrapNodes', node, issues);

  const proxy = delta.proxy as
    | {
        protocol: string;
        hostname: string;
        port: number;
        username: string;
        password: string;
        resolveHostnames: boolean;
      }
    | null
    | undefined;
  if (proxy != null) {
    if (isBadToken(proxy.hostname)) issues.push({ field: 'proxy', message: msg('hostToken') });
    checkPort('proxy', proxy.port, issues);
    const hasUser = proxy.username !== '';
    const hasPass = proxy.password !== '';
    switch (proxy.protocol) {
      case 'SOCKS4':
        if (!hasUser) issues.push({ field: 'proxy', message: msg('socks4Username') });
        if (hasPass) issues.push({ field: 'proxy', message: msg('credentialsForbidden') });
        if (proxy.resolveHostnames) {
          issues.push({ field: 'proxy', message: msg('socks4Resolve') });
        }
        break;
      case 'SOCKS5':
      case 'HTTP':
        if (hasUser || hasPass) {
          issues.push({ field: 'proxy', message: msg('credentialsForbidden') });
        }
        break;
      case 'SOCKS5_PASSWORD':
      case 'HTTP_PASSWORD':
        if (!hasUser || !hasPass) {
          issues.push({ field: 'proxy', message: msg('credentialsRequired') });
        }
        break;
    }
  }

  const i2p = delta.i2p as
    | {
        hostname: string;
        port: number;
        inbound: { tunnels: number; hops: number; hopVariance: number };
        outbound: { tunnels: number; hops: number; hopVariance: number };
      }
    | null
    | undefined;
  if (i2p != null) {
    if (isBadToken(i2p.hostname)) issues.push({ field: 'i2p', message: msg('hostToken') });
    checkPort('i2p', i2p.port, issues);
    for (const tunnel of [i2p.inbound, i2p.outbound]) {
      if (tunnel.tunnels < 1 || tunnel.tunnels > 16) {
        issues.push({ field: 'i2p', message: msg('i2pTunnels') });
      }
      if (tunnel.hops < 0 || tunnel.hops > 7) {
        issues.push({ field: 'i2p', message: msg('i2pHops') });
      }
      if (tunnel.hopVariance < -7 || tunnel.hopVariance > 7) {
        issues.push({ field: 'i2p', message: msg('i2pVariance') });
      }
    }
  }

  const encryption = delta.encryption as
    { methods: { plaintext: boolean; rc4: boolean }; preferRc4: boolean } | undefined;
  if (encryption) {
    if (!encryption.methods.plaintext && !encryption.methods.rc4) {
      issues.push({ field: 'encryption', message: msg('encryptionMethods') });
    }
    if (encryption.preferRc4 && !(encryption.methods.plaintext && encryption.methods.rc4)) {
      issues.push({ field: 'encryption', message: msg('preferRc4') });
    }
  }

  if (delta.userAgent === 'UNRECOGNIZED') {
    issues.push({ field: 'userAgent', message: msg('userAgentReadOnly') });
  }

  return issues;
}
