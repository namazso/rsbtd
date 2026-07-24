// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

/* Torrent status implementation. */
#include "ctorrent/ct_torrent_status.h"
#include "ct_common.hpp"
#include "handles.hpp"

#include <libtorrent/torrent_status.hpp>
#include <libtorrent/bitfield.hpp>
#include <cstring>

namespace ct {

static lt::torrent_status& st_of(ct_torrent_status* st) {
	return st->st;
}

static const lt::torrent_status& st_of(const ct_torrent_status* st) {
	return st->st;
}

} // namespace ct

extern "C" {

void ct_torrent_status_free(ct_torrent_status* st) {
	if (!st) return;
	delete st;
}

uint32_t ct_torrent_status_id(const ct_torrent_status* st) {
	if (!st) return 0;
	return st->id;
}

ct_info_hash ct_torrent_status_info_hashes(const ct_torrent_status* st) {
	ct_info_hash result = {};
	if (!st) return result;
	auto const& hashes = ct::st_of(st).info_hashes;
	if (hashes.has_v1())
		std::memcpy(result.v1.data, hashes.v1.data(), 20);
	if (hashes.has_v2())
		std::memcpy(result.v2.data, hashes.v2.data(), 32);
	return result;
}

ct_torrent_state_t ct_torrent_status_state(const ct_torrent_status* st) {
	if (!st) return CT_TORRENT_STATE_CHECKING_RESUME_DATA;
	return static_cast<ct_torrent_state_t>(ct::st_of(st).state);
}

ct_error ct_torrent_status_error(const ct_torrent_status* st) {
	ct_error err{0, CT_ERROR_CAT_NONE, nullptr};
	if (!st) return err;
	ct::set_error(&err, ct::st_of(st).errc);
	return err;
}

int32_t ct_torrent_status_error_file(const ct_torrent_status* st) {
	if (!st) return -1;
	return static_cast<int32_t>(ct::st_of(st).error_file);
}

ct_str_view ct_torrent_status_save_path(const ct_torrent_status* st) {
	if (!st) return {nullptr, 0};
	return ct::to_str_view(ct::st_of(st).save_path);
}

ct_str_view ct_torrent_status_name(const ct_torrent_status* st) {
	if (!st) return {nullptr, 0};
	return ct::to_str_view(ct::st_of(st).name);
}

int64_t ct_torrent_status_next_announce_seconds(const ct_torrent_status* st) {
	if (!st) return 0;
	return lt::duration_cast<lt::seconds>(ct::st_of(st).next_announce).count();
}

ct_str_view ct_torrent_status_current_tracker(const ct_torrent_status* st) {
	if (!st) return {nullptr, 0};
	return ct::to_str_view(ct::st_of(st).current_tracker);
}

int64_t ct_torrent_status_total_download(const ct_torrent_status* st) {
	if (!st) return 0;
	return ct::st_of(st).total_download;
}

int64_t ct_torrent_status_total_upload(const ct_torrent_status* st) {
	if (!st) return 0;
	return ct::st_of(st).total_upload;
}

int64_t ct_torrent_status_total_payload_download(const ct_torrent_status* st) {
	if (!st) return 0;
	return ct::st_of(st).total_payload_download;
}

int64_t ct_torrent_status_total_payload_upload(const ct_torrent_status* st) {
	if (!st) return 0;
	return ct::st_of(st).total_payload_upload;
}

int64_t ct_torrent_status_total_failed_bytes(const ct_torrent_status* st) {
	if (!st) return 0;
	return ct::st_of(st).total_failed_bytes;
}

int64_t ct_torrent_status_total_redundant_bytes(const ct_torrent_status* st) {
	if (!st) return 0;
	return ct::st_of(st).total_redundant_bytes;
}

int64_t ct_torrent_status_total_done(const ct_torrent_status* st) {
	if (!st) return 0;
	return ct::st_of(st).total_done;
}

int64_t ct_torrent_status_total(const ct_torrent_status* st) {
	if (!st) return 0;
	return ct::st_of(st).total;
}

int64_t ct_torrent_status_total_wanted_done(const ct_torrent_status* st) {
	if (!st) return 0;
	return ct::st_of(st).total_wanted_done;
}

int64_t ct_torrent_status_total_wanted(const ct_torrent_status* st) {
	if (!st) return 0;
	return ct::st_of(st).total_wanted;
}

int64_t ct_torrent_status_all_time_upload(const ct_torrent_status* st) {
	if (!st) return 0;
	return ct::st_of(st).all_time_upload;
}

int64_t ct_torrent_status_all_time_download(const ct_torrent_status* st) {
	if (!st) return 0;
	return ct::st_of(st).all_time_download;
}

int64_t ct_torrent_status_added_time(const ct_torrent_status* st) {
	if (!st) return 0;
	return ct::st_of(st).added_time;
}

int64_t ct_torrent_status_completed_time(const ct_torrent_status* st) {
	if (!st) return 0;
	return ct::st_of(st).completed_time;
}

int64_t ct_torrent_status_last_seen_complete(const ct_torrent_status* st) {
	if (!st) return 0;
	return ct::st_of(st).last_seen_complete;
}

ct_storage_mode_t ct_torrent_status_storage_mode(const ct_torrent_status* st) {
	if (!st) return CT_STORAGE_MODE_SPARSE;
	auto mode = ct::st_of(st).storage_mode;
	return static_cast<ct_storage_mode_t>(static_cast<int>(mode));
}

float ct_torrent_status_progress(const ct_torrent_status* st) {
	if (!st) return 0.0f;
	return ct::st_of(st).progress;
}

int32_t ct_torrent_status_progress_ppm(const ct_torrent_status* st) {
	if (!st) return 0;
	return ct::st_of(st).progress_ppm;
}

int32_t ct_torrent_status_queue_position(const ct_torrent_status* st) {
	if (!st) return -1;
	return static_cast<int32_t>(ct::st_of(st).queue_position);
}

int32_t ct_torrent_status_download_rate(const ct_torrent_status* st) {
	if (!st) return 0;
	return ct::st_of(st).download_rate;
}

int32_t ct_torrent_status_upload_rate(const ct_torrent_status* st) {
	if (!st) return 0;
	return ct::st_of(st).upload_rate;
}

int32_t ct_torrent_status_download_payload_rate(const ct_torrent_status* st) {
	if (!st) return 0;
	return ct::st_of(st).download_payload_rate;
}

int32_t ct_torrent_status_upload_payload_rate(const ct_torrent_status* st) {
	if (!st) return 0;
	return ct::st_of(st).upload_payload_rate;
}

int32_t ct_torrent_status_num_seeds(const ct_torrent_status* st) {
	if (!st) return 0;
	return ct::st_of(st).num_seeds;
}

int32_t ct_torrent_status_num_peers(const ct_torrent_status* st) {
	if (!st) return 0;
	return ct::st_of(st).num_peers;
}

int32_t ct_torrent_status_num_complete(const ct_torrent_status* st) {
	if (!st) return -1;
	return ct::st_of(st).num_complete;
}

int32_t ct_torrent_status_num_incomplete(const ct_torrent_status* st) {
	if (!st) return -1;
	return ct::st_of(st).num_incomplete;
}

int32_t ct_torrent_status_list_seeds(const ct_torrent_status* st) {
	if (!st) return 0;
	return ct::st_of(st).list_seeds;
}

int32_t ct_torrent_status_list_peers(const ct_torrent_status* st) {
	if (!st) return 0;
	return ct::st_of(st).list_peers;
}

int32_t ct_torrent_status_connect_candidates(const ct_torrent_status* st) {
	if (!st) return 0;
	return ct::st_of(st).connect_candidates;
}

int32_t ct_torrent_status_num_pieces(const ct_torrent_status* st) {
	if (!st) return 0;
	return ct::st_of(st).num_pieces;
}

int32_t ct_torrent_status_distributed_full_copies(const ct_torrent_status* st) {
	if (!st) return 0;
	return ct::st_of(st).distributed_full_copies;
}

int32_t ct_torrent_status_distributed_fraction(const ct_torrent_status* st) {
	if (!st) return 0;
	return ct::st_of(st).distributed_fraction;
}

float ct_torrent_status_distributed_copies(const ct_torrent_status* st) {
	if (!st) return 0.0f;
	return ct::st_of(st).distributed_copies;
}

int32_t ct_torrent_status_block_size(const ct_torrent_status* st) {
	if (!st) return 0;
	return ct::st_of(st).block_size;
}

int32_t ct_torrent_status_num_uploads(const ct_torrent_status* st) {
	if (!st) return 0;
	return ct::st_of(st).num_uploads;
}

int32_t ct_torrent_status_num_connections(const ct_torrent_status* st) {
	if (!st) return 0;
	return ct::st_of(st).num_connections;
}

int32_t ct_torrent_status_uploads_limit(const ct_torrent_status* st) {
	if (!st) return 0;
	return ct::st_of(st).uploads_limit;
}

int32_t ct_torrent_status_connections_limit(const ct_torrent_status* st) {
	if (!st) return 0;
	return ct::st_of(st).connections_limit;
}

int32_t ct_torrent_status_upload_limit(const ct_torrent_status* st) {
	if (!st) return -1;
	return ct::st_of(st).upload_limit;
}

int32_t ct_torrent_status_download_limit(const ct_torrent_status* st) {
	if (!st) return -1;
	return ct::st_of(st).download_limit;
}

int32_t ct_torrent_status_up_bandwidth_queue(const ct_torrent_status* st) {
	if (!st) return 0;
	return ct::st_of(st).up_bandwidth_queue;
}

int32_t ct_torrent_status_down_bandwidth_queue(const ct_torrent_status* st) {
	if (!st) return 0;
	return ct::st_of(st).down_bandwidth_queue;
}

int32_t ct_torrent_status_seed_rank(const ct_torrent_status* st) {
	if (!st) return 0;
	return ct::st_of(st).seed_rank;
}

uint32_t ct_torrent_status_need_save_resume_data(const ct_torrent_status* st) {
	if (!st) return 0;
	// resume_data_flags_t is flags::bitfield_flag<uint8_t, ...>
	return static_cast<uint32_t>(static_cast<uint8_t>(
		ct::st_of(st).need_save_resume_data));
}

bool ct_torrent_status_is_seeding(const ct_torrent_status* st) {
	if (!st) return false;
	return ct::st_of(st).is_seeding;
}

bool ct_torrent_status_is_finished(const ct_torrent_status* st) {
	if (!st) return false;
	return ct::st_of(st).is_finished;
}

bool ct_torrent_status_has_metadata(const ct_torrent_status* st) {
	if (!st) return false;
	return ct::st_of(st).has_metadata;
}

bool ct_torrent_status_has_incoming(const ct_torrent_status* st) {
	if (!st) return false;
	return ct::st_of(st).has_incoming;
}

bool ct_torrent_status_moving_storage(const ct_torrent_status* st) {
	if (!st) return false;
	return ct::st_of(st).moving_storage;
}

bool ct_torrent_status_announcing_to_trackers(const ct_torrent_status* st) {
	if (!st) return false;
	return ct::st_of(st).announcing_to_trackers;
}

bool ct_torrent_status_announcing_to_lsd(const ct_torrent_status* st) {
	if (!st) return false;
	return ct::st_of(st).announcing_to_lsd;
}

bool ct_torrent_status_announcing_to_dht(const ct_torrent_status* st) {
	if (!st) return false;
	return ct::st_of(st).announcing_to_dht;
}

const uint8_t* ct_torrent_status_pieces(const ct_torrent_status* st, size_t* out_len) {
	if (!st || !out_len) return nullptr;
	const auto& pieces = ct::st_of(st).pieces;
	if (pieces.empty()) {
		*out_len = 0;
		return nullptr;
	}
	*out_len = pieces.size();
	return reinterpret_cast<const uint8_t*>(pieces.data());
}

const uint8_t* ct_torrent_status_verified_pieces(const ct_torrent_status* st, size_t* out_len) {
	if (!st || !out_len) return nullptr;
	const auto& pieces = ct::st_of(st).verified_pieces;
	if (pieces.empty()) {
		*out_len = 0;
		return nullptr;
	}
	*out_len = pieces.size();
	return reinterpret_cast<const uint8_t*>(pieces.data());
}

} // extern "C"
