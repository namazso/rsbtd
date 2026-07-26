// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

import { create } from 'zustand';

/**
 * Row selection for the torrent list (torrent uuids). Range selection
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

  click: (uuid: string, mods: ClickModifiers, order: readonly string[]) => void;
  /** Right-click: keep an existing multi-selection, else select the row. */
  contextSelect: (uuid: string) => void;
  keyMove: (uuid: string, extend: boolean, order: readonly string[]) => void;
  toggle: (uuid: string) => void;
  selectAll: (order: readonly string[]) => void;
  clear: () => void;
  discard: (uuids: readonly string[]) => void;
}

export const useSelection = create<SelectionState>((set, get) => ({
  selected: new Set<string>(),
  anchor: null,
  focus: null,

  click: (uuid, mods, order) => {
    const { selected, anchor } = get();
    if (mods.shift && anchor !== null) {
      const from = order.indexOf(anchor);
      const to = order.indexOf(uuid);
      if (from !== -1 && to !== -1) {
        const [lo, hi] = from <= to ? [from, to] : [to, from];
        const range = order.slice(lo, hi + 1);
        const next = mods.ctrl ? new Set(selected) : new Set<string>();
        for (const u of range) next.add(u);
        set({ selected: next, focus: uuid });
        return;
      }
    }
    if (mods.ctrl) {
      const next = new Set(selected);
      if (next.has(uuid)) next.delete(uuid);
      else next.add(uuid);
      set({ selected: next, anchor: uuid, focus: uuid });
      return;
    }
    set({ selected: new Set([uuid]), anchor: uuid, focus: uuid });
  },

  contextSelect: (uuid) => {
    const { selected } = get();
    if (!selected.has(uuid)) set({ selected: new Set([uuid]), anchor: uuid, focus: uuid });
  },

  keyMove: (uuid, extend, order) => {
    if (extend) {
      get().click(uuid, { ctrl: false, shift: true }, order);
    } else {
      set({ selected: new Set([uuid]), anchor: uuid, focus: uuid });
    }
  },

  toggle: (uuid) => {
    const next = new Set(get().selected);
    if (next.has(uuid)) next.delete(uuid);
    else next.add(uuid);
    set({ selected: next, anchor: uuid, focus: uuid });
  },

  selectAll: (order) => set({ selected: new Set(order) }),

  clear: () => set({ selected: new Set(), anchor: null, focus: null }),

  discard: (uuids) => {
    const { selected, anchor, focus } = get();
    const gone = new Set(uuids);
    if (![...gone].some((u) => selected.has(u) || u === anchor || u === focus)) return;
    const next = new Set([...selected].filter((u) => !gone.has(u)));
    set({
      selected: next,
      anchor: anchor !== null && gone.has(anchor) ? null : anchor,
      focus: focus !== null && gone.has(focus) ? null : focus,
    });
  },
}));
