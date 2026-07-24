// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

import { describe, expect, it } from 'vitest';
import { buildFileTree, buildPriorityList, type TorrentFile } from './fileTree';

function file(index: number, path: string): TorrentFile {
  return {
    index,
    path,
    size: 100,
    offset: 0,
    priority: 4,
    progressBytes: 50,
    isPadFile: false,
    isSymlink: false,
    symlinkTarget: null,
    isExecutable: false,
    isHidden: false,
  };
}

describe('buildFileTree', () => {
  it('splits forward-slash paths into directories', () => {
    const rows = buildFileTree([file(0, 'dir/sub/a.bin')], false, new Set());
    expect(rows.map((r) => [r.name, r.depth, r.isDir])).toEqual([
      ['dir', 0, true],
      ['sub', 1, true],
      ['a.bin', 2, false],
    ]);
  });

  it('splits backslash (Windows) paths into directories', () => {
    const rows = buildFileTree(
      [file(0, 'dir\\sub\\a.bin'), file(1, 'dir\\b.bin')],
      false,
      new Set(),
    );
    expect(rows.map((r) => [r.name, r.depth, r.isDir])).toEqual([
      ['dir', 0, true],
      ['sub', 1, true],
      ['a.bin', 2, false],
      ['b.bin', 1, false],
    ]);
    // The original path is preserved for rename operations.
    expect(rows[2]!.file?.path).toBe('dir\\sub\\a.bin');
  });

  it('aggregates directory size and progress', () => {
    const rows = buildFileTree(
      [file(0, 'dir\\sub\\a.bin'), file(1, 'dir\\b.bin')],
      false,
      new Set(),
    );
    expect(rows[0]).toMatchObject({ size: 200, progressBytes: 100, fileIndexes: [0, 1] });
  });
});

describe('buildPriorityList', () => {
  it('applies pending overrides on top of the snapshot', () => {
    const files = [file(0, 'a'), { ...file(2, 'c'), priority: 7 }];
    const overrides = new Map([
      [0, 0],
      [1, 6],
    ]);
    // Index 1 has no snapshot entry (gap defaults to 4 before override);
    // index 2 keeps its snapshot priority.
    expect(buildPriorityList(files, overrides)).toEqual([0, 6, 7]);
    expect(buildPriorityList(files, new Map())).toEqual([4, 4, 7]);
  });
});
