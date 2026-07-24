// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

import { TORRENT_FIELDS, lookupTorrentField } from '@/features/torrents/fields';
import { fieldLabel } from '@/features/torrents/fields';

/**
 * Search-box autocomplete: property-name suggestions before the colon
 * (accepting completes *up to the colon*, per spec), value suggestions for
 * enum/boolean properties after it.
 */
export interface Suggestion {
  kind: 'property' | 'value';
  /** Text shown in the list. */
  label: string;
  /** Localized field label / detail column. */
  detail?: string;
  /** Replacement for the current token. */
  insert: string;
}

export interface SuggestionResult {
  items: Suggestion[];
  /** Range of the current token within the input to replace. */
  replaceStart: number;
  replaceEnd: number;
}

const EMPTY: SuggestionResult = { items: [], replaceStart: 0, replaceEnd: 0 };

export function suggestionsFor(input: string, caret: number): SuggestionResult {
  // Locate the whitespace-delimited token containing the caret.
  let start = caret;
  while (start > 0 && !/\s/.test(input[start - 1] ?? '')) start--;
  let end = caret;
  while (end < input.length && !/\s/.test(input[end] ?? '')) end++;
  const token = input.slice(start, caret); // complete only what's typed so far
  if (token === '') return EMPTY;

  const negated = token.startsWith('-');
  const body = negated ? token.slice(1) : token;
  const quoted = body.startsWith('"');
  const bare = quoted ? body.slice(1) : body;
  const colon = bare.indexOf(':');
  const negPrefix = negated ? '-' : '';
  const quotePrefix = quoted ? '"' : '';

  if (colon === -1) {
    const needle = bare.toLowerCase();
    if (needle === '') return EMPTY;
    const items: Suggestion[] = [];
    for (const field of TORRENT_FIELDS) {
      if (field.filterable === false) continue;
      const names = [field.key, ...(field.aliases ?? [])];
      const hit = names.find((n) => n.toLowerCase().startsWith(needle));
      if (hit !== undefined && hit.toLowerCase() !== needle) {
        items.push({
          kind: 'property',
          label: `${hit}:`,
          detail: fieldLabel(field.key),
          insert: `${negPrefix}${quotePrefix}${hit}:`,
        });
      }
      if (items.length >= 12) break;
    }
    return { items, replaceStart: start, replaceEnd: end };
  }

  const key = bare.slice(0, colon);
  const typed = bare.slice(colon + 1).toLowerCase();
  const field = lookupTorrentField(key);
  if (!field) return EMPTY;
  let values: readonly string[];
  if (field.type === 'bool') values = ['true', 'false'];
  else if (field.type === 'enum') values = field.enumValues ?? [];
  else return EMPTY;

  const items = values
    .filter((v) => v.toLowerCase().startsWith(typed) && v.toLowerCase() !== typed)
    .slice(0, 12)
    .map((v) => ({
      kind: 'value' as const,
      label: v,
      insert: `${negPrefix}${quotePrefix}${key}:${v}`,
    }));
  return { items, replaceStart: start, replaceEnd: end };
}
