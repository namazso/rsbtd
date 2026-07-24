// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

import i18next from 'i18next';
import { useEffect, useRef, useState } from 'react';
import { ArrowDown, ArrowUp, Plus, Trash2 } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Input, NativeSelect } from '@/components/ui/input';
import { Switch } from '@/components/ui/switch-tabs';
import { cn } from '@/lib/cn';
import { tDynamic } from '@/lib/i18nDynamic';
import {
  CATALOG_BY_NAME,
  buildDelta,
  effectiveValue,
  useSettingsDraft,
  type SettingsSnapshot,
} from './draft';
import type { SettingsCatalogEntry, SettingsCatalogMember } from '@/gen/settings-catalog';

/**
 * Generic, catalog-driven settings editors. Curated fields get translated
 * labels (settings:fields.*); everything else falls back to a humanized
 * camelCase name with the SDL docstring as help text (the SDL is the
 * normative documentation — a documented exemption from the no-literal
 * i18n rule; the strings live in the schema, not in components).
 */
export function humanize(name: string): string {
  const words = name.replace(/([a-z0-9])([A-Z])/g, '$1 $2').toLowerCase();
  return words.charAt(0).toUpperCase() + words.slice(1);
}

export function humanizeEnum(value: string): string {
  const words = value.replace(/_/g, ' ').toLowerCase();
  return words.charAt(0).toUpperCase() + words.slice(1);
}

export function settingLabel(name: string): string {
  const key = `settings:fields.${name}`;
  return i18next.exists(key) ? tDynamic(key) : humanize(name);
}

/**
 * Help text: every catalog field has an end-user rewrite (settings:help.*);
 * the SDL docstring remains as a fallback so fields added by a future
 * schema refresh still show help until their rewrite lands.
 */
export function settingHelp(name: string): string {
  const key = `settings:help.${name}`;
  if (i18next.exists(key)) return tDynamic(key);
  return CATALOG_BY_NAME.get(name)?.description ?? '';
}

interface EditorProps {
  snapshot: SettingsSnapshot;
  name: string;
}

function useFieldState(snapshot: SettingsSnapshot, name: string) {
  const draft = useSettingsDraft((s) => s.draft);
  const setField = useSettingsDraft((s) => s.setField);
  const value = effectiveValue(snapshot, draft, name);
  const dirty = name in draft && name in buildDelta(snapshot, { [name]: draft[name] });
  return { value, dirty, set: (v: unknown) => setField(name, v) };
}

function NumberField({
  value,
  onCommit,
  className,
  id,
}: {
  value: number;
  onCommit: (value: number) => void;
  className?: string;
  id?: string;
}) {
  const [text, setText] = useState(String(value));
  useEffect(() => setText(String(value)), [value]);
  return (
    <Input
      id={id}
      type="number"
      value={text}
      onChange={(e) => {
        setText(e.target.value);
        const n = Number(e.target.value);
        if (e.target.value.trim() !== '' && Number.isFinite(n)) onCommit(Math.trunc(n));
      }}
      onBlur={() => setText(String(value))}
      className={cn('max-w-40 tabular-nums', className)}
    />
  );
}

/** One settings row: label, control, dirty dot, SDL description. */
export function SettingRow({ snapshot, name }: EditorProps) {
  const entry = CATALOG_BY_NAME.get(name);
  if (!entry) return null;
  return (
    <div id={`setting-${name}`} className="border-b border-border/40 py-3 last:border-b-0">
      <RowInner entry={entry} snapshot={snapshot} name={name} />
    </div>
  );
}

