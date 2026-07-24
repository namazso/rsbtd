// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

import { Check } from 'lucide-react';
import { createContext, useContext, type ReactNode } from 'react';
import { cn } from '@/lib/cn';
import type { MenuKit } from './menuKit';

/**
 * Renders ActionDef menus as a flat touch list inside a bottom sheet (or
 * inline on the mobile details page). Submenus flatten into labeled groups
 * — everything a desktop context menu offers is reachable here (spec
 * requirement).
 */
const SheetCloseContext = createContext<() => void>(() => {});

export function SheetActionScope({ close, children }: { close: () => void; children: ReactNode }) {
  return <SheetCloseContext.Provider value={close}>{children}</SheetCloseContext.Provider>;
}

function Item({
  disabled,
  onSelect,
  className,
  children,
}: {
  disabled?: boolean;
  onSelect?: (e: Event) => void;
  className?: string;
  children?: ReactNode;
}) {
  const close = useContext(SheetCloseContext);
  return (
    <button
      type="button"
      disabled={disabled}
      className={cn(
        'flex w-full items-center gap-3 px-4 py-2.5 text-left text-sm active:bg-accent disabled:opacity-40 [&_svg]:size-4.5 [&_svg]:shrink-0 [&_svg]:text-muted-foreground',
        className,
      )}
      onClick={() => {
        onSelect?.(new Event('select'));
        close();
      }}
    >
      {children}
    </button>
  );
}

function CheckboxItem({
  checked,
  disabled,
  onCheckedChange,
  children,
}: {
  checked?: boolean;
  disabled?: boolean;
  onCheckedChange?: (checked: boolean) => void;
  onSelect?: (e: Event) => void;
  children?: ReactNode;
}) {
  const close = useContext(SheetCloseContext);
  return (
    <button
      type="button"
      disabled={disabled}
      aria-pressed={checked === true}
      className="flex w-full items-center gap-3 px-4 py-2.5 text-left text-sm active:bg-accent disabled:opacity-40"
      onClick={() => {
        onCheckedChange?.(!(checked ?? false));
        close();
      }}
    >
      <span
        aria-hidden
        className={cn(
          'flex size-4.5 shrink-0 items-center justify-center rounded-sm border border-border',
          checked === true && 'border-primary bg-primary text-primary-foreground',
        )}
      >
        {checked === true && <Check className="size-3" strokeWidth={3} />}
      </span>
      {children}
    </button>
  );
}

function Separator() {
  return <div className="my-1 h-px bg-border" />;
}

function Label({ children }: { children?: ReactNode }) {
  return <div className="px-4 pt-2 pb-1 text-xs font-medium text-muted-foreground">{children}</div>;
}

function Sub({ children }: { children?: ReactNode }) {
  return <div>{children}</div>;
}

function SubTrigger({ children }: { children?: ReactNode }) {
  return (
    <div className="flex items-center gap-3 px-4 pt-2 pb-1 text-xs font-medium text-muted-foreground [&_svg]:size-3.5">
      {children}
    </div>
  );
}

function SubContent({ children }: { children?: ReactNode }) {
  return <div className="pl-3">{children}</div>;
}

export const SHEET_MENU_KIT: MenuKit = {
  Item,
  CheckboxItem,
  Separator,
  Label,
  Sub,
  SubTrigger,
  SubContent,
};
