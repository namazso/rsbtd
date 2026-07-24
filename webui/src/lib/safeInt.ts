// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

/**
 * rsbtd serializes signed/unsigned 64-bit values as plain JSON numbers.
 * JavaScript numbers are exact only up to 2^53 - 1 — about 9 PB
 * when the value is a byte count — which no realistic torrent quantity
 * exceeds. We therefore deliberately do NOT use a BigInt-preserving JSON
 * parser; this guard just makes the theoretical precision loss visible
 * during development.
 */
export function safeInt(value: number): number {
  if (import.meta.env.DEV && Number.isFinite(value) && !Number.isSafeInteger(value)) {
    console.warn(`safeInt: ${value} exceeds Number.MAX_SAFE_INTEGER; display may be imprecise`);
  }
  return value;
}
