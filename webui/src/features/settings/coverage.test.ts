// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

import { describe, expect, it } from 'vitest';
import { AllSettingsQuery, SettingsValues } from '@/api/operations/settings';
import { SETTINGS_CATALOG, SETTINGS_FIELD_COUNT } from '@/gen/settings-catalog';
import { ADVANCED_FIELDS, CURATED_SECTIONS } from './manifest';

/**
 * Tripwires: the hand-written full-settings selection and
 * the curated manifest must cover the entire catalog — a schema refresh
 * that adds fields fails here until it is triaged.
 */
describe('settings coverage', () => {
  it('catalog has the documented field count', () => {
    expect(SETTINGS_CATALOG.length).toBe(SETTINGS_FIELD_COUNT);
  });

  it('SettingsValues fragment selects every catalog field', () => {
    const text = SettingsValues.toString() + AllSettingsQuery.toString();
    for (const entry of SETTINGS_CATALOG) {
      expect(text, `fragment is missing settings field ${entry.name}`).toMatch(
        new RegExp(`\\b${entry.name}\\b`),
      );
    }
  });

  it('curated sections plus Advanced cover all 191 fields exactly once', () => {
    const curated = CURATED_SECTIONS.flatMap((s) => s.fields);
    const all = [...curated, ...ADVANCED_FIELDS];
    const unique = new Set(all);
    expect(unique.size, 'duplicate field placement').toBe(all.length);
    expect(all.length).toBe(SETTINGS_CATALOG.length);
    for (const entry of SETTINGS_CATALOG) {
      expect(unique.has(entry.name), `field ${entry.name} not placed`).toBe(true);
    }
    // Curated fields must exist in the catalog (typo guard).
    const names = new Set(SETTINGS_CATALOG.map((e) => e.name));
    for (const field of curated) {
      expect(names.has(field), `curated field ${field} not in catalog`).toBe(true);
    }
  });
});
