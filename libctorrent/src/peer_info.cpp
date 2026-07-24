// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

/* Peer info implementation. */
#include "ctorrent/ct_peer_info.h"
#include "ct_common.hpp"

#include <libtorrent/peer_info.hpp>
#include <libtorrent/bitfield.hpp>
#include <cstring>

namespace ct {

static const lt::peer_info& pi_of(const ct_peer_info* pi) {
	return *reinterpret_cast<const lt::peer_info*>(pi);
}

} // namespace ct

namespace {

// Pin every constant to its libtorrent counterpart (note bit 8, the removed
// `queued` flag, is intentionally skipped).
static_assert(CT_PEER_INTERESTING == static_cast<uint32_t>(lt::peer_info::interesting));
static_assert(CT_PEER_CHOKED == static_cast<uint32_t>(lt::peer_info::choked));
static_assert(CT_PEER_REMOTE_INTERESTED == static_cast<uint32_t>(lt::peer_info::remote_interested));
static_assert(CT_PEER_REMOTE_CHOKED == static_cast<uint32_t>(lt::peer_info::remote_choked));
static_assert(CT_PEER_SUPPORTS_EXTENSIONS == static_cast<uint32_t>(lt::peer_info::supports_extensions));
static_assert(CT_PEER_OUTGOING_CONNECTION == static_cast<uint32_t>(lt::peer_info::outgoing_connection));
static_assert(CT_PEER_HANDSHAKE == static_cast<uint32_t>(lt::peer_info::handshake));
static_assert(CT_PEER_CONNECTING == static_cast<uint32_t>(lt::peer_info::connecting));
static_assert(CT_PEER_ON_PAROLE == static_cast<uint32_t>(lt::peer_info::on_parole));
static_assert(CT_PEER_SEED == static_cast<uint32_t>(lt::peer_info::seed));
static_assert(CT_PEER_OPTIMISTIC_UNCHOKE == static_cast<uint32_t>(lt::peer_info::optimistic_unchoke));
static_assert(CT_PEER_SNUBBED == static_cast<uint32_t>(lt::peer_info::snubbed));
static_assert(CT_PEER_UPLOAD_ONLY == static_cast<uint32_t>(lt::peer_info::upload_only));
static_assert(CT_PEER_ENDGAME_MODE == static_cast<uint32_t>(lt::peer_info::endgame_mode));
static_assert(CT_PEER_HOLEPUNCHED == static_cast<uint32_t>(lt::peer_info::holepunched));
static_assert(CT_PEER_I2P_SOCKET == static_cast<uint32_t>(lt::peer_info::i2p_socket));
static_assert(CT_PEER_UTP_SOCKET == static_cast<uint32_t>(lt::peer_info::utp_socket));
static_assert(CT_PEER_SSL_SOCKET == static_cast<uint32_t>(lt::peer_info::ssl_socket));
static_assert(CT_PEER_RC4_ENCRYPTED == static_cast<uint32_t>(lt::peer_info::rc4_encrypted));
static_assert(CT_PEER_PLAINTEXT_ENCRYPTED == static_cast<uint32_t>(lt::peer_info::plaintext_encrypted));

// peer_source_flags_t and connection_type_t are uint8_t-backed.
static_assert(CT_PEER_SOURCE_TRACKER == static_cast<uint8_t>(lt::peer_info::tracker));
static_assert(CT_PEER_SOURCE_DHT == static_cast<uint8_t>(lt::peer_info::dht));
static_assert(CT_PEER_SOURCE_PEX == static_cast<uint8_t>(lt::peer_info::pex));
static_assert(CT_PEER_SOURCE_LSD == static_cast<uint8_t>(lt::peer_info::lsd));
static_assert(CT_PEER_SOURCE_RESUME_DATA == static_cast<uint8_t>(lt::peer_info::resume_data));
static_assert(CT_PEER_SOURCE_INCOMING == static_cast<uint8_t>(lt::peer_info::incoming));

static_assert(CT_CONNECTION_STANDARD_BITTORRENT == static_cast<uint8_t>(lt::peer_info::standard_bittorrent));
static_assert(CT_CONNECTION_WEB_SEED == static_cast<uint8_t>(lt::peer_info::web_seed));
static_assert(CT_CONNECTION_HTTP_SEED == static_cast<uint8_t>(lt::peer_info::http_seed));

} // namespace

