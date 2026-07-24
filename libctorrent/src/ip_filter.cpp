// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

#include <ctorrent/ct_ip_filter.h>

#include "ct_common.hpp"

#include <libtorrent/ip_filter.hpp>
#include <libtorrent/address.hpp>

#include <new>
#include <cstring>

namespace {

lt::ip_filter* unwrap(ct_ip_filter* f)
{
	return reinterpret_cast<lt::ip_filter*>(f);
}

lt::ip_filter const* unwrap(ct_ip_filter const* f)
{
	return reinterpret_cast<lt::ip_filter const*>(f);
}

lt::port_filter* unwrap(ct_port_filter* f)
{
	return reinterpret_cast<lt::port_filter*>(f);
}

lt::port_filter const* unwrap(ct_port_filter const* f)
{
	return reinterpret_cast<lt::port_filter const*>(f);
}

lt::address to_lt_address(ct_address const* addr)
{
	if (addr->is_v6) {
		lt::address_v6::bytes_type bytes;
		std::memcpy(bytes.data(), addr->bytes, 16);
		return lt::address_v6(bytes);
	} else {
		lt::address_v4::bytes_type bytes;
		std::memcpy(bytes.data(), addr->bytes, 4);
		return lt::address_v4(bytes);
	}
}

void from_lt_address(lt::address const& addr, ct_address* out)
{
	std::memset(out, 0, sizeof(ct_address));
	if (addr.is_v6()) {
		out->is_v6 = 1;
		auto bytes = addr.to_v6().to_bytes();
		std::memcpy(out->bytes, bytes.data(), 16);
	} else {
		out->is_v6 = 0;
		auto bytes = addr.to_v4().to_bytes();
		std::memcpy(out->bytes, bytes.data(), 4);
	}
}

} // namespace

extern "C" {

ct_ip_filter* ct_ip_filter_new(ct_error* err)
{
	return ct::guard(err, []() -> ct_ip_filter* {
		return reinterpret_cast<ct_ip_filter*>(new lt::ip_filter());
	});
}

void ct_ip_filter_free(ct_ip_filter* filter)
{
	delete unwrap(filter);
}

bool ct_ip_filter_empty(const ct_ip_filter* filter, ct_error* err)
{
	return ct::guard(err, [&] {
		return unwrap(filter)->empty();
	});
}

void ct_ip_filter_add_rule(ct_ip_filter* filter,
	const ct_address* first, const ct_address* last,
	uint32_t flags, ct_error* err)
{
	ct::guard(err, [&] {
		lt::address lt_first = to_lt_address(first);
		lt::address lt_last = to_lt_address(last);
		unwrap(filter)->add_rule(lt_first, lt_last, flags);
	});
}

uint32_t ct_ip_filter_access(const ct_ip_filter* filter,
	const ct_address* addr, ct_error* err)
{
	return ct::guard(err, [&] {
		lt::address lt_addr = to_lt_address(addr);
		return unwrap(filter)->access(lt_addr);
	});
}

ct_ip_range* ct_ip_filter_export(const ct_ip_filter* filter,
	size_t* out_len, ct_error* err)
{
	// Zeroed up front so a caller that ignores *err never reads garbage.
	*out_len = 0;
	return ct::guard(err, [&]() -> ct_ip_range* {
		auto tuple = unwrap(filter)->export_filter();
		auto const& v4_ranges = std::get<0>(tuple);
		auto const& v6_ranges = std::get<1>(tuple);

		size_t total = v4_ranges.size() + v6_ranges.size();
		if (total == 0) {
			*out_len = 0;
			return nullptr;
		}

		auto* result = new ct_ip_range[total];
		size_t idx = 0;

		for (auto const& r : v4_ranges) {
			from_lt_address(lt::address(r.first), &result[idx].first);
			from_lt_address(lt::address(r.last), &result[idx].last);
			result[idx].flags = r.flags;
			++idx;
		}

		for (auto const& r : v6_ranges) {
			from_lt_address(lt::address(r.first), &result[idx].first);
			from_lt_address(lt::address(r.last), &result[idx].last);
			result[idx].flags = r.flags;
			++idx;
		}

		*out_len = total;
		return result;
	});
}

void ct_ip_filter_export_free(ct_ip_range* ranges)
{
	delete[] ranges;
}

ct_port_filter* ct_port_filter_new(ct_error* err)
{
	return ct::guard(err, []() -> ct_port_filter* {
		return reinterpret_cast<ct_port_filter*>(new lt::port_filter());
	});
}

void ct_port_filter_free(ct_port_filter* filter)
{
	delete unwrap(filter);
}

void ct_port_filter_add_rule(ct_port_filter* filter,
	uint16_t first, uint16_t last,
	uint32_t flags, ct_error* err)
{
	ct::guard(err, [&] {
		unwrap(filter)->add_rule(first, last, flags);
	});
}

uint32_t ct_port_filter_access(const ct_port_filter* filter,
	uint16_t port, ct_error* err)
{
	return ct::guard(err, [&] {
		return unwrap(filter)->access(port);
	});
}

} // extern "C"
