// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

#include <ctorrent/ct_create_torrent.h>

#include "ct_common.hpp"

#include <libtorrent/create_torrent.hpp>
#include <libtorrent/disk_interface.hpp>
#include <libtorrent/io_context.hpp>
#include <libtorrent/session.hpp>
#include <libtorrent/session_params.hpp>
#include <libtorrent/settings_pack.hpp>
#include <libtorrent/torrent_info.hpp>

#include <boost/system/error_code.hpp>

#include <cstdint>
#include <limits>
#include <memory>
#include <stdexcept>
#include <string>
#include <string_view>
#include <unordered_map>

// rbtorrent's CreateTorrent::MAX_PIECE_COUNT mirrors this value.
static_assert(lt::file_storage::max_num_pieces == (std::int32_t(1) << 30) - 1,
    "lt::file_storage::max_num_pieces changed; update CreateTorrent::MAX_PIECE_COUNT");

// The opaque builder handle. lt::create_torrent canonicalizes the file
// list inside its constructor for v2/hybrid (and canonical_files) modes:
// entries are sorted by path and pad entries are interleaved. The C API's
// file indices stay in the caller's original entry order, so the handle
// carries the translation into the canonicalized file_list() order.
struct ct_create_torrent {
	lt::create_torrent ct;
	// original entry index -> canonical file_list() index; -1 for
	// caller-supplied pad entries (dropped by canonicalization, and
	// never a valid hash target).
	std::vector<std::int32_t> file_map;

	ct_create_torrent(std::vector<lt::create_file_entry> files,
	    std::int32_t piece_size, lt::create_flags_t flags)
	    : ct(std::move(files), piece_size, flags)
	{
	}
};

namespace {

// The entries returned by ct_list_files hold borrowed string pointers; the
// strings live in this box, stored (as a pointer) in one hidden extra entry
// at the end of the array and recovered by ct_create_file_list_free.
struct file_list_box {
	std::vector<std::string> paths;
	std::vector<std::string> symlinks;
};

} // anonymous namespace

