// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

/**
 * KiB/s form-field helpers for the daemon's bytes/s rate limits, whose
 * domain is -1 (unlimited) or a positive rate.
 */

/**
 * Formats a bytes/s limit for a KiB/s input field; unlimited shows as
 * '-1'. Sub-KiB rates keep their fractional KiB value (512 B/s is
 * '0.5') so the displayed text parses back to the exact same limit.
 */
export function formatRateKiB(v: number | undefined): string {
  return v === undefined || v <= 0 ? '-1' : String(v / 1024);
}

/**
 * Parses a KiB/s input field into bytes/s: blank means "leave
 * unchanged" (undefined), exactly -1 means unlimited, and positive
 * values round to whole bytes (at least 1). Anything else — 0, other
 * negatives, rates under 1 B/s, non-numbers — is invalid (null).
 */
export function parseRateKiB(text: string): number | undefined | null {
  const trimmed = text.trim();
  if (trimmed === '') return undefined;
  const value = Number(trimmed);
  if (!Number.isFinite(value)) return null;
  if (value === -1) return -1;
  if (value <= 0) return null;
  const bytes = Math.round(value * 1024);
  return bytes >= 1 ? bytes : null;
}
