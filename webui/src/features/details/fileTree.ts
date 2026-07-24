// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

import type { TorrentFilesQuery } from '@/gen/gql/graphql';

export type TorrentFile = NonNullable<NonNullable<TorrentFilesQuery['torrent']>['files']>[number];

export interface FileTreeRow {
  /** Unique path-based id. */
  id: string;
  depth: number;
  name: string;
  isDir: boolean;
  /** Leaf file (undefined for directories). */
  file?: TorrentFile;
  /** Aggregates (directories: over non-pad descendants). */
  size: number;
  progressBytes: number;
  /** Uniform priority of descendants, or null when mixed. */
  priority: number | null;
  /** File indexes covered (self for leaves, descendants for dirs). */
  fileIndexes: number[];
}

interface DirNode {
  name: string;
  path: string;
  dirs: Map<string, DirNode>;
  files: TorrentFile[];
}

/**
 * Path → tree (directories aggregate size/progress/priority), flattened in
 * metadata order with collapsed directories skipped.
 */
export function buildFileTree(
  files: readonly TorrentFile[],
  showPadFiles: boolean,
  collapsed: ReadonlySet<string>,
): FileTreeRow[] {
  const root: DirNode = { name: '', path: '', dirs: new Map(), files: [] };
  for (const file of files) {
    if (!showPadFiles && file.isPadFile) continue;
    const parts = file.path.split(/[\\/]/).filter((p) => p !== '');
    let node = root;
    for (let i = 0; i < parts.length - 1; i++) {
      const part = parts[i]!;
      let child = node.dirs.get(part);
      if (!child) {
        child = {
          name: part,
          path: node.path === '' ? part : `${node.path}/${part}`,
          dirs: new Map(),
          files: [],
        };
        node.dirs.set(part, child);
      }
      node = child;
    }
    node.files.push(file);
  }

  const rows: FileTreeRow[] = [];

  interface Aggregate {
    size: number;
    progressBytes: number;
    priority: number | null;
    fileIndexes: number[];
    any: boolean;
  }

  const visit = (node: DirNode, depth: number, hidden: boolean): Aggregate => {
    const agg: Aggregate = {
      size: 0,
      progressBytes: 0,
      priority: null,
      fileIndexes: [],
      any: false,
    };
    const merge = (size: number, progress: number, priority: number, indexes: number[]) => {
      agg.size += size;
      agg.progressBytes += progress;
      agg.priority = agg.any ? (agg.priority === priority ? priority : null) : priority;
      agg.fileIndexes.push(...indexes);
      agg.any = true;
    };

    for (const dir of node.dirs.values()) {
      const rowIndex = rows.length;
      const isCollapsed = collapsed.has(dir.path);
      if (!hidden) {
        rows.push({
          id: dir.path,
          depth,
          name: dir.name,
          isDir: true,
          size: 0,
          progressBytes: 0,
          priority: null,
          fileIndexes: [],
        });
      }
      const sub = visit(dir, depth + 1, hidden || isCollapsed);
      if (!hidden) {
        const row = rows[rowIndex]!;
        row.size = sub.size;
        row.progressBytes = sub.progressBytes;
        row.priority = sub.priority;
        row.fileIndexes = sub.fileIndexes;
      }
      if (sub.any) {
        merge(sub.size, sub.progressBytes, sub.priority ?? -1, sub.fileIndexes);
        if (sub.priority === null) agg.priority = null;
      }
    }

    for (const file of node.files) {
      if (!hidden) {
        rows.push({
          id: `f:${file.index}`,
          depth,
          name: file.path.split(/[\\/]/).pop() ?? file.path,
          isDir: false,
          file,
          size: file.size,
          progressBytes: file.progressBytes,
          priority: file.priority,
          fileIndexes: [file.index],
        });
      }
      merge(file.size, file.progressBytes, file.priority, [file.index]);
    }
    return agg;
  };

  visit(root, 0, false);
  return rows;
}

/**
 * Full-length priorities list with `overrides` applied on top of the
 * snapshot — setFilePriorities fills an omitted tail with default 4, so
 * we always send every file's current value.
 */
export function buildPriorityList(
  files: readonly TorrentFile[],
  overrides: ReadonlyMap<number, number>,
): number[] {
  const maxIndex = files.reduce((m, f) => Math.max(m, f.index), -1);
  const list = new Array<number>(maxIndex + 1).fill(4);
  for (const file of files) list[file.index] = file.priority;
  for (const [index, priority] of overrides) {
    if (index >= 0 && index < list.length) list[index] = priority;
  }
  return list;
}