function RowInner({ entry, snapshot, name }: EditorProps & { entry: SettingsCatalogEntry }) {
  const { value, dirty, set } = useFieldState(snapshot, name);
  const help = settingHelp(name);
  const controlId = `setting-${name}-control`;
  const labelId = `setting-${name}-label`;

  const header = (control: React.ReactNode, block = false) => {
    const title = (
      <>
        {settingLabel(name)}
        {dirty && <span className="size-1.5 rounded-full bg-primary" aria-hidden />}
      </>
    );
    const titleClass = 'flex items-center gap-1.5 text-sm leading-4 font-medium';
    return (
      <>
        <div className={cn('flex items-start justify-between gap-4', block && 'mb-2')}>
          <span className="min-w-0 pt-1">
            {block ? (
              <span id={labelId} className={titleClass}>
                {title}
              </span>
            ) : (
              <label id={labelId} htmlFor={controlId} className={titleClass}>
                {title}
              </label>
            )}
            {help !== '' && (
              <span
                className="mt-0.5 line-clamp-3 block max-w-xl text-xs whitespace-pre-line text-muted-foreground"
                title={help}
              >
                {help}
              </span>
            )}
          </span>
          {!block && <span className="shrink-0">{control}</span>}
        </div>
        {block && control}
      </>
    );
  };

  switch (entry.kind) {
    case 'boolean':
      return header(
        <Switch id={controlId} checked={value === true} onCheckedChange={(v) => set(v)} />,
      );
    case 'int':
      return header(
        <NumberField id={controlId} value={typeof value === 'number' ? value : 0} onCommit={set} />,
      );
    case 'string':
      return header(
        <Input
          id={controlId}
          value={typeof value === 'string' ? value : ''}
          onChange={(e) => set(e.target.value)}
          className="max-w-64"
          spellCheck={false}
        />,
      );
    case 'enum':
      return header(
        <NativeSelect
          id={controlId}
          value={String(value)}
          onChange={(e) => set(e.target.value)}
          className="w-auto max-w-64"
        >
          {(entry.enumValues ?? []).map((v) => (
            <option key={v} value={v} disabled={name === 'userAgent' && v === 'UNRECOGNIZED'}>
              {humanizeEnum(v)}
            </option>
          ))}
          {/* effective value outside the input enum (e.g. UNRECOGNIZED) */}
          {entry.enumValues?.includes(String(value)) === false && (
            <option value={String(value)} disabled>
              {humanizeEnum(String(value))}
            </option>
          )}
        </NativeSelect>,
      );
    case 'group':
      return header(
        <GroupEditor entry={entry} name={name} value={value} onChange={set} labelId={labelId} />,
        true,
      );
    case 'list':
      return header(
        <ListEditor entry={entry} name={name} value={value} onChange={set} labelId={labelId} />,
        true,
      );
  }
}

/* ---------------- group editors ---------------- */

interface GroupEditorProps {
  entry: SettingsCatalogEntry;
  name: string;
  value: unknown;
  onChange: (value: unknown) => void;
  labelId: string;
}

function defaultGroupValue(members: readonly SettingsCatalogMember[]): Record<string, unknown> {
  const out: Record<string, unknown> = {};
  for (const m of members) {
    if (m.defaultValue !== undefined) out[m.name] = m.defaultValue;
    else if (m.kind === 'boolean') out[m.name] = false;
    else if (m.kind === 'int') out[m.name] = defaultIntFor(m.name);
    else if (m.kind === 'string') out[m.name] = '';
    else if (m.kind === 'enum') out[m.name] = m.enumValues?.[0] ?? '';
    else if (m.kind === 'group') out[m.name] = defaultGroupValue(m.members ?? []);
  }
  return out;
}

function defaultIntFor(memberName: string): number {
  switch (memberName) {
    case 'port':
      return 1080;
    case 'tunnels':
      return 3;
    case 'first':
      return 6881;
    case 'last':
      return 6891;
    default:
      return 0;
  }
}

