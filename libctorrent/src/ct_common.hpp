// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

// Boundary helpers shared by every section: the exception guard, error
// translation, and string/buffer boxing.
#pragma once

#include <ctorrent/ct_types.h>

#include <libtorrent/address.hpp>
#include <libtorrent/error_code.hpp>
#include <libtorrent/info_hash.hpp>
#include <libtorrent/peer_request.hpp>
#include <libtorrent/sha1_hash.hpp>
#include <libtorrent/socket.hpp>
#include <libtorrent/span.hpp>
#include <libtorrent/string_view.hpp>

#include <cstdint>
#include <cstring>
#include <string>
#include <type_traits>
#include <utility>
#include <vector>

namespace ct {

namespace lt = libtorrent;

// Implemented in error.cpp.
void set_error(ct_error* err, lt::error_code const& ec) noexcept;
void set_error_current_exception(ct_error* err) noexcept;

inline void clear_error(ct_error* err) noexcept
{
	if (err != nullptr) *err = ct_error{0, CT_ERROR_CAT_NONE, nullptr};
}

// Runs f() with the boundary exception guard: clears *err first, converts
// any escaping exception into *err, and returns a default-constructed value
// on failure. Every extern "C" function body with failure modes goes through
// this; exceptions must never cross the C boundary.
template <typename F>
auto guard(ct_error* err, F&& f) noexcept -> decltype(f())
{
	clear_error(err);
	try {
		return std::forward<F>(f)();
	} catch (...) {
		set_error_current_exception(err);
		if constexpr (!std::is_void_v<decltype(f())>) return decltype(f()){};
	}
}

// Runs f() for nominally infallible mutators whose only failure mode is
// allocation (bad_alloc / length_error). Mirroring Rust's abort-on-OOM
// doctrine, an escaping exception terminates the process (via noexcept)
// instead of being swallowed into a silent no-op or partial mutation.
template <typename F>
auto infallible(F&& f) noexcept -> decltype(f())
{
	return std::forward<F>(f)();
}

// Moves a std::string onto the heap and exposes its storage. The zero-copy
// counterpart of returning std::string by value.
ct_str box_string(std::string s) noexcept;

// As above for buffers, but throws bad_alloc if the box cannot be
// allocated; callers run under guard() and must report the failure rather
// than replace valid state with an empty buffer.
ct_buf box_buffer(std::vector<char> v);

inline ct_str_view view(lt::string_view sv) noexcept
{
	return ct_str_view{sv.data(), sv.size()};
}

inline lt::string_view view(ct_str_view sv) noexcept
{
	return lt::string_view{sv.ptr, sv.len};
}

inline lt::span<char const> span(ct_span s) noexcept
{
	return lt::span<char const>{
		reinterpret_cast<char const*>(s.ptr), static_cast<std::ptrdiff_t>(s.len)};
}

// -- POD conversions (lt -> ct) ---------------------------------------------

inline ct_endpoint to_ct(lt::address const& addr, std::uint16_t port) noexcept
{
	ct_endpoint out{};
	if (addr.is_v6()) {
		auto const v6 = addr.to_v6();
		auto const bytes = v6.to_bytes();
		std::memcpy(out.addr, bytes.data(), 16);
		out.scope_id = static_cast<std::uint32_t>(v6.scope_id());
		out.is_v6 = 1;
	} else {
		auto const bytes = addr.to_v4().to_bytes();
		std::memcpy(out.addr, bytes.data(), 4);
		out.is_v6 = 0;
	}
	out.port = port;
	return out;
}

inline ct_endpoint to_ct(lt::tcp::endpoint const& ep) noexcept
{
	return to_ct(ep.address(), ep.port());
}

inline ct_endpoint to_ct(lt::udp::endpoint const& ep) noexcept
{
	return to_ct(ep.address(), ep.port());
}

inline ct_sha1 to_ct(lt::sha1_hash const& h) noexcept
{
	ct_sha1 out;
	std::memcpy(out.data, h.data(), sizeof(out.data));
	return out;
}

inline ct_sha256 to_ct(lt::sha256_hash const& h) noexcept
{
	ct_sha256 out;
	std::memcpy(out.data, h.data(), sizeof(out.data));
	return out;
}

inline ct_info_hash to_ct(lt::info_hash_t const& h) noexcept
{
	ct_info_hash out;
	out.v1 = to_ct(h.v1);
	out.v2 = to_ct(h.v2);
	return out;
}

inline ct_peer_request to_ct(lt::peer_request const& r) noexcept
{
	return ct_peer_request{static_cast<int32_t>(r.piece), r.start, r.length};
}

// -- POD conversions (ct -> lt) ---------------------------------------------

inline lt::address to_lt_address(ct_endpoint const& ep)
{
	if (ep.is_v6 != 0) {
		lt::address_v6::bytes_type bytes;
		std::memcpy(bytes.data(), ep.addr, 16);
		return lt::address_v6(bytes, ep.scope_id);
	}
	lt::address_v4::bytes_type bytes;
	std::memcpy(bytes.data(), ep.addr, 4);
	return lt::address_v4(bytes);
}

inline lt::tcp::endpoint to_lt_tcp(ct_endpoint const& ep)
{
	return lt::tcp::endpoint(to_lt_address(ep), ep.port);
}

inline lt::udp::endpoint to_lt_udp(ct_endpoint const& ep)
{
	return lt::udp::endpoint(to_lt_address(ep), ep.port);
}

// Helper for string_view returns (borrowed strings)
inline ct_str_view to_str_view(std::string const& s) noexcept
{
	return ct_str_view{s.data(), s.size()};
}

} // namespace ct
