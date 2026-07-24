// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen } from '@testing-library/react';
import { MemoryRouter } from 'react-router';
import { describe, expect, it, vi } from 'vitest';
import '@/i18n';

const request = vi.fn();
vi.mock('@/api/client', () => ({
  request: (...args: unknown[]) => request(...args),
}));

const subscribeRetrying = vi.fn((..._args: unknown[]) => () => {});
vi.mock('@/api/ws', () => ({
  subscribeRetrying: (...args: unknown[]) => subscribeRetrying(...args),
}));

vi.mock('@/store/connection', () => {
  const snap = { state: 'up', generation: 0 };
  return {
    connection: { client: {} },
    useConnection: (selector: (s: typeof snap) => unknown) => selector(snap),
  };
});

import { CreateJobsQuery, StartCreateTorrentMutation } from '@/api/operations/create';
import CreatePage from './CreatePage';

describe('CreatePage', () => {
  /**
   * The first job started on an empty (or terminal-only) list: the jobs
   * query is not polling then, so only the mutation result can make the
   * job visible and subscribed.
   */
  it('shows a started job immediately on an empty jobs list', async () => {
    const job = {
      id: 1,
      state: 'HASHING',
      piecesDone: 0,
      piecesTotal: 8,
      error: null,
      hasTorrentData: false,
      outputPath: null,
    };
    request.mockImplementation((doc: unknown) => {
      if (doc === CreateJobsQuery) return Promise.resolve({ createJobs: [] });
      if (doc === StartCreateTorrentMutation) return Promise.resolve({ startCreateTorrent: job });
      return Promise.reject(new Error('unexpected request'));
    });

    render(
      <MemoryRouter>
        <QueryClientProvider
          client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}
        >
          <CreatePage />
        </QueryClientProvider>
      </MemoryRouter>,
    );
    expect(await screen.findByText(/No creation jobs/)).toBeInTheDocument();

    fireEvent.change(screen.getByLabelText(/Source file or directory/), {
      target: { value: '/data/things' },
    });
    fireEvent.click(screen.getByRole('button', { name: 'Start' }));

    expect(await screen.findByText('Job #1')).toBeInTheDocument();
    expect(screen.getByText('Hashing')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Cancel' })).toBeInTheDocument();
    expect(subscribeRetrying).toHaveBeenCalledWith(
      expect.anything(),
      expect.anything(),
      { id: 1 },
      expect.anything(),
    );
  });
});