extern "C" {

ct_create_torrent* ct_create_torrent_new(
    const ct_create_file_entry* files,
    size_t file_count,
    int32_t piece_size,
    uint32_t flags,
    ct_error* err)
{
    return ct::guard(err, [&]() -> ct_create_torrent* {
        // Validate independently of the Rust layer: out-of-domain sizes reach
        // TORRENT_ASSERTs inside lt::create_torrent, which abort instead of
        // throwing, so the guard could not contain them.
        if (piece_size < 0 || piece_size > 128 * 1024 * 1024) {
            throw std::invalid_argument(
                "create_torrent: piece_size " + std::to_string(piece_size)
                + " out of range (0 = auto, max 128 MiB)");
        }
        if ((flags & CT_CREATE_V1_ONLY) && (flags & CT_CREATE_V2_ONLY)) {
            throw std::invalid_argument(
                "create_torrent: v1_only and v2_only are mutually exclusive");
        }

        std::vector<lt::create_file_entry> lt_files;
        lt_files.reserve(file_count);

        std::int64_t total_size = 0;
        for (size_t i = 0; i < file_count; ++i) {
            const auto& f = files[i];
            if (f.path == nullptr) {
                throw std::invalid_argument(
                    "create_torrent: file entry " + std::to_string(i) + " has a null path");
            }
            if (f.size < 0) {
                throw std::invalid_argument(
                    "create_torrent: file entry " + std::to_string(i) + " has a negative size");
            }
            if (f.size > lt::file_storage::max_file_size
                || total_size > lt::file_storage::max_file_offset - f.size) {
                throw std::invalid_argument(
                    "create_torrent: file sizes exceed libtorrent's supported total");
            }
            total_size += f.size;
            lt::file_flags_t lt_flags{};
            if (f.flags & CT_FILE_FLAG_PAD_FILE) lt_flags |= lt::file_storage::flag_pad_file;
            if (f.flags & CT_FILE_FLAG_HIDDEN) lt_flags |= lt::file_storage::flag_hidden;
            if (f.flags & CT_FILE_FLAG_EXECUTABLE) lt_flags |= lt::file_storage::flag_executable;
            if (f.flags & CT_FILE_FLAG_SYMLINK) lt_flags |= lt::file_storage::flag_symlink;

            std::string symlink = (f.symlink_target != nullptr) ? f.symlink_target : "";
            lt_files.emplace_back(f.path, f.size, lt_flags, f.mtime, std::move(symlink));
        }

        // lt::create_torrent stores the piece count as int and only asserts
        // the numeric_cast; canonicalization pads each file to a piece
        // boundary, adding at most one piece per file to the count.
        if (piece_size > 0) {
            const std::int64_t partial = (total_size % piece_size) != 0 ? 1 : 0;
            // Subtract instead of adding to the quotient: with a tiny
            // piece_size the sum could overflow int64.
            const std::int64_t budget =
                std::int64_t(lt::file_storage::max_num_pieces)
                - partial - static_cast<std::int64_t>(file_count);
            if (total_size / piece_size > budget) {
                throw std::invalid_argument(
                    "create_torrent: total size / piece_size exceeds the "
                    "maximum piece count");
            }
        }

        lt::create_flags_t lt_flags{};
        if (flags & CT_CREATE_MODIFICATION_TIME) lt_flags |= lt::create_torrent::modification_time;
        if (flags & CT_CREATE_SYMLINKS) lt_flags |= lt::create_torrent::symlinks;
        if (flags & CT_CREATE_V2_ONLY) lt_flags |= lt::create_torrent::v2_only;
        if (flags & CT_CREATE_V1_ONLY) lt_flags |= lt::create_torrent::v1_only;
        if (flags & CT_CREATE_CANONICAL_FILES) lt_flags |= lt::create_torrent::canonical_files;
        if (flags & CT_CREATE_NO_ATTRIBUTES) lt_flags |= lt::create_torrent::no_attributes;
        if (flags & CT_CREATE_CANONICAL_FILES_NO_TAIL_PADDING)
            lt_flags |= lt::create_torrent::canonical_files_no_tail_padding;

        auto handle =
            std::make_unique<ct_create_torrent>(std::move(lt_files), piece_size, lt_flags);

        // Map each original entry to its canonicalized position by path
        // (create_file_entry stores paths verbatim, so the caller's
        // strings compare equal to the stored filenames). Duplicate
        // paths would make both the mapping and the torrent ambiguous.
        auto const& canon = handle->ct.file_list();
        std::unordered_map<std::string_view, std::int32_t> by_path;
        by_path.reserve(canon.size());
        for (std::int32_t i = 0; i < std::int32_t(canon.size()); ++i) {
            const auto& entry = canon[lt::file_index_t{i}];
            if (entry.flags & lt::file_storage::flag_pad_file) continue;
            if (!by_path.emplace(entry.filename, i).second) {
                throw std::invalid_argument(
                    "create_torrent: duplicate path " + entry.filename);
            }
        }
        handle->file_map.reserve(file_count);
        for (size_t i = 0; i < file_count; ++i) {
            const auto it = by_path.find(files[i].path);
            handle->file_map.push_back(it != by_path.end() ? it->second : -1);
        }

        return handle.release();
    });
}

void ct_create_torrent_free(ct_create_torrent* ct)
{
    delete ct;
}

ct_buf ct_create_torrent_generate_buf(const ct_create_torrent* ct, ct_error* err)
{
    return ct::guard(err, [&]() -> ct_buf {
        const auto* lt_ct = &ct->ct;
        std::vector<char> buf = lt_ct->generate_buf();
        return ct::box_buffer(std::move(buf));
    });
}

void ct_create_torrent_set_comment(ct_create_torrent* ct, const char* comment, ct_error* err)
{
    ct::guard(err, [&]() {
        auto* lt_ct = &ct->ct;
        lt_ct->set_comment(comment);
    });
}

void ct_create_torrent_set_creator(ct_create_torrent* ct, const char* creator, ct_error* err)
{
    ct::guard(err, [&]() {
        auto* lt_ct = &ct->ct;
        lt_ct->set_creator(creator);
    });
}

void ct_create_torrent_set_creation_date(ct_create_torrent* ct, time_t timestamp, ct_error* err)
{
    ct::guard(err, [&]() {
        auto* lt_ct = &ct->ct;
        lt_ct->set_creation_date(timestamp);
    });
}

void ct_create_torrent_set_hash(ct_create_torrent* ct, int32_t index, const uint8_t hash[20], ct_error* err)
{
    ct::guard(err, [&]() {
        auto* lt_ct = &ct->ct;
        // lt::create_torrent::set_hash only asserts the index (compiled out of
        // release builds); an out-of-range index would be a heap OOB write.
        if (index < 0 || index >= lt_ct->num_pieces()) {
            throw std::out_of_range(
                "set_hash: piece index " + std::to_string(index)
                + " out of range (num_pieces = " + std::to_string(lt_ct->num_pieces()) + ")");
        }
        lt::sha1_hash h;
        std::memcpy(h.data(), hash, 20);
        lt_ct->set_hash(lt::piece_index_t{index}, h);
    });
}

void ct_create_torrent_set_hash2(ct_create_torrent* ct, int32_t file_index, int32_t piece, const uint8_t hash[32], ct_error* err)
{
    ct::guard(err, [&]() {
        auto* lt_ct = &ct->ct;
        // lt::create_torrent::set_hash2 only asserts its preconditions
        // (compiled out of release builds); out-of-range indices would be heap
        // OOB reads/writes and a pad file has no hashable pieces.
        //
        // file_index is in the caller's original entry order; translate it
        // into the canonicalized file_list() order the builder uses.
        if (file_index < 0 || std::size_t(file_index) >= ct->file_map.size()) {
            throw std::out_of_range(
                "set_hash2: file index " + std::to_string(file_index)
                + " out of range (num_files = " + std::to_string(ct->file_map.size()) + ")");
        }
        std::int32_t const canon_index = ct->file_map[std::size_t(file_index)];
        if (canon_index < 0) {
            throw std::invalid_argument(
                "set_hash2: file index " + std::to_string(file_index) + " is a pad file");
        }
        auto const& f = lt_ct->file_list()[lt::file_index_t{canon_index}];
        std::int64_t const piece_length = lt_ct->piece_length();
        std::int64_t const file_pieces = (f.size + piece_length - 1) / piece_length;
        if (piece < 0 || piece >= file_pieces) {
            throw std::out_of_range(
                "set_hash2: piece " + std::to_string(piece) + " out of range for file "
                + std::to_string(file_index) + " (file pieces = " + std::to_string(file_pieces) + ")");
        }
        lt::sha256_hash h;
        std::memcpy(h.data(), hash, 32);
        // lt::create_torrent::set_hash2 also asserts the hash is not all
        // zeros (an impossible SHA-256 output, reserved as "unset").
        if (h.is_all_zeros()) {
            throw std::invalid_argument(
                "set_hash2: an all-zero hash is not a valid piece hash");
        }
        lt_ct->set_hash2(lt::file_index_t{canon_index}, lt::piece_index_t::diff_type{piece}, h);
    });
}

void ct_create_torrent_add_url_seed(ct_create_torrent* ct, const char* url, ct_error* err)
{
    ct::guard(err, [&]() {
        auto* lt_ct = &ct->ct;
        lt_ct->add_url_seed(url);
    });
}

void ct_create_torrent_add_tracker(ct_create_torrent* ct, const char* url, int32_t tier, ct_error* err)
{
    ct::guard(err, [&]() {
        auto* lt_ct = &ct->ct;
        lt_ct->add_tracker(url, tier);
    });
}

void ct_create_torrent_add_node(ct_create_torrent* ct, const char* hostname, int32_t port, ct_error* err)
{
    ct::guard(err, [&]() {
        auto* lt_ct = &ct->ct;
        lt_ct->add_node({hostname, port});
    });
}

void ct_create_torrent_set_root_cert(ct_create_torrent* ct, const char* pem, ct_error* err)
{
    ct::guard(err, [&]() {
        auto* lt_ct = &ct->ct;
        lt_ct->set_root_cert(pem);
    });
}

void ct_create_torrent_set_priv(ct_create_torrent* ct, bool is_private)
{
    auto* lt_ct = &ct->ct;
    lt_ct->set_priv(is_private);
}

bool ct_create_torrent_priv(const ct_create_torrent* ct)
{
    const auto* lt_ct = &ct->ct;
    return lt_ct->priv();
}

void ct_create_torrent_add_similar_torrent(ct_create_torrent* ct, const uint8_t info_hash[20], ct_error* err)
{
    ct::guard(err, [&]() {
        auto* lt_ct = &ct->ct;
        lt::sha1_hash ih;
        std::memcpy(ih.data(), info_hash, 20);
        lt_ct->add_similar_torrent(ih);
    });
}

void ct_create_torrent_add_collection(ct_create_torrent* ct, const char* name, ct_error* err)
{
    ct::guard(err, [&]() {
        auto* lt_ct = &ct->ct;
        lt_ct->add_collection(name);
    });
}

bool ct_create_torrent_is_v2_only(const ct_create_torrent* ct)
{
    const auto* lt_ct = &ct->ct;
    return lt_ct->is_v2_only();
}

bool ct_create_torrent_is_v1_only(const ct_create_torrent* ct)
{
    const auto* lt_ct = &ct->ct;
    return lt_ct->is_v1_only();
}

int32_t ct_create_torrent_num_pieces(const ct_create_torrent* ct)
{
    const auto* lt_ct = &ct->ct;
    return lt_ct->num_pieces();
}

int32_t ct_create_torrent_piece_length(const ct_create_torrent* ct)
{
    const auto* lt_ct = &ct->ct;
    return lt_ct->piece_length();
}

int32_t ct_create_torrent_piece_size(const ct_create_torrent* ct, int32_t index)
{
    const auto* lt_ct = &ct->ct;
    return lt_ct->piece_size(lt::piece_index_t{index});
}

int64_t ct_create_torrent_total_size(const ct_create_torrent* ct)
{
    const auto* lt_ct = &ct->ct;
    return lt_ct->total_size();
}

ct_create_file_entry* ct_list_files(const char* path, uint32_t flags, size_t* out_count, ct_error* err)
{
    // Zeroed up front so a caller that ignores *err never reads garbage.
    *out_count = 0;
    return ct::guard(err, [&]() -> ct_create_file_entry* {
        lt::create_flags_t lt_flags{};
        if (flags & CT_CREATE_MODIFICATION_TIME) lt_flags |= lt::create_torrent::modification_time;
        if (flags & CT_CREATE_SYMLINKS) lt_flags |= lt::create_torrent::symlinks;
        if (flags & CT_CREATE_NO_ATTRIBUTES) lt_flags |= lt::create_torrent::no_attributes;

        std::vector<lt::create_file_entry> lt_files = lt::list_files(path, lt_flags);

        if (lt_files.empty()) {
            *out_count = 0;
            return nullptr;
        }

        // The entries hold borrowed string pointers; see file_list_box.
        auto box = std::make_unique<file_list_box>();
        box->paths.reserve(lt_files.size());
        box->symlinks.reserve(lt_files.size());

        auto entries = std::make_unique<ct_create_file_entry[]>(lt_files.size() + 1);

        for (size_t i = 0; i < lt_files.size(); ++i) {
            const auto& f = lt_files[i];

            box->paths.push_back(f.filename);
            entries[i].path = box->paths.back().c_str();
            entries[i].size = f.size;

            uint32_t ct_flags = 0;
            if (f.flags & lt::file_storage::flag_pad_file) ct_flags |= CT_FILE_FLAG_PAD_FILE;
            if (f.flags & lt::file_storage::flag_hidden) ct_flags |= CT_FILE_FLAG_HIDDEN;
            if (f.flags & lt::file_storage::flag_executable) ct_flags |= CT_FILE_FLAG_EXECUTABLE;
            if (f.flags & lt::file_storage::flag_symlink) ct_flags |= CT_FILE_FLAG_SYMLINK;
            entries[i].flags = ct_flags;

            entries[i].mtime = f.mtime;

            if (!f.symlink.empty()) {
                box->symlinks.push_back(f.symlink);
                entries[i].symlink_target = box->symlinks.back().c_str();
            } else {
                entries[i].symlink_target = nullptr;
            }
        }

        auto* box_ptr = box.release();
        std::memcpy(&entries[lt_files.size()], &box_ptr, sizeof(void*));

        *out_count = lt_files.size();
        return entries.release();
    });
}

void ct_create_file_list_free(ct_create_file_entry* list, size_t count)
{
    if (list == nullptr) return;

    // Retrieve box pointer from hidden entry
    void* box_ptr;
    std::memcpy(&box_ptr, &list[count], sizeof(void*));

    delete static_cast<file_list_box*>(box_ptr);
    delete[] list;
}

void ct_set_piece_hashes(
    ct_create_torrent* ct,
    const char* base_path,
    ct_piece_hash_progress_fn progress,
    void* userdata,
    ct_error* err)
{
    ct::guard(err, [&]() {
        auto* lt_ct = &ct->ct;

        // Cancellation throws out of the progress callback, unwinding
        // lt::set_piece_hashes mid-run. Its locals are declared in the
        // order (disk_aborter, file_storage, hash_state), so unwinding
        // destroys the file_storage before the disk_aborter joins the
        // disk threads -- which are still hashing and reading that
        // file_storage: a use-after-free (debug builds trip the
        // file_index_at_offset assert in file_storage.cpp). Capture the
        // disk_interface so the callback can join the disk threads
        // first; abort(true) from inside a completion handler is the
        // same call libtorrent's own on_hash error path makes.
        lt::disk_interface* disk = nullptr;
        lt::disk_io_constructor_type disk_ctor =
            [&disk](lt::io_context& ios, lt::settings_interface const& sett,
                lt::counters& cnt) {
                auto io = lt::default_disk_io_constructor(ios, sett, cnt);
                disk = io.get();
                return io;
            };

        std::function<void(lt::piece_index_t)> callback;
        if (progress != nullptr) {
            callback = [progress, userdata, &disk](lt::piece_index_t idx) {
                if (!progress(static_cast<int32_t>(idx), userdata)) {
                    if (disk != nullptr) disk->abort(true);
                    throw lt::system_error(lt::error_code(
                        boost::system::errc::operation_canceled,
                        boost::system::generic_category()));
                }
            };
        } else {
            callback = lt::aux::nop;
        }

        lt::error_code ec;
        lt::set_piece_hashes(*lt_ct, base_path, lt::settings_pack{},
            std::move(disk_ctor), callback, ec);
        if (ec) {
            throw lt::system_error(ec);
        }
    });
}

} // extern "C"
