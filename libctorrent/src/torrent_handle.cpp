// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

/* Torrent handle implementation.
 *
 * Exception discipline: lt::torrent_handle methods throw
 * lt::system_error(invalid_torrent_handle) when the torrent has been
 * removed. The is_valid() pre-checks below only narrow that window (the
 * torrent can go away between check and call), so every body that touches
 * the handle runs under a catch-all guard; no exception ever crosses the C
 * boundary. Fallible accessors report through ct::guard instead.
 */
#include "ctorrent/ct_torrent_handle.h"
#include "ct_common.hpp"
#include "handles.hpp"

#include <libtorrent/file_storage.hpp>
#include <libtorrent/torrent_handle.hpp>
#include <libtorrent/torrent_status.hpp>

#include <cstring>
#include <memory>
#include <new>
#include <stdexcept>
#include <utility>
#include <vector>

// Masquerade layout
static_assert(sizeof(ct_torrent_handle) == sizeof(lt::torrent_handle),
	"ct_torrent_handle size");
static_assert(alignof(ct_torrent_handle) == alignof(lt::torrent_handle),
	"ct_torrent_handle alignment");

// Torrent flags are CT_TORRENT_FLAG_* (asserted against lt::torrent_flags
// in add_torrent_params.cpp); this file defines no flag constants of its own.

// Typed flag constants exposed in the header
static_assert(CT_DEADLINE_ALERT_WHEN_AVAILABLE
	== static_cast<uint32_t>(static_cast<uint8_t>(lt::torrent_handle::alert_when_available)));
static_assert(CT_STATUS_QUERY_DISTRIBUTED_COPIES
	== static_cast<uint32_t>(lt::torrent_handle::query_distributed_copies));
static_assert(CT_STATUS_QUERY_ACCURATE_DOWNLOAD_COUNTERS
	== static_cast<uint32_t>(lt::torrent_handle::query_accurate_download_counters));
static_assert(CT_STATUS_QUERY_LAST_SEEN_COMPLETE
	== static_cast<uint32_t>(lt::torrent_handle::query_last_seen_complete));
static_assert(CT_STATUS_QUERY_PIECES
	== static_cast<uint32_t>(lt::torrent_handle::query_pieces));
static_assert(CT_STATUS_QUERY_VERIFIED_PIECES
	== static_cast<uint32_t>(lt::torrent_handle::query_verified_pieces));
static_assert(CT_STATUS_QUERY_TORRENT_FILE
	== static_cast<uint32_t>(lt::torrent_handle::query_torrent_file));
static_assert(CT_STATUS_QUERY_NAME
	== static_cast<uint32_t>(lt::torrent_handle::query_name));
static_assert(CT_STATUS_QUERY_SAVE_PATH
	== static_cast<uint32_t>(lt::torrent_handle::query_save_path));
static_assert(CT_RESUME_FLUSH_DISK_CACHE
	== static_cast<uint32_t>(static_cast<uint8_t>(lt::torrent_handle::flush_disk_cache)));
static_assert(CT_RESUME_SAVE_INFO_DICT
	== static_cast<uint32_t>(static_cast<uint8_t>(lt::torrent_handle::save_info_dict)));
static_assert(CT_RESUME_IF_COUNTERS_CHANGED
	== static_cast<uint32_t>(static_cast<uint8_t>(lt::torrent_handle::if_counters_changed)));
static_assert(CT_RESUME_IF_DOWNLOAD_PROGRESS
	== static_cast<uint32_t>(static_cast<uint8_t>(lt::torrent_handle::if_download_progress)));
static_assert(CT_RESUME_IF_CONFIG_CHANGED
	== static_cast<uint32_t>(static_cast<uint8_t>(lt::torrent_handle::if_config_changed)));
static_assert(CT_RESUME_IF_STATE_CHANGED
	== static_cast<uint32_t>(static_cast<uint8_t>(lt::torrent_handle::if_state_changed)));
