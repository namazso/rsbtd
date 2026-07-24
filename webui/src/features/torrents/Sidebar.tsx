// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

import { useTranslation } from 'react-i18next';
import { cn } from '@/lib/cn';
import { useUi, CATEGORY_IDS } from '@/store/ui';
import type { TorrentsView } from './useTorrentsView';

export function Sidebar({ view }: { view: TorrentsView }) {
  const { t } = useTranslation('torrents');
  const category = useUi((s) => s.category);
  const setCategory = useUi((s) => s.setCategory);

  return (
    <nav
      aria-label={t('categories.all')}
      className="w-44 shrink-0 overflow-y-auto border-r border-border py-1.5"
    >
      {CATEGORY_IDS.map((id) => (
        <button
          key={id}
          type="button"
          aria-current={category === id}
          onClick={() => setCategory(id)}
          className={cn(
            'flex w-full items-center justify-between px-3 py-1.5 text-left text-sm outline-none hover:bg-accent focus-visible:ring-2 focus-visible:ring-ring',
            category === id && 'bg-selected font-medium',
          )}
        >
          <span className="truncate">{t(`categories.${id}`)}</span>
          <span className="ml-2 text-xs text-muted-foreground tabular-nums">{view.counts[id]}</span>
        </button>
      ))}
    </nav>
  );
}
