/* Copyright (C) 2026  namazso <admin@namazso.eu>
 * SPDX-License-Identifier: MPL-2.0
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/.
 */

/* Read access to torrent metadata (lt::torrent_info) and its file list.
 * A populated ct_torrent_info is obtained by loading a .torrent file via
 * ct_load_torrent_file/ct_load_torrent_buffer (see ct_add_torrent_params.h)
 * and reading the `ti` field of the resulting add_torrent_params.
 *
 * Indexed accessors require a valid index (0 <= i < the corresponding
 * count); out-of-range indices return a zeroed value.
 */
#ifndef CT_TORRENT_INFO_H_INCLUDED
#define CT_TORRENT_INFO_H_INCLUDED

#include "ct_types.h"

#ifdef __cplusplus
extern "C" {
#endif

/* Opaque shared handle to an immutable lt::torrent_info. Cloning is cheap
 * (shared ownership of the same object). */
typedef struct ct_torrent_info ct_torrent_info;

/* Metadata-less torrent_info carrying only an info-hash (magnet-style).
 * Returns NULL on allocation failure. */
ct_torrent_info* ct_torrent_info_from_info_hash(const ct_info_hash* ih);

ct_torrent_info* ct_torrent_info_clone(const ct_torrent_info* ti);
void ct_torrent_info_free(ct_torrent_info* ti);

/* ---- torrent-wide properties -------------------------------------------- */

/* True if metadata is loaded (an info-hash-only object returns false). */
bool ct_torrent_info_is_valid(const ct_torrent_info* ti);

/* Name of the torrent (UTF-8). Borrowed; valid while the handle lives. */
ct_str_view ct_torrent_info_name(const ct_torrent_info* ti);

/* Total byte count including pad files / excluding pad files. */
int64_t ct_torrent_info_total_size(const ct_torrent_info* ti);
int64_t ct_torrent_info_size_on_disk(const ct_torrent_info* ti);

int32_t ct_torrent_info_piece_length(const ct_torrent_info* ti);
int32_t ct_torrent_info_num_pieces(const ct_torrent_info* ti);
int32_t ct_torrent_info_blocks_per_piece(const ct_torrent_info* ti);

ct_info_hash ct_torrent_info_info_hashes(const ct_torrent_info* ti);
bool ct_torrent_info_has_v1(const ct_torrent_info* ti);
bool ct_torrent_info_has_v2(const ct_torrent_info* ti);

/* Size of the given piece: piece_length() except possibly the last piece.
 * The _for_req variant accounts for v2-only torrents whose pieces may be
 * truncated at file boundaries. */
int32_t ct_torrent_info_piece_size(const ct_torrent_info* ti,
	ct_piece_index piece);
int32_t ct_torrent_info_piece_size_for_req(const ct_torrent_info* ti,
	ct_piece_index piece);

/* SHA-1 hash of a piece (v1/hybrid torrents). */
ct_sha1 ct_torrent_info_hash_for_piece(const ct_torrent_info* ti,
	ct_piece_index piece);

/* SSL root certificate (x509, PEM) for SSL torrents; empty otherwise.
 * Borrowed; valid while the handle lives. */
ct_str_view ct_torrent_info_ssl_cert(const ct_torrent_info* ti);

bool ct_torrent_info_is_private(const ct_torrent_info* ti);
bool ct_torrent_info_is_i2p(const ct_torrent_info* ti);

/* The raw bencoded info section. Borrowed; valid while the handle lives. */
ct_span ct_torrent_info_info_section(const ct_torrent_info* ti);

/* BEP 38: "similar" info-hashes, returned as concatenated 20-byte SHA-1
 * digests (len is a multiple of 20). Owned. */
ct_buf ct_torrent_info_similar_torrents(const ct_torrent_info* ti);

/* BEP 38: "collections" strings. Owned list; NULL on allocation failure. */
ct_str_list* ct_torrent_info_collections(const ct_torrent_info* ti);

/* ---- file list ----------------------------------------------------------- */

/* Bits of lt::file_flags_t (verified by static_assert). */
#define CT_FILE_FLAG_PAD_FILE (1u << 0)
#define CT_FILE_FLAG_HIDDEN (1u << 1)
#define CT_FILE_FLAG_EXECUTABLE (1u << 2)
#define CT_FILE_FLAG_SYMLINK (1u << 3)

int32_t ct_torrent_info_num_files(const ct_torrent_info* ti);

int64_t ct_torrent_info_file_size(const ct_torrent_info* ti,
	ct_file_index file);

/* Full path of the file inside the torrent (UTF-8). Owned. */
ct_str ct_torrent_info_file_path(const ct_torrent_info* ti,
	ct_file_index file);

/* Just the file name. Borrowed; valid while the handle lives. */
ct_str_view ct_torrent_info_file_name(const ct_torrent_info* ti,
	ct_file_index file);

/* Byte offset of the file within the torrent's data. */
int64_t ct_torrent_info_file_offset(const ct_torrent_info* ti,
	ct_file_index file);

/* CT_FILE_FLAG_* bits. */
uint8_t ct_torrent_info_file_flags(const ct_torrent_info* ti,
	ct_file_index file);

/* Symlink target if the file is a symlink, empty otherwise. Owned. */
ct_str ct_torrent_info_file_symlink(const ct_torrent_info* ti,
	ct_file_index file);

/* Posix modification time recorded in the torrent, or 0. */
int64_t ct_torrent_info_file_mtime(const ct_torrent_info* ti,
	ct_file_index file);

/* SHA-256 merkle root of the file (v2 torrents); zeros otherwise. */
ct_sha256 ct_torrent_info_file_root(const ct_torrent_info* ti,
	ct_file_index file);

/* Number of pieces/blocks the file spans, assuming piece alignment (only
 * meaningful for v2 torrents). */
int32_t ct_torrent_info_file_num_pieces(const ct_torrent_info* ti,
	ct_file_index file);
int32_t ct_torrent_info_file_num_blocks(const ct_torrent_info* ti,
	ct_file_index file);

/* True if the file has been renamed to an absolute path (not anchored in
 * the torrent's save path). */
bool ct_torrent_info_file_absolute_path(const ct_torrent_info* ti,
	ct_file_index file);

/* ---- piece/file mapping --------------------------------------------------- */

/* Maps a byte range within a file to the piece-space request covering it.
 * Returns a zeroed request unless 0 <= offset, 0 <= size and
 * offset + size <= file size (validated overflow-safely). */
ct_peer_request ct_torrent_info_map_file(const ct_torrent_info* ti,
	ct_file_index file, int64_t offset, int32_t size);

/* A window of a file, as returned by ct_torrent_info_map_block. */
typedef struct ct_file_slice {
  ct_file_index file_index;
  int64_t offset;
  int64_t size;
} ct_file_slice;

/* Owning array of ct_file_slice. A zeroed value is valid and empty. */
typedef struct ct_file_slice_array {
  const ct_file_slice* ptr;
  size_t len;
  void* box_;
} ct_file_slice_array;

void ct_file_slice_array_free(ct_file_slice_array* array);

/* Maps a byte range within a piece to the file windows storing it. Returns
 * an empty array unless the range is within the torrent (0 <= offset,
 * 0 <= size, piece * piece_length + offset + size <= total_size; validated
 * overflow-safely). */
ct_file_slice_array ct_torrent_info_map_block(const ct_torrent_info* ti,
	ct_piece_index piece, int64_t offset, int32_t size);

/* File containing the given byte offset / the first (or last) file
 * overlapping the given piece. */
ct_file_index ct_torrent_info_file_index_at_offset(const ct_torrent_info* ti,
	int64_t offset);
ct_file_index ct_torrent_info_file_index_at_piece(const ct_torrent_info* ti,
	ct_piece_index piece);
ct_file_index ct_torrent_info_last_file_index_at_piece(
	const ct_torrent_info* ti, ct_piece_index piece);

/* File with the given v2 merkle root, or -1. */
ct_file_index ct_torrent_info_file_index_for_root(const ct_torrent_info* ti,
	const ct_sha256* root);

/* First / last piece overlapping the given file. */
ct_piece_index ct_torrent_info_piece_index_at_file(const ct_torrent_info* ti,
	ct_file_index file);
ct_piece_index ct_torrent_info_last_piece_index_at_file(
	const ct_torrent_info* ti, ct_file_index file);

#ifdef __cplusplus
}
#endif

#endif