extern "C" {

void ct_peer_info_free(ct_peer_info* pi) {
	if (!pi) return;
	delete &ct::pi_of(pi);
}

ct_str_view ct_peer_info_client(const ct_peer_info* pi) {
	if (!pi) return {nullptr, 0};
	return ct::to_str_view(ct::pi_of(pi).client);
}

int64_t ct_peer_info_total_download(const ct_peer_info* pi) {
	if (!pi) return 0;
	return ct::pi_of(pi).total_download;
}

int64_t ct_peer_info_total_upload(const ct_peer_info* pi) {
	if (!pi) return 0;
	return ct::pi_of(pi).total_upload;
}

int64_t ct_peer_info_last_request_us(const ct_peer_info* pi) {
	if (!pi) return 0;
	return lt::duration_cast<lt::microseconds>(ct::pi_of(pi).last_request).count();
}

int64_t ct_peer_info_last_active_us(const ct_peer_info* pi) {
	if (!pi) return 0;
	return lt::duration_cast<lt::microseconds>(ct::pi_of(pi).last_active).count();
}

int64_t ct_peer_info_download_queue_time_us(const ct_peer_info* pi) {
	if (!pi) return 0;
	return lt::duration_cast<lt::microseconds>(ct::pi_of(pi).download_queue_time).count();
}

ct_peer_flags_t ct_peer_info_flags(const ct_peer_info* pi) {
	if (!pi) return 0;
	return static_cast<ct_peer_flags_t>(ct::pi_of(pi).flags);
}

ct_peer_source_flags_t ct_peer_info_source(const ct_peer_info* pi) {
	if (!pi) return 0;
	return static_cast<ct_peer_source_flags_t>(ct::pi_of(pi).source);
}

int32_t ct_peer_info_up_speed(const ct_peer_info* pi) {
	if (!pi) return 0;
	return ct::pi_of(pi).up_speed;
}

int32_t ct_peer_info_down_speed(const ct_peer_info* pi) {
	if (!pi) return 0;
	return ct::pi_of(pi).down_speed;
}

int32_t ct_peer_info_payload_up_speed(const ct_peer_info* pi) {
	if (!pi) return 0;
	return ct::pi_of(pi).payload_up_speed;
}

int32_t ct_peer_info_payload_down_speed(const ct_peer_info* pi) {
	if (!pi) return 0;
	return ct::pi_of(pi).payload_down_speed;
}

void ct_peer_info_pid(const ct_peer_info* pi, uint8_t out_pid[20]) {
	if (!pi || !out_pid) return;
	std::memcpy(out_pid, ct::pi_of(pi).pid.data(), 20);
}

int32_t ct_peer_info_queue_bytes(const ct_peer_info* pi) {
	if (!pi) return 0;
	return ct::pi_of(pi).queue_bytes;
}

int32_t ct_peer_info_request_timeout(const ct_peer_info* pi) {
	if (!pi) return -1;
	return ct::pi_of(pi).request_timeout;
}

int32_t ct_peer_info_send_buffer_size(const ct_peer_info* pi) {
	if (!pi) return 0;
	return ct::pi_of(pi).send_buffer_size;
}

int32_t ct_peer_info_used_send_buffer(const ct_peer_info* pi) {
	if (!pi) return 0;
	return ct::pi_of(pi).used_send_buffer;
}

int32_t ct_peer_info_receive_buffer_size(const ct_peer_info* pi) {
	if (!pi) return 0;
	return ct::pi_of(pi).receive_buffer_size;
}

int32_t ct_peer_info_used_receive_buffer(const ct_peer_info* pi) {
	if (!pi) return 0;
	return ct::pi_of(pi).used_receive_buffer;
}

int32_t ct_peer_info_receive_buffer_watermark(const ct_peer_info* pi) {
	if (!pi) return 0;
	return ct::pi_of(pi).receive_buffer_watermark;
}

int32_t ct_peer_info_num_hashfails(const ct_peer_info* pi) {
	if (!pi) return 0;
	return ct::pi_of(pi).num_hashfails;
}

int32_t ct_peer_info_download_queue_length(const ct_peer_info* pi) {
	if (!pi) return 0;
	return ct::pi_of(pi).download_queue_length;
}

int32_t ct_peer_info_timed_out_requests(const ct_peer_info* pi) {
	if (!pi) return 0;
	return ct::pi_of(pi).timed_out_requests;
}

int32_t ct_peer_info_busy_requests(const ct_peer_info* pi) {
	if (!pi) return 0;
	return ct::pi_of(pi).busy_requests;
}

int32_t ct_peer_info_requests_in_buffer(const ct_peer_info* pi) {
	if (!pi) return 0;
	return ct::pi_of(pi).requests_in_buffer;
}

int32_t ct_peer_info_target_dl_queue_length(const ct_peer_info* pi) {
	if (!pi) return 0;
	return ct::pi_of(pi).target_dl_queue_length;
}

int32_t ct_peer_info_upload_queue_length(const ct_peer_info* pi) {
	if (!pi) return 0;
	return ct::pi_of(pi).upload_queue_length;
}

int32_t ct_peer_info_failcount(const ct_peer_info* pi) {
	if (!pi) return 0;
	return ct::pi_of(pi).failcount;
}

int32_t ct_peer_info_downloading_piece_index(const ct_peer_info* pi) {
	if (!pi) return -1;
	return static_cast<int32_t>(ct::pi_of(pi).downloading_piece_index);
}

int32_t ct_peer_info_downloading_block_index(const ct_peer_info* pi) {
	if (!pi) return 0;
	return ct::pi_of(pi).downloading_block_index;
}

int32_t ct_peer_info_downloading_progress(const ct_peer_info* pi) {
	if (!pi) return 0;
	return ct::pi_of(pi).downloading_progress;
}

int32_t ct_peer_info_downloading_total(const ct_peer_info* pi) {
	if (!pi) return 0;
	return ct::pi_of(pi).downloading_total;
}

ct_connection_type_t ct_peer_info_connection_type(const ct_peer_info* pi) {
	if (!pi) return 0;
	return static_cast<ct_connection_type_t>(ct::pi_of(pi).connection_type);
}

int32_t ct_peer_info_pending_disk_bytes(const ct_peer_info* pi) {
	if (!pi) return 0;
	return ct::pi_of(pi).pending_disk_bytes;
}

int32_t ct_peer_info_pending_disk_read_bytes(const ct_peer_info* pi) {
	if (!pi) return 0;
	return ct::pi_of(pi).pending_disk_read_bytes;
}

int32_t ct_peer_info_send_quota(const ct_peer_info* pi) {
	if (!pi) return 0;
	return ct::pi_of(pi).send_quota;
}

int32_t ct_peer_info_receive_quota(const ct_peer_info* pi) {
	if (!pi) return 0;
	return ct::pi_of(pi).receive_quota;
}

int32_t ct_peer_info_rtt(const ct_peer_info* pi) {
	if (!pi) return 0;
	return ct::pi_of(pi).rtt;
}

int32_t ct_peer_info_num_pieces(const ct_peer_info* pi) {
	if (!pi) return 0;
	return ct::pi_of(pi).num_pieces;
}

int32_t ct_peer_info_download_rate_peak(const ct_peer_info* pi) {
	if (!pi) return 0;
	return ct::pi_of(pi).download_rate_peak;
}

int32_t ct_peer_info_upload_rate_peak(const ct_peer_info* pi) {
	if (!pi) return 0;
	return ct::pi_of(pi).upload_rate_peak;
}

float ct_peer_info_progress(const ct_peer_info* pi) {
	if (!pi) return 0.0f;
	return ct::pi_of(pi).progress;
}

int32_t ct_peer_info_progress_ppm(const ct_peer_info* pi) {
	if (!pi) return 0;
	return ct::pi_of(pi).progress_ppm;
}

bool ct_peer_info_remote_endpoint(const ct_peer_info* pi, ct_endpoint* out) {
	if (!pi || !out) return false;
	if (ct::pi_of(pi).flags & lt::peer_info::i2p_socket) return false;
	try {
		auto ep = ct::pi_of(pi).remote_endpoint();
		*out = ct::to_ct(ep);
		return true;
	} catch (...) {
		return false;
	}
}

bool ct_peer_info_local_endpoint(const ct_peer_info* pi, ct_endpoint* out) {
	if (!pi || !out) return false;
	if (ct::pi_of(pi).flags & lt::peer_info::i2p_socket) return false;
	try {
		auto ep = ct::pi_of(pi).local_endpoint();
		*out = ct::to_ct(ep);
		return true;
	} catch (...) {
		return false;
	}
}

bool ct_peer_info_i2p_destination(const ct_peer_info* pi, ct_sha256* out) {
#if TORRENT_USE_I2P
	if (!pi || !out) return false;
	if (!(ct::pi_of(pi).flags & lt::peer_info::i2p_socket)) return false;
	*out = ct::to_ct(ct::pi_of(pi).i2p_destination());
	return true;
#else
	(void)pi;
	(void)out;
	return false;
#endif
}

const uint8_t* ct_peer_info_pieces(const ct_peer_info* pi, size_t* out_len) {
	if (!pi || !out_len) return nullptr;
	const auto& pieces = ct::pi_of(pi).pieces;
	if (pieces.empty()) {
		*out_len = 0;
		return nullptr;
	}
	*out_len = pieces.size();
	return reinterpret_cast<const uint8_t*>(pieces.data());
}

} // extern "C"
