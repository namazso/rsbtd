// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

#include <ctorrent/ct_peer_class.h>

#include "ct_common.hpp"

#include <libtorrent/peer_class.hpp>
#include <libtorrent/peer_class_type_filter.hpp>

#include <cstring>
#include <new>
#include <stdexcept>

namespace {

lt::peer_class_type_filter* unwrap(ct_peer_class_type_filter* f)
{
	return reinterpret_cast<lt::peer_class_type_filter*>(f);
}

lt::peer_class_type_filter const* unwrap(ct_peer_class_type_filter const* f)
{
	return reinterpret_cast<lt::peer_class_type_filter const*>(f);
}

lt::peer_class_type_filter::socket_type_t to_lt_socket_type(ct_socket_type st)
{
	// libtorrent only asserts the socket-type domain; release builds
	// silently no-op while this API would report success.
	if (static_cast<int>(st)
		>= static_cast<int>(lt::peer_class_type_filter::num_socket_types)
		|| static_cast<int>(st) < 0)
	{
		throw std::invalid_argument("invalid socket type");
	}
	return static_cast<lt::peer_class_type_filter::socket_type_t>(st);
}

void check_peer_class(ct_peer_class_t peer_class)
{
	// The filter's bitmasks are 32 bits; libtorrent only asserts this.
	if (peer_class > 31)
		throw std::invalid_argument(
			"peer class id above the filter maximum of 31");
}

} // namespace

extern "C" {

ct_peer_class_type_filter* ct_peer_class_type_filter_new(ct_error* err)
{
	return ct::guard(err, []() -> ct_peer_class_type_filter* {
		return reinterpret_cast<ct_peer_class_type_filter*>(
			new lt::peer_class_type_filter());
	});
}

void ct_peer_class_type_filter_free(ct_peer_class_type_filter* filter)
{
	delete unwrap(filter);
}

void ct_peer_class_type_filter_add(ct_peer_class_type_filter* filter,
	ct_socket_type st, ct_peer_class_t peer_class, ct_error* err)
{
	ct::guard(err, [&] {
		check_peer_class(peer_class);
		unwrap(filter)->add(to_lt_socket_type(st), lt::peer_class_t{peer_class});
	});
}

void ct_peer_class_type_filter_remove(ct_peer_class_type_filter* filter,
	ct_socket_type st, ct_peer_class_t peer_class, ct_error* err)
{
	ct::guard(err, [&] {
		check_peer_class(peer_class);
		unwrap(filter)->remove(to_lt_socket_type(st), lt::peer_class_t{peer_class});
	});
}

void ct_peer_class_type_filter_disallow(ct_peer_class_type_filter* filter,
	ct_socket_type st, ct_peer_class_t peer_class, ct_error* err)
{
	ct::guard(err, [&] {
		check_peer_class(peer_class);
		unwrap(filter)->disallow(to_lt_socket_type(st), lt::peer_class_t{peer_class});
	});
}

void ct_peer_class_type_filter_allow(ct_peer_class_type_filter* filter,
	ct_socket_type st, ct_peer_class_t peer_class, ct_error* err)
{
	ct::guard(err, [&] {
		check_peer_class(peer_class);
		unwrap(filter)->allow(to_lt_socket_type(st), lt::peer_class_t{peer_class});
	});
}

uint32_t ct_peer_class_type_filter_apply(const ct_peer_class_type_filter* filter,
	ct_socket_type st, uint32_t peer_class_mask, ct_error* err)
{
	return ct::guard(err, [&] {
		// apply() is (anomalously) non-const upstream.
		return const_cast<lt::peer_class_type_filter*>(unwrap(filter))->apply(
			to_lt_socket_type(st), peer_class_mask);
	});
}

} // extern "C"
