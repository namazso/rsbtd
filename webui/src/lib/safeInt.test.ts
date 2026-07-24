// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

import { describe, expect, it, vi } from 'vitest';
import { safeInt } from './safeInt';

describe('safeInt', () => {
  it('returns safe integers unchanged without warning', () => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
    expect(safeInt(0)).toBe(0);
    expect(safeInt(Number.MAX_SAFE_INTEGER)).toBe(Number.MAX_SAFE_INTEGER);
    expect(warn).not.toHaveBeenCalled();
    warn.mockRestore();
  });

  it('passes through and warns (dev) on unsafe integers', () => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
    const huge = 2 ** 60;
    expect(safeInt(huge)).toBe(huge);
    expect(warn).toHaveBeenCalledOnce();
    warn.mockRestore();
  });
});
