// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

/** v1 info-hash (40 hex / 32 base32) or v2 (multihash 1220 + 64 hex). */
const XT_RE = /^urn:bt(?:ih:(?:[0-9a-f]{40}|[a-z2-7]{32})|mh:1220[0-9a-f]{64})$/i;

/**
 * Pull valid magnet URIs out of arbitrary text (one-per-line lists, but
 * also prose around a pasted link). Trailing sentence punctuation is
 * stripped; a URI counts only if some xt names a v1/v2 info-hash. The
 * scheme, parameter names, and urn prefix are matched case-insensitively
 * (libtorrent accepts all of them in any case).
 */
export function extractMagnets(text: string): string[] {
  const out = new Set<string>();
  for (const token of text.match(/magnet:\?\S+/gi) ?? []) {
    const uri = token.replace(/[.,;)\]}>"'…]+$/, '');
    const params = new URLSearchParams(uri.slice(uri.indexOf('?') + 1));
    for (const [key, value] of params) {
      if (key.toLowerCase() === 'xt' && XT_RE.test(value)) {
        out.add(uri);
        break;
      }
    }
  }
  return [...out];
}
