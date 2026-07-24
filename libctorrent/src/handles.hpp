// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

// Unwrap/wrap helpers for the heap-opaque handles shared by more than one
// section.
#pragma once

#include <ctorrent/ct_add_torrent_params.h>
#include <ctorrent/ct_torrent_info.h>

#include <libtorrent/add_torrent_params.hpp>
#include <libtorrent/torrent_info.hpp>
#include <libtorrent/torrent_status.hpp>

#include <cstdint>
#include <memory>
#include <string>
#include <utility>
#include <vector>

// A boxed std::vector<std::string>; the accessors live in torrent_info.cpp.
struct ct_str_list {
	std::vector<std::string> items;
};

// An owned status snapshot: the lt object plus the torrent id captured when
// the snapshot was created. torrent_handle::id() reads 0 once the torrent is
// destroyed, so the id must be recorded at creation to keep stale snapshots
// attributable.
struct ct_torrent_status {
	libtorrent::torrent_status st;
	uint32_t id;
};

namespace ct {

namespace lt = libtorrent;

// ct_torrent_info boxes a shared_ptr to an immutable torrent_info. The
// element type is const so the box converts directly at TORRENT_ABI_VERSION
// >= 4 (where add_torrent_params::ti is shared_ptr<torrent_info const>);
// the ABI < 4 setter const_pointer_casts, which is safe because nothing
// mutates through add_torrent_params::ti.
using ti_ptr = std::shared_ptr<lt::torrent_info const>;

inline ti_ptr const& unwrap(const ct_torrent_info* ti) noexcept
{
	return *reinterpret_cast<ti_ptr const*>(ti);
}

inline ct_torrent_info* wrap(ti_ptr p)
{
	return reinterpret_cast<ct_torrent_info*>(new ti_ptr(std::move(p)));
}

inline lt::add_torrent_params& unwrap(ct_add_torrent_params* atp) noexcept
{
	return *reinterpret_cast<lt::add_torrent_params*>(atp);
}

inline lt::add_torrent_params const& unwrap(
	const ct_add_torrent_params* atp) noexcept
{
	return *reinterpret_cast<lt::add_torrent_params const*>(atp);
}

inline ct_add_torrent_params* wrap(lt::add_torrent_params* atp) noexcept
{
	return reinterpret_cast<ct_add_torrent_params*>(atp);
}

// Forward declarations for shared helpers (defined in add_torrent_params.cpp).
lt::load_torrent_limits to_lt_load_limits(const ct_load_torrent_limits* limits);
ct_add_torrent_params* box_atp(lt::add_torrent_params atp);

} // namespace ct
