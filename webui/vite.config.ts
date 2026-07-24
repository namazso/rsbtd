// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

/// <reference types="vitest/config" />
import { fileURLToPath } from 'node:url';
import tailwindcss from '@tailwindcss/vite';
import react from '@vitejs/plugin-react';
import { defineConfig } from 'vite';

// In development the UI talks to the daemon directly (no proxy) — the dev
// daemon config allows the Vite origin via its `cors` option. RSBTD_URL
// points the dev server at another daemon; production builds always default
// to same-origin /graphql (see src/api/endpoint.ts).
const daemonUrl = process.env.RSBTD_URL ?? 'http://127.0.0.1:3928';

export default defineConfig(({ mode }) => ({
  // Relative asset paths: the built SPA works from any path on any static host.
  base: './',
  plugins: [react(), tailwindcss()],
  resolve: {
    alias: { '@': fileURLToPath(new URL('./src', import.meta.url)) },
  },
  define: {
    __DEV_GRAPHQL_URL__: JSON.stringify(
      mode === 'development' ? new URL('/graphql', daemonUrl).toString() : null,
    ),
  },
  test: {
    environment: 'happy-dom',
    globals: true,
    setupFiles: './test/setup.ts',
    include: ['src/**/*.test.{ts,tsx}'],
  },
}));
