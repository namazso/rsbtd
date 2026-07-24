// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

import { toast } from 'sonner';
import { tDynamic } from '@/lib/i18nDynamic';

/** Copies immediately available text, with the usual outcome toasts. */
export async function copyText(text: string): Promise<void> {
  try {
    await navigator.clipboard.writeText(text);
    toast.success(tDynamic('common:clipboard.copied'));
  } catch {
    toast.error(tDynamic('common:clipboard.failed'));
  }
}

/**
 * Copies text that still has to be produced (a network round-trip). A
 * plain `await produce(); writeText(...)` runs the write after the
 * click's transient user activation may have expired, and the browser
 * rejects it; handing ClipboardItem the promise instead pins the
 * activation at call time. Browsers without promise-accepting
 * ClipboardItem fall back to write-after-await.
 */
export async function copyTextFrom(produce: () => Promise<string>): Promise<void> {
  try {
    if (typeof ClipboardItem === 'undefined') {
      await navigator.clipboard.writeText(await produce());
    } else {
      const item = new ClipboardItem({
        'text/plain': produce().then((text) => new Blob([text], { type: 'text/plain' })),
      });
      await navigator.clipboard.write([item]);
    }
    toast.success(tDynamic('common:clipboard.copied'));
  } catch {
    toast.error(tDynamic('common:clipboard.failed'));
  }
}