static_assert(CT_RESUME_IF_METADATA_CHANGED
	== static_cast<uint32_t>(static_cast<uint8_t>(lt::torrent_handle::if_metadata_changed)));
static_assert(CT_RESUME_ONLY_IF_MODIFIED
	== static_cast<uint32_t>(static_cast<uint8_t>(lt::torrent_handle::only_if_modified)));
static_assert(CT_REANNOUNCE_IGNORE_MIN_INTERVAL
	== static_cast<uint32_t>(static_cast<uint8_t>(lt::torrent_handle::ignore_min_interval)));
static_assert(CT_REANNOUNCE_HIGH_PRIORITY
	== static_cast<uint32_t>(static_cast<uint8_t>(lt::torrent_handle::high_priority)));
static_assert(CT_PAUSE_GRACEFUL
	== static_cast<uint32_t>(static_cast<uint8_t>(lt::torrent_handle::graceful_pause)));
static_assert(CT_FILE_PROGRESS_PIECE_GRANULARITY
	== static_cast<uint32_t>(static_cast<uint8_t>(lt::torrent_handle::piece_granularity)));
static_assert(CT_MOVE_ALWAYS_REPLACE_FILES
	== static_cast<uint32_t>(lt::move_flags_t::always_replace_files));
static_assert(CT_MOVE_FAIL_IF_EXIST
	== static_cast<uint32_t>(lt::move_flags_t::fail_if_exist));
static_assert(CT_MOVE_DONT_REPLACE
	== static_cast<uint32_t>(lt::move_flags_t::dont_replace));
static_assert(CT_MOVE_RESET_SAVE_PATH
	== static_cast<uint32_t>(lt::move_flags_t::reset_save_path));
static_assert(CT_MOVE_RESET_SAVE_PATH_UNCHECKED
	== static_cast<uint32_t>(lt::move_flags_t::reset_save_path_unchecked));

namespace ct {

static const lt::torrent_handle& th_of(const ct_torrent_handle* h) {
	return *reinterpret_cast<const lt::torrent_handle*>(h);
}

static lt::torrent_handle& th_mut(ct_torrent_handle* h) {
	return *reinterpret_cast<lt::torrent_handle*>(h);
}

// Boundary guards for functions without a ct_error out-parameter: swallow
// everything (an expired handle is a documented no-op, allocation failure
// has nowhere to be reported).
template <typename F>
static void op(const ct_torrent_handle* h, F&& f) noexcept {
	if (h == nullptr) return;
	try {
		auto const& th = th_of(h);
		if (!th.is_valid()) return;
		f(th);
	} catch (...) {}
}

template <typename T, typename F>
static T query(const ct_torrent_handle* h, T def, F&& f) noexcept {
	if (h == nullptr) return def;
	try {
		auto const& th = th_of(h);
		if (!th.is_valid()) return def;
		return f(th);
	} catch (...) {
		return def;
	}
}

} // namespace ct

