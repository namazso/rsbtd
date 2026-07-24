/* Copyright (C) 2026  namazso <admin@namazso.eu>
 * SPDX-License-Identifier: MPL-2.0
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/.
 */

/* lt::add_torrent_params: the parameters for adding a torrent to a session,
 * a parsed .torrent file (ct_load_torrent_*), a parsed magnet link
 * (ct_parse_magnet_uri), and resume data.
 *
 * String/span/bitfield getters return borrowed storage owned by the params
 * object: valid until the same object is mutated or freed. Indexed
 * accessors require a valid index; out-of-range indices return a zeroed
 * value.
 *
 * Setters with no ct_error out-param are infallible: their only failure
 * mode is allocation, which terminates the process (matching Rust's
 * abort-on-OOM doctrine) rather than silently dropping the mutation.
 *
 * Not exposed: `userdata` (reserved by the bindings for request
 * correlation) and `extensions` (custom plugin vtables are out of scope).
 */
#ifndef CT_ADD_TORRENT_PARAMS_H_INCLUDED
#define CT_ADD_TORRENT_PARAMS_H_INCLUDED

#include "ct_torrent_info.h"
#include "ct_types.h"

#ifdef __cplusplus
extern "C" {
#endif

/* Opaque owning lt::add_torrent_params. */
typedef struct ct_add_torrent_params ct_add_torrent_params;

/* Returns NULL on allocation failure. A fresh object has libtorrent's
 * default flags set. */
ct_add_torrent_params* ct_atp_new(void);
ct_add_torrent_params* ct_atp_clone(const ct_add_torrent_params* atp);
void ct_atp_free(ct_add_torrent_params* atp);

/* ---- loading .torrent files ---------------------------------------------- */

/* Limits applied when parsing .torrent data, to protect against maliciously
 * crafted torrents. Converted field-by-field to lt::load_torrent_limits. */
typedef struct ct_load_torrent_limits {
  int32_t max_buffer_size;
  int32_t max_pieces;
  int32_t max_decode_depth;
  int32_t max_decode_tokens;
  int32_t max_duplicate_filenames;
  int32_t max_directory_depth;
} ct_load_torrent_limits;

/* libtorrent's default limits. */
ct_load_torrent_limits ct_load_torrent_limits_default(void);

/* Parse a .torrent file / bencoded buffer into add_torrent_params (fields
 * filled: ti, trackers, tracker_tiers, url_seeds, dht_nodes, info_hashes,
 * comment, created_by, creation_date, renamed_files, merkle trees).
 * `limits` may be NULL for the defaults. NULL and *err set on failure. */
ct_add_torrent_params* ct_load_torrent_file(ct_str_view path,
	const ct_load_torrent_limits* limits, ct_error* err);
ct_add_torrent_params* ct_load_torrent_buffer(ct_span buffer,
	const ct_load_torrent_limits* limits, ct_error* err);

/* ---- magnet links --------------------------------------------------------- */

/* Parse a magnet URI into add_torrent_params (fields filled: info_hashes,
 * name, trackers, url_seeds, dht_nodes, peers, file_priorities, flags).
 * NULL and *err set on failure. */
ct_add_torrent_params* ct_parse_magnet_uri(ct_str_view uri, ct_error* err);

/* Generate a magnet URI from ti/info_hashes, url_seeds, dht_nodes,
 * file_priorities, trackers, name and peers. Empty string if no info-hash
 * is available. */
ct_str ct_make_magnet_uri(const ct_add_torrent_params* atp, ct_error* err);

/* ---- torrent metadata (ti) ------------------------------------------------ */

/* Shares ownership of the torrent_info with the params object (cheap).
 * The getter returns a new handle (caller frees), or NULL if unset. The
 * setter accepts NULL to clear the field. */
ct_torrent_info* ct_atp_get_ti(const ct_add_torrent_params* atp);
void ct_atp_set_ti(ct_add_torrent_params* atp, const ct_torrent_info* ti);

/* Filled in by the constructor for forward binary compatibility. */
int32_t ct_atp_version(const ct_add_torrent_params* atp);

/* ---- strings -------------------------------------------------------------- */

ct_str_view ct_atp_name(const ct_add_torrent_params* atp);
void ct_atp_set_name(ct_add_torrent_params* atp, ct_str_view value);

ct_str_view ct_atp_save_path(const ct_add_torrent_params* atp);
void ct_atp_set_save_path(ct_add_torrent_params* atp, ct_str_view value);

ct_str_view ct_atp_part_file_dir(const ct_add_torrent_params* atp);
void ct_atp_set_part_file_dir(ct_add_torrent_params* atp, ct_str_view value);

ct_str_view ct_atp_trackerid(const ct_add_torrent_params* atp);
void ct_atp_set_trackerid(ct_add_torrent_params* atp, ct_str_view value);

ct_str_view ct_atp_comment(const ct_add_torrent_params* atp);
void ct_atp_set_comment(ct_add_torrent_params* atp, ct_str_view value);

ct_str_view ct_atp_created_by(const ct_add_torrent_params* atp);
void ct_atp_set_created_by(ct_add_torrent_params* atp, ct_str_view value);

ct_str_view ct_atp_root_certificate(const ct_add_torrent_params* atp);
void ct_atp_set_root_certificate(ct_add_torrent_params* atp,
	ct_str_view value);

/* ---- trackers -------------------------------------------------------------- */

size_t ct_atp_num_trackers(const ct_add_torrent_params* atp);
ct_str_view ct_atp_tracker(const ct_add_torrent_params* atp, size_t i);
/* tracker_tiers may be shorter than trackers; missing entries default to
 * tier 0 (or the last seen tier) per the multi-tracker extension. */
size_t ct_atp_num_tracker_tiers(const ct_add_torrent_params* atp);
int32_t ct_atp_tracker_tier(const ct_add_torrent_params* atp, size_t i);
/* Appends to both `trackers` and `tracker_tiers`, keeping them aligned. */
void ct_atp_add_tracker(ct_add_torrent_params* atp, ct_str_view url,
	int32_t tier);
void ct_atp_clear_trackers(ct_add_torrent_params* atp);

/* ---- DHT nodes -------------------------------------------------------------- */

size_t ct_atp_num_dht_nodes(const ct_add_torrent_params* atp);
bool ct_atp_dht_node(const ct_add_torrent_params* atp, size_t i,
	ct_str_view* host, int32_t* port);
void ct_atp_add_dht_node(ct_add_torrent_params* atp, ct_str_view host,
	int32_t port);
void ct_atp_clear_dht_nodes(ct_add_torrent_params* atp);

/* ---- web seeds -------------------------------------------------------------- */

size_t ct_atp_num_url_seeds(const ct_add_torrent_params* atp);
ct_str_view ct_atp_url_seed(const ct_add_torrent_params* atp, size_t i);
void ct_atp_add_url_seed(ct_add_torrent_params* atp, ct_str_view url);
void ct_atp_clear_url_seeds(ct_add_torrent_params* atp);

/* ---- storage mode ------------------------------------------------------------ */

/* Storage mode enum (lt::storage_mode_t values). */
typedef enum {
	CT_STORAGE_MODE_ALLOCATE = 0,
	CT_STORAGE_MODE_SPARSE = 1
} ct_storage_mode_t;

int32_t ct_atp_storage_mode(const ct_add_torrent_params* atp);
void ct_atp_set_storage_mode(ct_add_torrent_params* atp, int32_t mode);

/* ---- flags --------------------------------------------------------------------- */

/* Bits of lt::torrent_flags_t (verified by static_assert). */
#define CT_TORRENT_FLAG_SEED_MODE (1ull << 0)
#define CT_TORRENT_FLAG_UPLOAD_MODE (1ull << 1)
#define CT_TORRENT_FLAG_SHARE_MODE (1ull << 2)
#define CT_TORRENT_FLAG_APPLY_IP_FILTER (1ull << 3)
#define CT_TORRENT_FLAG_PAUSED (1ull << 4)
#define CT_TORRENT_FLAG_AUTO_MANAGED (1ull << 5)
#define CT_TORRENT_FLAG_DUPLICATE_IS_ERROR (1ull << 6)
#define CT_TORRENT_FLAG_UPDATE_SUBSCRIBE (1ull << 7)
#define CT_TORRENT_FLAG_SUPER_SEEDING (1ull << 8)
#define CT_TORRENT_FLAG_SEQUENTIAL_DOWNLOAD (1ull << 9)
#define CT_TORRENT_FLAG_STOP_WHEN_READY (1ull << 10)
#define CT_TORRENT_FLAG_NEED_SAVE_RESUME (1ull << 13)
#define CT_TORRENT_FLAG_DISABLE_DHT (1ull << 19)
#define CT_TORRENT_FLAG_DISABLE_LSD (1ull << 20)
#define CT_TORRENT_FLAG_DISABLE_PEX (1ull << 21)
#define CT_TORRENT_FLAG_NO_VERIFY_FILES (1ull << 22)
#define CT_TORRENT_FLAG_DEFAULT_DONT_DOWNLOAD (1ull << 23)
#define CT_TORRENT_FLAG_I2P_TORRENT (1ull << 24)
#define CT_TORRENT_FLAG_DISABLE_V1_HASHES (1ull << 25)
#define CT_TORRENT_FLAGS_ALL 0xffffffffffffffffull

/* The flags a fresh add_torrent_params starts with. */
#define CT_TORRENT_FLAGS_DEFAULT \
  (CT_TORRENT_FLAG_UPDATE_SUBSCRIBE | CT_TORRENT_FLAG_AUTO_MANAGED | \
   CT_TORRENT_FLAG_PAUSED | CT_TORRENT_FLAG_APPLY_IP_FILTER | \
   CT_TORRENT_FLAG_NEED_SAVE_RESUME)

uint64_t ct_atp_flags(const ct_add_torrent_params* atp);
void ct_atp_set_flags(ct_add_torrent_params* atp, uint64_t flags);

/* ---- info-hash ------------------------------------------------------------------- */

ct_info_hash ct_atp_info_hashes(const ct_add_torrent_params* atp);
void ct_atp_set_info_hashes(ct_add_torrent_params* atp,
	const ct_info_hash* value);

/* ---- limits and counters (-1 = unlimited / unknown) -------------------------------- */

int32_t ct_atp_max_uploads(const ct_add_torrent_params* atp);
void ct_atp_set_max_uploads(ct_add_torrent_params* atp, int32_t value);

int32_t ct_atp_max_connections(const ct_add_torrent_params* atp);
void ct_atp_set_max_connections(ct_add_torrent_params* atp, int32_t value);

int32_t ct_atp_upload_limit(const ct_add_torrent_params* atp);
void ct_atp_set_upload_limit(ct_add_torrent_params* atp, int32_t value);

int32_t ct_atp_download_limit(const ct_add_torrent_params* atp);
void ct_atp_set_download_limit(ct_add_torrent_params* atp, int32_t value);

/* Scrape data: seeds / non-seeds / completed downloads in the swarm. */
int32_t ct_atp_num_complete(const ct_add_torrent_params* atp);
void ct_atp_set_num_complete(ct_add_torrent_params* atp, int32_t value);

int32_t ct_atp_num_incomplete(const ct_add_torrent_params* atp);
void ct_atp_set_num_incomplete(ct_add_torrent_params* atp, int32_t value);

int32_t ct_atp_num_downloaded(const ct_add_torrent_params* atp);
void ct_atp_set_num_downloaded(ct_add_torrent_params* atp, int32_t value);

/* ---- resume statistics ---------------------------------------------------------- */

int64_t ct_atp_total_uploaded(const ct_add_torrent_params* atp);
void ct_atp_set_total_uploaded(ct_add_torrent_params* atp, int64_t value);

int64_t ct_atp_total_downloaded(const ct_add_torrent_params* atp);
void ct_atp_set_total_downloaded(ct_add_torrent_params* atp, int64_t value);

/* Seconds spent in started / finished / seeding state. */
int32_t ct_atp_active_time(const ct_add_torrent_params* atp);
void ct_atp_set_active_time(ct_add_torrent_params* atp, int32_t value);

int32_t ct_atp_finished_time(const ct_add_torrent_params* atp);
void ct_atp_set_finished_time(ct_add_torrent_params* atp, int32_t value);

int32_t ct_atp_seeding_time(const ct_add_torrent_params* atp);
void ct_atp_set_seeding_time(ct_add_torrent_params* atp, int32_t value);

/* Posix timestamps; 0 = unknown / not set. */
int64_t ct_atp_added_time(const ct_add_torrent_params* atp);
void ct_atp_set_added_time(ct_add_torrent_params* atp, int64_t value);

int64_t ct_atp_completed_time(const ct_add_torrent_params* atp);
void ct_atp_set_completed_time(ct_add_torrent_params* atp, int64_t value);

int64_t ct_atp_last_seen_complete(const ct_add_torrent_params* atp);
void ct_atp_set_last_seen_complete(ct_add_torrent_params* atp, int64_t value);

int64_t ct_atp_last_download(const ct_add_torrent_params* atp);
void ct_atp_set_last_download(ct_add_torrent_params* atp, int64_t value);

int64_t ct_atp_last_upload(const ct_add_torrent_params* atp);
void ct_atp_set_last_upload(ct_add_torrent_params* atp, int64_t value);

int64_t ct_atp_creation_date(const ct_add_torrent_params* atp);
void ct_atp_set_creation_date(ct_add_torrent_params* atp, int64_t value);

/* ---- priorities ------------------------------------------------------------------- */

/* Byte-per-file / byte-per-piece download priorities (0-7, see
 * ct_download_priority). Borrowed. */
ct_span ct_atp_file_priorities(const ct_add_torrent_params* atp);
void ct_atp_set_file_priorities(ct_add_torrent_params* atp,
	const uint8_t* priorities, size_t len);

ct_span ct_atp_piece_priorities(const ct_add_torrent_params* atp);
void ct_atp_set_piece_priorities(ct_add_torrent_params* atp,
	const uint8_t* priorities, size_t len);

/* ---- piece state (resume data) ------------------------------------------------------ */

ct_bitfield_view ct_atp_have_pieces(const ct_add_torrent_params* atp);
void ct_atp_set_have_pieces(ct_add_torrent_params* atp, const uint8_t* bytes,
	int32_t num_bits);

ct_bitfield_view ct_atp_verified_pieces(const ct_add_torrent_params* atp);
void ct_atp_set_verified_pieces(ct_add_torrent_params* atp,
	const uint8_t* bytes, int32_t num_bits);

/* Partially downloaded pieces: per piece, a bitfield with one bit per
 * 16 kiB block. Entries are iterated in piece-index order. */
size_t ct_atp_num_unfinished_pieces(const ct_add_torrent_params* atp);
bool ct_atp_unfinished_piece(const ct_add_torrent_params* atp, size_t i,
	ct_piece_index* piece, ct_bitfield_view* blocks);
void ct_atp_add_unfinished_piece(ct_add_torrent_params* atp,
	ct_piece_index piece, const uint8_t* bytes, int32_t num_bits);
void ct_atp_clear_unfinished_pieces(ct_add_torrent_params* atp);

/* ---- v2 merkle trees ------------------------------------------------------------------ */

/* Per-file sparse merkle trees (see add_torrent_params::merkle_trees).
 * A tree is returned as concatenated 32-byte SHA-256 hashes; the mask, if
 * non-empty, marks which nodes of the full tree the hashes correspond to.
 * The setters resize all three vectors to `num` first (via
 * ct_atp_set_num_merkle_trees). */
size_t ct_atp_num_merkle_trees(const ct_add_torrent_params* atp);
ct_span ct_atp_merkle_tree(const ct_add_torrent_params* atp,
	ct_file_index file);
ct_bitfield_view ct_atp_merkle_tree_mask(const ct_add_torrent_params* atp,
	ct_file_index file);
ct_bitfield_view ct_atp_verified_leaf_hashes(const ct_add_torrent_params* atp,
	ct_file_index file);
void ct_atp_set_num_merkle_trees(ct_add_torrent_params* atp, size_t num);
/* hashes.len must be a multiple of 32. */
void ct_atp_set_merkle_tree(ct_add_torrent_params* atp, ct_file_index file,
	ct_span hashes);
void ct_atp_set_merkle_tree_mask(ct_add_torrent_params* atp,
	ct_file_index file, const uint8_t* bytes, int32_t num_bits);
void ct_atp_set_verified_leaf_hashes(ct_add_torrent_params* atp,
	ct_file_index file, const uint8_t* bytes, int32_t num_bits);

/* ---- renamed files ----------------------------------------------------------------------- */

/* File renames applied when the torrent is added. Entries are iterated in
 * file-index order. Negative indices — and, when metadata (`ti`) is set,
 * indices at or past its file count — are ignored. */
size_t ct_atp_num_renamed_files(const ct_add_torrent_params* atp);
bool ct_atp_renamed_file(const ct_add_torrent_params* atp, size_t i,
	ct_file_index* file, ct_str_view* name);
void ct_atp_add_renamed_file(ct_add_torrent_params* atp, ct_file_index file,
	ct_str_view name);
void ct_atp_clear_renamed_files(ct_add_torrent_params* atp);

/* ---- peers -------------------------------------------------------------------------------- */

size_t ct_atp_num_peers(const ct_add_torrent_params* atp);
ct_endpoint ct_atp_peer(const ct_add_torrent_params* atp, size_t i);
void ct_atp_add_peer(ct_add_torrent_params* atp, const ct_endpoint* peer);
void ct_atp_clear_peers(ct_add_torrent_params* atp);

size_t ct_atp_num_banned_peers(const ct_add_torrent_params* atp);
ct_endpoint ct_atp_banned_peer(const ct_add_torrent_params* atp, size_t i);
void ct_atp_add_banned_peer(ct_add_torrent_params* atp,
	const ct_endpoint* peer);
void ct_atp_clear_banned_peers(ct_add_torrent_params* atp);

#ifdef __cplusplus
}
#endif

#endif
