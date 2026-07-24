// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

import { useEffect, useRef } from 'react';
import { base64ToBytes } from '@/lib/base64';

/**
 * Read-only downloaded-pieces bar (per plan: display only — no per-piece
 * actions). Bit 7 of byte 0 is piece 0. Each pixel column shows
 * the fraction of its piece range that is present.
 */
export function PieceBar({
  bitfield,
  totalPieces,
  className,
}: {
  bitfield: string | null | undefined;
  totalPieces: number;
  className?: string;
}) {
  const canvasRef = useRef<HTMLCanvasElement>(null);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;

    const draw = () => {
      const ctx = canvas.getContext('2d');
      if (!ctx) return;
      const dpr = window.devicePixelRatio || 1;
      const width = Math.max(1, Math.floor(canvas.clientWidth * dpr));
      const height = Math.max(1, Math.floor(canvas.clientHeight * dpr));
      canvas.width = width;
      canvas.height = height;

      const styles = getComputedStyle(canvas);
      const baseColor = styles.getPropertyValue('--muted').trim() || '#ddd';
      const fillColor = styles.getPropertyValue('--st-download').trim() || '#46f';
      ctx.fillStyle = baseColor;
      ctx.fillRect(0, 0, width, height);
      if (bitfield == null || totalPieces <= 0) return;

      const bytes = base64ToBytes(bitfield);
      const bitCount = Math.min(totalPieces, bytes.length * 8);
      const hasPiece = (i: number) => {
        const byte = bytes[i >> 3];
        return byte !== undefined && (byte & (0x80 >> (i & 7))) !== 0;
      };

      ctx.fillStyle = fillColor;
      for (let x = 0; x < width; x++) {
        const from = Math.floor((x / width) * bitCount);
        const to = Math.max(from + 1, Math.floor(((x + 1) / width) * bitCount));
        let have = 0;
        for (let i = from; i < to; i++) if (hasPiece(i)) have++;
        const frac = have / (to - from);
        if (frac > 0) {
          ctx.globalAlpha = 0.25 + 0.75 * frac;
          ctx.fillRect(x, 0, 1, height);
        }
      }
      ctx.globalAlpha = 1;
    };

    draw();
    const observer = new ResizeObserver(draw);
    observer.observe(canvas);
    return () => observer.disconnect();
  }, [bitfield, totalPieces]);

  return <canvas ref={canvasRef} className={className ?? 'h-3 w-full rounded-sm'} aria-hidden />;
}
