// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

import i18next from 'i18next';

/**
 * Dynamic-key translation for catalog/registry-driven labels (field names,
 * enum values, settings docs). Keeps the `as never` casts required by the
 * strictly-typed i18next resources in exactly one place.
 */
export function tDynamic(key: string, options?: Record<string, unknown>): string {
  return i18next.t(key as never, options as never) as unknown as string;
}

/**
 * Translate an enum value with graceful fallback to the raw value — unknown
 * future enum values must render, not crash.
 */
export function tEnum(prefix: string, value: string): string {
  const key = `${prefix}.${value}`;
  return i18next.exists(key) ? tDynamic(key) : value;
}
