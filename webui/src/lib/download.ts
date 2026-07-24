// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

import { base64ToBytes } from './base64';

/** Trigger a browser download of raw bytes. */
export function downloadBytes(
  filename: string,
  bytes: Uint8Array,
  mime = 'application/x-bittorrent',
): void {
  const blob = new Blob([bytes.buffer as ArrayBuffer], { type: mime });
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = filename;
  a.click();
  setTimeout(() => URL.revokeObjectURL(url), 30_000);
}

export function downloadBase64(filename: string, base64: string): void {
  downloadBytes(filename, base64ToBytes(base64));
}
