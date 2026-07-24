// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

// add_torrent_params field access, .torrent loading and magnet links.

#include <ctorrent/ct_add_torrent_params.h>

#include "ct_common.hpp"
#include "handles.hpp"

#include <libtorrent/add_torrent_params.hpp>
#include <libtorrent/bitfield.hpp>
#include <libtorrent/download_priority.hpp>
#include <libtorrent/error_code.hpp>
#include <libtorrent/load_torrent.hpp>
#include <libtorrent/magnet_uri.hpp>
#include <libtorrent/sha1_hash.hpp>
#include <libtorrent/storage_defs.hpp>
#include <libtorrent/torrent_flags.hpp>
#include <libtorrent/units.hpp>

#include <cstring>
#include <iterator>
#include <memory>
#include <new>
#include <string>
#include <utility>
#include <vector>

namespace lt = libtorrent;

// storage_mode_t values relied on by CT_STORAGE_MODE_*.
static_assert(CT_STORAGE_MODE_ALLOCATE
	== static_cast<int>(lt::storage_mode_allocate));
static_assert(CT_STORAGE_MODE_SPARSE
	== static_cast<int>(lt::storage_mode_sparse));

// torrent_flags_t bit values relied on by CT_TORRENT_FLAG_*.
static_assert(CT_TORRENT_FLAG_SEED_MODE
	== static_cast<uint64_t>(lt::torrent_flags::seed_mode));
static_assert(CT_TORRENT_FLAG_UPLOAD_MODE
	== static_cast<uint64_t>(lt::torrent_flags::upload_mode));
static_assert(CT_TORRENT_FLAG_SHARE_MODE
	== static_cast<uint64_t>(lt::torrent_flags::share_mode));
static_assert(CT_TORRENT_FLAG_APPLY_IP_FILTER
	== static_cast<uint64_t>(lt::torrent_flags::apply_ip_filter));
static_assert(CT_TORRENT_FLAG_PAUSED
	== static_cast<uint64_t>(lt::torrent_flags::paused));
static_assert(CT_TORRENT_FLAG_AUTO_MANAGED
	== static_cast<uint64_t>(lt::torrent_flags::auto_managed));
static_assert(CT_TORRENT_FLAG_DUPLICATE_IS_ERROR
	== static_cast<uint64_t>(lt::torrent_flags::duplicate_is_error));
static_assert(CT_TORRENT_FLAG_UPDATE_SUBSCRIBE
	== static_cast<uint64_t>(lt::torrent_flags::update_subscribe));
static_assert(CT_TORRENT_FLAG_SUPER_SEEDING
	== static_cast<uint64_t>(lt::torrent_flags::super_seeding));
static_assert(CT_TORRENT_FLAG_SEQUENTIAL_DOWNLOAD
	== static_cast<uint64_t>(lt::torrent_flags::sequential_download));
static_assert(CT_TORRENT_FLAG_STOP_WHEN_READY
	== static_cast<uint64_t>(lt::torrent_flags::stop_when_ready));
static_assert(CT_TORRENT_FLAG_NEED_SAVE_RESUME
	== static_cast<uint64_t>(lt::torrent_flags::need_save_resume));
static_assert(CT_TORRENT_FLAG_DISABLE_DHT
	== static_cast<uint64_t>(lt::torrent_flags::disable_dht));
static_assert(CT_TORRENT_FLAG_DISABLE_LSD
	== static_cast<uint64_t>(lt::torrent_flags::disable_lsd));
static_assert(CT_TORRENT_FLAG_DISABLE_PEX
	== static_cast<uint64_t>(lt::torrent_flags::disable_pex));
static_assert(CT_TORRENT_FLAG_NO_VERIFY_FILES
	== static_cast<uint64_t>(lt::torrent_flags::no_verify_files));
static_assert(CT_TORRENT_FLAG_DEFAULT_DONT_DOWNLOAD
	== static_cast<uint64_t>(lt::torrent_flags::default_dont_download));
static_assert(CT_TORRENT_FLAG_I2P_TORRENT
	== static_cast<uint64_t>(lt::torrent_flags::i2p_torrent));
