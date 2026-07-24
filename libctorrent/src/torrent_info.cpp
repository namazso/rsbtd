// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

// Torrent metadata accessors (lt::torrent_info and its file_storage), plus
// the ct_str_list and ct_file_slice_array containers.

#include <ctorrent/ct_torrent_info.h>

#include "ct_common.hpp"
#include "handles.hpp"

#include <libtorrent/download_priority.hpp>
#include <libtorrent/file_storage.hpp>
#include <libtorrent/info_hash.hpp>
#include <libtorrent/torrent_info.hpp>
#include <libtorrent/units.hpp>

#include <cstring>
#include <memory>
#include <new>
#include <string>
#include <utility>
#include <vector>

namespace lt = libtorrent;

// file_flags_t bit values relied on by CT_FILE_FLAG_*.
static_assert(CT_FILE_FLAG_PAD_FILE
	== static_cast<uint8_t>(lt::file_storage::flag_pad_file));
static_assert(CT_FILE_FLAG_HIDDEN
	== static_cast<uint8_t>(lt::file_storage::flag_hidden));
static_assert(CT_FILE_FLAG_EXECUTABLE
	== static_cast<uint8_t>(lt::file_storage::flag_executable));
static_assert(CT_FILE_FLAG_SYMLINK
	== static_cast<uint8_t>(lt::file_storage::flag_symlink));

// ct_download_priority doc comment relies on these values.
static_assert(static_cast<uint8_t>(lt::dont_download) == 0);
static_assert(static_cast<uint8_t>(lt::low_priority) == 1);
static_assert(static_cast<uint8_t>(lt::default_priority) == 4);
static_assert(static_cast<uint8_t>(lt::top_priority) == 7);
static_assert(sizeof(lt::download_priority_t) == 1);
static_assert(std::is_trivially_copyable_v<lt::download_priority_t>);

namespace {

lt::torrent_info const& ti_of(const ct_torrent_info* ti) noexcept
{
	return *ct::unwrap(ti);
}

// Only valid to call when ti_of(ti).is_loaded(); layout() asserts
// otherwise. Every use is behind a valid_file/valid_piece/is_loaded guard.
lt::file_storage const& fs_of(const ct_torrent_info* ti) noexcept
{
	return ct::unwrap(ti)->layout();
}

bool valid_file(const ct_torrent_info* ti, ct_file_index file) noexcept
{
	return ti_of(ti).is_loaded() && file >= 0
		&& file < ti_of(ti).num_files();
}

bool valid_piece(const ct_torrent_info* ti, ct_piece_index piece) noexcept
{
	return ti_of(ti).is_valid() && piece >= 0
		&& piece < ti_of(ti).num_pieces();
}

} // anonymous namespace

