// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

// Internal shim tests: string/buffer boxing, error mapping, the exception
// guard. Feature-level behavior belongs in rbtorrent's tests, not here.

#include "ct_common.hpp"

#include <ctorrent/ct_peer_class.h>

#include <libtorrent/error_code.hpp>

#include <cstdio>
#include <cstring>
#include <stdexcept>
#include <string>

namespace {

int g_failures = 0;

#define CHECK(cond) \
	do { \
		if (!(cond)) { \
			std::fprintf(stderr, "FAIL %s:%d: %s\n", __FILE__, __LINE__, #cond); \
			++g_failures; \
		} \
	} while (0)

void test_box_string()
{
	ct_str s = ct::box_string("hello");
	CHECK(s.len == 5);
	CHECK(std::memcmp(s.ptr, "hello", 5) == 0);
	ct_str_free(&s);
	CHECK(s.ptr == nullptr && s.len == 0 && s.box_ == nullptr);
	// double free is a no-op
	ct_str_free(&s);

	ct_str empty = ct::box_string("");
	CHECK(empty.len == 0 && empty.box_ == nullptr);
	ct_str_free(&empty);
}

void test_box_buffer()
{
	std::vector<char> v{1, 2, 3};
	ct_buf b = ct::box_buffer(std::move(v));
	CHECK(b.len == 3);
	CHECK(b.ptr[2] == 3);
	ct_buf_free(&b);
	CHECK(b.ptr == nullptr && b.box_ == nullptr);
}

void test_error_mapping()
{
	ct_error err;
	lt::error_code const ec(lt::errors::torrent_missing_info,
		lt::libtorrent_category());
	ct::set_error(&err, ec);
	CHECK(err.category == CT_ERROR_CAT_LIBTORRENT);
	CHECK(err.value == lt::errors::torrent_missing_info);
	CHECK(err.category_ptr == &lt::libtorrent_category());

	ct_str_view const name = ct_error_category_name(&err);
	CHECK(name.len > 0);

	ct_str msg;
	ct_error_message(&err, &msg);
	CHECK(msg.len > 0);
	ct_str_free(&msg);

	// success clears
	ct::set_error(&err, lt::error_code());
	CHECK(err.category == CT_ERROR_CAT_NONE && err.value == 0);
}

void test_guard()
{
	ct_error err;

	int const ok = ct::guard(&err, [] { return 42; });
	CHECK(ok == 42);
	CHECK(err.category == CT_ERROR_CAT_NONE);

	int const bad = ct::guard(&err, []() -> int {
		throw lt::system_error(lt::error_code(
			lt::errors::invalid_torrent_handle, lt::libtorrent_category()));
	});
	CHECK(bad == 0);
	CHECK(err.category == CT_ERROR_CAT_LIBTORRENT);
	CHECK(err.value == lt::errors::invalid_torrent_handle);

	ct::guard(&err, []() -> int { throw std::runtime_error("boom"); });
	CHECK(err.category == CT_ERROR_CAT_UNKNOWN);
	CHECK(err.category_ptr == nullptr);
	ct_str msg;
	ct_error_message(&err, &msg);
	CHECK(msg.len == 4 && std::memcmp(msg.ptr, "boom", 4) == 0);
	ct_str_free(&msg);
}

void test_peer_class_type_filter_class_31()
{
	// Class 31 lands in the sign bit of the 32-bit masks. Tested here
	// rather than in rbtorrent because the shift runs in this library's
	// translation units and must stay well-defined (the shim builds as
	// C++20 for exactly this; see the top-level CMakeLists.txt).
	ct_error err;
	ct_peer_class_type_filter* f = ct_peer_class_type_filter_new(&err);
	CHECK(f != nullptr);

	ct_peer_class_type_filter_add(f, CT_SOCKET_TCP, 31, &err);
	CHECK(err.category == CT_ERROR_CAT_NONE);
	CHECK(ct_peer_class_type_filter_apply(f, CT_SOCKET_TCP, 0, &err)
		== 0x80000000u);
	ct_peer_class_type_filter_remove(f, CT_SOCKET_TCP, 31, &err);
	CHECK(ct_peer_class_type_filter_apply(f, CT_SOCKET_TCP, 0, &err) == 0);

	ct_peer_class_type_filter_disallow(f, CT_SOCKET_TCP, 31, &err);
	CHECK(ct_peer_class_type_filter_apply(f, CT_SOCKET_TCP, 0xffffffffu, &err)
		== 0x7fffffffu);
	ct_peer_class_type_filter_allow(f, CT_SOCKET_TCP, 31, &err);
	CHECK(ct_peer_class_type_filter_apply(f, CT_SOCKET_TCP, 0xffffffffu, &err)
		== 0xffffffffu);

	// 32 is outside the filter's 32-bit domain.
	ct_peer_class_type_filter_add(f, CT_SOCKET_TCP, 32, &err);
	CHECK(err.category != CT_ERROR_CAT_NONE);

	ct_peer_class_type_filter_free(f);
}

} // namespace

int main()
{
	test_box_string();
	test_box_buffer();
	test_error_mapping();
	test_guard();
	test_peer_class_type_filter_class_31();
	if (g_failures != 0) {
		std::fprintf(stderr, "%d check(s) failed\n", g_failures);
		return 1;
	}
	std::printf("all ok\n");
	return 0;
}