static_assert(CT_TORRENT_FLAG_DISABLE_V1_HASHES
	== static_cast<uint64_t>(lt::torrent_flags::disable_v1_hashes));
static_assert(CT_TORRENT_FLAGS_ALL
	== static_cast<uint64_t>(lt::torrent_flags::all));
static_assert(CT_TORRENT_FLAGS_DEFAULT
	== static_cast<uint64_t>(lt::torrent_flags::default_flags));

static_assert(sizeof(lt::sha256_hash) == 32);

namespace {

using ct::unwrap;
using ct::wrap;

ct_str_view str_field(std::string const& s) noexcept
{
	return ct_str_view{s.data(), s.size()};
}

void set_str_field(std::string& field, ct_str_view value)
{
	// Allocation failure terminates (see ct::infallible); a swallowed
	// exception here would silently drop the caller's assignment.
	ct::infallible([&] { field.assign(value.ptr, value.len); });
}

ct_bitfield_view bitfield_view(lt::bitfield const& b) noexcept
{
	return ct_bitfield_view{reinterpret_cast<uint8_t const*>(b.data()),
		b.size()};
}

lt::bitfield make_bitfield(const uint8_t* bytes, int32_t num_bits)
{
	if (bytes == nullptr || num_bits <= 0) return lt::bitfield();
	return lt::bitfield(reinterpret_cast<char const*>(bytes), num_bits);
}

} // anonymous namespace

namespace ct {

lt::load_torrent_limits to_lt_load_limits(const ct_load_torrent_limits* limits)
{
	lt::load_torrent_limits out;
	if (limits == nullptr) return out;
	out.max_buffer_size = limits->max_buffer_size;
	out.max_pieces = limits->max_pieces;
	out.max_decode_depth = limits->max_decode_depth;
	out.max_decode_tokens = limits->max_decode_tokens;
	out.max_duplicate_filenames = limits->max_duplicate_filenames;
	out.max_directory_depth = limits->max_directory_depth;
	return out;
}

ct_add_torrent_params* box_atp(lt::add_torrent_params atp)
{
	return wrap(new lt::add_torrent_params(std::move(atp)));
}

} // namespace ct