function GroupEditor({ entry, name, value, onChange, labelId }: GroupEditorProps) {
  const nullable = entry.nullableDisable === true;
  const enabled = value !== null && value !== undefined;
  const members = entry.members ?? [];
  const lastEnabled = useRef<unknown>(null);

  return (
    <div role="group" aria-labelledby={labelId} className="rounded-md border border-border/60 p-3">
      {nullable && (
        <label className="mb-2 flex items-center gap-2 text-sm">
          <Switch
            checked={enabled}
            onCheckedChange={(v) => {
              if (v) {
                onChange(lastEnabled.current ?? defaultGroupValue(members));
              } else {
                lastEnabled.current = value;
                onChange(null);
              }
            }}
          />
          {tDynamic('settings:group.enabled')}
        </label>
      )}
      {enabled && (
        <MemberFields
          members={members}
          value={value as Record<string, unknown>}
          onChange={(next) => onChange(next)}
          groupName={name}
        />
      )}
    </div>
  );
}

function MemberFields({
  members,
  value,
  onChange,
  groupName,
}: {
  members: readonly SettingsCatalogMember[];
  value: Record<string, unknown>;
  onChange: (value: Record<string, unknown>) => void;
  groupName: string;
}) {
  const patch = (memberName: string, memberValue: unknown) =>
    onChange({ ...value, [memberName]: memberValue });

  return (
    <div className="grid gap-x-6 gap-y-2 sm:grid-cols-2">
      {members.map((m) => {
        const v = value[m.name];
        const label = (
          <span
            className="text-xs font-medium text-muted-foreground"
            title={m.description !== '' ? m.description : undefined}
          >
            {humanize(m.name)}
          </span>
        );
        if (m.kind === 'boolean') {
          return (
            <label key={m.name} className="flex items-center justify-between gap-3">
              {label}
              <Switch checked={v === true} onCheckedChange={(nv) => patch(m.name, nv)} />
            </label>
          );
        }
        if (m.kind === 'int') {
          return (
            <label key={m.name} className="flex items-center justify-between gap-3">
              {label}
              <NumberField
                value={typeof v === 'number' ? v : 0}
                onCommit={(nv) => patch(m.name, nv)}
                className="max-w-28"
              />
            </label>
          );
        }
        if (m.kind === 'string') {
          const secret = m.name === 'password';
          return (
            <label key={m.name} className="flex items-center justify-between gap-3">
              {label}
              <Input
                type={secret ? 'password' : 'text'}
                value={typeof v === 'string' ? v : ''}
                onChange={(e) => patch(m.name, e.target.value)}
                className="max-w-48"
                spellCheck={false}
              />
            </label>
          );
        }
        if (m.kind === 'enum') {
          return (
            <label key={m.name} className="flex items-center justify-between gap-3">
              {label}
              <NativeSelect
                value={String(v)}
                onChange={(e) => patch(m.name, e.target.value)}
                className="w-auto max-w-48"
              >
                {(m.enumValues ?? []).map((ev) => (
                  <option key={ev} value={ev}>
                    {humanizeEnum(ev)}
                  </option>
                ))}
              </NativeSelect>
            </label>
          );
        }
        // nested group (i2p tunnels, encryption methods, peer transports)
        return (
          <div key={m.name} className="sm:col-span-2">
            <p className="mb-1 text-xs font-semibold text-muted-foreground">{humanize(m.name)}</p>
            <div className="pl-3">
              <MemberFields
                members={m.members ?? []}
                value={(v ?? {}) as Record<string, unknown>}
                onChange={(nv) => patch(m.name, nv)}
                groupName={`${groupName}.${m.name}`}
              />
            </div>
          </div>
        );
      })}
    </div>
  );
}

/* ---------------- list editors ---------------- */

/** Missing keys humanize like settingLabel, so new actions render usably. */
function listActionLabel(action: string): string {
  const key = `settings:list.${action}`;
  return i18next.exists(key) ? tDynamic(key) : humanize(action);
}

function parseStringList(text: string): string[] {
  return text
    .split('\n')
    .map((l) => l.trim())
    .filter((l) => l !== '');
}

/**
 * The stored value strips blank lines, so a textarea controlled directly
 * by it would swallow the newline the user just pressed. The raw text is
 * kept locally while it still parses to the stored value; an outside
 * change (reset, another editor) wins, and blur renormalizes.
 */
