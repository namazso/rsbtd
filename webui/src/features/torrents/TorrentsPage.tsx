// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

import { useEffect } from 'react';
import { useIsMobile } from '@/lib/platform';
import { useSelection } from '@/store/selection';
import { usePrefs } from '@/store/prefs';
import { useUi } from '@/store/ui';
import { DetailsPanel } from '@/features/details/DetailsPanel';
import { StatusBar } from '@/features/statusbar/StatusBar';
import { MobileTorrents } from './MobileTorrents';
import { Sidebar } from './Sidebar';
import { Toolbar } from './Toolbar';
import { TorrentTable } from './TorrentTable';
import { useTorrentsView } from './useTorrentsView';

function isEditableTarget(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  return (
    target instanceof HTMLInputElement ||
    target instanceof HTMLTextAreaElement ||
    target instanceof HTMLSelectElement ||
    target.isContentEditable
  );
}

/** Main screen: desktop table shell or mobile list shell. */
export default function TorrentsPage() {
  const view = useTorrentsView();
  const isMobile = useIsMobile();
  const sidebarCollapsed = usePrefs((s) => s.sidebarCollapsed);

  // Global shortcuts + add-torrent drag-drop / magnet paste (desktop).
  useEffect(() => {
    if (isMobile) return;
    const onKeyDown = (e: KeyboardEvent) => {
      if (isEditableTarget(e.target)) return;
      const sel = useSelection.getState();
      const ui = useUi.getState();
      if (e.key === '/') {
        document.getElementById('torrent-search')?.focus();
        e.preventDefault();
      } else if (e.key === 'Delete' && sel.selected.size > 0) {
        ui.openRemoveDialog([...sel.selected]);
        e.preventDefault();
      } else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'a') {
        sel.selectAll(currentOrder);
        e.preventDefault();
      } else if (e.key === 'Escape') {
        sel.clear();
      }
    };
    const onPaste = (e: ClipboardEvent) => {
      if (isEditableTarget(e.target)) return;
      const text = e.clipboardData?.getData('text') ?? '';
      if (text.toLowerCase().includes('magnet:?')) {
        useUi.getState().openAddDialog({ magnet: text.trim() });
        e.preventDefault();
      }
    };
    const onDragOver = (e: DragEvent) => {
      if (e.dataTransfer?.types.includes('Files')) e.preventDefault();
    };
    const onDrop = (e: DragEvent) => {
      const files = [...(e.dataTransfer?.files ?? [])].filter((f) =>
        f.name.toLowerCase().endsWith('.torrent'),
      );
      if (files.length > 0) {
        useUi.getState().openAddDialog({ files });
        e.preventDefault();
      }
    };
    window.addEventListener('keydown', onKeyDown);
    window.addEventListener('paste', onPaste);
    window.addEventListener('dragover', onDragOver);
    window.addEventListener('drop', onDrop);
    return () => {
      window.removeEventListener('keydown', onKeyDown);
      window.removeEventListener('paste', onPaste);
      window.removeEventListener('dragover', onDragOver);
      window.removeEventListener('drop', onDrop);
    };
  }, [isMobile]);

  // Kept fresh for the ctrl+A handler without re-registering listeners.
  currentOrder = view.order;

  if (isMobile) return <MobileTorrents view={view} />;

  return (
    <div className="flex h-dvh flex-col">
      <Toolbar />
      <div className="flex min-h-0 flex-1">
        {!sidebarCollapsed && <Sidebar view={view} />}
        <TorrentTable view={view} />
      </div>
      <DetailsPanel />
      <StatusBar
        totalDownRate={view.totalDownRate}
        totalUpRate={view.totalUpRate}
        torrentCount={view.total}
      />
    </div>
  );
}

let currentOrder: string[] = [];