extern "C" {

// ---- lifecycle --------------------------------------------------------------

ct_add_torrent_params* ct_atp_new(void)
{
	return wrap(new (std::nothrow) lt::add_torrent_params());
}

ct_add_torrent_params* ct_atp_clone(const ct_add_torrent_params* atp)
{
	try {
		return wrap(new lt::add_torrent_params(unwrap(atp)));
	} catch (...) {
		return nullptr;
	}
}

void ct_atp_free(ct_add_torrent_params* atp)
{
	delete &unwrap(atp);
}

// ---- loading .torrent files ---------------------------------------------------

ct_load_torrent_limits ct_load_torrent_limits_default(void)
{
	lt::load_torrent_limits const def;
	ct_load_torrent_limits out;
	out.max_buffer_size = def.max_buffer_size;
	out.max_pieces = def.max_pieces;
	out.max_decode_depth = def.max_decode_depth;
	out.max_decode_tokens = def.max_decode_tokens;
	out.max_duplicate_filenames = def.max_duplicate_filenames;
	out.max_directory_depth = def.max_directory_depth;
	return out;
}

ct_add_torrent_params* ct_load_torrent_file(ct_str_view path,
	const ct_load_torrent_limits* limits, ct_error* err)
{
	return ct::guard(err, [&]() -> ct_add_torrent_params* {
		lt::error_code ec;
		auto atp = lt::load_torrent_file(
			std::string(path.ptr, path.len), ec, ct::to_lt_load_limits(limits));
		if (ec) {
			ct::set_error(err, ec);
			return nullptr;
		}
		return ct::box_atp(std::move(atp));
	});
}

ct_add_torrent_params* ct_load_torrent_buffer(ct_span buffer,
	const ct_load_torrent_limits* limits, ct_error* err)
{
	return ct::guard(err, [&]() -> ct_add_torrent_params* {
		auto const lt_limits = ct::to_lt_load_limits(limits);
		// lt::load_torrent_buffer never consults max_buffer_size (only the
		// file loader does); enforce it here so both entry points agree.
		if (lt_limits.max_buffer_size < 0
			|| buffer.len > static_cast<size_t>(lt_limits.max_buffer_size)) {
			ct::set_error(err, lt::errors::metadata_too_large);
			return nullptr;
		}
		lt::error_code ec;
		auto atp = lt::load_torrent_buffer(ct::span(buffer), ec, lt_limits);
		if (ec) {
			ct::set_error(err, ec);
			return nullptr;
		}
		return ct::box_atp(std::move(atp));
	});
}

// ---- magnet links ---------------------------------------------------------------

ct_add_torrent_params* ct_parse_magnet_uri(ct_str_view uri, ct_error* err)
{
	return ct::guard(err, [&]() -> ct_add_torrent_params* {
		lt::error_code ec;
		auto atp = lt::parse_magnet_uri(ct::view(uri), ec);
		if (ec) {
			ct::set_error(err, ec);
			return nullptr;
		}
		return ct::box_atp(std::move(atp));
	});
}

ct_str ct_make_magnet_uri(const ct_add_torrent_params* atp, ct_error* err)
{
	return ct::guard(err, [&]() -> ct_str {
		return ct::box_string(lt::make_magnet_uri(unwrap(atp)));
	});
}

// ---- torrent metadata (ti) --------------------------------------------------------

ct_torrent_info* ct_atp_get_ti(const ct_add_torrent_params* atp)
{
	if (!unwrap(atp).ti) return nullptr;
	try {
		return ct::wrap(ct::ti_ptr(unwrap(atp).ti));
	} catch (...) {
		return nullptr;
	}
}

void ct_atp_set_ti(ct_add_torrent_params* atp, const ct_torrent_info* ti)
{
	if (ti == nullptr) {
		unwrap(atp).ti.reset();
		return;
	}
#if TORRENT_ABI_VERSION < 4
	// add_torrent_params::ti is shared_ptr<torrent_info> (non-const) below
	// ABI 4; nothing mutates through it.
	unwrap(atp).ti
		= std::const_pointer_cast<lt::torrent_info>(ct::unwrap(ti));
#else
	unwrap(atp).ti = ct::unwrap(ti);
#endif
}

int32_t ct_atp_version(const ct_add_torrent_params* atp)
{
	return unwrap(atp).version;
}

// ---- strings ------------------------------------------------------------------------

ct_str_view ct_atp_name(const ct_add_torrent_params* atp)
{
	return str_field(unwrap(atp).name);
}

void ct_atp_set_name(ct_add_torrent_params* atp, ct_str_view value)
{
	set_str_field(unwrap(atp).name, value);
}

ct_str_view ct_atp_save_path(const ct_add_torrent_params* atp)
{
	return str_field(unwrap(atp).save_path);
}

void ct_atp_set_save_path(ct_add_torrent_params* atp, ct_str_view value)
{
	set_str_field(unwrap(atp).save_path, value);
}

ct_str_view ct_atp_part_file_dir(const ct_add_torrent_params* atp)
{
	return str_field(unwrap(atp).part_file_dir);
}

void ct_atp_set_part_file_dir(ct_add_torrent_params* atp, ct_str_view value)
{
	set_str_field(unwrap(atp).part_file_dir, value);
}

ct_str_view ct_atp_trackerid(const ct_add_torrent_params* atp)
{
	return str_field(unwrap(atp).trackerid);
}

void ct_atp_set_trackerid(ct_add_torrent_params* atp, ct_str_view value)
{
	set_str_field(unwrap(atp).trackerid, value);
}

ct_str_view ct_atp_comment(const ct_add_torrent_params* atp)
{
	return str_field(unwrap(atp).comment);
}

void ct_atp_set_comment(ct_add_torrent_params* atp, ct_str_view value)
{
	set_str_field(unwrap(atp).comment, value);
}

ct_str_view ct_atp_created_by(const ct_add_torrent_params* atp)
{
	return str_field(unwrap(atp).created_by);
}

void ct_atp_set_created_by(ct_add_torrent_params* atp, ct_str_view value)
{
	set_str_field(unwrap(atp).created_by, value);
}

ct_str_view ct_atp_root_certificate(const ct_add_torrent_params* atp)
{
	return str_field(unwrap(atp).root_certificate);
}

void ct_atp_set_root_certificate(ct_add_torrent_params* atp,
	ct_str_view value)
{
	set_str_field(unwrap(atp).root_certificate, value);
}

// ---- trackers ---------------------------------------------------------------------------

size_t ct_atp_num_trackers(const ct_add_torrent_params* atp)
{
	return unwrap(atp).trackers.size();
}

ct_str_view ct_atp_tracker(const ct_add_torrent_params* atp, size_t i)
{
	auto const& trackers = unwrap(atp).trackers;
	if (i >= trackers.size()) return ct_str_view{nullptr, 0};
	return str_field(trackers[i]);
}

size_t ct_atp_num_tracker_tiers(const ct_add_torrent_params* atp)
{
	return unwrap(atp).tracker_tiers.size();
}

int32_t ct_atp_tracker_tier(const ct_add_torrent_params* atp, size_t i)
{
	auto const& tiers = unwrap(atp).tracker_tiers;
	if (i >= tiers.size()) return 0;
	return tiers[i];
}

void ct_atp_add_tracker(ct_add_torrent_params* atp, ct_str_view url,
	int32_t tier)
{
	ct::infallible([&] {
		auto& p = unwrap(atp);
		p.trackers.emplace_back(url.ptr, url.len);
		p.tracker_tiers.push_back(tier);
	});
}

void ct_atp_clear_trackers(ct_add_torrent_params* atp)
{
	unwrap(atp).trackers.clear();
	unwrap(atp).tracker_tiers.clear();
}

// ---- DHT nodes -----------------------------------------------------------------------------

size_t ct_atp_num_dht_nodes(const ct_add_torrent_params* atp)
{
	return unwrap(atp).dht_nodes.size();
}

bool ct_atp_dht_node(const ct_add_torrent_params* atp, size_t i,
	ct_str_view* host, int32_t* port)
{
	auto const& nodes = unwrap(atp).dht_nodes;
	if (i >= nodes.size()) return false;
	if (host != nullptr) *host = str_field(nodes[i].first);
	if (port != nullptr) *port = nodes[i].second;
	return true;
}

void ct_atp_add_dht_node(ct_add_torrent_params* atp, ct_str_view host,
	int32_t port)
{
	ct::infallible([&] {
		unwrap(atp).dht_nodes.emplace_back(
			std::string(host.ptr, host.len), port);
	});
}

void ct_atp_clear_dht_nodes(ct_add_torrent_params* atp)
{
	unwrap(atp).dht_nodes.clear();
}

// ---- web seeds -----------------------------------------------------------------------------

size_t ct_atp_num_url_seeds(const ct_add_torrent_params* atp)
{
	return unwrap(atp).url_seeds.size();
}

ct_str_view ct_atp_url_seed(const ct_add_torrent_params* atp, size_t i)
{
	auto const& seeds = unwrap(atp).url_seeds;
	if (i >= seeds.size()) return ct_str_view{nullptr, 0};
	return str_field(seeds[i]);
}

void ct_atp_add_url_seed(ct_add_torrent_params* atp, ct_str_view url)
{
	ct::infallible([&] {
		unwrap(atp).url_seeds.emplace_back(url.ptr, url.len);
	});
}

void ct_atp_clear_url_seeds(ct_add_torrent_params* atp)
{
	unwrap(atp).url_seeds.clear();
}

// ---- storage mode / flags / info-hash --------------------------------------------------------

int32_t ct_atp_storage_mode(const ct_add_torrent_params* atp)
{
	return static_cast<int32_t>(unwrap(atp).storage_mode);
}

void ct_atp_set_storage_mode(ct_add_torrent_params* atp, int32_t mode)
{
	unwrap(atp).storage_mode = mode == CT_STORAGE_MODE_ALLOCATE
		? lt::storage_mode_allocate
		: lt::storage_mode_sparse;
}

uint64_t ct_atp_flags(const ct_add_torrent_params* atp)
{
	return static_cast<uint64_t>(unwrap(atp).flags);
}

void ct_atp_set_flags(ct_add_torrent_params* atp, uint64_t flags)
{
	unwrap(atp).flags = lt::torrent_flags_t(flags);
}

ct_info_hash ct_atp_info_hashes(const ct_add_torrent_params* atp)
{
	return ct::to_ct(unwrap(atp).info_hashes);
}

void ct_atp_set_info_hashes(ct_add_torrent_params* atp,
	const ct_info_hash* value)
{
	lt::info_hash_t h;
	std::memcpy(h.v1.data(), value->v1.data, 20);
	std::memcpy(h.v2.data(), value->v2.data, 32);
	unwrap(atp).info_hashes = h;
}

// ---- scalar fields ----------------------------------------------------------------------------

#define CT_ATP_SCALAR(ctype, name) \
	ctype ct_atp_##name(const ct_add_torrent_params* atp) \
	{ \
		return static_cast<ctype>(unwrap(atp).name); \
	} \
	void ct_atp_set_##name(ct_add_torrent_params* atp, ctype value) \
	{ \
		unwrap(atp).name = value; \
	}

CT_ATP_SCALAR(int32_t, max_uploads)
CT_ATP_SCALAR(int32_t, max_connections)
CT_ATP_SCALAR(int32_t, upload_limit)
CT_ATP_SCALAR(int32_t, download_limit)
CT_ATP_SCALAR(int32_t, num_complete)
CT_ATP_SCALAR(int32_t, num_incomplete)
CT_ATP_SCALAR(int32_t, num_downloaded)
CT_ATP_SCALAR(int64_t, total_uploaded)
CT_ATP_SCALAR(int64_t, total_downloaded)
CT_ATP_SCALAR(int32_t, active_time)
CT_ATP_SCALAR(int32_t, finished_time)
CT_ATP_SCALAR(int32_t, seeding_time)
CT_ATP_SCALAR(int64_t, added_time)
CT_ATP_SCALAR(int64_t, completed_time)
CT_ATP_SCALAR(int64_t, last_seen_complete)
CT_ATP_SCALAR(int64_t, last_download)
CT_ATP_SCALAR(int64_t, last_upload)
CT_ATP_SCALAR(int64_t, creation_date)

#undef CT_ATP_SCALAR

// ---- priorities ----------------------------------------------------------------------------------

ct_span ct_atp_file_priorities(const ct_add_torrent_params* atp)
{
	auto const& prios = unwrap(atp).file_priorities;
	return ct_span{reinterpret_cast<uint8_t const*>(prios.data()),
		prios.size()};
}

void ct_atp_set_file_priorities(ct_add_torrent_params* atp,
	const uint8_t* priorities, size_t len)
{
	ct::infallible([&] {
		auto& prios = unwrap(atp).file_priorities;
		prios.clear();
		prios.reserve(len);
		for (size_t i = 0; i < len; ++i)
			prios.push_back(lt::download_priority_t{priorities[i]});
	});
}

ct_span ct_atp_piece_priorities(const ct_add_torrent_params* atp)
{
	auto const& prios = unwrap(atp).piece_priorities;
	return ct_span{reinterpret_cast<uint8_t const*>(prios.data()),
		prios.size()};
}

void ct_atp_set_piece_priorities(ct_add_torrent_params* atp,
	const uint8_t* priorities, size_t len)
{
	ct::infallible([&] {
		auto& prios = unwrap(atp).piece_priorities;
		prios.clear();
		prios.reserve(len);
		for (size_t i = 0; i < len; ++i)
			prios.push_back(lt::download_priority_t{priorities[i]});
	});
}

// ---- piece state (resume data) --------------------------------------------------------------------

ct_bitfield_view ct_atp_have_pieces(const ct_add_torrent_params* atp)
{
	return bitfield_view(unwrap(atp).have_pieces);
}

void ct_atp_set_have_pieces(ct_add_torrent_params* atp, const uint8_t* bytes,
	int32_t num_bits)
{
	ct::infallible([&] {
		unwrap(atp).have_pieces = make_bitfield(bytes, num_bits);
	});
}

ct_bitfield_view ct_atp_verified_pieces(const ct_add_torrent_params* atp)
{
	return bitfield_view(unwrap(atp).verified_pieces);
}

void ct_atp_set_verified_pieces(ct_add_torrent_params* atp,
	const uint8_t* bytes, int32_t num_bits)
{
	ct::infallible([&] {
		unwrap(atp).verified_pieces = make_bitfield(bytes, num_bits);
	});
}

size_t ct_atp_num_unfinished_pieces(const ct_add_torrent_params* atp)
{
	return unwrap(atp).unfinished_pieces.size();
}

bool ct_atp_unfinished_piece(const ct_add_torrent_params* atp, size_t i,
	ct_piece_index* piece, ct_bitfield_view* blocks)
{
	auto const& map = unwrap(atp).unfinished_pieces;
	if (i >= map.size()) return false;
	// std::next over a std::map is O(i), so iterating all entries through
	// this accessor is O(n^2); fine for realistic resume data (a handful of
	// partially downloaded pieces).
	auto const it = std::next(map.begin(),
		static_cast<std::ptrdiff_t>(i));
	if (piece != nullptr) *piece = static_cast<int32_t>(it->first);
	if (blocks != nullptr) *blocks = bitfield_view(it->second);
	return true;
}

void ct_atp_add_unfinished_piece(ct_add_torrent_params* atp,
	ct_piece_index piece, const uint8_t* bytes, int32_t num_bits)
{
	ct::infallible([&] {
		unwrap(atp).unfinished_pieces[lt::piece_index_t{piece}]
			= make_bitfield(bytes, num_bits);
	});
}

void ct_atp_clear_unfinished_pieces(ct_add_torrent_params* atp)
{
	unwrap(atp).unfinished_pieces.clear();
}

// ---- v2 merkle trees ---------------------------------------------------------------------------------

size_t ct_atp_num_merkle_trees(const ct_add_torrent_params* atp)
{
	return unwrap(atp).merkle_trees.size();
}

ct_span ct_atp_merkle_tree(const ct_add_torrent_params* atp,
	ct_file_index file)
{
	auto const& trees = unwrap(atp).merkle_trees;
	if (file < 0 || lt::file_index_t{file} >= trees.end_index())
		return ct_span{nullptr, 0};
	auto const& tree = trees[lt::file_index_t{file}];
	return ct_span{reinterpret_cast<uint8_t const*>(tree.data()),
		tree.size() * 32};
}

ct_bitfield_view ct_atp_merkle_tree_mask(const ct_add_torrent_params* atp,
	ct_file_index file)
{
	auto const& masks = unwrap(atp).merkle_tree_mask;
	if (file < 0 || lt::file_index_t{file} >= masks.end_index())
		return ct_bitfield_view{nullptr, 0};
	return bitfield_view(masks[lt::file_index_t{file}]);
}

ct_bitfield_view ct_atp_verified_leaf_hashes(const ct_add_torrent_params* atp,
	ct_file_index file)
{
	auto const& verified = unwrap(atp).verified_leaf_hashes;
	if (file < 0 || lt::file_index_t{file} >= verified.end_index())
		return ct_bitfield_view{nullptr, 0};
	return bitfield_view(verified[lt::file_index_t{file}]);
}

void ct_atp_set_num_merkle_trees(ct_add_torrent_params* atp, size_t num)
{
	// Terminating on allocation failure also rules out the partial state a
	// mid-sequence throw would otherwise leave (mismatched vector lengths).
	ct::infallible([&] {
		auto& p = unwrap(atp);
		p.merkle_trees.resize(num);
		p.merkle_tree_mask.resize(num);
		p.verified_leaf_hashes.resize(num);
	});
}

void ct_atp_set_merkle_tree(ct_add_torrent_params* atp, ct_file_index file,
	ct_span hashes)
{
	auto& trees = unwrap(atp).merkle_trees;
	if (file < 0 || lt::file_index_t{file} >= trees.end_index()) return;
	ct::infallible([&] {
		std::vector<lt::sha256_hash> tree(hashes.len / 32);
		if (!tree.empty())
			std::memcpy(tree.data(), hashes.ptr, tree.size() * 32);
		trees[lt::file_index_t{file}] = std::move(tree);
	});
}

void ct_atp_set_merkle_tree_mask(ct_add_torrent_params* atp,
	ct_file_index file, const uint8_t* bytes, int32_t num_bits)
{
	auto& masks = unwrap(atp).merkle_tree_mask;
	if (file < 0 || lt::file_index_t{file} >= masks.end_index()) return;
	ct::infallible([&] {
		masks[lt::file_index_t{file}] = make_bitfield(bytes, num_bits);
	});
}

void ct_atp_set_verified_leaf_hashes(ct_add_torrent_params* atp,
	ct_file_index file, const uint8_t* bytes, int32_t num_bits)
{
	auto& verified = unwrap(atp).verified_leaf_hashes;
	if (file < 0 || lt::file_index_t{file} >= verified.end_index()) return;
	ct::infallible([&] {
		verified[lt::file_index_t{file}] = make_bitfield(bytes, num_bits);
	});
}

// ---- renamed files --------------------------------------------------------------------------------------

size_t ct_atp_num_renamed_files(const ct_add_torrent_params* atp)
{
	return unwrap(atp).renamed_files.size();
}

bool ct_atp_renamed_file(const ct_add_torrent_params* atp, size_t i,
	ct_file_index* file, ct_str_view* name)
{
	auto const& map = unwrap(atp).renamed_files;
	if (i >= map.size()) return false;
	// std::next over a std::map is O(i); see ct_atp_unfinished_piece.
	auto const it = std::next(map.begin(),
		static_cast<std::ptrdiff_t>(i));
	if (file != nullptr) *file = static_cast<int32_t>(it->first);
	if (name != nullptr) *name = str_field(it->second);
	return true;
}

void ct_atp_add_renamed_file(ct_add_torrent_params* atp, ct_file_index file,
	ct_str_view name)
{
	auto& params = unwrap(atp);
	// A negative index would round-trip through write_resume_data as a
	// huge size_t and index past the mapped-files list.
	if (file < 0) return;
	if (params.ti && file >= params.ti->num_files()) return;
	ct::infallible([&] {
		params.renamed_files[lt::file_index_t{file}]
			= std::string(name.ptr, name.len);
	});
}

void ct_atp_clear_renamed_files(ct_add_torrent_params* atp)
{
	unwrap(atp).renamed_files.clear();
}

// ---- peers -------------------------------------------------------------------------------------------------

size_t ct_atp_num_peers(const ct_add_torrent_params* atp)
{
	return unwrap(atp).peers.size();
}

ct_endpoint ct_atp_peer(const ct_add_torrent_params* atp, size_t i)
{
	auto const& peers = unwrap(atp).peers;
	if (i >= peers.size()) return ct_endpoint{};
	return ct::to_ct(peers[i]);
}

void ct_atp_add_peer(ct_add_torrent_params* atp, const ct_endpoint* peer)
{
	ct::infallible([&] {
		unwrap(atp).peers.push_back(ct::to_lt_tcp(*peer));
	});
}

void ct_atp_clear_peers(ct_add_torrent_params* atp)
{
	unwrap(atp).peers.clear();
}

size_t ct_atp_num_banned_peers(const ct_add_torrent_params* atp)
{
	return unwrap(atp).banned_peers.size();
}

ct_endpoint ct_atp_banned_peer(const ct_add_torrent_params* atp, size_t i)
{
	auto const& peers = unwrap(atp).banned_peers;
	if (i >= peers.size()) return ct_endpoint{};
	return ct::to_ct(peers[i]);
}

void ct_atp_add_banned_peer(ct_add_torrent_params* atp,
	const ct_endpoint* peer)
{
	ct::infallible([&] {
		unwrap(atp).banned_peers.push_back(ct::to_lt_tcp(*peer));
	});
}

void ct_atp_clear_banned_peers(ct_add_torrent_params* atp)
{
	unwrap(atp).banned_peers.clear();
}

} // extern "C"