function StringListEditor({ value, onChange, labelId }: Omit<GroupEditorProps, 'entry' | 'name'>) {
  const items = Array.isArray(value) ? (value as string[]) : [];
  const canonical = items.join('\n');
  const [draft, setDraft] = useState<string | null>(null);
  const text =
    draft !== null && parseStringList(draft).join('\n') === canonical ? draft : canonical;
  return (
    <textarea
      aria-labelledby={labelId}
      value={text}
      onChange={(e) => {
        setDraft(e.target.value);
        onChange(parseStringList(e.target.value));
      }}
      onBlur={() => setDraft(null)}
      rows={3}
      spellCheck={false}
      placeholder={tDynamic('settings:list.onePerLine')}
      className="w-full max-w-xl rounded-md border border-border bg-transparent px-2.5 py-1.5 font-mono text-xs outline-none focus-visible:ring-2 focus-visible:ring-ring"
    />
  );
}

function ListEditor({ entry, name, value, onChange, labelId }: GroupEditorProps) {
  if (entry.listItem === 'string') {
    return <StringListEditor value={value} onChange={onChange} labelId={labelId} />;
  }

  const members = entry.members ?? [];
  const rows = Array.isArray(value) ? (value as Record<string, unknown>[]) : [];
  const patchRow = (index: number, row: Record<string, unknown>) =>
    onChange(rows.map((r, i) => (i === index ? row : r)));
  const moveRow = (index: number, to: number) => {
    const next = [...rows];
    const [row] = next.splice(index, 1);
    if (row === undefined) return;
    next.splice(to, 0, row);
    onChange(next);
  };

  return (
    <div role="group" aria-labelledby={labelId} className="max-w-2xl space-y-2">
      {rows.map((row, i) => (
        <div key={i} className="flex items-end gap-2 rounded-md border border-border/60 p-2">
          {members.map((m) => (
            <label key={m.name} className="min-w-0 flex-1">
              <span className="block text-[11px] text-muted-foreground">{humanize(m.name)}</span>
              {m.kind === 'boolean' ? (
                <Switch
                  checked={row[m.name] === true}
                  onCheckedChange={(v) => patchRow(i, { ...row, [m.name]: v })}
                />
              ) : m.kind === 'int' ? (
                <NumberField
                  value={typeof row[m.name] === 'number' ? (row[m.name] as number) : 0}
                  onCommit={(v) => patchRow(i, { ...row, [m.name]: v })}
                  className="w-24"
                />
              ) : (
                <Input
                  value={typeof row[m.name] === 'string' ? (row[m.name] as string) : ''}
                  onChange={(e) => patchRow(i, { ...row, [m.name]: e.target.value })}
                  spellCheck={false}
                  className="font-mono text-xs"
                />
              )}
            </label>
          ))}
          <Button
            variant="ghost"
            size="iconSm"
            aria-label={listActionLabel('moveUp')}
            disabled={i === 0}
            onClick={() => moveRow(i, i - 1)}
          >
            <ArrowUp />
          </Button>
          <Button
            variant="ghost"
            size="iconSm"
            aria-label={listActionLabel('moveDown')}
            disabled={i === rows.length - 1}
            onClick={() => moveRow(i, i + 1)}
          >
            <ArrowDown />
          </Button>
          <Button
            variant="ghost"
            size="iconSm"
            aria-label={tDynamic('settings:list.remove')}
            onClick={() => onChange(rows.filter((_, j) => j !== i))}
          >
            <Trash2 />
          </Button>
        </div>
      ))}
      <Button
        variant="outline"
        size="sm"
        onClick={() => {
          const fresh: Record<string, unknown> = {};
          for (const m of members) {
            fresh[m.name] =
              m.defaultValue !== undefined
                ? m.defaultValue
                : m.kind === 'boolean'
                  ? false
                  : m.kind === 'int'
                    ? name === 'listenInterfaces'
                      ? 6881
                      : defaultIntFor(m.name)
                    : '';
          }
          onChange([...rows, fresh]);
        }}
      >
        <Plus />
        {tDynamic('settings:list.add')}
      </Button>
    </div>
  );
}
