// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

import i18next from 'i18next';
import { initReactI18next } from 'react-i18next';
import auth from './en/auth.json';
import common from './en/common.json';
import create from './en/create.json';
import details from './en/details.json';
import settings from './en/settings.json';
import stats from './en/stats.json';
import torrents from './en/torrents.json';

export const defaultNS = 'common';

/**
 * English-only for now, but every user-facing string flows through
 * i18next (enforced by eslint-plugin-i18next).
 */
export const resources = {
  en: {
    common,
    auth,
    torrents,
    details,
    settings,
    create,
    stats,
  },
} as const;

void i18next.use(initReactI18next).init({
  lng: 'en',
  fallbackLng: 'en',
  supportedLngs: ['en'],
  defaultNS,
  resources,
  interpolation: {
    // React already escapes interpolated values.
    escapeValue: false,
  },
});

export default i18next;
