// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

import { fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import '@/i18n';

const reconnect = vi.fn();
const snap = {
  state: 'authRequired',
  generation: 0,
  error: null,
  authFailed: false,
  sessionError: null,
};

vi.mock('@/store/connection', () => ({
  connection: { reconnect: () => reconnect() },
  useConnection: (selector: (s: unknown) => unknown) => selector(snap),
}));

import { getToken } from '@/api/auth';
import LoginPage from './LoginPage';

/** The value of the (endpoint-namespaced) token key in `storage`. */
function storedToken(storage: Storage): string | null {
  const key = Object.keys(storage).find((k) => k.startsWith('rsbtd.token'));
  return key === undefined ? null : storage.getItem(key);
}

describe('LoginPage', () => {
  beforeEach(() => {
    reconnect.mockClear();
    snap.authFailed = false;
    localStorage.clear();
    sessionStorage.clear();
  });

  it('stores the token per remember choice and reconnects', () => {
    render(<LoginPage />);
    fireEvent.change(screen.getByLabelText('API token'), { target: { value: 'secret' } });
    fireEvent.click(screen.getByRole('button', { name: 'Connect' }));

    expect(reconnect).toHaveBeenCalledOnce();
    // remember defaults to checked -> localStorage
    expect(storedToken(localStorage)).toBe('secret');
    expect(storedToken(sessionStorage)).toBeNull();
  });

  it('uses sessionStorage when remember is unchecked', () => {
    render(<LoginPage />);
    fireEvent.change(screen.getByLabelText('API token'), { target: { value: 'secret' } });
    fireEvent.click(screen.getByLabelText('Stay signed in'));
    fireEvent.click(screen.getByRole('button', { name: 'Connect' }));

    expect(storedToken(sessionStorage)).toBe('secret');
    expect(storedToken(localStorage)).toBeNull();
  });

  it('clears stored tokens when submitted empty', () => {
    // Both a current (namespaced) and a legacy token must be cleared.
    localStorage.setItem('rsbtd.token', 'legacy');
    render(<LoginPage />);
    fireEvent.change(screen.getByLabelText('API token'), { target: { value: 'secret' } });
    fireEvent.click(screen.getByRole('button', { name: 'Connect' }));
    expect(getToken()).toBe('secret');

    fireEvent.change(screen.getByLabelText('API token'), { target: { value: '' } });
    fireEvent.click(screen.getByRole('button', { name: 'Connect' }));
    expect(storedToken(localStorage)).toBeNull();
    expect(getToken()).toBeNull();
    expect(reconnect).toHaveBeenCalledTimes(2);
  });

  it('shows an error when the daemon rejected the credentials', () => {
    snap.authFailed = true;
    render(<LoginPage />);
    expect(screen.getByRole('alert')).toHaveTextContent(/rejected/i);
  });
});
