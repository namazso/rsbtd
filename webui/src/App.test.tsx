// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import '@/i18n';
import App from './App';

/**
 * Render smoke test for the app shell. The connection probe fails fast in
 * the test environment (no daemon): the shell renders while the probe is
 * in flight, then the connect page (with the endpoint editor) takes over
 * for the `down` state.
 */
describe('App', () => {
  it('renders the shell, then keeps the endpoint editor reachable when down', async () => {
    render(<App />);
    expect(screen.getByPlaceholderText(/Search or filter/)).toBeInTheDocument();
    // sidebar categories
    expect(screen.getByText('Downloading')).toBeInTheDocument();
    expect(screen.getByText('Seeding')).toBeInTheDocument();

    // Probe failure -> down: the user must be able to fix the endpoint.
    expect(
      await screen.findByRole('heading', { name: 'Connect to rsbtd' }, { timeout: 3_000 }),
    ).toBeInTheDocument();
    fireEvent.click(screen.getByText('Advanced'));
    expect(screen.getByLabelText('GraphQL endpoint')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /Connect|Connecting/ })).toBeInTheDocument();
  });
});
