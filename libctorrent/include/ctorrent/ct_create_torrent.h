/* Copyright (C) 2026  namazso <admin@namazso.eu>
 * SPDX-License-Identifier: MPL-2.0
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/.
 */

#ifndef CT_CREATE_TORRENT_H
#define CT_CREATE_TORRENT_H

#include <ctorrent/ct_torrent_info.h> /* CT_FILE_FLAG_* */
#include <ctorrent/ct_types.h>
#include <errno.h>
#include <stdint.h>
#include <stdbool.h>
#include <time.h>

#ifdef __cplusplus
extern "C" {
#endif

// Opaque handle to create_torrent builder
typedef struct ct_create_torrent ct_create_torrent;

typedef struct ct_create_file_entry {
    const char* path;           // relative path (borrowed)
    int64_t size;               // file size in bytes
    uint32_t flags;             // file_flags_t bitmask
    time_t mtime;               // modification time (0 = omit)
    const char* symlink_target; // symlink target path (borrowed, NULL if not symlink)
} ct_create_file_entry;

// File flags: CT_FILE_FLAG_* from ct_torrent_info.h (one definition only).

// Creation flags
#define CT_CREATE_MODIFICATION_TIME                0x04  // 2_bit
#define CT_CREATE_SYMLINKS                         0x08  // 3_bit
#define CT_CREATE_V2_ONLY                          0x20  // 5_bit
#define CT_CREATE_V1_ONLY                          0x40  // 6_bit
#define CT_CREATE_CANONICAL_FILES                  0x80  // 7_bit
#define CT_CREATE_NO_ATTRIBUTES                    0x100 // 8_bit
#define CT_CREATE_CANONICAL_FILES_NO_TAIL_PADDING  0x200 // 9_bit

// Create a new torrent builder from a list of files.
// piece_size: piece size in bytes (0 = auto, must be power of 2, min 16 KiB)
// flags: bitmask of CT_CREATE_* flags
ct_create_torrent* ct_create_torrent_new(
    const ct_create_file_entry* files,
    size_t file_count,
    int32_t piece_size,
    uint32_t flags,
    ct_error* err);

void ct_create_torrent_free(ct_create_torrent* ct);

// Generate the .torrent file as a bencoded buffer.
// Returns ct_buf (owned buffer) that must be freed with ct_buf_free().
ct_buf ct_create_torrent_generate_buf(const ct_create_torrent* ct, ct_error* err);

// Set torrent properties
void ct_create_torrent_set_comment(ct_create_torrent* ct, const char* comment, ct_error* err);
void ct_create_torrent_set_creator(ct_create_torrent* ct, const char* creator, ct_error* err);
void ct_create_torrent_set_creation_date(ct_create_torrent* ct, time_t timestamp, ct_error* err);

// Set piece hashes (v1 torrents)
// index: piece index (0-based); reported through *err if out of range
// hash: 20-byte SHA-1 hash
void ct_create_torrent_set_hash(ct_create_torrent* ct, int32_t index, const uint8_t hash[20], ct_error* err);

// Set piece hashes (v2 torrents)
// file_index: file index in the ORIGINAL entry order passed to
//             ct_create_torrent_new (0-based); translated internally to the
//             canonicalized order the builder uses (v2/hybrid construction
//             sorts files and inserts pad entries, which never consume
//             caller indices). Reported through *err if out of range or a
//             pad file.
// piece: piece index relative to first piece of file (0-based); reported
//        through *err if out of range for the file
// hash: 32-byte SHA-256 hash (merkle root of piece's blocks)
void ct_create_torrent_set_hash2(ct_create_torrent* ct, int32_t file_index, int32_t piece, const uint8_t hash[32], ct_error* err);

// Add sources
void ct_create_torrent_add_url_seed(ct_create_torrent* ct, const char* url, ct_error* err);
void ct_create_torrent_add_tracker(ct_create_torrent* ct, const char* url, int32_t tier, ct_error* err);
void ct_create_torrent_add_node(ct_create_torrent* ct, const char* hostname, int32_t port, ct_error* err);

// SSL torrents
void ct_create_torrent_set_root_cert(ct_create_torrent* ct, const char* pem, ct_error* err);

// Private flag
void ct_create_torrent_set_priv(ct_create_torrent* ct, bool is_private);
bool ct_create_torrent_priv(const ct_create_torrent* ct);

// BEP 38: similar torrents and collections
void ct_create_torrent_add_similar_torrent(ct_create_torrent* ct, const uint8_t info_hash[20], ct_error* err);
void ct_create_torrent_add_collection(ct_create_torrent* ct, const char* name, ct_error* err);

// Query properties
bool ct_create_torrent_is_v2_only(const ct_create_torrent* ct);
bool ct_create_torrent_is_v1_only(const ct_create_torrent* ct);
int32_t ct_create_torrent_num_pieces(const ct_create_torrent* ct);
int32_t ct_create_torrent_piece_length(const ct_create_torrent* ct);
int32_t ct_create_torrent_piece_size(const ct_create_torrent* ct, int32_t index);
int64_t ct_create_torrent_total_size(const ct_create_torrent* ct);

// Recursively list files in a directory for torrent creation.
// Returns array of file entries (owned). Count returned in *out_count.
// An empty directory is success: returns NULL with *out_count == 0 and no
// error set; distinguish failure by checking *err.
// Free with ct_create_file_list_free().
ct_create_file_entry* ct_list_files(const char* path, uint32_t flags, size_t* out_count, ct_error* err);
void ct_create_file_list_free(ct_create_file_entry* list, size_t count);

// Progress callback for set_piece_hashes.
// Called once per completed piece.
// piece_index: the piece that was just hashed
// userdata: user-provided pointer passed to ct_set_piece_hashes
// Return true to keep hashing; false aborts the run.
typedef bool (*ct_piece_hash_progress_fn)(int32_t piece_index, void* userdata);

// The generic-category error value ct_set_piece_hashes reports when the
// progress callback aborts the run.
#define CT_ERRC_OPERATION_CANCELED ECANCELED

// Read files and compute piece hashes.
// base_path: directory containing the files (files are relative to this)
// progress: optional progress callback (NULL = no callback); returning
//           false aborts the run and reports CT_ERRC_OPERATION_CANCELED
// userdata: passed to progress callback
void ct_set_piece_hashes(
    ct_create_torrent* ct,
    const char* base_path,
    ct_piece_hash_progress_fn progress,
    void* userdata,
    ct_error* err);

#ifdef __cplusplus
}
#endif

#endif // CT_CREATE_TORRENT_H
