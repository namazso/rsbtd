// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

import { useCallback, useRef } from 'react';

/**
 * iOS-safe long-press detection via pointer events (iOS Safari has no
 * `contextmenu` for touch): fires after `ms` unless the pointer moves more
 * than `slop` px, lifts, or scrolling cancels it.
 */
export function useLongPress(
  onLongPress: () => void,
  { ms = 450, slop = 10 }: { ms?: number; slop?: number } = {},
) {
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const origin = useRef<{ x: number; y: number } | null>(null);
  const fired = useRef(false);

  const cancel = useCallback(() => {
    if (timer.current !== null) {
      clearTimeout(timer.current);
      timer.current = null;
    }
    origin.current = null;
  }, []);

  const onPointerDown = useCallback(
    (e: React.PointerEvent) => {
      if (e.pointerType === 'mouse' && e.button !== 0) return;
      fired.current = false;
      origin.current = { x: e.clientX, y: e.clientY };
      timer.current = setTimeout(() => {
        fired.current = true;
        timer.current = null;
        onLongPress();
      }, ms);
    },
    [onLongPress, ms],
  );

  const onPointerMove = useCallback(
    (e: React.PointerEvent) => {
      const o = origin.current;
      if (o === null) return;
      if (Math.abs(e.clientX - o.x) > slop || Math.abs(e.clientY - o.y) > slop) cancel();
    },
    [cancel, slop],
  );

  /** True exactly once when the long-press consumed this gesture. */
  const consumedClick = useCallback(() => {
    const was = fired.current;
    fired.current = false;
    return was;
  }, []);

  return {
    handlers: {
      onPointerDown,
      onPointerMove,
      onPointerUp: cancel,
      onPointerCancel: cancel,
      onPointerLeave: cancel,
      onContextMenu: (e: React.MouseEvent) => e.preventDefault(),
    },
    consumedClick,
  };
}
