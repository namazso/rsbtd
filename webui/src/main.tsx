// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { applyHashParams } from './api/hashParams';
import App from './App';
import './i18n';
import './index.css';

// Before anything reads the endpoint or token, and before the router sees
// the hash.
applyHashParams();

const container = document.getElementById('root');
if (!container) throw new Error('missing #root element');

createRoot(container).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
