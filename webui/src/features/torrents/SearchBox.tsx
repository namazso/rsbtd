// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

import { Search, TriangleAlert, X } from 'lucide-react';
import { useEffect, useMemo, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Input } from '@/components/ui/input';
import { cn } from '@/lib/cn';
import { tDynamic } from '@/lib/i18nDynamic';
import { suggestionsFor, type SuggestionResult } from '@/search/autocomplete';
import { parseQuery } from '@/search/query';
import { useUi } from '@/store/ui';

/**
 * Filter box with the query language + autocomplete (ARIA combobox).
 * Accepting a property suggestion completes up to the colon; enum/boolean
 * values are suggested after it.
 */
export function SearchBox() {
  const { t } = useTranslation(['torrents', 'common']);
  const searchText = useUi((s) => s.searchText);
  const setSearchText = useUi((s) => s.setSearchText);
  const inputRef = useRef<HTMLInputElement>(null);
  const [open, setOpen] = useState(false);
  const [active, setActive] = useState(0);
  const [showDiagnostics, setShowDiagnostics] = useState(false);
  const [suggestions, setSuggestions] = useState<SuggestionResult>({
    items: [],
    replaceStart: 0,
    replaceEnd: 0,
  });

  const diagnostics = useMemo(() => parseQuery(searchText).diagnostics, [searchText]);

  const refreshSuggestions = () => {
    const el = inputRef.current;
    if (!el) return;
    const result = suggestionsFor(el.value, el.selectionStart ?? el.value.length);
    setSuggestions(result);
    setOpen(result.items.length > 0);
    setActive(0);
  };

  useEffect(() => {
    // Text can change from outside (Esc clear, chips); refresh when focused.
    if (document.activeElement === inputRef.current) refreshSuggestions();
    else setOpen(false);
  }, [searchText]);

  const accept = (index: number) => {
    const item = suggestions.items[index];
    const el = inputRef.current;
    if (!item || !el) return;
    const value = el.value;
    const next =
      value.slice(0, suggestions.replaceStart) + item.insert + value.slice(suggestions.replaceEnd);
    const caret = suggestions.replaceStart + item.insert.length;
    setSearchText(next);
    requestAnimationFrame(() => {
      el.focus();
      el.setSelectionRange(caret, caret);
    });
    if (item.kind === 'value') setOpen(false);
  };

  return (
    <div className="relative mx-2 max-w-md min-w-40 flex-1">
      <Search className="pointer-events-none absolute top-1/2 left-2 size-3.5 -translate-y-1/2 text-muted-foreground" />
      <Input
        ref={inputRef}
        id="torrent-search"
        role="combobox"
        aria-expanded={open}
        aria-controls="torrent-search-listbox"
        aria-autocomplete="list"
        aria-activedescendant={open ? `torrent-search-option-${active}` : undefined}
        value={searchText}
        onChange={(e) => setSearchText(e.target.value)}
        onKeyUp={(e) => {
          if (e.key === 'ArrowLeft' || e.key === 'ArrowRight') refreshSuggestions();
        }}
        onClick={refreshSuggestions}
        onFocus={refreshSuggestions}
        onBlur={() => setOpen(false)}
        onKeyDown={(e) => {
          if (open && e.key === 'ArrowDown') {
            setActive((a) => Math.min(a + 1, suggestions.items.length - 1));
            e.preventDefault();
          } else if (open && e.key === 'ArrowUp') {
            setActive((a) => Math.max(a - 1, 0));
            e.preventDefault();
          } else if (open && (e.key === 'Enter' || e.key === 'Tab')) {
            accept(active);
            e.preventDefault();
          } else if (e.key === 'Escape') {
            if (open) setOpen(false);
            else setSearchText('');
          }
        }}
        placeholder={t('toolbar.searchPlaceholder')}
        className="pr-12 pl-7"
        autoComplete="off"
        spellCheck={false}
      />
      <span className="absolute top-1/2 right-1.5 flex -translate-y-1/2 items-center gap-1">
        {diagnostics.length > 0 && (
          <button
            type="button"
            aria-label={tDynamic('torrents:toolbar.searchWarnings', {
              defaultValue: 'Search warnings',
            })}
            aria-expanded={showDiagnostics}
            aria-controls="torrent-search-diagnostics"
            onClick={() => setShowDiagnostics((v) => !v)}
            className="rounded p-0.5 text-st-check hover:bg-accent"
          >
            <TriangleAlert aria-hidden className="size-3.5" />
          </button>
        )}
        {searchText !== '' && (
          <button
            type="button"
            aria-label={t('common:actions.close')}
            onClick={() => setSearchText('')}
            className="rounded p-0.5 text-muted-foreground hover:bg-accent"
          >
            <X className="size-3.5" />
          </button>
        )}
      </span>

      <div id="torrent-search-diagnostics" role="status">
        {showDiagnostics && diagnostics.length > 0 && (
          <ul className="absolute top-full right-0 z-40 mt-1 max-w-xs rounded-md border border-border bg-card px-2.5 py-1 text-xs shadow-md">
            {diagnostics.map((d, i) => (
              <li key={i}>{d}</li>
            ))}
          </ul>
        )}
      </div>

      {open && (
        <ul
          id="torrent-search-listbox"
          role="listbox"
          className="absolute top-full right-0 left-0 z-40 mt-1 max-h-64 overflow-y-auto rounded-md border border-border bg-card py-1 text-sm shadow-md"
        >
          {suggestions.items.map((item, i) => (
            <li
              key={`${item.kind}-${item.label}`}
              id={`torrent-search-option-${i}`}
              role="option"
              aria-selected={i === active}
              className={cn(
                'flex cursor-pointer items-baseline justify-between gap-3 px-2.5 py-1',
                i === active && 'bg-accent',
              )}
              onMouseDown={(e) => {
                e.preventDefault(); // keep focus in the input
                accept(i);
              }}
              onMouseEnter={() => setActive(i)}
            >
              <span className="font-mono text-xs">{item.label}</span>
              {item.detail !== undefined && (
                <span className="truncate text-xs text-muted-foreground">{item.detail}</span>
              )}
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
