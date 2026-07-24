// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

import { Check, ChevronRight } from 'lucide-react';
import { ContextMenu as CM, DropdownMenu as DM } from 'radix-ui';
import type { ComponentProps } from 'react';
import { cn } from '@/lib/cn';

/**
 * Shared menu styling for DropdownMenu (toolbar) and ContextMenu (rows,
 * table headers). Item shapes intentionally identical so ActionDefs render
 * the same everywhere.
 */
export const menuContentClass =
  'z-50 min-w-44 max-h-[70vh] overflow-y-auto rounded-md border border-border bg-card p-1 text-sm shadow-md outline-none';
export const menuItemClass =
  'relative flex cursor-default select-none items-center gap-2 rounded-sm px-2 py-1.5 outline-none data-[disabled]:pointer-events-none data-[disabled]:opacity-50 data-[highlighted]:bg-accent [&_svg]:size-4 [&_svg]:shrink-0 [&_svg]:text-muted-foreground';
export const menuSeparatorClass = 'my-1 h-px bg-border';
export const menuLabelClass = 'px-2 py-1 text-xs font-medium text-muted-foreground';
const menuCheckboxClass = cn(menuItemClass, 'pl-7');
const menuIndicatorClass = 'absolute left-1.5 flex size-4 items-center justify-center';

/* ---------------- dropdown ---------------- */

export const DropdownMenu = DM.Root;
export const DropdownMenuTrigger = DM.Trigger;
export const DropdownMenuSub = DM.Sub;

export function DropdownMenuContent({ className, ...props }: ComponentProps<typeof DM.Content>) {
  return (
    <DM.Portal>
      <DM.Content sideOffset={4} className={cn(menuContentClass, className)} {...props} />
    </DM.Portal>
  );
}
export function DropdownMenuItem({ className, ...props }: ComponentProps<typeof DM.Item>) {
  return <DM.Item className={cn(menuItemClass, className)} {...props} />;
}
export function DropdownMenuCheckboxItem({
  className,
  children,
  ...props
}: ComponentProps<typeof DM.CheckboxItem>) {
  return (
    <DM.CheckboxItem className={cn(menuCheckboxClass, className)} {...props}>
      <span className={menuIndicatorClass}>
        <DM.ItemIndicator>
          <Check className="size-3.5" />
        </DM.ItemIndicator>
      </span>
      {children}
    </DM.CheckboxItem>
  );
}
export function DropdownMenuSeparator(props: ComponentProps<typeof DM.Separator>) {
  return <DM.Separator className={menuSeparatorClass} {...props} />;
}
export function DropdownMenuLabel(props: ComponentProps<typeof DM.Label>) {
  return <DM.Label className={menuLabelClass} {...props} />;
}
export function DropdownMenuSubTrigger({
  className,
  children,
  ...props
}: ComponentProps<typeof DM.SubTrigger>) {
  return (
    <DM.SubTrigger className={cn(menuItemClass, className)} {...props}>
      {children}
      <ChevronRight className="ml-auto size-4" />
    </DM.SubTrigger>
  );
}
export function DropdownMenuSubContent({
  className,
  ...props
}: ComponentProps<typeof DM.SubContent>) {
  return (
    <DM.Portal>
      <DM.SubContent className={cn(menuContentClass, className)} {...props} />
    </DM.Portal>
  );
}

/* ---------------- context menu ---------------- */

export const ContextMenu = CM.Root;
export const ContextMenuTrigger = CM.Trigger;
export const ContextMenuSub = CM.Sub;

export function ContextMenuContent({ className, ...props }: ComponentProps<typeof CM.Content>) {
  return (
    <CM.Portal>
      <CM.Content className={cn(menuContentClass, className)} {...props} />
    </CM.Portal>
  );
}
export function ContextMenuItem({ className, ...props }: ComponentProps<typeof CM.Item>) {
  return <CM.Item className={cn(menuItemClass, className)} {...props} />;
}
export function ContextMenuCheckboxItem({
  className,
  children,
  ...props
}: ComponentProps<typeof CM.CheckboxItem>) {
  return (
    <CM.CheckboxItem className={cn(menuCheckboxClass, className)} {...props}>
      <span className={menuIndicatorClass}>
        <CM.ItemIndicator>
          <Check className="size-3.5" />
        </CM.ItemIndicator>
      </span>
      {children}
    </CM.CheckboxItem>
  );
}
export function ContextMenuSeparator(props: ComponentProps<typeof CM.Separator>) {
  return <CM.Separator className={menuSeparatorClass} {...props} />;
}
export function ContextMenuLabel(props: ComponentProps<typeof CM.Label>) {
  return <CM.Label className={menuLabelClass} {...props} />;
}
export function ContextMenuSubTrigger({
  className,
  children,
  ...props
}: ComponentProps<typeof CM.SubTrigger>) {
  return (
    <CM.SubTrigger className={cn(menuItemClass, className)} {...props}>
      {children}
      <ChevronRight className="ml-auto size-4" />
    </CM.SubTrigger>
  );
}
export function ContextMenuSubContent({
  className,
  ...props
}: ComponentProps<typeof CM.SubContent>) {
  return (
    <CM.Portal>
      <CM.SubContent className={cn(menuContentClass, className)} {...props} />
    </CM.Portal>
  );
}
