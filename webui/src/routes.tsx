// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

import { useEffect } from 'react';
import { createHashRouter, Navigate, Outlet, useNavigate, useParams } from 'react-router';
import { toast } from 'sonner';
import { useSynced } from '@/api/live';
import { AddTorrentDialog } from '@/features/dialogs/AddTorrentDialog';
import {
  MoveStorageDialog,
  RemoveTorrentDialog,
  TorrentLimitsDialog,
} from '@/features/dialogs/TorrentDialogs';
import { MobileTorrentPage } from '@/features/torrents/MobileTorrentPage';
import TorrentsPage from '@/features/torrents/TorrentsPage';
import { tDynamic } from '@/lib/i18nDynamic';
import { useIsMobile } from '@/lib/platform';
import { useTorrents } from '@/store/torrents';
import { useUi } from '@/store/ui';

/**
 * Hash routing: works from any static file server with zero SPA-fallback
 * configuration (the daemon serves no files itself). Dialog hosts live in
 * the root layout so actions work from every page.
 */
function RootLayout() {
  return (
    <>
      <Outlet />
      <AddTorrentDialog />
      <RemoveTorrentDialog />
      <MoveStorageDialog />
      <TorrentLimitsDialog />
    </>
  );
}

/**
 * `#/torrent/:uuid` — on mobile the full-screen Properties page; on
 * desktop a deep link that selects the torrent in the bottom panel.
 */
function TorrentRoute() {
  const { uuid } = useParams();
  const isMobile = useIsMobile();
  if (uuid === undefined) return <Navigate to="/" replace />;
  if (isMobile) return <MobileTorrentPage uuid={uuid} />;
  return <DesktopDeepLink uuid={uuid} />;
}

function DesktopDeepLink({ uuid }: { uuid: string }) {
  const navigate = useNavigate();
  const synced = useSynced((s) => s.synced);

  useEffect(() => {
    if (!synced) return; // wait for the initial resync
    if (useTorrents.getState().byUuid.has(uuid)) {
      useUi.getState().setDetailsUuid(uuid);
    } else {
      toast.error(tDynamic('common:notFound.torrent', { uuid: uuid.slice(0, 8) }));
    }
    void navigate('/', { replace: true });
  }, [uuid, synced, navigate]);

  return null;
}

export const router = createHashRouter([
  {
    path: '/',
    Component: RootLayout,
    children: [
      { index: true, Component: TorrentsPage },
      { path: 'torrent/:uuid', Component: TorrentRoute },
      {
        path: 'settings/:section?',
        lazy: async () => ({
          Component: (await import('@/features/settings/SettingsPage')).default,
        }),
      },
      {
        path: 'stats',
        lazy: async () => ({
          Component: (await import('@/features/stats/StatsPage')).default,
        }),
      },
      {
        path: 'create',
        lazy: async () => ({
          Component: (await import('@/features/create/CreatePage')).default,
        }),
      },
      { path: '*', element: <Navigate to="/" replace /> },
    ],
  },
]);
