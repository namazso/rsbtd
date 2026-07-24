/* Copyright (C) 2026  namazso <admin@namazso.eu>
 * SPDX-License-Identifier: MPL-2.0
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/.
 */

/* Torrent handle — control surface for a single torrent. All operations are
 * thread-safe and take a const handle (lt::torrent_handle's members are
 * const); only clone/drop mutate the handle storage itself. Every function
 * is a no-op (or returns a default) on a NULL or invalid handle, and no
 * C++ exception ever crosses this boundary.
 */
#ifndef CT_TORRENT_HANDLE_H_INCLUDED
#define CT_TORRENT_HANDLE_H_INCLUDED

#include "ct_add_torrent_params.h" /* CT_TORRENT_FLAG_* */
#include "ct_types.h"

typedef struct ct_torrent_info ct_torrent_info;
typedef struct ct_torrent_status ct_torrent_status;

#ifdef __cplusplus
extern "C" {
#endif

/* Masquerade storage for lt::torrent_handle (weak_ptr). Trivially relocatable
 * (memcpy-move OK), never duplicate (clone/drop protocol). See
 * abi_asserts.cpp ("masquerade relocatability") for the rationale. */
typedef struct ct_torrent_handle {
  CT_ALIGNAS(CT_ALIGNOF_LT_TORRENT_HANDLE)
  uint8_t data_[CT_SIZEOF_LT_TORRENT_HANDLE];
} ct_torrent_handle;

/* Creates a deep copy of the handle (shares the same underlying torrent). */
void ct_torrent_handle_clone(const ct_torrent_handle* src,
	ct_torrent_handle* dst);

/* Releases the handle (does not affect the torrent itself). */
void ct_torrent_handle_drop(ct_torrent_handle* handle);

/* Returns true if the handle refers to a valid torrent (the torrent may have
 * been removed from the session). */
bool ct_torrent_handle_is_valid(const ct_torrent_handle* handle);

/* Returns a session-unique identifier for this torrent (stable across resume).
 * Invalid handles return 0. */
uint32_t ct_torrent_handle_id(const ct_torrent_handle* handle);

/* Returns the info hashes (v1/v2). Fields are zeroed for absent hashes. */
ct_info_hash ct_torrent_handle_info_hashes(const ct_torrent_handle* handle);

/* Returns true if this handle is currently tracked by the session. */
bool ct_torrent_handle_in_session(const ct_torrent_handle* handle);

/* ---- torrent control ------------------------------------------------------ */

/* Torrent flags are bits of lt::torrent_flags_t; use the CT_TORRENT_FLAG_*
 * constants from ct_add_torrent_params.h (there is deliberately no second
 * flag vocabulary here). */

uint64_t ct_torrent_handle_flags(const ct_torrent_handle* handle);

/* Set the bits selected by *mask* to the corresponding bits of *flags*;
 * bits outside *mask* are left unchanged. */
void ct_torrent_handle_set_flags(const ct_torrent_handle* handle,
	uint64_t flags, uint64_t mask);

/* Clear the bits in *flags*. */
void ct_torrent_handle_unset_flags(const ct_torrent_handle* handle,
	uint64_t flags);

/* ---- piece operations ----------------------------------------------------- */

/* lt::torrent_handle deadline flags (verified by static_assert). */
#define CT_DEADLINE_ALERT_WHEN_AVAILABLE (1u << 0)

/* Requests that piece *index* be read from disk and delivered via
 * read_piece_alert. Multiple requests are queued. */
void ct_torrent_handle_read_piece(const ct_torrent_handle* handle,
	int32_t piece);

bool ct_torrent_handle_have_piece(const ct_torrent_handle* handle, int32_t piece);

/* Schedules piece *index* to be downloaded with deadline *deadline_ms*
 * milliseconds from now. Flags: CT_DEADLINE_*. */
void ct_torrent_handle_set_piece_deadline(const ct_torrent_handle* handle,
	int32_t piece, int32_t deadline_ms, uint32_t flags);

void ct_torrent_handle_reset_piece_deadline(const ct_torrent_handle* handle,
	int32_t piece);

void ct_torrent_handle_clear_piece_deadlines(const ct_torrent_handle* handle);

/* ---- status and resume data ----------------------------------------------- */

/* lt::torrent_handle status query flags (verified by static_assert). */
#define CT_STATUS_QUERY_DISTRIBUTED_COPIES          (1u << 0)
#define CT_STATUS_QUERY_ACCURATE_DOWNLOAD_COUNTERS  (1u << 1)
#define CT_STATUS_QUERY_LAST_SEEN_COMPLETE          (1u << 2)
#define CT_STATUS_QUERY_PIECES                      (1u << 3)
#define CT_STATUS_QUERY_VERIFIED_PIECES             (1u << 4)
#define CT_STATUS_QUERY_TORRENT_FILE                (1u << 5)
#define CT_STATUS_QUERY_NAME                        (1u << 6)
#define CT_STATUS_QUERY_SAVE_PATH                   (1u << 7)

/* lt::torrent_handle resume data flags (verified by static_assert). */
#define CT_RESUME_FLUSH_DISK_CACHE       (1u << 0)
#define CT_RESUME_SAVE_INFO_DICT         (1u << 1)
#define CT_RESUME_IF_COUNTERS_CHANGED    (1u << 3)
#define CT_RESUME_IF_DOWNLOAD_PROGRESS   (1u << 4)
#define CT_RESUME_IF_CONFIG_CHANGED      (1u << 5)
#define CT_RESUME_IF_STATE_CHANGED       (1u << 6)
#define CT_RESUME_IF_METADATA_CHANGED    (1u << 7)
#define CT_RESUME_ONLY_IF_MODIFIED \
	(CT_RESUME_IF_COUNTERS_CHANGED | CT_RESUME_IF_DOWNLOAD_PROGRESS \
	 | CT_RESUME_IF_CONFIG_CHANGED | CT_RESUME_IF_STATE_CHANGED \
	 | CT_RESUME_IF_METADATA_CHANGED)

/* Posts a state_update_alert with this torrent's status. Flags:
 * CT_STATUS_QUERY_* (0 = all non-optional fields). */
void ct_torrent_handle_post_status(const ct_torrent_handle* handle,
	uint32_t flags);

/* Requests resume data be saved and delivered via save_resume_data_alert.
 * Flags: CT_RESUME_* (0 = defaults). Returns false when the request was
 * not posted (null or invalid handle): no alert will follow. */
bool ct_torrent_handle_save_resume_data(const ct_torrent_handle* handle,
	uint32_t flags);

/* Returns true if resume data has changed since last save. */
bool ct_torrent_handle_need_save_resume_data(const ct_torrent_handle* handle);

/* ---- file operations ------------------------------------------------------ */

/* lt::torrent_handle file_progress flag (verified by static_assert):
 * report progress at piece granularity, counting whole pieces instead of
 * exact byte counts. Cheaper, but ignores partially downloaded pieces. */
#define CT_FILE_PROGRESS_PIECE_GRANULARITY (1u << 0)

/* Posts a file_progress alert with per-file byte counts. Flags:
 * CT_FILE_PROGRESS_* (0 = exact byte counts). */
void ct_torrent_handle_post_file_progress(const ct_torrent_handle* handle,
	uint32_t flags);

/* ---- priorities and limits ------------------------------------------------ */

/* Sets the download priority for piece *index*. 0=do not download, 1..7=priority,
 * higher is more urgent. */
void ct_torrent_handle_piece_priority_set(const ct_torrent_handle* handle,
	int32_t piece, uint8_t priority);

uint8_t ct_torrent_handle_piece_priority_get(const ct_torrent_handle* handle,
	int32_t piece);

/* Sets download priorities for all pieces. *priorities* must have
 * torrent_info->num_pieces() elements. */
void ct_torrent_handle_prioritize_pieces(const ct_torrent_handle* handle,
	const uint8_t* priorities, size_t count);

void ct_torrent_handle_file_priority_set(const ct_torrent_handle* handle,
	int32_t file, uint8_t priority);

uint8_t ct_torrent_handle_file_priority_get(const ct_torrent_handle* handle,
	int32_t file);

/* Sets download priorities for all files. *priorities* must have
 * torrent_info->num_files() elements. */
void ct_torrent_handle_prioritize_files(const ct_torrent_handle* handle,
	const uint8_t* priorities, size_t count);

/* Upload/download rate limits in bytes/sec. -1 = unlimited, 0 = use session
 * default. */
void ct_torrent_handle_set_upload_limit(const ct_torrent_handle* handle,
	int32_t limit);
void ct_torrent_handle_set_download_limit(const ct_torrent_handle* handle,
	int32_t limit);
int32_t ct_torrent_handle_upload_limit(const ct_torrent_handle* handle);
int32_t ct_torrent_handle_download_limit(const ct_torrent_handle* handle);

/* Max simultaneous uploads/connections. -1 = unlimited; otherwise the limit
 * must be >= 2 (libtorrent stores it in a 24-bit field and silently treats
 * 0 and 1 as unlimited). */
void ct_torrent_handle_set_max_uploads(const ct_torrent_handle* handle,
	int32_t limit);
void ct_torrent_handle_set_max_connections(const ct_torrent_handle* handle,
	int32_t limit);
int32_t ct_torrent_handle_max_uploads(const ct_torrent_handle* handle);
int32_t ct_torrent_handle_max_connections(const ct_torrent_handle* handle);

/* ---- trackers ------------------------------------------------------------- */

/* lt::torrent_handle reannounce flags (verified by static_assert). */
#define CT_REANNOUNCE_IGNORE_MIN_INTERVAL (1u << 0)
#define CT_REANNOUNCE_HIGH_PRIORITY       (1u << 1)

/* Posts a tracker_list_alert with current trackers. */
void ct_torrent_handle_post_trackers(const ct_torrent_handle* handle);

/* Adds a tracker announce URL with tier *tier* (lt::announce_entry::tier
 * is 8 bits wide). */
void ct_torrent_handle_add_tracker(const ct_torrent_handle* handle,
	ct_str_view url, uint8_t tier);

/* Replaces the full tracker list (count 0 removes all trackers). *urls* and
 * *tiers* are parallel arrays of *count* entries; the strings are copied
 * before return. */
void ct_torrent_handle_replace_trackers(const ct_torrent_handle* handle,
	const ct_str_view* urls, const uint8_t* tiers, size_t count);

/* Forces a tracker announce in *seconds* seconds (0 = now) to tracker
 * *tracker_index* (-1 = all). Flags: CT_REANNOUNCE_*. */
void ct_torrent_handle_force_reannounce(const ct_torrent_handle* handle,
	int32_t seconds, int32_t tracker_index, uint32_t flags);

void ct_torrent_handle_scrape_tracker(const ct_torrent_handle* handle,
	int32_t tracker_index);

/* ---- web seeds (BEP 19) ----------------------------------------------------- */

void ct_torrent_handle_add_url_seed(const ct_torrent_handle* handle,
	ct_str_view url);

void ct_torrent_handle_remove_url_seed(const ct_torrent_handle* handle,
	ct_str_view url);

/* Returns the current url-seeds as an owned list. Caller frees with
 * ct_str_list_free(). */
ct_str_list* ct_torrent_handle_url_seeds(const ct_torrent_handle* handle,
	ct_error* err);

/* ---- queue position ------------------------------------------------------- */

/* Returns the queue position (0-based, -1 if not queued or invalid handle). */
int32_t ct_torrent_handle_queue_position(const ct_torrent_handle* handle);

void ct_torrent_handle_queue_position_up(const ct_torrent_handle* handle);
void ct_torrent_handle_queue_position_down(const ct_torrent_handle* handle);
void ct_torrent_handle_queue_position_top(const ct_torrent_handle* handle);
void ct_torrent_handle_queue_position_bottom(const ct_torrent_handle* handle);
void ct_torrent_handle_queue_position_set(const ct_torrent_handle* handle,
	int32_t pos);

/* ---- control -------------------------------------------------------------- */

/* lt::torrent_handle pause flags (verified by static_assert). */
#define CT_PAUSE_GRACEFUL (1u << 0)

/* Pauses the torrent. Flags: CT_PAUSE_*. */
void ct_torrent_handle_pause(const ct_torrent_handle* handle, uint32_t flags);

void ct_torrent_handle_resume(const ct_torrent_handle* handle);

/* Forces a full hash recheck of all pieces. */
void ct_torrent_handle_force_recheck(const ct_torrent_handle* handle);

/* Flushes disk write cache for this torrent. */
void ct_torrent_handle_flush_cache(const ct_torrent_handle* handle);

/* Clears the error status (allows auto-managed torrents to retry). */
void ct_torrent_handle_clear_error(const ct_torrent_handle* handle);

/* Forces a DHT announce (if DHT is enabled). */
void ct_torrent_handle_force_dht_announce(const ct_torrent_handle* handle);

/* lt::move_flags_t values (an enum, not a bitmask; verified by
 * static_assert). */
#define CT_MOVE_ALWAYS_REPLACE_FILES        0
#define CT_MOVE_FAIL_IF_EXIST               1
#define CT_MOVE_DONT_REPLACE                2
#define CT_MOVE_RESET_SAVE_PATH             3
#define CT_MOVE_RESET_SAVE_PATH_UNCHECKED   4

/* Moves storage to *path*. *flags* is one CT_MOVE_* value. Generates
 * storage_moved_alert or storage_moved_failed_alert. */
void ct_torrent_handle_move_storage(const ct_torrent_handle* handle,
	ct_str_view path, uint32_t flags);

/* Renames file *index* to *name* (relative to save_path). Generates
 * file_renamed_alert or file_rename_failed_alert. */
void ct_torrent_handle_rename_file(const ct_torrent_handle* handle,
	int32_t index, ct_str_view name);

/* ---- SSL ------------------------------------------------------------------ */

/* Sets the SSL certificate for this torrent (paths to .pem files). */
void ct_torrent_handle_set_ssl_certificate(const ct_torrent_handle* handle,
	ct_str_view cert, ct_str_view private_key, ct_str_view dh_params,
	ct_str_view passphrase);

/* Sets the SSL certificate from in-memory buffers. */
void ct_torrent_handle_set_ssl_certificate_buffer(
	const ct_torrent_handle* handle, ct_str_view cert,
	ct_str_view private_key, ct_str_view dh_params);

/* ---- peer management ------------------------------------------------------ */

/* Connect to a peer at the given endpoint. The port in *endpoint* is required.
 * This is a fire-and-forget operation; the connection attempt proceeds
 * asynchronously. peer_connect_alert indicates success/failure. Returns false
 * if the handle is invalid or the endpoint is malformed. */
bool ct_torrent_handle_connect_peer(const ct_torrent_handle* handle,
	ct_endpoint endpoint, ct_error* err);

/* Posts a peer_info_alert with current peer list. */
void ct_torrent_handle_post_peer_info(const ct_torrent_handle* handle);

/* ---- accessors ------------------------------------------------------------ */

/* Returns an owned torrent_status snapshot. Flags: CT_STATUS_QUERY_* (0 = all
 * non-optional fields). Caller must free with ct_torrent_status_free(). */
ct_torrent_status* ct_torrent_handle_status(const ct_torrent_handle* handle,
	uint32_t flags, ct_error* err);

/* Returns the save path (owned string); a status() query under the hood. */
ct_str ct_torrent_handle_save_path(const ct_torrent_handle* handle, ct_error* err);

/* Returns the torrent name (owned string); a status() query under the hood. */
ct_str ct_torrent_handle_name(const ct_torrent_handle* handle, ct_error* err);

/* Returns a new reference to the torrent_info (NULL if no metadata). Caller
 * owns the returned pointer. */
ct_torrent_info* ct_torrent_handle_torrent_file(const ct_torrent_handle* handle,
	ct_error* err);

/* Returns each file's current path in file-index order (relative to the
 * save path unless renamed to an absolute path), reflecting renames
 * applied via ct_torrent_handle_rename_file. NULL without an error when
 * metadata is not available yet. Caller frees with ct_str_list_free(). */
ct_str_list* ct_torrent_handle_file_paths(const ct_torrent_handle* handle,
	ct_error* err);

#ifdef __cplusplus
}
#endif

#endif
