// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

/**
 * Generates src/gen/settings-catalog.ts from ../schema.graphql.
 *
 * The catalog is the single machine-readable description of every
 * `SettingsInput` field: name, value shape, enum values, nested group
 * members, and the SDL docstring (shown as help text in the UI). It drives
 * the auto-generated "Advanced" settings section, curated-page field lookup,
 * and settings search.
 *
 * Runs under plain `node` (erasable-TS syntax only — see tsconfig
 * `erasableSyntaxOnly`). Invoked by `npm run codegen`.
 */
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import {
  buildSchema,
  getNamedType,
  isEnumType,
  isInputObjectType,
  isListType,
  isNonNullType,
  isScalarType,
  type GraphQLInputObjectType,
  type GraphQLInputType,
  type GraphQLNamedType,
} from 'graphql';

/**
 * Tripwire: the API guide documents exactly 191 settings fields. If a schema
 * refresh changes this, update the constant deliberately and triage the new
 * fields into curated pages or let them fall into "Advanced".
 */
const EXPECTED_SETTINGS_FIELD_COUNT = 184;

/**
 * Groups where an explicit `null` in `applySettings` disables the feature
 * (API guide §6). Everything else must never be sent as null.
 */
const NULLABLE_DISABLE_GROUPS = ['proxy', 'i2p', 'outgoingPortRange', 'webtorrentStunServer'];

interface CatalogMember {
  name: string;
  kind: 'int' | 'boolean' | 'string' | 'enum' | 'group';
  description: string;
  enumValues?: string[];
  members?: CatalogMember[];
  /** SDL default value on the input field, when declared. */
  defaultValue?: unknown;
}

interface CatalogEntry {
  name: string;
  kind: 'int' | 'boolean' | 'string' | 'enum' | 'group' | 'list';
  description: string;
  enumValues?: string[];
  /** Group members, or the object shape of list items. */
  members?: CatalogMember[];
  /** For kind 'list': the item shape. */
  listItem?: 'string' | 'group';
  /** True when explicit null disables the group (proxy, i2p, ...). */
  nullableDisable?: boolean;
}

function scalarKind(type: GraphQLNamedType): 'int' | 'boolean' | 'string' {
  switch (type.name) {
    case 'Int':
      return 'int';
    case 'Boolean':
      return 'boolean';
    case 'String':
      return 'string';
    default:
      throw new Error(`settings catalog: unexpected scalar type ${type.name}`);
  }
}

function unwrapNonNull(type: GraphQLInputType): GraphQLInputType {
  return isNonNullType(type) ? type.ofType : type;
}

function groupMembers(obj: GraphQLInputObjectType): CatalogMember[] {
  return Object.values(obj.getFields()).map((field) => {
    const type = unwrapNonNull(field.type);
    const named = getNamedType(type);
    const base = {
      name: field.name,
      description: field.description ?? '',
      ...(field.defaultValue !== undefined ? { defaultValue: field.defaultValue } : {}),
    };
    if (isEnumType(named)) {
      return { ...base, kind: 'enum' as const, enumValues: named.getValues().map((v) => v.name) };
    }
    if (isInputObjectType(named)) {
      return { ...base, kind: 'group' as const, members: groupMembers(named) };
    }
    if (isScalarType(named)) {
      return { ...base, kind: scalarKind(named) };
    }
    throw new Error(`settings catalog: unexpected member type on ${obj.name}.${field.name}`);
  });
}

const scriptDir = dirname(fileURLToPath(import.meta.url));
const schemaPath = join(scriptDir, '..', 'schema.graphql');
const outPath = join(scriptDir, '..', 'src', 'gen', 'settings-catalog.ts');

const schema = buildSchema(readFileSync(schemaPath, 'utf8'));

const input = schema.getType('SettingsInput');
const output = schema.getType('Settings');
if (!input || !isInputObjectType(input)) throw new Error('SettingsInput not found in schema');
if (!output || output.astNode?.kind !== 'ObjectTypeDefinition') {
  throw new Error('Settings not found in schema');
}