extern "C" {

void ct_torrent_handle_clone(const ct_torrent_handle* src,
	ct_torrent_handle* dst)
{
	if (!src || !dst) return;
	new (dst->data_) lt::torrent_handle(ct::th_of(src));
}

void ct_torrent_handle_drop(ct_torrent_handle* handle) {
	if (!handle) return;
	ct::th_mut(handle).~torrent_handle();
}

bool ct_torrent_handle_is_valid(const ct_torrent_handle* handle) {
	if (!handle) return false;
	return ct::th_of(handle).is_valid();
}

uint32_t ct_torrent_handle_id(const ct_torrent_handle* handle) {
	return ct::query(handle, uint32_t{0},
		[](auto const& th) { return th.id(); });
}

ct_info_hash ct_torrent_handle_info_hashes(const ct_torrent_handle* handle) {
	return ct::query(handle, ct_info_hash{}, [](auto const& th) {
		ct_info_hash result = {};
		auto hashes = th.info_hashes();
		if (hashes.has_v1())
			std::memcpy(result.v1.data, hashes.v1.data(), 20);
		if (hashes.has_v2())
			std::memcpy(result.v2.data, hashes.v2.data(), 32);
		return result;
	});
}

bool ct_torrent_handle_in_session(const ct_torrent_handle* handle) {
	return ct::query(handle, false,
		[](auto const& th) { return th.in_session(); });
}

uint64_t ct_torrent_handle_flags(const ct_torrent_handle* handle) {
	return ct::query(handle, uint64_t{0}, [](auto const& th) {
		return static_cast<uint64_t>(th.flags());
	});
}

void ct_torrent_handle_set_flags(const ct_torrent_handle* handle,
	uint64_t flags, uint64_t mask)
{
	ct::op(handle, [&](auto const& th) {
		th.set_flags(static_cast<lt::torrent_flags_t>(flags),
			static_cast<lt::torrent_flags_t>(mask));
	});
}

void ct_torrent_handle_unset_flags(const ct_torrent_handle* handle,
	uint64_t flags)
{
	ct::op(handle, [&](auto const& th) {
		th.unset_flags(static_cast<lt::torrent_flags_t>(flags));
	});
}

bool ct_torrent_handle_connect_peer(const ct_torrent_handle* handle,
	ct_endpoint endpoint, ct_error* err)
{
	// Report through a local error when the caller passed NULL, so a failed
	// call never reads as success.
	ct_error local;
	ct_error* e = err != nullptr ? err : &local;
	// Handle checks happen inside the guard: every exit must initialize the
	// caller's error, and formatting an uninitialized one is UB.
	ct::guard(e, [&]() {
		if (!handle || !ct_torrent_handle_is_valid(handle))
			throw std::invalid_argument("invalid torrent handle");
		ct::th_of(handle).connect_peer(ct::to_lt_tcp(endpoint));
	});
	return e->category == CT_ERROR_CAT_NONE;
}

// ---- piece operations ------------------------------------------------------

void ct_torrent_handle_read_piece(const ct_torrent_handle* handle,
	int32_t piece)
{
	ct::op(handle, [&](auto const& th) {
		th.read_piece(lt::piece_index_t{piece});
	});
}

bool ct_torrent_handle_have_piece(const ct_torrent_handle* handle, int32_t piece) {
	return ct::query(handle, false, [&](auto const& th) {
		return th.have_piece(lt::piece_index_t{piece});
	});
}

void ct_torrent_handle_set_piece_deadline(const ct_torrent_handle* handle,
	int32_t piece, int32_t deadline_ms, uint32_t flags)
{
	ct::op(handle, [&](auto const& th) {
		th.set_piece_deadline(lt::piece_index_t{piece}, deadline_ms,
			static_cast<lt::deadline_flags_t>(flags));
	});
}

void ct_torrent_handle_reset_piece_deadline(const ct_torrent_handle* handle,
	int32_t piece)
{
	ct::op(handle, [&](auto const& th) {
		th.reset_piece_deadline(lt::piece_index_t{piece});
	});
}

void ct_torrent_handle_clear_piece_deadlines(const ct_torrent_handle* handle) {
	ct::op(handle, [](auto const& th) { th.clear_piece_deadlines(); });
}

// ---- status and resume data ------------------------------------------------

void ct_torrent_handle_post_status(const ct_torrent_handle* handle,
	uint32_t flags)
{
	ct::op(handle, [&](auto const& th) {
		th.post_status(static_cast<lt::status_flags_t>(flags));
	});
}

bool ct_torrent_handle_save_resume_data(const ct_torrent_handle* handle,
	uint32_t flags)
{
	return ct::query(handle, false, [&](auto const& th) {
		th.save_resume_data(static_cast<lt::resume_data_flags_t>(flags));
		return true;
	});
}

bool ct_torrent_handle_need_save_resume_data(const ct_torrent_handle* handle) {
	return ct::query(handle, false,
		[](auto const& th) { return th.need_save_resume_data(); });
}

// ---- file operations -------------------------------------------------------

void ct_torrent_handle_post_file_progress(const ct_torrent_handle* handle,
	uint32_t flags)
{
	ct::op(handle, [&](auto const& th) {
		th.post_file_progress(static_cast<lt::file_progress_flags_t>(flags));
	});
}

// ---- priorities and limits -------------------------------------------------

void ct_torrent_handle_piece_priority_set(const ct_torrent_handle* handle,
	int32_t piece, uint8_t priority)
{
	ct::op(handle, [&](auto const& th) {
		th.piece_priority(lt::piece_index_t{piece},
			static_cast<lt::download_priority_t>(priority));
	});
}

uint8_t ct_torrent_handle_piece_priority_get(const ct_torrent_handle* handle,
	int32_t piece)
{
	return ct::query(handle, uint8_t{0}, [&](auto const& th) {
		return static_cast<uint8_t>(
			th.piece_priority(lt::piece_index_t{piece}));
	});
}

void ct_torrent_handle_prioritize_pieces(const ct_torrent_handle* handle,
	const uint8_t* priorities, size_t count)
{
	if (!priorities) return;
	ct::op(handle, [&](auto const& th) {
		// lt::torrent::prioritize_pieces indexes the piece picker for
		// every supplied entry with only an assert guarding the bounds;
		// refuse out-of-domain input instead of corrupting in release.
		auto ti = th.torrent_file();
		if (!ti || count > static_cast<size_t>(ti->num_pieces()))
			return;
		std::vector<lt::download_priority_t> vec;
		vec.reserve(count);
		for (size_t i = 0; i < count; ++i)
			vec.push_back(static_cast<lt::download_priority_t>(priorities[i]));
		th.prioritize_pieces(vec);
	});
}

void ct_torrent_handle_file_priority_set(const ct_torrent_handle* handle,
	int32_t file, uint8_t priority)
{
	ct::op(handle, [&](auto const& th) {
		// Without metadata lt::torrent::set_file_priority accepts any
		// non-negative index and resizes its priority vector to
		// index + 1, overflowing signed int at INT_MAX; only accept
		// in-range indexes of a metadata-complete torrent.
		auto ti = th.torrent_file();
		if (!ti || file < 0 || file >= ti->num_files())
			return;
		th.file_priority(lt::file_index_t{file},
			static_cast<lt::download_priority_t>(priority));
	});
}

uint8_t ct_torrent_handle_file_priority_get(const ct_torrent_handle* handle,
	int32_t file)
{
	return ct::query(handle, uint8_t{0}, [&](auto const& th) {
		auto ti = th.torrent_file();
		if (!ti || file < 0 || file >= ti->num_files())
			return uint8_t{0};
		return static_cast<uint8_t>(th.file_priority(lt::file_index_t{file}));
	});
}

void ct_torrent_handle_prioritize_files(const ct_torrent_handle* handle,
	const uint8_t* priorities, size_t count)
{
	if (!priorities) return;
	ct::op(handle, [&](auto const& th) {
		// Mirror prioritize_pieces: only accept lists that fit the
		// actual file count of a metadata-complete torrent.
		auto ti = th.torrent_file();
		if (!ti || count > static_cast<size_t>(ti->num_files()))
			return;
		std::vector<lt::download_priority_t> vec;
		vec.reserve(count);
		for (size_t i = 0; i < count; ++i)
			vec.push_back(static_cast<lt::download_priority_t>(priorities[i]));
		th.prioritize_files(vec);
	});
}

void ct_torrent_handle_set_upload_limit(const ct_torrent_handle* handle,
	int32_t limit)
{
	ct::op(handle, [&](auto const& th) { th.set_upload_limit(limit); });
}

void ct_torrent_handle_set_download_limit(const ct_torrent_handle* handle,
	int32_t limit)
{
	ct::op(handle, [&](auto const& th) { th.set_download_limit(limit); });
}

int32_t ct_torrent_handle_upload_limit(const ct_torrent_handle* handle) {
	return ct::query(handle, int32_t{-1},
		[](auto const& th) { return th.upload_limit(); });
}

int32_t ct_torrent_handle_download_limit(const ct_torrent_handle* handle) {
	return ct::query(handle, int32_t{-1},
		[](auto const& th) { return th.download_limit(); });
}

void ct_torrent_handle_set_max_uploads(const ct_torrent_handle* handle,
	int32_t limit)
{
	ct::op(handle, [&](auto const& th) { th.set_max_uploads(limit); });
}

void ct_torrent_handle_set_max_connections(const ct_torrent_handle* handle,
	int32_t limit)
{
	ct::op(handle, [&](auto const& th) { th.set_max_connections(limit); });
}

int32_t ct_torrent_handle_max_uploads(const ct_torrent_handle* handle) {
	return ct::query(handle, int32_t{-1},
		[](auto const& th) { return th.max_uploads(); });
}

int32_t ct_torrent_handle_max_connections(const ct_torrent_handle* handle) {
	return ct::query(handle, int32_t{-1},
		[](auto const& th) { return th.max_connections(); });
}

// ---- trackers ----------------------------------------------------------------

void ct_torrent_handle_post_trackers(const ct_torrent_handle* handle) {
	ct::op(handle, [](auto const& th) { th.post_trackers(); });
}

void ct_torrent_handle_add_tracker(const ct_torrent_handle* handle,
	ct_str_view url, uint8_t tier)
{
	ct::op(handle, [&](auto const& th) {
		lt::announce_entry ae(std::string(ct::view(url)));
		ae.tier = tier;
		th.add_tracker(ae);
	});
}

void ct_torrent_handle_replace_trackers(const ct_torrent_handle* handle,
	const ct_str_view* urls, const uint8_t* tiers, size_t count)
{
	ct::op(handle, [&](auto const& th) {
		std::vector<lt::announce_entry> aes;
		aes.reserve(count);
		for (size_t i = 0; i < count; ++i) {
			lt::announce_entry ae(std::string(ct::view(urls[i])));
			ae.tier = tiers[i];
			aes.push_back(std::move(ae));
		}
		th.replace_trackers(aes);
	});
}

void ct_torrent_handle_force_reannounce(const ct_torrent_handle* handle,
	int32_t seconds, int32_t tracker_index, uint32_t flags)
{
	ct::op(handle, [&](auto const& th) {
		th.force_reannounce(seconds, tracker_index,
			static_cast<lt::reannounce_flags_t>(flags));
	});
}

void ct_torrent_handle_scrape_tracker(const ct_torrent_handle* handle,
	int32_t tracker_index)
{
	ct::op(handle, [&](auto const& th) { th.scrape_tracker(tracker_index); });
}

// ---- web seeds ---------------------------------------------------------------

void ct_torrent_handle_add_url_seed(const ct_torrent_handle* handle,
	ct_str_view url)
{
	ct::op(handle, [&](auto const& th) {
		th.add_url_seed(std::string(ct::view(url)));
	});
}

void ct_torrent_handle_remove_url_seed(const ct_torrent_handle* handle,
	ct_str_view url)
{
	ct::op(handle, [&](auto const& th) {
		th.remove_url_seed(std::string(ct::view(url)));
	});
}

ct_str_list* ct_torrent_handle_url_seeds(const ct_torrent_handle* handle,
	ct_error* err)
{
	return ct::guard(err, [&]() -> ct_str_list* {
		if (!handle) throw std::invalid_argument("null handle");
		const auto& th = ct::th_of(handle);
		if (!th.is_valid()) throw std::invalid_argument("invalid handle");
		auto seeds = th.url_seeds();
		auto list = std::make_unique<ct_str_list>();
		list->items.assign(seeds.begin(), seeds.end());
		return list.release();
	});
}

// ---- queue position ----------------------------------------------------------

int32_t ct_torrent_handle_queue_position(const ct_torrent_handle* handle) {
	return ct::query(handle, int32_t{-1}, [](auto const& th) {
		return static_cast<int32_t>(th.queue_position());
	});
}

void ct_torrent_handle_queue_position_up(const ct_torrent_handle* handle) {
	ct::op(handle, [](auto const& th) { th.queue_position_up(); });
}

void ct_torrent_handle_queue_position_down(const ct_torrent_handle* handle) {
	ct::op(handle, [](auto const& th) { th.queue_position_down(); });
}

void ct_torrent_handle_queue_position_top(const ct_torrent_handle* handle) {
	ct::op(handle, [](auto const& th) { th.queue_position_top(); });
}

void ct_torrent_handle_queue_position_bottom(const ct_torrent_handle* handle) {
	ct::op(handle, [](auto const& th) { th.queue_position_bottom(); });
}

void ct_torrent_handle_queue_position_set(const ct_torrent_handle* handle,
	int32_t pos)
{
	ct::op(handle, [&](auto const& th) {
		th.queue_position_set(lt::queue_position_t{pos});
	});
}

// ---- control -----------------------------------------------------------------

void ct_torrent_handle_pause(const ct_torrent_handle* handle, uint32_t flags) {
	ct::op(handle, [&](auto const& th) {
		th.pause(static_cast<lt::pause_flags_t>(flags));
	});
}

void ct_torrent_handle_resume(const ct_torrent_handle* handle) {
	ct::op(handle, [](auto const& th) { th.resume(); });
}

void ct_torrent_handle_force_recheck(const ct_torrent_handle* handle) {
	ct::op(handle, [](auto const& th) { th.force_recheck(); });
}

void ct_torrent_handle_flush_cache(const ct_torrent_handle* handle) {
	ct::op(handle, [](auto const& th) { th.flush_cache(); });
}

void ct_torrent_handle_clear_error(const ct_torrent_handle* handle) {
	ct::op(handle, [](auto const& th) { th.clear_error(); });
}

void ct_torrent_handle_force_dht_announce(const ct_torrent_handle* handle) {
	ct::op(handle, [](auto const& th) { th.force_dht_announce(); });
}

void ct_torrent_handle_move_storage(const ct_torrent_handle* handle,
	ct_str_view path, uint32_t flags)
{
	ct::op(handle, [&](auto const& th) {
		th.move_storage(std::string(ct::view(path)),
			static_cast<lt::move_flags_t>(flags));
	});
}

void ct_torrent_handle_rename_file(const ct_torrent_handle* handle,
	int32_t index, ct_str_view name)
{
	ct::op(handle, [&](auto const& th) {
		// lt::torrent::rename_file indexes file_storage with only an
		// assert guarding the bounds; refuse out-of-domain input.
		auto ti = th.torrent_file();
		if (!ti || index < 0 || index >= ti->num_files())
			return;
		th.rename_file(lt::file_index_t{index}, std::string(ct::view(name)));
	});
}

// ---- SSL ---------------------------------------------------------------------

void ct_torrent_handle_set_ssl_certificate(const ct_torrent_handle* handle,
	ct_str_view cert, ct_str_view private_key, ct_str_view dh_params,
	ct_str_view passphrase)
{
	// set_ssl_certificate is (anomalously) non-const upstream; it only
	// posts an async message like every other operation.
	ct::op(handle, [&](auto const& th) {
		const_cast<lt::torrent_handle&>(th).set_ssl_certificate(
			std::string(ct::view(cert)),
			std::string(ct::view(private_key)),
			std::string(ct::view(dh_params)),
			std::string(ct::view(passphrase)));
	});
}

void ct_torrent_handle_set_ssl_certificate_buffer(
	const ct_torrent_handle* handle, ct_str_view cert,
	ct_str_view private_key, ct_str_view dh_params)
{
	// Non-const upstream, same as set_ssl_certificate.
	ct::op(handle, [&](auto const& th) {
		const_cast<lt::torrent_handle&>(th).set_ssl_certificate_buffer(
			std::string(ct::view(cert)),
			std::string(ct::view(private_key)),
			std::string(ct::view(dh_params)));
	});
}

// ---- peer management ---------------------------------------------------------

void ct_torrent_handle_post_peer_info(const ct_torrent_handle* handle) {
	ct::op(handle, [](auto const& th) { th.post_peer_info(); });
}

// ---- accessors ---------------------------------------------------------------

ct_torrent_status* ct_torrent_handle_status(const ct_torrent_handle* handle,
	uint32_t flags, ct_error* err)
{
	return ct::guard(err, [&]() -> ct_torrent_status* {
		if (!handle) throw std::invalid_argument("null handle");
		const auto& th = ct::th_of(handle);
		if (!th.is_valid()) throw std::invalid_argument("invalid handle");
		auto st = th.status(static_cast<lt::status_flags_t>(flags));
		// Capture the id before the move; the status's embedded handle is
		// the authoritative source (same torrent as `th`).
		uint32_t const id = st.handle.id();
		return new ct_torrent_status{std::move(st), id};
	});
}

ct_str ct_torrent_handle_save_path(const ct_torrent_handle* handle, ct_error* err) {
	return ct::guard(err, [&]() -> ct_str {
		if (!handle) throw std::invalid_argument("null handle");
		const auto& th = ct::th_of(handle);
		if (!th.is_valid()) throw std::invalid_argument("invalid handle");
		auto st = th.status(lt::torrent_handle::query_save_path);
		return ct::box_string(std::move(st.save_path));
	});
}

ct_str ct_torrent_handle_name(const ct_torrent_handle* handle, ct_error* err) {
	return ct::guard(err, [&]() -> ct_str {
		if (!handle) throw std::invalid_argument("null handle");
		const auto& th = ct::th_of(handle);
		if (!th.is_valid()) throw std::invalid_argument("invalid handle");
		auto st = th.status(lt::torrent_handle::query_name);
		return ct::box_string(std::move(st.name));
	});
}

ct_torrent_info* ct_torrent_handle_torrent_file(const ct_torrent_handle* handle,
	ct_error* err)
{
	return ct::guard(err, [&]() -> ct_torrent_info* {
		if (!handle) throw std::invalid_argument("null handle");
		const auto& th = ct::th_of(handle);
		if (!th.is_valid()) throw std::invalid_argument("invalid handle");
		auto ti = th.torrent_file();
		if (!ti) return nullptr;
		return reinterpret_cast<ct_torrent_info*>(
			new std::shared_ptr<const lt::torrent_info>(ti));
	});
}

ct_str_list* ct_torrent_handle_file_paths(const ct_torrent_handle* handle,
	ct_error* err)
{
	return ct::guard(err, [&]() -> ct_str_list* {
		if (!handle) throw std::invalid_argument("null handle");
		const auto& th = ct::th_of(handle);
		if (!th.is_valid()) throw std::invalid_argument("invalid handle");
		auto ti = th.torrent_file();
		if (!ti) return nullptr;
		auto const renames = th.get_renamed_files();
		auto const& fs = ti->layout();
		auto list = std::make_unique<ct_str_list>();
		list->items.reserve(static_cast<size_t>(fs.num_files()));
		for (auto const i : fs.file_range())
			list->items.push_back(renames.file_path(fs, i));
		return list.release();
	});
}

} // extern "C"
