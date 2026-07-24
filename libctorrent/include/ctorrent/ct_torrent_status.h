/* Copyright (C) 2026  namazso <admin@namazso.eu>
 * SPDX-License-Identifier: MPL-2.0
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/.
 */

/* Torrent status API (lt::torrent_status snapshot). */
#ifndef CTORRENT_TORRENT_STATUS_H
#define CTORRENT_TORRENT_STATUS_H

#include "ct_types.h"
#include "ct_add_torrent_params.h"

#ifdef __cplusplus
extern "C" {
#endif

/* Opaque handle to an owned torrent_status snapshot. */
typedef struct ct_torrent_status ct_torrent_status;

/* State enum (torrent_status::state_t). */
typedef enum {
	CT_TORRENT_STATE_CHECKING_FILES = 1,
	CT_TORRENT_STATE_DOWNLOADING_METADATA = 2,
	CT_TORRENT_STATE_DOWNLOADING = 3,
	CT_TORRENT_STATE_FINISHED = 4,
	CT_TORRENT_STATE_SEEDING = 5,
	CT_TORRENT_STATE_CHECKING_RESUME_DATA = 7
} ct_torrent_state_t;

/* ---- Lifecycle ----------------------------------------------------------- */

void ct_torrent_status_free(ct_torrent_status* st);

/* ---- Accessors ----------------------------------------------------------- */

/* Session-unique id of the torrent this snapshot describes, captured when
 * the snapshot was taken — it stays meaningful even after the torrent is
 * removed (a live handle's id() reads 0 then). 0 when the torrent was
 * already gone at snapshot time. */
uint32_t ct_torrent_status_id(const ct_torrent_status* st);

/* The torrent's info-hash(es); absent hashes zeroed. */
ct_info_hash ct_torrent_status_info_hashes(const ct_torrent_status* st);

ct_torrent_state_t ct_torrent_status_state(const ct_torrent_status* st);

/* The torrent's error state as a full ct_error (category == CT_ERROR_CAT_NONE
 * when the torrent has no error). category_ptr is set, so ct_error_message
 * works from any thread. */
ct_error ct_torrent_status_error(const ct_torrent_status* st);

/* Error file index (-1 if none, or special values < -1). */
int32_t ct_torrent_status_error_file(const ct_torrent_status* st);

/* Save path (only if queried with query_save_path flag). */
ct_str_view ct_torrent_status_save_path(const ct_torrent_status* st);

/* Torrent name (only if queried with query_name flag). */
ct_str_view ct_torrent_status_name(const ct_torrent_status* st);

/* Next announce time (seconds from now). */
int64_t ct_torrent_status_next_announce_seconds(const ct_torrent_status* st);

/* Current tracker URL (empty if none successful yet). */
ct_str_view ct_torrent_status_current_tracker(const ct_torrent_status* st);

/* Byte counters (session stats, reset on pause). */
int64_t ct_torrent_status_total_download(const ct_torrent_status* st);
int64_t ct_torrent_status_total_upload(const ct_torrent_status* st);
int64_t ct_torrent_status_total_payload_download(const ct_torrent_status* st);
int64_t ct_torrent_status_total_payload_upload(const ct_torrent_status* st);
int64_t ct_torrent_status_total_failed_bytes(const ct_torrent_status* st);
int64_t ct_torrent_status_total_redundant_bytes(const ct_torrent_status* st);

/* Total bytes done/wanted. */
int64_t ct_torrent_status_total_done(const ct_torrent_status* st);
int64_t ct_torrent_status_total(const ct_torrent_status* st);
int64_t ct_torrent_status_total_wanted_done(const ct_torrent_status* st);
int64_t ct_torrent_status_total_wanted(const ct_torrent_status* st);

/* All-time stats (persistent across sessions). */
int64_t ct_torrent_status_all_time_upload(const ct_torrent_status* st);
int64_t ct_torrent_status_all_time_download(const ct_torrent_status* st);

/* Time fields (posix time_t). */
int64_t ct_torrent_status_added_time(const ct_torrent_status* st);
int64_t ct_torrent_status_completed_time(const ct_torrent_status* st);
int64_t ct_torrent_status_last_seen_complete(const ct_torrent_status* st);

ct_storage_mode_t ct_torrent_status_storage_mode(const ct_torrent_status* st);

/* Progress (0.0 to 1.0) and ppm (0 to 1000000). */
float ct_torrent_status_progress(const ct_torrent_status* st);
int32_t ct_torrent_status_progress_ppm(const ct_torrent_status* st);

/* Queue position (-1 if not queued). */
int32_t ct_torrent_status_queue_position(const ct_torrent_status* st);

/* Transfer rates (bytes per second). */
int32_t ct_torrent_status_download_rate(const ct_torrent_status* st);
int32_t ct_torrent_status_upload_rate(const ct_torrent_status* st);
int32_t ct_torrent_status_download_payload_rate(const ct_torrent_status* st);
int32_t ct_torrent_status_upload_payload_rate(const ct_torrent_status* st);

/* Peer counts. */
int32_t ct_torrent_status_num_seeds(const ct_torrent_status* st);
int32_t ct_torrent_status_num_peers(const ct_torrent_status* st);
int32_t ct_torrent_status_num_complete(const ct_torrent_status* st);
int32_t ct_torrent_status_num_incomplete(const ct_torrent_status* st);
int32_t ct_torrent_status_list_seeds(const ct_torrent_status* st);
int32_t ct_torrent_status_list_peers(const ct_torrent_status* st);
int32_t ct_torrent_status_connect_candidates(const ct_torrent_status* st);

int32_t ct_torrent_status_num_pieces(const ct_torrent_status* st);

/* Distributed copies. */
int32_t ct_torrent_status_distributed_full_copies(const ct_torrent_status* st);
int32_t ct_torrent_status_distributed_fraction(const ct_torrent_status* st);
float ct_torrent_status_distributed_copies(const ct_torrent_status* st);

/* Block size (typically 16 KiB). */
int32_t ct_torrent_status_block_size(const ct_torrent_status* st);

/* Connection and upload slot counts. */
int32_t ct_torrent_status_num_uploads(const ct_torrent_status* st);
int32_t ct_torrent_status_num_connections(const ct_torrent_status* st);
int32_t ct_torrent_status_uploads_limit(const ct_torrent_status* st);
int32_t ct_torrent_status_connections_limit(const ct_torrent_status* st);

/* Rate limits. */
int32_t ct_torrent_status_upload_limit(const ct_torrent_status* st);
int32_t ct_torrent_status_download_limit(const ct_torrent_status* st);

/* Bandwidth queue sizes. */
int32_t ct_torrent_status_up_bandwidth_queue(const ct_torrent_status* st);
int32_t ct_torrent_status_down_bandwidth_queue(const ct_torrent_status* st);

int32_t ct_torrent_status_seed_rank(const ct_torrent_status* st);

/* Resume data flags. */
uint32_t ct_torrent_status_need_save_resume_data(const ct_torrent_status* st);

/* Boolean flags. */
bool ct_torrent_status_is_seeding(const ct_torrent_status* st);
bool ct_torrent_status_is_finished(const ct_torrent_status* st);
bool ct_torrent_status_has_metadata(const ct_torrent_status* st);
bool ct_torrent_status_has_incoming(const ct_torrent_status* st);
bool ct_torrent_status_moving_storage(const ct_torrent_status* st);
bool ct_torrent_status_announcing_to_trackers(const ct_torrent_status* st);
bool ct_torrent_status_announcing_to_lsd(const ct_torrent_status* st);
bool ct_torrent_status_announcing_to_dht(const ct_torrent_status* st);

/* Piece bitfield (NULL if not available; len = num_pieces, owned by status). */
const uint8_t* ct_torrent_status_pieces(const ct_torrent_status* st, size_t* out_len);
const uint8_t* ct_torrent_status_verified_pieces(const ct_torrent_status* st, size_t* out_len);

#ifdef __cplusplus
}
#endif

#endif /* CTORRENT_TORRENT_STATUS_H */
