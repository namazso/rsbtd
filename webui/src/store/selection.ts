// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

import { create } from 'zustand';

/**
 * Row selection for the torrent list (canonical hashes). Range selection
 * resolves against the *current view order* passed by the caller.
 */
export interface ClickModifiers {
  ctrl: boolean;
  shift: boolean;
}

interface SelectionState {
  selected: ReadonlySet<string>;
  anchor: string | null;
  focus: string | null;

  click: (hash: string, mods: ClickModifiers, order: readonly string[]) => void;
  /** Right-click: keep an existing multi-selection, else select the row. */
  contextSelect: (hash: string) => void;
  keyMove: (hash: string, extend: boolean, order: readonly string[]) => void;
  toggle: (hash: string) => void;
  selectAll: (order: readonly string[]) => void;
  clear: () => void;
  discard: (hashes: readonly string[]) => void;
  migrate: (oldHash: string, newHash: string) => void;
}

export const useSelection = create<SelectionState>((set, get) => ({
  selected: new Set<string>(),
  anchor: null,
  focus: null,

  click: (hash, mods, order) => {
    const { selected, anchor } = get();
    if (mods.shift && anchor !== null) {
      const from = order.indexOf(anchor);
      const to = order.indexOf(hash);
      if (from !== -1 && to !== -1) {
        const [lo, hi] = from <= to ? [from, to] : [to, from];
        const range = order.slice(lo, hi + 1);
        const next = mods.ctrl ? new Set(selected) : new Set<string>();
        for (const h of range) next.add(h);
        set({ selected: next, focus: hash });
        return;
      }
    }
    if (mods.ctrl) {
      const next = new Set(selected);
      if (next.has(hash)) next.delete(hash);
      else next.add(hash);
      set({ selected: next, anchor: hash, focus: hash });
      return;
    }
    set({ selected: new Set([hash]), anchor: hash, focus: hash });
  },

  contextSelect: (hash) => {
    const { selected } = get();
    if (!selected.has(hash)) set({ selected: new Set([hash]), anchor: hash, focus: hash });
  },

  keyMove: (hash, extend, order) => {
    if (extend) {
      get().click(hash, { ctrl: false, shift: true }, order);
    } else {
      set({ selected: new Set([hash]), anchor: hash, focus: hash });
    }
  },

  toggle: (hash) => {
    const next = new Set(get().selected);
    if (next.has(hash)) next.delete(hash);
    else next.add(hash);
    set({ selected: next, anchor: hash, focus: hash });
  },

  selectAll: (order) => set({ selected: new Set(order) }),

  clear: () => set({ selected: new Set(), anchor: null, focus: null }),

  discard: (hashes) => {
    const { selected, anchor, focus } = get();
    const gone = new Set(hashes);
    if (![...gone].some((h) => selected.has(h) || h === anchor || h === focus)) return;
    const next = new Set([...selected].filter((h) => !gone.has(h)));
    set({
      selected: next,
      anchor: anchor !== null && gone.has(anchor) ? null : anchor,
      focus: focus !== null && gone.has(focus) ? null : focus,
    });
  },

  migrate: (oldHash, newHash) => {
    const { selected, anchor, focus } = get();
    if (!selected.has(oldHash) && anchor !== oldHash && focus !== oldHash) return;
    const next = new Set(selected);
    if (next.delete(oldHash)) next.add(newHash);
    set({
      selected: next,
      anchor: anchor === oldHash ? newHash : anchor,
      focus: focus === oldHash ? newHash : focus,
    });
  },
}));