extern "C" {

// ---- ct_str_list ------------------------------------------------------------

size_t ct_str_list_len(const ct_str_list* list)
{
	return list->items.size();
}

ct_str_view ct_str_list_get(const ct_str_list* list, size_t i)
{
	if (i >= list->items.size()) return ct_str_view{nullptr, 0};
	return ct::view(lt::string_view(list->items[i]));
}

void ct_str_list_free(ct_str_list* list)
{
	delete list;
}

// ---- ct_file_slice_array ----------------------------------------------------

void ct_file_slice_array_free(ct_file_slice_array* array)
{
	delete static_cast<std::vector<ct_file_slice>*>(array->box_);
	*array = ct_file_slice_array{nullptr, 0, nullptr};
}

// ---- lifecycle --------------------------------------------------------------

ct_torrent_info* ct_torrent_info_from_info_hash(const ct_info_hash* ih)
{
	try {
		lt::info_hash_t h;
		std::memcpy(h.v1.data(), ih->v1.data, 20);
		std::memcpy(h.v2.data(), ih->v2.data, 32);
		return ct::wrap(std::make_shared<lt::torrent_info>(h));
	} catch (...) {
		return nullptr;
	}
}

ct_torrent_info* ct_torrent_info_clone(const ct_torrent_info* ti)
{
	try {
		return ct::wrap(ct::unwrap(ti));
	} catch (...) {
		return nullptr;
	}
}

void ct_torrent_info_free(ct_torrent_info* ti)
{
	delete reinterpret_cast<ct::ti_ptr*>(ti);
}

// ---- torrent-wide properties ------------------------------------------------

bool ct_torrent_info_is_valid(const ct_torrent_info* ti)
{
	return ti_of(ti).is_valid();
}

ct_str_view ct_torrent_info_name(const ct_torrent_info* ti)
{
	return ct::view(lt::string_view(ti_of(ti).name()));
}

int64_t ct_torrent_info_total_size(const ct_torrent_info* ti)
{
	return ti_of(ti).total_size();
}

int64_t ct_torrent_info_size_on_disk(const ct_torrent_info* ti)
{
	return ti_of(ti).size_on_disk();
}

int32_t ct_torrent_info_piece_length(const ct_torrent_info* ti)
{
	if (!ti_of(ti).is_valid()) return 0;
	return ti_of(ti).piece_length();
}

int32_t ct_torrent_info_num_pieces(const ct_torrent_info* ti)
{
	if (!ti_of(ti).is_valid()) return 0;
	return ti_of(ti).num_pieces();
}

int32_t ct_torrent_info_blocks_per_piece(const ct_torrent_info* ti)
{
	if (!ti_of(ti).is_valid()) return 0;
	return ti_of(ti).blocks_per_piece();
}

ct_info_hash ct_torrent_info_info_hashes(const ct_torrent_info* ti)
{
	return ct::to_ct(ti_of(ti).info_hashes());
}

bool ct_torrent_info_has_v1(const ct_torrent_info* ti)
{
	return ti_of(ti).v1();
}

bool ct_torrent_info_has_v2(const ct_torrent_info* ti)
{
	return ti_of(ti).v2();
}

int32_t ct_torrent_info_piece_size(const ct_torrent_info* ti,
	ct_piece_index piece)
{
	if (!valid_piece(ti, piece)) return 0;
	return ti_of(ti).piece_size(lt::piece_index_t{piece});
}

int32_t ct_torrent_info_piece_size_for_req(const ct_torrent_info* ti,
	ct_piece_index piece)
{
	if (!valid_piece(ti, piece)) return 0;
	return ti_of(ti).piece_size_for_req(lt::piece_index_t{piece});
}

ct_sha1 ct_torrent_info_hash_for_piece(const ct_torrent_info* ti,
	ct_piece_index piece)
{
	// v2-only torrents carry no SHA-1 piece hashes; indexing the (empty)
	// v1 hash table would read out of bounds.
	if (!valid_piece(ti, piece) || !ti_of(ti).is_loaded()
		|| !ti_of(ti).v1())
		return ct_sha1{};
	return ct::to_ct(ti_of(ti).hash_for_piece(lt::piece_index_t{piece}));
}

ct_str_view ct_torrent_info_ssl_cert(const ct_torrent_info* ti)
{
	return ct::view(ti_of(ti).ssl_cert());
}

bool ct_torrent_info_is_private(const ct_torrent_info* ti)
{
	return ti_of(ti).priv();
}

bool ct_torrent_info_is_i2p(const ct_torrent_info* ti)
{
	return ti_of(ti).is_i2p();
}

ct_span ct_torrent_info_info_section(const ct_torrent_info* ti)
{
	auto const s = ti_of(ti).info_section();
	return ct_span{reinterpret_cast<uint8_t const*>(s.data()),
		static_cast<size_t>(s.size())};
}

ct_buf ct_torrent_info_similar_torrents(const ct_torrent_info* ti)
{
	try {
		auto const similar = ti_of(ti).similar_torrents();
		std::vector<char> bytes;
		bytes.reserve(similar.size() * 20);
		for (auto const& h : similar)
			bytes.insert(bytes.end(), h.data(), h.data() + 20);
		return ct::box_buffer(std::move(bytes));
	} catch (...) {
		return ct_buf{};
	}
}

ct_str_list* ct_torrent_info_collections(const ct_torrent_info* ti)
{
	try {
		auto list = std::make_unique<ct_str_list>();
		list->items = ti_of(ti).collections();
		return list.release();
	} catch (...) {
		return nullptr;
	}
}

// ---- file list ----------------------------------------------------------------

int32_t ct_torrent_info_num_files(const ct_torrent_info* ti)
{
	return ti_of(ti).num_files();
}

int64_t ct_torrent_info_file_size(const ct_torrent_info* ti,
	ct_file_index file)
{
	if (!valid_file(ti, file)) return 0;
	return fs_of(ti).file_size(lt::file_index_t{file});
}

ct_str ct_torrent_info_file_path(const ct_torrent_info* ti,
	ct_file_index file)
{
	if (!valid_file(ti, file)) return ct_str{};
	try {
		return ct::box_string(fs_of(ti).file_path(lt::file_index_t{file}));
	} catch (...) {
		return ct_str{};
	}
}

ct_str_view ct_torrent_info_file_name(const ct_torrent_info* ti,
	ct_file_index file)
{
	if (!valid_file(ti, file)) return ct_str_view{nullptr, 0};
	return ct::view(fs_of(ti).file_name(lt::file_index_t{file}));
}

int64_t ct_torrent_info_file_offset(const ct_torrent_info* ti,
	ct_file_index file)
{
	if (!valid_file(ti, file)) return 0;
	return fs_of(ti).file_offset(lt::file_index_t{file});
}

uint8_t ct_torrent_info_file_flags(const ct_torrent_info* ti,
	ct_file_index file)
{
	if (!valid_file(ti, file)) return 0;
	return static_cast<uint8_t>(fs_of(ti).file_flags(lt::file_index_t{file}));
}

ct_str ct_torrent_info_file_symlink(const ct_torrent_info* ti,
	ct_file_index file)
{
	if (!valid_file(ti, file)) return ct_str{};
	try {
		return ct::box_string(fs_of(ti).symlink(lt::file_index_t{file}));
	} catch (...) {
		return ct_str{};
	}
}

int64_t ct_torrent_info_file_mtime(const ct_torrent_info* ti,
	ct_file_index file)
{
	if (!valid_file(ti, file)) return 0;
	return fs_of(ti).mtime(lt::file_index_t{file});
}

ct_sha256 ct_torrent_info_file_root(const ct_torrent_info* ti,
	ct_file_index file)
{
	if (!valid_file(ti, file)) return ct_sha256{};
	return ct::to_ct(fs_of(ti).root(lt::file_index_t{file}));
}

int32_t ct_torrent_info_file_num_pieces(const ct_torrent_info* ti,
	ct_file_index file)
{
	if (!valid_file(ti, file)) return 0;
	return fs_of(ti).file_num_pieces(lt::file_index_t{file});
}

int32_t ct_torrent_info_file_num_blocks(const ct_torrent_info* ti,
	ct_file_index file)
{
	if (!valid_file(ti, file)) return 0;
	return fs_of(ti).file_num_blocks(lt::file_index_t{file});
}

bool ct_torrent_info_file_absolute_path(const ct_torrent_info* ti,
	ct_file_index file)
{
	if (!valid_file(ti, file)) return false;
	return fs_of(ti).file_absolute_path(lt::file_index_t{file});
}

// ---- piece/file mapping ----------------------------------------------------------

ct_peer_request ct_torrent_info_map_file(const ct_torrent_info* ti,
	ct_file_index file, int64_t offset, int32_t size)
{
	if (!valid_file(ti, file)) return ct_peer_request{};
	// lt::file_storage::map_file only asserts its range preconditions
	// (compiled out of release builds), and its internal signed additions
	// overflow on hostile offsets. Subtraction form cannot overflow: both
	// operands are >= 0 here.
	int64_t const file_size = fs_of(ti).file_size(lt::file_index_t{file});
	if (offset < 0 || size < 0 || int64_t(size) > file_size - offset)
		return ct_peer_request{};
	return ct::to_ct(
		fs_of(ti).map_file(lt::file_index_t{file}, offset, size));
}

ct_file_slice_array ct_torrent_info_map_block(const ct_torrent_info* ti,
	ct_piece_index piece, int64_t offset, int32_t size)
{
	if (!valid_piece(ti, piece)) return ct_file_slice_array{nullptr, 0, nullptr};
	// Same overflow-safe validation as map_file; piece * piece_length fits
	// in int64 because both factors are int32.
	int64_t const piece_start
		= int64_t(piece) * fs_of(ti).piece_length();
	int64_t const remaining = fs_of(ti).total_size() - piece_start;
	if (offset < 0 || size < 0 || int64_t(size) > remaining - offset)
		return ct_file_slice_array{nullptr, 0, nullptr};
	try {
		auto const slices
			= fs_of(ti).map_block(lt::piece_index_t{piece}, offset, size);
		auto out = std::make_unique<std::vector<ct_file_slice>>();
		out->reserve(slices.size());
		for (auto const& s : slices)
			out->push_back(ct_file_slice{static_cast<int32_t>(s.file_index),
				s.offset, s.size});
		ct_file_slice_array array{out->data(), out->size(), out.get()};
		out.release();
		return array;
	} catch (...) {
		return ct_file_slice_array{nullptr, 0, nullptr};
	}
}

ct_file_index ct_torrent_info_file_index_at_offset(const ct_torrent_info* ti,
	int64_t offset)
{
	if (!ti_of(ti).is_loaded() || offset < 0
		|| offset >= ti_of(ti).total_size())
		return -1;
	return static_cast<int32_t>(fs_of(ti).file_index_at_offset(offset));
}

ct_file_index ct_torrent_info_file_index_at_piece(const ct_torrent_info* ti,
	ct_piece_index piece)
{
	if (!valid_piece(ti, piece)) return -1;
	return static_cast<int32_t>(
		fs_of(ti).file_index_at_piece(lt::piece_index_t{piece}));
}

ct_file_index ct_torrent_info_last_file_index_at_piece(
	const ct_torrent_info* ti, ct_piece_index piece)
{
	if (!valid_piece(ti, piece)) return -1;
	return static_cast<int32_t>(
		fs_of(ti).last_file_index_at_piece(lt::piece_index_t{piece}));
}

ct_file_index ct_torrent_info_file_index_for_root(const ct_torrent_info* ti,
	const ct_sha256* root)
{
	if (!ti_of(ti).is_loaded()) return -1;
	lt::sha256_hash h;
	std::memcpy(h.data(), root->data, 32);
	return static_cast<int32_t>(fs_of(ti).file_index_for_root(h));
}

ct_piece_index ct_torrent_info_piece_index_at_file(const ct_torrent_info* ti,
	ct_file_index file)
{
	if (!valid_file(ti, file)) return -1;
	return static_cast<int32_t>(
		fs_of(ti).piece_index_at_file(lt::file_index_t{file}));
}

ct_piece_index ct_torrent_info_last_piece_index_at_file(
	const ct_torrent_info* ti, ct_file_index file)
{
	if (!valid_file(ti, file)) return -1;
	return static_cast<int32_t>(
		fs_of(ti).last_piece_index_at_file(lt::file_index_t{file}));
}

} // extern "C"
