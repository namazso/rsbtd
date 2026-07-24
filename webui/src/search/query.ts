// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

import { lookupTorrentField, type TorrentFieldDef } from '@/features/torrents/fields';
import type { TorrentRow } from '@/store/torrents';
import { parseTypedValue, type DateValue } from './units';

/**
 * The search/filter language:
 *
 *   input  := term (ws term)*        term := '-'? (quoted | token)
 *   token  := prop ':' cmp? value | free-text
 *   cmp    := '>=' | '<=' | '>' | '<'          (numeric/date props)
 *
 * - free text          → case-insensitive substring on the name
 * - string values      → exact match unless '*' at start/end/both
 * - quoting            → binds spaces ("name:*ntu 12*")
 * - '-'                → negates the term
 * - enums/booleans     → case-insensitive values
 * - unknown properties → degrade to free text (forgiving: pasted URLs)
 * - unparsable values  → no-op while typing, reported as a diagnostic
 */
export type Comparator = '>' | '>=' | '<' | '<=' | '=';

export interface RawToken {
  text: string;
  negated: boolean;
}

export type QueryNode =
  | { kind: 'text'; needle: string; negated: boolean }
  | {
      kind: 'filter';
      field: TorrentFieldDef;
      op: Comparator;
      value: CompiledValue;
      negated: boolean;
    };

type CompiledValue =
  | { type: 'string'; text: string; prefix: boolean; suffix: boolean }
  | { type: 'number'; value: number }
  | { type: 'date'; value: DateValue }
  | { type: 'bool'; value: boolean }
  | { type: 'enum'; value: string }
  | { type: 'flag'; value: string };

export interface ParsedQuery {
  nodes: QueryNode[];
  diagnostics: string[];
}

/** Whitespace-split honoring double quotes; no escape sequences. */
export function tokenize(input: string): { tokens: RawToken[]; unterminated: boolean } {
  const tokens: RawToken[] = [];
  let current = '';
  let negated = false;
  let started = false;
  let inQuote = false;
  let unterminated = false;

  const push = () => {
    if (started && current !== '') tokens.push({ text: current, negated });
    current = '';
    negated = false;
    started = false;
  };

  for (const ch of input) {
    if (ch === '"') {
      inQuote = !inQuote;
      started = true;
      continue; // quotes bind, they are not part of the value
    }
    if (!inQuote && /\s/.test(ch)) {
      push();
      continue;
    }
    if (!started && ch === '-' && current === '') {
      negated = true;
      started = true;
      continue;
    }
    started = true;
    current += ch;
  }
  if (inQuote) unterminated = true;
  push();
  return { tokens, unterminated };
}

const NUMERIC_TYPES = new Set([
  'bytes',
  'rate',
  'number',
  'float',
  'percentPpm',
  'date',
  'durationSecs',
  'etaSecs',
]);

export function parseQuery(input: string): ParsedQuery {
  const { tokens, unterminated } = tokenize(input);
  const nodes: QueryNode[] = [];
  const diagnostics: string[] = [];
  if (unterminated) diagnostics.push('unterminated quote');

  for (const token of tokens) {
    const colon = token.text.indexOf(':');
    if (colon <= 0) {
      nodes.push({ kind: 'text', needle: token.text.toLowerCase(), negated: token.negated });
      continue;
    }
    const key = token.text.slice(0, colon);
    const field = lookupTorrentField(key);
    if (!field || field.filterable === false) {
      // Unknown property: treat the whole token as text (magnet:?..., URLs).
      if (/^[a-zA-Z][a-zA-Z0-9]*$/.test(key)) diagnostics.push(`unknown property: ${key}`);
      nodes.push({ kind: 'text', needle: token.text.toLowerCase(), negated: token.negated });
      continue;
    }

    let rhs = token.text.slice(colon + 1);
    if (rhs === '') continue; // still typing — no constraint yet

    let op: Comparator = '=';
    if (NUMERIC_TYPES.has(field.type)) {
      for (const candidate of ['>=', '<=', '>', '<'] as const) {
        if (rhs.startsWith(candidate)) {
          op = candidate;
          rhs = rhs.slice(candidate.length);
          break;
        }
      }
      if (rhs === '') continue;
    }

    const value = parseValue(field, rhs);
    if (value === null) {
      diagnostics.push(`cannot parse "${rhs}" for ${field.key}`);
      continue; // no-op while typing
    }
    nodes.push({ kind: 'filter', field, op, value, negated: token.negated });
  }
  return { nodes, diagnostics };
}

