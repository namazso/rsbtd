// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

import { useState, type FormEvent } from 'react';
import { useTranslation } from 'react-i18next';
import { clearToken, setToken } from '@/api/auth';
import { getEndpointSetting, setEndpointSetting } from '@/api/endpoint';
import { tDynamic } from '@/lib/i18nDynamic';
import { connection, useConnection } from '@/store/connection';

/**
 * Shown whenever the daemon requires (different) credentials: HTTP 401 or a
 * failed WebSocket init classified as unauthorized.
 */
export default function LoginPage() {
  const { t } = useTranslation('auth');
  const state = useConnection((s) => s.state);
  const error = useConnection((s) => s.error);
  const authFailed = useConnection((s) => s.authFailed);
  const [token, setTokenInput] = useState('');
  const [remember, setRemember] = useState(true);
  const [endpoint, setEndpoint] = useState(getEndpointSetting() ?? '');
  const busy = state === 'probing' || state === 'connecting';

  const submit = (e: FormEvent) => {
    e.preventDefault();
    if (busy) return;
    setEndpointSetting(endpoint === '' ? null : endpoint);
    const trimmed = token.trim();
    if (trimmed === '') clearToken();
    else setToken(trimmed, remember);
    connection.reconnect();
  };

  return (
    <main className="flex min-h-dvh items-center justify-center p-4">
      <form
        onSubmit={submit}
        className="w-full max-w-sm rounded-xl border border-neutral-200 bg-white p-6 shadow-sm dark:border-neutral-800 dark:bg-neutral-900"
      >
        <h1 className="mb-1 text-xl font-semibold">{t('title')}</h1>
        <p className="mb-4 text-sm text-neutral-500 dark:text-neutral-400">{t('subtitle')}</p>

        <label className="mb-1 block text-sm font-medium" htmlFor="login-token">
          {t('tokenLabel')}
        </label>
        <input
          id="login-token"
          type="password"
          autoComplete="current-password"
          value={token}
          onChange={(e) => setTokenInput(e.target.value)}
          placeholder={t('tokenPlaceholder')}
          className="mb-3 w-full rounded-md border border-neutral-300 bg-transparent px-3 py-2 text-sm outline-none focus:ring-2 focus:ring-blue-500 dark:border-neutral-700"
        />

        <label className="mb-4 flex items-center gap-2 text-sm">
          <input
            type="checkbox"
            checked={remember}
            onChange={(e) => setRemember(e.target.checked)}
            className="size-4 accent-blue-600"
          />
          {t('remember')}
        </label>

        <details className="mb-4">
          <summary className="cursor-pointer text-sm text-neutral-500 dark:text-neutral-400">
            {t('advanced')}
          </summary>
          <label className="mt-2 mb-1 block text-sm font-medium" htmlFor="login-endpoint">
            {t('endpointLabel')}
          </label>
          <input
            id="login-endpoint"
            type="text"
            value={endpoint}
            onChange={(e) => setEndpoint(e.target.value)}
            placeholder="/graphql"
            className="w-full rounded-md border border-neutral-300 bg-transparent px-3 py-2 text-sm outline-none focus:ring-2 focus:ring-blue-500 dark:border-neutral-700"
          />
          <p className="mt-1 text-xs text-neutral-500 dark:text-neutral-400">{t('endpointHint')}</p>
        </details>

        {authFailed && (
          <p role="alert" className="mb-3 text-sm text-red-600 dark:text-red-400">
            {tDynamic('auth:authFailed', { defaultValue: 'The daemon rejected this token.' })}
          </p>
        )}
        {error !== null && state !== 'up' && (
          <p role="alert" className="mb-3 text-sm text-red-600 dark:text-red-400">
            {error}
          </p>
        )}

        <button
          type="submit"
          disabled={busy}
          className="w-full rounded-md bg-blue-600 px-3 py-2 text-sm font-medium text-white hover:bg-blue-700 disabled:opacity-50"
        >
          {busy ? t('checking') : t('submit')}
        </button>
      </form>
    </main>
  );
}
