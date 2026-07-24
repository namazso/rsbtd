// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

import { BarChart3, List, Settings } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { useLocation, useNavigate } from 'react-router';
import { cn } from '@/lib/cn';

const TABS = [
  { to: '/', icon: List, key: 'torrents' },
  { to: '/stats', icon: BarChart3, key: 'stats' },
  { to: '/settings', icon: Settings, key: 'settings' },
] as const;

export function BottomNav() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const { pathname } = useLocation();

  return (
    <nav className="flex shrink-0 border-t border-border bg-card pb-[env(safe-area-inset-bottom)]">
      {TABS.map(({ to, icon: Icon, key }) => {
        const active = to === '/' ? pathname === '/' : pathname.startsWith(to);
        return (
          <button
            key={key}
            type="button"
            aria-current={active ? 'page' : undefined}
            onClick={() => void navigate(to)}
            className={cn(
              'flex h-14 flex-1 flex-col items-center justify-center gap-0.5 text-[11px]',
              active ? 'text-primary' : 'text-muted-foreground',
            )}
          >
            <Icon className="size-5" />
            {t(`nav.${key}`)}
          </button>
        );
      })}
    </nav>
  );
}
