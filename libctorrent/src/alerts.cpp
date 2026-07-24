// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

#include <ctorrent/ct_alerts.h>

#include "ct_common.hpp"
#include "handles.hpp"

#include <libtorrent/alert.hpp>
#include <libtorrent/alert_types.hpp>
#include <libtorrent/announce_entry.hpp>
#include <libtorrent/peer_info.hpp>
#include <libtorrent/session.hpp>
#include <libtorrent/time.hpp>
#include <libtorrent/torrent_status.hpp>

#include <chrono>
#include <cstring>
#include <new>
#include <type_traits>
#include <vector>

#include "alert_asserts.inc"

// category bits
static_assert(CT_ALERT_CAT_ERROR
	== static_cast<uint32_t>(lt::alert_category::error));
static_assert(CT_ALERT_CAT_PEER
	== static_cast<uint32_t>(lt::alert_category::peer));
static_assert(CT_ALERT_CAT_PORT_MAPPING
	== static_cast<uint32_t>(lt::alert_category::port_mapping));
static_assert(CT_ALERT_CAT_STORAGE
	== static_cast<uint32_t>(lt::alert_category::storage));
static_assert(CT_ALERT_CAT_TRACKER
	== static_cast<uint32_t>(lt::alert_category::tracker));
static_assert(CT_ALERT_CAT_CONNECT
	== static_cast<uint32_t>(lt::alert_category::connect));
static_assert(CT_ALERT_CAT_STATUS
	== static_cast<uint32_t>(lt::alert_category::status));
static_assert(CT_ALERT_CAT_IP_BLOCK
	== static_cast<uint32_t>(lt::alert_category::ip_block));
static_assert(CT_ALERT_CAT_PERFORMANCE_WARNING
	== static_cast<uint32_t>(lt::alert_category::performance_warning));
static_assert(CT_ALERT_CAT_DHT
	== static_cast<uint32_t>(lt::alert_category::dht));
static_assert(CT_ALERT_CAT_STATS
	== static_cast<uint32_t>(lt::alert_category::stats));
static_assert(CT_ALERT_CAT_SESSION_LOG
	== static_cast<uint32_t>(lt::alert_category::session_log));
static_assert(CT_ALERT_CAT_TORRENT_LOG
	== static_cast<uint32_t>(lt::alert_category::torrent_log));
static_assert(CT_ALERT_CAT_PEER_LOG
	== static_cast<uint32_t>(lt::alert_category::peer_log));
static_assert(CT_ALERT_CAT_INCOMING_REQUEST
	== static_cast<uint32_t>(lt::alert_category::incoming_request));
static_assert(CT_ALERT_CAT_DHT_LOG
	== static_cast<uint32_t>(lt::alert_category::dht_log));
static_assert(CT_ALERT_CAT_DHT_OPERATION
	== static_cast<uint32_t>(lt::alert_category::dht_operation));
static_assert(CT_ALERT_CAT_PORT_MAPPING_LOG
	== static_cast<uint32_t>(lt::alert_category::port_mapping_log));
static_assert(CT_ALERT_CAT_PICKER_LOG
	== static_cast<uint32_t>(lt::alert_category::picker_log));
static_assert(CT_ALERT_CAT_FILE_PROGRESS
	== static_cast<uint32_t>(lt::alert_category::file_progress));
static_assert(CT_ALERT_CAT_PIECE_PROGRESS
	== static_cast<uint32_t>(lt::alert_category::piece_progress));
static_assert(CT_ALERT_CAT_UPLOAD
	== static_cast<uint32_t>(lt::alert_category::upload));
static_assert(CT_ALERT_CAT_BLOCK_PROGRESS
	== static_cast<uint32_t>(lt::alert_category::block_progress));

namespace {

using batch_t = std::vector<lt::alert*>;

batch_t* unwrap(ct_alert_batch* b)
{
	return reinterpret_cast<batch_t*>(b);
}

batch_t const* unwrap(ct_alert_batch const* b)
{
	return reinterpret_cast<batch_t const*>(b);
}

lt::alert const* unwrap(ct_alert const* a)
{
	return reinterpret_cast<lt::alert const*>(a);
}

ct_str_view cstr_view(char const* s)
{
	if (s == nullptr) return ct_str_view{nullptr, 0};
	return ct_str_view{s, std::strlen(s)};
}

// The typed downcast every fill function goes through.
template <typename T>
T const* cast(ct_alert const* a)
{
	return lt::alert_cast<T>(unwrap(a));
}

} // namespace