// The input and output types must describe the same field set; the UI reads
// effective values from `Settings` and writes deltas as `SettingsInput`.
const inputNames = Object.keys(input.getFields());
const outputNames = (output.astNode.fields ?? []).map((f) => f.name.value);
const missingInOutput = inputNames.filter((n) => !outputNames.includes(n));
const missingInInput = outputNames.filter((n) => !inputNames.includes(n));
if (missingInOutput.length > 0 || missingInInput.length > 0) {
  throw new Error(
    `settings catalog: Settings/SettingsInput field mismatch ` +
      `(input-only: ${missingInOutput.join(', ') || '-'}; output-only: ${missingInInput.join(', ') || '-'})`,
  );
}

const entries: CatalogEntry[] = Object.values(input.getFields()).map((field) => {
  // Every SettingsInput field is nullable (delta semantics: omitted =
  // unchanged), so the interesting shape starts below the top level.
  const type = unwrapNonNull(field.type);
  const named = getNamedType(type);
  const base = { name: field.name, description: field.description ?? '' };
  const nullableDisable = NULLABLE_DISABLE_GROUPS.includes(field.name);

  if (isListType(type)) {
    const item = getNamedType(type.ofType);
    if (isInputObjectType(item)) {
      return {
        ...base,
        kind: 'list' as const,
        listItem: 'group' as const,
        members: groupMembers(item),
      };
    }
    return { ...base, kind: 'list' as const, listItem: 'string' as const };
  }
  if (isEnumType(named)) {
    return { ...base, kind: 'enum' as const, enumValues: named.getValues().map((v) => v.name) };
  }
  if (isInputObjectType(named)) {
    return {
      ...base,
      kind: 'group' as const,
      members: groupMembers(named),
      ...(nullableDisable ? { nullableDisable: true } : {}),
    };
  }
  if (isScalarType(named)) {
    return { ...base, kind: scalarKind(named) };
  }
  throw new Error(`settings catalog: unexpected type on SettingsInput.${field.name}`);
});

if (entries.length !== EXPECTED_SETTINGS_FIELD_COUNT) {
  throw new Error(
    `settings catalog: expected ${EXPECTED_SETTINGS_FIELD_COUNT} settings fields, found ${entries.length}. ` +
      `If the daemon schema legitimately changed, update EXPECTED_SETTINGS_FIELD_COUNT and triage the new fields.`,
  );
}

const counts = {
  scalar: entries.filter((e) => e.kind === 'int' || e.kind === 'boolean' || e.kind === 'string')
    .length,
  enum: entries.filter((e) => e.kind === 'enum').length,
  group: entries.filter((e) => e.kind === 'group').length,
  list: entries.filter((e) => e.kind === 'list').length,
};

const header = `// AUTO-GENERATED by scripts/gen-settings-catalog.ts — do not edit.
// Source: webui/schema.graphql (refresh with \`npm run schema:refresh\`).

export type SettingsFieldKind = 'int' | 'boolean' | 'string' | 'enum' | 'group' | 'list';

export interface SettingsCatalogMember {
  name: string;
  kind: 'int' | 'boolean' | 'string' | 'enum' | 'group';
  description: string;
  enumValues?: string[];
  members?: SettingsCatalogMember[];
  defaultValue?: unknown;
}

export interface SettingsCatalogEntry {
  name: string;
  kind: SettingsFieldKind;
  description: string;
  enumValues?: string[];
  members?: SettingsCatalogMember[];
  listItem?: 'string' | 'group';
  nullableDisable?: boolean;
}

export const SETTINGS_FIELD_COUNT = ${EXPECTED_SETTINGS_FIELD_COUNT};

/** Groups disabled by sending an explicit null to applySettings. */
export const NULLABLE_DISABLE_GROUPS: readonly string[] = ${JSON.stringify(NULLABLE_DISABLE_GROUPS)};

export const SETTINGS_CATALOG: readonly SettingsCatalogEntry[] = `;

mkdirSync(dirname(outPath), { recursive: true });
writeFileSync(outPath, `${header}${JSON.stringify(entries, null, 2)};\n`);

console.log(
  `settings catalog: ${entries.length} fields ` +
    `(${counts.scalar} scalars, ${counts.enum} enums, ${counts.group} groups, ${counts.list} lists) -> ${outPath}`,
);