function parseValue(field: TorrentFieldDef, rhs: string): CompiledValue | null {
  switch (field.type) {
    case 'string': {
      const prefix = rhs.startsWith('*');
      const suffix = rhs.endsWith('*') && rhs.length > (prefix ? 1 : 0);
      const text = rhs.slice(prefix ? 1 : 0, suffix ? rhs.length - 1 : undefined);
      return { type: 'string', text: text.toLowerCase(), prefix, suffix };
    }
    case 'bool': {
      const lower = rhs.toLowerCase();
      if (lower === 'true' || lower === 'yes') return { type: 'bool', value: true };
      if (lower === 'false' || lower === 'no') return { type: 'bool', value: false };
      return null;
    }
    case 'enum': {
      const canonical = field.enumValues?.find((v) => v.toLowerCase() === rhs.toLowerCase());
      return canonical !== undefined ? { type: 'enum', value: canonical } : null;
    }
    case 'flags':
      return { type: 'flag', value: rhs.toLowerCase() };
    case 'date': {
      const value = parseTypedValue('date', rhs);
      return value !== null && typeof value === 'object' ? { type: 'date', value } : null;
    }
    default: {
      if (field.unlimitedSentinel && rhs.trim() === '-1') return { type: 'number', value: -1 };
      const value = parseTypedValue(field.type, rhs);
      return typeof value === 'number' ? { type: 'number', value } : null;
    }
  }
}

function matchNode(node: QueryNode, row: TorrentRow): boolean {
  if (node.kind === 'text') {
    return row.name.toLowerCase().includes(node.needle);
  }
  const raw = node.field.get(row);
  const value = node.value;
  switch (value.type) {
    case 'string': {
      if (raw == null) return false;
      const s = String(raw).toLowerCase();
      const { text, prefix, suffix } = value;
      if (prefix && suffix) return s.includes(text);
      if (prefix) return s.endsWith(text); // *foo
      if (suffix) return s.startsWith(text); // foo*
      return s === text;
    }
    case 'bool':
      return raw === value.value;
    case 'enum':
      return raw === value.value;
    case 'flag':
      return Array.isArray(raw) && raw.some((f) => String(f).toLowerCase() === value.value);
    case 'date': {
      if (typeof raw !== 'number' || raw <= 0) return false;
      const { startSec, endSec } = value.value;
      switch (node.op) {
        case '=':
          return raw >= startSec && raw < endSec;
        case '>':
          return raw >= endSec;
        case '>=':
          return raw >= startSec;
        case '<':
          return raw < startSec;
        case '<=':
          return raw < endSec;
      }
      break;
    }
    case 'number': {
      // A missing value means "not applicable" and matches no comparison
      // — pieceLength:>1G must not match metadata-less torrents. ETA is
      // the one field whose null genuinely means "infinite", so eta:>1d
      // keeps matching idle torrents.
      const missing = node.field.type === 'etaSecs' ? Number.POSITIVE_INFINITY : Number.NaN;
      const n = raw == null ? missing : typeof raw === 'number' ? raw : Number.NaN;
      if (Number.isNaN(n)) return false;
      switch (node.op) {
        case '=':
          return n === value.value;
        case '>':
          return n > value.value;
        case '>=':
          return n >= value.value;
        case '<':
          return n < value.value;
        case '<=':
          return n <= value.value;
      }
    }
  }
  return false;
}

/** AND of all terms; null when the query has no effective constraints. */
export function compileQuery(parsed: ParsedQuery): ((row: TorrentRow) => boolean) | null {
  const nodes = parsed.nodes;
  if (nodes.length === 0) return null;
  return (row) => {
    for (const node of nodes) {
      if (matchNode(node, row) === node.negated) return false;
    }
    return true;
  };
}