extern "C" {

ct_alert_batch* ct_alert_batch_new(void)
{
	return reinterpret_cast<ct_alert_batch*>(new (std::nothrow) batch_t());
}

void ct_alert_batch_free(ct_alert_batch* batch)
{
	delete unwrap(batch);
}

void ct_session_pop_alerts(ct_session* session, ct_alert_batch* batch,
	ct_error* err)
{
	ct::guard(err, [&] {
		reinterpret_cast<lt::session*>(session)->pop_alerts(unwrap(batch));
	});
}

bool ct_session_wait_for_alert(ct_session* session, int64_t timeout_ms,
	ct_error* err)
{
	return ct::guard(err, [&]() -> bool {
		// at TORRENT_ABI_VERSION < 4 this returns alert*; both convert in a
		// boolean context
		return reinterpret_cast<lt::session*>(session)->wait_for_alert(
			std::chrono::milliseconds(timeout_ms)) ? true : false;
	});
}

void ct_session_set_alert_notify(ct_session* session,
	void (*callback)(void* userdata), void* userdata, ct_error* err)
{
	ct::guard(err, [&] {
		auto* s = reinterpret_cast<lt::session*>(session);
		if (callback == nullptr) {
			s->set_alert_notify({});
		} else {
			s->set_alert_notify([callback, userdata] { callback(userdata); });
		}
	});
}

size_t ct_alert_batch_len(const ct_alert_batch* batch)
{
	return unwrap(batch)->size();
}

const ct_alert* ct_alert_batch_get(const ct_alert_batch* batch, size_t i)
{
	return reinterpret_cast<ct_alert const*>((*unwrap(batch))[i]);
}

int32_t ct_alert_type(const ct_alert* alert)
{
	return unwrap(alert)->type();
}

uint32_t ct_alert_category(const ct_alert* alert)
{
	return static_cast<uint32_t>(unwrap(alert)->category());
}

ct_str_view ct_alert_what(const ct_alert* alert)
{
	return cstr_view(unwrap(alert)->what());
}

void ct_alert_message(const ct_alert* alert, ct_str* out)
{
	if (out == nullptr) return;
	try {
		*out = ct::box_string(unwrap(alert)->message());
	} catch (...) {
		*out = ct_str{nullptr, 0, nullptr};
	}
}

int64_t ct_alert_timestamp_us(const ct_alert* alert)
{
	using std::chrono::duration_cast;
	using std::chrono::microseconds;
	using std::chrono::system_clock;
	auto const since_epoch = unwrap(alert)->timestamp().time_since_epoch();
	if constexpr (std::is_same_v<lt::clock_type, system_clock>) {
		auto us = duration_cast<microseconds>(since_epoch).count();
		// Clamp to [0, INT64_MAX] to prevent overflow issues
		if (us < 0) return 0;
		return us;
	} else {
		// lt::clock_type has an unspecified epoch here; rebase onto the
		// system clock with an offset sampled once
		static auto const offset =
			duration_cast<microseconds>(
				system_clock::now().time_since_epoch())
			- duration_cast<microseconds>(
				lt::clock_type::now().time_since_epoch());
		return (duration_cast<microseconds>(since_epoch) + offset).count();
	}
}

const struct ct_torrent_handle* ct_alert_torrent_handle(const ct_alert* alert)
{
	auto const* a = dynamic_cast<lt::torrent_alert const*>(unwrap(alert));
	if (a == nullptr) return nullptr;
	return reinterpret_cast<const ct_torrent_handle*>(&a->handle);
}

bool ct_alert_torrent_name(const ct_alert* alert, ct_str_view* out)
{
#if TORRENT_ABI_VERSION < 4
	// torrent_name() is marked deprecated but is the only source of the
	// name below ABI 4 (the replacement is a torrent_handle lookup).
#if defined(__GNUC__) || defined(__clang__)
#pragma GCC diagnostic push
#pragma GCC diagnostic ignored "-Wdeprecated-declarations"
#endif
	auto const* a = dynamic_cast<lt::torrent_alert const*>(unwrap(alert));
	if (a == nullptr) return false;
	*out = cstr_view(a->torrent_name());
	return true;
#if defined(__GNUC__) || defined(__clang__)
#pragma GCC diagnostic pop
#endif
#else
	// torrent_alert no longer stores the name at ABI >= 4; it has to be
	// looked up through the torrent handle instead.
	(void)alert;
	(void)out;
	return false;
#endif
}

bool ct_alert_tracker_url(const ct_alert* alert, ct_str_view* out)
{
	auto const* a = dynamic_cast<lt::tracker_alert const*>(unwrap(alert));
	if (a == nullptr) return false;
	*out = cstr_view(a->tracker_url());
	return true;
}

bool ct_alert_peer_endpoint(const ct_alert* alert, ct_endpoint* out_ep,
	ct_sha1* out_pid)
{
	auto const* a = dynamic_cast<lt::peer_alert const*>(unwrap(alert));
	if (a == nullptr) return false;
	auto const* ep = std::get_if<lt::aux::noexcept_movable<lt::tcp::endpoint>>(
		&a->ep);
	if (ep == nullptr) return false;
	*out_ep = ct::to_ct(static_cast<lt::tcp::endpoint const&>(*ep));
	*out_pid = ct::to_ct(a->pid);
	return true;
}

/* ---- session / network ---------------------------------------------------- */

bool ct_alert_as_listen_succeeded(const ct_alert* alert,
	ct_listen_succeeded_view* out)
{
	auto const* a = cast<lt::listen_succeeded_alert>(alert);
	if (a == nullptr) return false;
	out->endpoint = ct::to_ct(a->address, static_cast<uint16_t>(a->port));
	out->socket_type = static_cast<int32_t>(a->socket_type);
	return true;
}

bool ct_alert_as_listen_failed(const ct_alert* alert,
	ct_listen_failed_view* out)
{
	auto const* a = cast<lt::listen_failed_alert>(alert);
	if (a == nullptr) return false;
	out->interface_name = cstr_view(a->listen_interface());
	out->endpoint = ct::to_ct(a->address, static_cast<uint16_t>(a->port));
	ct::set_error(&out->error, a->error);
	out->operation = static_cast<int32_t>(a->op);
	out->socket_type = static_cast<int32_t>(a->socket_type);
	return true;
}

bool ct_alert_as_external_ip(const ct_alert* alert, ct_external_ip_view* out)
{
	auto const* a = cast<lt::external_ip_alert>(alert);
	if (a == nullptr) return false;
	out->address = ct::to_ct(a->external_address, 0);
	return true;
}

bool ct_alert_as_udp_error(const ct_alert* alert, ct_udp_error_view* out)
{
	auto const* a = cast<lt::udp_error_alert>(alert);
	if (a == nullptr) return false;
	out->endpoint = ct::to_ct(static_cast<lt::udp::endpoint const&>(a->endpoint));
	out->operation = static_cast<int32_t>(a->operation);
	ct::set_error(&out->error, a->error);
	return true;
}

bool ct_alert_as_session_stats(const ct_alert* alert,
	ct_session_stats_view* out)
{
	auto const* a = cast<lt::session_stats_alert>(alert);
	if (a == nullptr) return false;
	auto const counters = a->counters();
	out->counters = counters.data();
	out->len = static_cast<size_t>(counters.size());
	return true;
}

bool ct_alert_as_session_error(const ct_alert* alert,
	ct_session_error_view* out)
{
	auto const* a = cast<lt::session_error_alert>(alert);
	if (a == nullptr) return false;
	ct::set_error(&out->error, a->error);
	return true;
}

bool ct_alert_as_add_torrent(const ct_alert* alert, ct_add_torrent_view* out)
{
	auto const* a = cast<lt::add_torrent_alert>(alert);
	if (a == nullptr) return false;
	out->handle = reinterpret_cast<const ct_torrent_handle*>(&a->handle);
	out->params = reinterpret_cast<const ct_add_torrent_params*>(&a->params);
	ct::set_error(&out->error, a->error);
	// client_data_t is type-tagged: it was stored as void (see
	// ct_session_async_add_torrent), so it must be read as void -
	// get<void*>() would compare against type<void*> and always miss.
	out->userdata = a->params.userdata.get<void>();
	return true;
}

bool ct_alert_as_torrent_removed(const ct_alert* alert,
	ct_torrent_removed_view* out)
{
	auto const* a = cast<lt::torrent_removed_alert>(alert);
	if (a == nullptr) return false;
	std::memset(&out->info_hashes, 0, sizeof(out->info_hashes));
	if (a->info_hashes.has_v1())
		std::memcpy(out->info_hashes.v1.data, a->info_hashes.v1.data(), 20);
	if (a->info_hashes.has_v2())
		std::memcpy(out->info_hashes.v2.data, a->info_hashes.v2.data(), 32);
	// Type-tagged as void; see ct_alert_as_add_torrent.
	out->userdata = a->userdata.get<void>();
	return true;
}

bool ct_alert_as_torrent_finished(const ct_alert* alert,
	ct_torrent_finished_view* out)
{
	auto const* a = cast<lt::torrent_finished_alert>(alert);
	if (a == nullptr) return false;
	(void)out;
	return true;
}

bool ct_alert_as_alerts_dropped(const ct_alert* alert,
	ct_alerts_dropped_view* out)
{
	auto const* a = cast<lt::alerts_dropped_alert>(alert);
	if (a == nullptr) return false;
	static_assert(lt::abi_alert_count <= 128);
	std::memset(out->dropped, 0, sizeof(out->dropped));
	for (std::size_t i = 0; i < a->dropped_alerts.size(); ++i) {
		if (a->dropped_alerts.test(i))
			out->dropped[i / 8] |= static_cast<uint8_t>(1u << (i % 8));
	}
	return true;
}

bool ct_alert_as_incoming_connection(const ct_alert* alert,
	ct_incoming_connection_view* out)
{
	auto const* a = cast<lt::incoming_connection_alert>(alert);
	if (a == nullptr) return false;
	out->socket_type = static_cast<int32_t>(a->socket_type);
	out->endpoint = ct::to_ct(static_cast<lt::tcp::endpoint const&>(a->endpoint));
	return true;
}

bool ct_alert_as_portmap(const ct_alert* alert, ct_portmap_view* out)
{
	auto const* a = cast<lt::portmap_alert>(alert);
	if (a == nullptr) return false;
	out->mapping = static_cast<int32_t>(a->mapping);
	out->external_port = a->external_port;
	out->protocol = static_cast<int32_t>(a->map_protocol);
	out->transport = static_cast<int32_t>(a->map_transport);
	out->local_address = ct::to_ct(a->local_address, 0);
	return true;
}

bool ct_alert_as_portmap_error(const ct_alert* alert,
	ct_portmap_error_view* out)
{
	auto const* a = cast<lt::portmap_error_alert>(alert);
	if (a == nullptr) return false;
	out->mapping = static_cast<int32_t>(a->mapping);
	out->transport = static_cast<int32_t>(a->map_transport);
	ct::set_error(&out->error, a->error);
	out->local_address = ct::to_ct(a->local_address, 0);
	return true;
}

bool ct_alert_as_socks5(const ct_alert* alert, ct_socks5_view* out)
{
	auto const* a = cast<lt::socks5_alert>(alert);
	if (a == nullptr) return false;
	ct::set_error(&out->error, a->error);
	out->operation = static_cast<int32_t>(a->op);
	out->ip = ct::to_ct(static_cast<lt::tcp::endpoint const&>(a->ip));
	return true;
}

bool ct_alert_as_i2p(const ct_alert* alert, ct_i2p_view* out)
{
	auto const* a = cast<lt::i2p_alert>(alert);
	if (a == nullptr) return false;
	ct::set_error(&out->error, a->error);
	return true;
}

bool ct_alert_as_lsd_error(const ct_alert* alert, ct_lsd_error_view* out)
{
	auto const* a = cast<lt::lsd_error_alert>(alert);
	if (a == nullptr) return false;
	ct::set_error(&out->error, a->error);
	out->local_address = ct::to_ct(a->local_address, 0);
	return true;
}

bool ct_alert_as_log(const ct_alert* alert, ct_log_view* out)
{
	auto const* a = cast<lt::log_alert>(alert);
	if (a == nullptr) return false;
	out->message = cstr_view(a->log_message());
	return true;
}

bool ct_alert_as_torrent_log(const ct_alert* alert, ct_log_view* out)
{
	auto const* a = cast<lt::torrent_log_alert>(alert);
	if (a == nullptr) return false;
	out->message = cstr_view(a->log_message());
	return true;
}

bool ct_alert_as_portmap_log(const ct_alert* alert, ct_log_view* out)
{
	auto const* a = cast<lt::portmap_log_alert>(alert);
	if (a == nullptr) return false;
	out->message = cstr_view(a->log_message());
	return true;
}

bool ct_alert_as_dht_log(const ct_alert* alert, ct_dht_log_view* out)
{
	auto const* a = cast<lt::dht_log_alert>(alert);
	if (a == nullptr) return false;
	out->message = cstr_view(a->log_message());
	out->module = static_cast<int32_t>(a->module);
	return true;
}

bool ct_alert_as_peer_log(const ct_alert* alert, ct_peer_log_view* out)
{
	auto const* a = cast<lt::peer_log_alert>(alert);
	if (a == nullptr) return false;
	out->message = cstr_view(a->log_message());
	out->event_type = static_cast<int32_t>(a->event_type);
	out->direction = static_cast<int32_t>(a->direction);
	return true;
}

/* ---- torrent state --------------------------------------------------------- */

bool ct_alert_as_state_changed(const ct_alert* alert,
	ct_state_changed_view* out)
{
	auto const* a = cast<lt::state_changed_alert>(alert);
	if (a == nullptr) return false;
	out->state = static_cast<int32_t>(a->state);
	out->prev_state = static_cast<int32_t>(a->prev_state);
	return true;
}

bool ct_alert_as_torrent_error(const ct_alert* alert,
	ct_torrent_error_view* out)
{
	auto const* a = cast<lt::torrent_error_alert>(alert);
	if (a == nullptr) return false;
	ct::set_error(&out->error, a->error);
	out->filename = cstr_view(a->filename());
	return true;
}

bool ct_alert_as_torrent_deleted(const ct_alert* alert,
	ct_torrent_deleted_view* out)
{
	auto const* a = cast<lt::torrent_deleted_alert>(alert);
	if (a == nullptr) return false;
	out->info_hashes = ct::to_ct(a->info_hashes);
	return true;
}

bool ct_alert_as_torrent_delete_failed(const ct_alert* alert,
	ct_torrent_delete_failed_view* out)
{
	auto const* a = cast<lt::torrent_delete_failed_alert>(alert);
	if (a == nullptr) return false;
	ct::set_error(&out->error, a->error);
	out->info_hashes = ct::to_ct(a->info_hashes);
	return true;
}

bool ct_alert_as_performance(const ct_alert* alert, ct_performance_view* out)
{
	auto const* a = cast<lt::performance_alert>(alert);
	if (a == nullptr) return false;
	out->warning_code = static_cast<int32_t>(a->warning_code);
	return true;
}

bool ct_alert_as_metadata_failed(const ct_alert* alert,
	ct_metadata_failed_view* out)
{
	auto const* a = cast<lt::metadata_failed_alert>(alert);
	if (a == nullptr) return false;
	ct::set_error(&out->error, a->error);
	return true;
}

bool ct_alert_as_fastresume_rejected(const ct_alert* alert,
	ct_fastresume_rejected_view* out)
{
	auto const* a = cast<lt::fastresume_rejected_alert>(alert);
	if (a == nullptr) return false;
	ct::set_error(&out->error, a->error);
	out->file_path = cstr_view(a->file_path());
	out->operation = static_cast<int32_t>(a->op);
	return true;
}

bool ct_alert_as_save_resume_data(const ct_alert* alert,
	ct_save_resume_data_view* out)
{
	auto const* a = cast<lt::save_resume_data_alert>(alert);
	if (a == nullptr) return false;
	out->params = reinterpret_cast<const ct_add_torrent_params*>(&a->params);
	return true;
}

bool ct_alert_as_save_resume_data_failed(const ct_alert* alert,
	ct_save_resume_data_failed_view* out)
{
	auto const* a = cast<lt::save_resume_data_failed_alert>(alert);
	if (a == nullptr) return false;
	ct::set_error(&out->error, a->error);
	return true;
}

bool ct_alert_as_state_update(const ct_alert* alert,
	ct_state_update_view* out)
{
	auto const* a = cast<lt::state_update_alert>(alert);
	if (a == nullptr) return false;
	out->count = a->status.size();
	return true;
}

ct_torrent_status* ct_alert_state_update_status(const ct_alert* alert,
	size_t i)
{
	auto const* a = cast<lt::state_update_alert>(alert);
	if (a == nullptr || i >= a->status.size()) return nullptr;
	// No error out-param on this accessor: allocation failure is NULL.
	try {
		auto const& s = a->status[i];
		// id() reads 0 if the torrent was removed between alert post and
		// pop; info_hashes remain usable on such snapshots.
		return new ct_torrent_status{s, s.handle.id()};
	} catch (...) {
		return nullptr;
	}
}

bool ct_alert_as_peer_info(const ct_alert* alert,
	ct_peer_info_list_view* out)
{
	auto const* a = cast<lt::peer_info_alert>(alert);
	if (a == nullptr) return false;
	out->count = a->peer_info.size();
	return true;
}

ct_peer_info* ct_alert_peer_info(const ct_alert* alert, size_t i)
{
	auto const* a = cast<lt::peer_info_alert>(alert);
	if (a == nullptr || i >= a->peer_info.size()) return nullptr;
	// No error out-param on this accessor: allocation failure is NULL.
	try {
		return reinterpret_cast<ct_peer_info*>(
			new lt::peer_info(a->peer_info[i]));
	} catch (...) {
		return nullptr;
	}
}

bool ct_alert_as_file_progress(const ct_alert* alert,
	ct_file_progress_view* out)
{
	auto const* a = cast<lt::file_progress_alert>(alert);
	if (a == nullptr) return false;
	out->progress = a->files.data();
	out->len = a->files.size();
	return true;
}

bool ct_alert_as_tracker_list(const ct_alert* alert,
	ct_tracker_list_view* out)
{
	auto const* a = cast<lt::tracker_list_alert>(alert);
	if (a == nullptr) return false;
	out->count = a->trackers.size();
	return true;
}

bool ct_alert_tracker_list_entry(const ct_alert* alert, size_t i,
	ct_tracker_list_entry* out)
{
	auto const* a = cast<lt::tracker_list_alert>(alert);
	if (a == nullptr || i >= a->trackers.size()) return false;
	auto const& t = a->trackers[i];
	out->url = ct_str_view{t.url.data(), t.url.size()};
	out->trackerid = ct_str_view{t.trackerid.data(), t.trackerid.size()};
	out->tier = t.tier;
	out->fail_limit = t.fail_limit;
	out->source = t.source;
	out->verified = t.verified;
	return true;
}

/* ---- piece / block / file --------------------------------------------------- */

bool ct_alert_as_read_piece(const ct_alert* alert, ct_read_piece_view* out)
{
	auto const* a = cast<lt::read_piece_alert>(alert);
	if (a == nullptr) return false;
	ct::set_error(&out->error, a->error);
	out->buffer = reinterpret_cast<uint8_t const*>(a->buffer.get());
	out->piece = static_cast<int32_t>(a->piece);
	out->size = a->size;
	return true;
}

bool ct_alert_as_piece_finished(const ct_alert* alert,
	ct_piece_finished_view* out)
{
	auto const* a = cast<lt::piece_finished_alert>(alert);
	if (a == nullptr) return false;
	out->piece_index = static_cast<int32_t>(a->piece_index);
	return true;
}

bool ct_alert_as_hash_failed(const ct_alert* alert, ct_hash_failed_view* out)
{
	auto const* a = cast<lt::hash_failed_alert>(alert);
	if (a == nullptr) return false;
	out->piece_index = static_cast<int32_t>(a->piece_index);
	return true;
}

bool ct_alert_as_block(const ct_alert* alert, ct_block_view* out)
{
	switch (ct_alert_type(alert)) {
		case CT_ALERT_TYPE_REQUEST_DROPPED: {
			auto const* a = cast<lt::request_dropped_alert>(alert);
			out->block_index = a->block_index;
			out->piece_index = static_cast<int32_t>(a->piece_index);
			return true;
		}
		case CT_ALERT_TYPE_BLOCK_TIMEOUT: {
			auto const* a = cast<lt::block_timeout_alert>(alert);
			out->block_index = a->block_index;
			out->piece_index = static_cast<int32_t>(a->piece_index);
			return true;
		}
		case CT_ALERT_TYPE_BLOCK_FINISHED: {
			auto const* a = cast<lt::block_finished_alert>(alert);
			out->block_index = a->block_index;
			out->piece_index = static_cast<int32_t>(a->piece_index);
			return true;
		}
		case CT_ALERT_TYPE_BLOCK_DOWNLOADING: {
			auto const* a = cast<lt::block_downloading_alert>(alert);
			out->block_index = a->block_index;
			out->piece_index = static_cast<int32_t>(a->piece_index);
			return true;
		}
		case CT_ALERT_TYPE_UNWANTED_BLOCK: {
			auto const* a = cast<lt::unwanted_block_alert>(alert);
			out->block_index = a->block_index;
			out->piece_index = static_cast<int32_t>(a->piece_index);
			return true;
		}
		case CT_ALERT_TYPE_BLOCK_UPLOADED: {
			auto const* a = cast<lt::block_uploaded_alert>(alert);
			out->block_index = a->block_index;
			out->piece_index = static_cast<int32_t>(a->piece_index);
			return true;
		}
		default:
			return false;
	}
}

bool ct_alert_as_invalid_request(const ct_alert* alert,
	ct_invalid_request_view* out)
{
	auto const* a = cast<lt::invalid_request_alert>(alert);
	if (a == nullptr) return false;
	out->request = ct::to_ct(a->request);
	out->we_have = a->we_have;
	out->peer_interested = a->peer_interested;
	out->withheld = a->withheld;
	return true;
}

bool ct_alert_as_incoming_request(const ct_alert* alert,
	ct_incoming_request_view* out)
{
	auto const* a = cast<lt::incoming_request_alert>(alert);
	if (a == nullptr) return false;
	out->request = ct::to_ct(a->req);
	return true;
}

bool ct_alert_as_file_completed(const ct_alert* alert,
	ct_file_completed_view* out)
{
	auto const* a = cast<lt::file_completed_alert>(alert);
	if (a == nullptr) return false;
	out->index = static_cast<int32_t>(a->index);
	return true;
}

bool ct_alert_as_file_renamed(const ct_alert* alert,
	ct_file_renamed_view* out)
{
	auto const* a = cast<lt::file_renamed_alert>(alert);
	if (a == nullptr) return false;
	out->index = static_cast<int32_t>(a->index);
	out->new_name = cstr_view(a->new_name());
	out->old_name = cstr_view(a->old_name());
	return true;
}

bool ct_alert_as_file_rename_failed(const ct_alert* alert,
	ct_file_rename_failed_view* out)
{
	auto const* a = cast<lt::file_rename_failed_alert>(alert);
	if (a == nullptr) return false;
	out->index = static_cast<int32_t>(a->index);
	ct::set_error(&out->error, a->error);
	return true;
}

bool ct_alert_as_file_error(const ct_alert* alert, ct_file_error_view* out)
{
	auto const* a = cast<lt::file_error_alert>(alert);
	if (a == nullptr) return false;
	ct::set_error(&out->error, a->error);
	out->filename = cstr_view(a->filename());
	out->operation = static_cast<int32_t>(a->op);
	return true;
}

bool ct_alert_as_file_prio(const ct_alert* alert, ct_file_prio_view* out)
{
	auto const* a = cast<lt::file_prio_alert>(alert);
	if (a == nullptr) return false;
	(void)out;
	return true;
}

bool ct_alert_as_storage_moved(const ct_alert* alert,
	ct_storage_moved_view* out)
{
	auto const* a = cast<lt::storage_moved_alert>(alert);
	if (a == nullptr) return false;
	out->storage_path = cstr_view(a->storage_path());
	out->old_path = cstr_view(a->old_path());
	return true;
}

bool ct_alert_as_storage_moved_failed(const ct_alert* alert,
	ct_storage_moved_failed_view* out)
{
	auto const* a = cast<lt::storage_moved_failed_alert>(alert);
	if (a == nullptr) return false;
	ct::set_error(&out->error, a->error);
	out->file_path = cstr_view(a->file_path());
	out->operation = static_cast<int32_t>(a->op);
	return true;
}

/* ---- peers ------------------------------------------------------------------ */

bool ct_alert_as_peer_connect(const ct_alert* alert,
	ct_peer_connect_view* out)
{
	auto const* a = cast<lt::peer_connect_alert>(alert);
	if (a == nullptr) return false;
	out->direction = static_cast<int32_t>(a->direction);
	out->socket_type = static_cast<int32_t>(a->socket_type);
	return true;
}

bool ct_alert_as_peer_disconnected(const ct_alert* alert,
	ct_peer_disconnected_view* out)
{
	auto const* a = cast<lt::peer_disconnected_alert>(alert);
	if (a == nullptr) return false;
	out->socket_type = static_cast<int32_t>(a->socket_type);
	out->operation = static_cast<int32_t>(a->op);
	ct::set_error(&out->error, a->error);
	out->reason = static_cast<int32_t>(a->reason);
	return true;
}

bool ct_alert_as_peer_error(const ct_alert* alert, ct_peer_error_view* out)
{
	auto const* a = cast<lt::peer_error_alert>(alert);
	if (a == nullptr) return false;
	out->operation = static_cast<int32_t>(a->op);
	ct::set_error(&out->error, a->error);
	return true;
}

bool ct_alert_as_peer_blocked(const ct_alert* alert,
	ct_peer_blocked_view* out)
{
	auto const* a = cast<lt::peer_blocked_alert>(alert);
	if (a == nullptr) return false;
	out->reason = a->reason;
	return true;
}

/* ---- trackers ----------------------------------------------------------------- */

bool ct_alert_as_tracker_error(const ct_alert* alert,
	ct_tracker_error_view* out)
{
	auto const* a = cast<lt::tracker_error_alert>(alert);
	if (a == nullptr) return false;
	out->times_in_row = a->times_in_row;
	ct::set_error(&out->error, a->error);
	out->operation = static_cast<int32_t>(a->op);
	out->failure_reason = cstr_view(a->failure_reason());
	out->version = static_cast<int32_t>(a->version);
	return true;
}

bool ct_alert_as_tracker_warning(const ct_alert* alert,
	ct_tracker_warning_view* out)
{
	auto const* a = cast<lt::tracker_warning_alert>(alert);
	if (a == nullptr) return false;
	out->warning_message = cstr_view(a->warning_message());
	return true;
}

bool ct_alert_as_tracker_reply(const ct_alert* alert,
	ct_tracker_reply_view* out)
{
	auto const* a = cast<lt::tracker_reply_alert>(alert);
	if (a == nullptr) return false;
	out->num_peers = a->num_peers;
	out->version = static_cast<int32_t>(a->version);
	return true;
}

bool ct_alert_as_tracker_announce(const ct_alert* alert,
	ct_tracker_announce_view* out)
{
	auto const* a = cast<lt::tracker_announce_alert>(alert);
	if (a == nullptr) return false;
	out->event = static_cast<int32_t>(a->event);
	out->version = static_cast<int32_t>(a->version);
	return true;
}

bool ct_alert_as_scrape_reply(const ct_alert* alert,
	ct_scrape_reply_view* out)
{
	auto const* a = cast<lt::scrape_reply_alert>(alert);
	if (a == nullptr) return false;
	out->incomplete = a->incomplete;
	out->complete = a->complete;
	return true;
}

bool ct_alert_as_scrape_failed(const ct_alert* alert,
	ct_scrape_failed_view* out)
{
	auto const* a = cast<lt::scrape_failed_alert>(alert);
	if (a == nullptr) return false;
	ct::set_error(&out->error, a->error);
	out->error_message = cstr_view(a->error_message());
	return true;
}

bool ct_alert_as_dht_reply(const ct_alert* alert, ct_dht_reply_view* out)
{
	auto const* a = cast<lt::dht_reply_alert>(alert);
	if (a == nullptr) return false;
	out->num_peers = a->num_peers;
	return true;
}

bool ct_alert_as_trackerid(const ct_alert* alert, ct_trackerid_view* out)
{
	auto const* a = cast<lt::trackerid_alert>(alert);
	if (a == nullptr) return false;
	out->trackerid = cstr_view(a->tracker_id());
	return true;
}

bool ct_alert_as_url_seed(const ct_alert* alert, ct_url_seed_view* out)
{
	auto const* a = cast<lt::url_seed_alert>(alert);
	if (a == nullptr) return false;
	out->server_url = cstr_view(a->server_url());
	out->error_message = cstr_view(a->error_message());
	ct::set_error(&out->error, a->error);
	return true;
}

/* ---- DHT ------------------------------------------------------------------------ */

bool ct_alert_as_dht_announce(const ct_alert* alert,
	ct_dht_announce_view* out)
{
	auto const* a = cast<lt::dht_announce_alert>(alert);
	if (a == nullptr) return false;
	out->ip = ct::to_ct(a->ip, static_cast<uint16_t>(a->port));
	out->info_hash = ct::to_ct(a->info_hash);
	return true;
}

bool ct_alert_as_dht_get_peers(const ct_alert* alert,
	ct_dht_get_peers_view* out)
{
	auto const* a = cast<lt::dht_get_peers_alert>(alert);
	if (a == nullptr) return false;
	out->info_hash = ct::to_ct(a->info_hash);
	return true;
}

bool ct_alert_as_dht_error(const ct_alert* alert, ct_dht_error_view* out)
{
	auto const* a = cast<lt::dht_error_alert>(alert);
	if (a == nullptr) return false;
	ct::set_error(&out->error, a->error);
	out->operation = static_cast<int32_t>(a->op);
	return true;
}

bool ct_alert_as_dht_put(const ct_alert* alert, ct_dht_put_view* out)
{
	auto const* a = cast<lt::dht_put_alert>(alert);
	if (a == nullptr) return false;
	out->target = ct::to_ct(a->target);
	std::memcpy(out->public_key, a->public_key.data(), 32);
	std::memcpy(out->signature, a->signature.data(), 64);
	out->salt = ct_str_view{a->salt.data(), a->salt.size()};
	out->seq = a->seq;
	out->num_success = a->num_success;
	return true;
}

bool ct_alert_as_dht_outgoing_get_peers(const ct_alert* alert,
	ct_dht_outgoing_get_peers_view* out)
{
	auto const* a = cast<lt::dht_outgoing_get_peers_alert>(alert);
	if (a == nullptr) return false;
	out->info_hash = ct::to_ct(a->info_hash);
	out->obfuscated_info_hash = ct::to_ct(a->obfuscated_info_hash);
	out->endpoint = ct::to_ct(static_cast<lt::udp::endpoint const&>(a->endpoint));
	return true;
}

bool ct_alert_as_dht_pkt(const ct_alert* alert, ct_dht_pkt_view* out)
{
	auto const* a = cast<lt::dht_pkt_alert>(alert);
	if (a == nullptr) return false;
	auto const buf = a->pkt_buf();
	out->packet = ct_span{
		reinterpret_cast<uint8_t const*>(buf.data()),
		static_cast<size_t>(buf.size())};
	out->direction = static_cast<int32_t>(a->direction);
	out->node = ct::to_ct(static_cast<lt::udp::endpoint const&>(a->node));
	return true;
}

} // extern "C"
