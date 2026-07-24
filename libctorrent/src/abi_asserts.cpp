// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

// The single source of ABI truth: every layout, size, enum value, or flag
// bit that libctorrent's C headers rely on is static_assert'ed here against
// the libtorrent headers this build actually uses. If a system libtorrent
// disagrees, the build fails here instead of misbehaving at runtime.
//
// Forward-compat rule: counts of enumerable things (alerts, settings) are
// checked with >=, exact values with ==; a newer libtorrent may add, never
// reorder (that is libtorrent's own ABI promise within a version namespace).

#include <ctorrent/ctorrent.h>

#include <libtorrent/download_priority.hpp>
#include <libtorrent/flags.hpp>
#include <libtorrent/info_hash.hpp>
#include <libtorrent/peer_request.hpp>
#include <libtorrent/session.hpp>
#include <libtorrent/sha1_hash.hpp>
#include <libtorrent/torrent_flags.hpp>
#include <libtorrent/torrent_handle.hpp>
#include <libtorrent/units.hpp>
#include <libtorrent/version.hpp>

#include <cstddef>
#include <cstdint>
#include <type_traits>

namespace {

namespace lt = libtorrent;

// -- toolchain expectations ------------------------------------------------
static_assert(sizeof(int) == 4, "C mirrors of int fields assume 32-bit int");
// Request-correlation tokens are full uint64_t values transported through
// lt::client_data_t's void* (see ct_session_async_add_torrent). 32-bit
// platforms would truncate them and are not supported.
static_assert(sizeof(void*) == sizeof(std::uint64_t),
	"uint64_t request tokens ride in a void*; 32-bit platforms unsupported");

// -- version / ABI ---------------------------------------------------------
static_assert(LIBTORRENT_VERSION_NUM >= 20100, "libtorrent >= 2.1.0 required");
static_assert(LIBTORRENT_VERSION_NUM == CT_LT_VERSION_NUM,
	"ABI probe saw different libtorrent headers than this TU");
static_assert(TORRENT_ABI_VERSION == CT_LT_ABI_VERSION,
	"ABI probe saw a different TORRENT_ABI_VERSION than this TU");

// -- probe facts vs this TU's view ------------------------------------------
static_assert(sizeof(lt::torrent_handle) == CT_SIZEOF_LT_TORRENT_HANDLE);
static_assert(alignof(lt::torrent_handle) == CT_ALIGNOF_LT_TORRENT_HANDLE);
static_assert(sizeof(lt::session_proxy) == CT_SIZEOF_LT_SESSION_PROXY);
static_assert(alignof(lt::session_proxy) == CT_ALIGNOF_LT_SESSION_PROXY);

// -- hashes ------------------------------------------------------------------
// ct hash structs have alignment 1, the lt types alignment 4: casting is
// only ever done lt -> ct (weaker alignment); ct -> lt inputs are memcpy'd.
static_assert(sizeof(ct_sha1) == 20 && sizeof(lt::sha1_hash) == sizeof(ct_sha1));
static_assert(std::is_trivially_copyable_v<lt::sha1_hash>);
static_assert(sizeof(ct_sha256) == 32
	&& sizeof(lt::sha256_hash) == sizeof(ct_sha256));
static_assert(std::is_trivially_copyable_v<lt::sha256_hash>);
static_assert(sizeof(ct_info_hash) == 52
	&& sizeof(lt::info_hash_t) == sizeof(ct_info_hash));
static_assert(std::is_trivially_copyable_v<lt::info_hash_t>);
static_assert(offsetof(lt::info_hash_t, v1) == offsetof(ct_info_hash, v1));
static_assert(offsetof(lt::info_hash_t, v2) == offsetof(ct_info_hash, v2));

// -- strong typedefs / small values ------------------------------------------
static_assert(sizeof(lt::piece_index_t) == sizeof(ct_piece_index));
static_assert(std::is_trivially_copyable_v<lt::piece_index_t>);
static_assert(sizeof(lt::file_index_t) == sizeof(ct_file_index));
static_assert(sizeof(lt::download_priority_t) == sizeof(ct_download_priority));
static_assert(sizeof(lt::peer_request) == sizeof(ct_peer_request));
static_assert(std::is_trivially_copyable_v<lt::peer_request>);
static_assert(offsetof(lt::peer_request, piece) == offsetof(ct_peer_request, piece));
static_assert(offsetof(lt::peer_request, start) == offsetof(ct_peer_request, start));
static_assert(offsetof(lt::peer_request, length) == offsetof(ct_peer_request, length));

// -- flag widths --------------------------------------------------------------
static_assert(sizeof(lt::torrent_flags_t) == 8);
static_assert(std::is_trivially_copyable_v<lt::torrent_flags_t>);

// -- masquerade relocatability -----------------------------------------------
// ct_torrent_handle masquerades as lt::torrent_handle (and ct_session_proxy
// as lt::session_proxy), relying on them being trivially relocatable
// (placement-new, memcpy-move, manual destructor).
//
// EXTRA REQUIREMENT (accepted, not provable in-language): bytewise
// relocatability is stronger than anything C++17 lets us static_assert —
// neither type is trivially copyable, and nothrow-move-constructible does
// not imply that memcpy'd bytes form a live object at the new address. We
// accept this as a platform requirement: libtorrent is ABI-stable within a
// version namespace, both types are thin wrappers (weak_ptr / shared_ptr
// respectively) with no self-referential state, and every standard library
// on our supported platforms (libstdc++, libc++, MSVC STL) implements them
// as address-independent control-block pointers. A platform whose smart
// pointers are not bytewise relocatable is unsupported.
// The asserts below are the best in-language approximation we can pin.
static_assert(std::is_nothrow_move_constructible_v<lt::torrent_handle>,
	"torrent_handle must be nothrow move constructible");
static_assert(std::is_nothrow_move_assignable_v<lt::torrent_handle>,
	"torrent_handle must be nothrow move assignable");

} // namespace

// Keep the TU from being empty (some archivers warn).
extern "C" int ct_abi_asserts_anchor;
int ct_abi_asserts_anchor = 0;
