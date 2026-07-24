// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

import { create } from 'zustand';
import type { SettingsValuesFragment } from '@/gen/gql/graphql';
import { NULLABLE_DISABLE_GROUPS, SETTINGS_CATALOG } from '@/gen/settings-catalog';

/**
 * Settings editing model: the server snapshot (AllSettings / applySettings
 * response) plus a draft patch keyed by field name. `buildDelta` produces
 * the applySettings input honoring the delta rules:
 *  - omitted field = unchanged;
 *  - explicit null ONLY for the nullable-disable groups;
 *  - structured groups are always sent complete;
 *  - a value equal to the snapshot is dropped (revert = no-op).
 */
export type SettingsSnapshot = SettingsValuesFragment;

export const CATALOG_BY_NAME = new Map(SETTINGS_CATALOG.map((e) => [e.name, e]));

/** Deep-copy dropping GraphQL's __typename keys (outputs -> inputs). */
export function stripTypename<T>(value: T): T {
  if (Array.isArray(value)) {
    return value.map((v) => stripTypename(v)) as T;
  }
  if (value !== null && typeof value === 'object') {
    const out: Record<string, unknown> = {};
    for (const [k, v] of Object.entries(value)) {
      if (k === '__typename') continue;
      out[k] = stripTypename(v);
    }
    return out as T;
  }
  return value;
}

export function deepEqual(a: unknown, b: unknown): boolean {
  if (Object.is(a, b)) return true;
  if (Array.isArray(a) && Array.isArray(b)) {
    if (a.length !== b.length) return false;
    return a.every((v, i) => deepEqual(v, b[i]));
  }
  if (a !== null && b !== null && typeof a === 'object' && typeof b === 'object') {
    const keysA = Object.keys(a);
    const keysB = Object.keys(b);
    if (keysA.length !== keysB.length) return false;
    return keysA.every((k) =>
      deepEqual((a as Record<string, unknown>)[k], (b as Record<string, unknown>)[k]),
    );
  }
  return false;
}

interface SettingsDraftState {
  /** Daemon identity (endpoint URL) the draft belongs to. */
  scope: string | null;
  draft: Record<string, unknown>;
  setField: (name: string, value: unknown) => void;
  clearField: (name: string) => void;
  /** Bind the draft to a daemon; a different scope discards it. */
  ensureScope: (scope: string) => void;
  /** After a successful apply: drop entries still equal to the submitted values. */
  pruneApplied: (applied: Record<string, unknown>) => void;
  reset: () => void;
}

export const useSettingsDraft = create<SettingsDraftState>((set) => ({
  scope: null,
  draft: {},
  setField: (name, value) => set((s) => ({ draft: { ...s.draft, [name]: value } })),
  clearField: (name) =>
    set((s) => {
      const { [name]: _removed, ...rest } = s.draft;
      return { draft: rest };
    }),
  ensureScope: (scope) => set((s) => (s.scope === scope ? {} : { scope, draft: {} })),
  pruneApplied: (applied) =>
    set((s) => {
      const draft = { ...s.draft };
      for (const [name, value] of Object.entries(applied)) {
        if (name in draft && deepEqual(draft[name], value)) delete draft[name];
      }
      return { draft };
    }),
  reset: () => set({ draft: {} }),
}));

/** Effective (draft-over-snapshot) value for one field, typename-free. */
export function effectiveValue(
  snapshot: SettingsSnapshot,
  draft: Record<string, unknown>,
  name: string,
): unknown {
  if (name in draft) return draft[name];
  return stripTypename((snapshot as unknown as Record<string, unknown>)[name]);
}

export function buildDelta(
  snapshot: SettingsSnapshot,
  draft: Record<string, unknown>,
): Record<string, unknown> {
  const delta: Record<string, unknown> = {};
  const snap = snapshot as unknown as Record<string, unknown>;
  for (const [name, value] of Object.entries(draft)) {
    if (!CATALOG_BY_NAME.has(name)) continue;
    if (value === undefined) continue;
    if (value === null && !NULLABLE_DISABLE_GROUPS.includes(name)) continue;
    const current = stripTypename(snap[name]);
    if (deepEqual(value, current)) continue;
    delta[name] = value;
  }
  return delta;
}
