// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import '@/i18n';

const request = vi.fn();
const requestTolerant = vi.fn();
vi.mock('@/api/client', () => ({
  request: (...args: unknown[]) => request(...args),
  requestTolerant: (...args: unknown[]) => requestTolerant(...args),
}));

vi.mock('@/store/connection', () => {
  const snap = { state: 'up', generation: 0 };
  return {
    connection: { client: {} },
    useConnection: (selector: (s: typeof snap) => unknown) => selector(snap),
  };
});

import { TooltipProvider } from '@/components/ui/tooltip';
import { TrackersTab } from './TrackersTab';

const tracker = (url: string, tier: number) => ({
  url,
  trackerId: '',
  tier,
  failLimit: 0,
  verified: false,
  source: [],
});

describe('TrackersTab', () => {
  /**
   * Removal is read-modify-replace over the full tracker list: the
   * payload must contain exactly the remaining trackers with their
   * tiers preserved.
   */
  it('removes a tracker by replacing the list, keeping tiers', async () => {
    requestTolerant.mockResolvedValue({
      data: {
        torrent: {
          trackers: [
            tracker('http://a.example/announce', 0),
            tracker('http://b.example/announce', 1),
          ],
          urlSeeds: [],
        },
      },
      errors: [],
    });
    request.mockResolvedValue({ replaceTrackers: true });

    render(
      <QueryClientProvider
        client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}
      >
        <TooltipProvider>
          <TrackersTab hash="aa11" visible />
        </TooltipProvider>
      </QueryClientProvider>,
    );
    expect(await screen.findByText('http://a.example/announce')).toBeInTheDocument();

    const removeButtons = screen.getAllByRole('button', { name: 'Remove tracker' });
    fireEvent.click(removeButtons[0] as HTMLElement);

    expect(request).toHaveBeenCalledTimes(1);
    const [doc, vars] = request.mock.calls[0] as [unknown, unknown];
    expect(String(doc)).toContain('replaceTrackers');
    expect(vars).toEqual({
      hash: 'aa11',
      trackers: [{ url: 'http://b.example/announce', tier: 1 }],
    });
  });
});
