// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

// Error translation and owned string/buffer plumbing.

#include "ct_common.hpp"

#include <libtorrent/bdecode.hpp>
#include <libtorrent/config.hpp>
#include <libtorrent/error_code.hpp>
#include <libtorrent/gzip.hpp>
#include <libtorrent/natpmp.hpp>
#include <libtorrent/socks5_stream.hpp>
#include <libtorrent/upnp.hpp>
#if TORRENT_USE_I2P
#include <libtorrent/i2p_stream.hpp>
#endif

#include <boost/system/error_code.hpp>

#include <cstring>
#include <exception>
#include <new>

namespace {

// Message of the most recent exception that carried no error_code, kept per
// thread so ct_error_message can recover it (only on the same thread; see
// ct_types.h).
thread_local std::string t_exception_message;

struct known_category {
	boost::system::error_category const* cat;
	ct_error_category kind;
};

ct_error_category category_kind(boost::system::error_category const& cat) noexcept
{
	known_category const known[] = {
		{&lt::libtorrent_category(), CT_ERROR_CAT_LIBTORRENT},
		{&lt::http_category(), CT_ERROR_CAT_HTTP},
		{&lt::bdecode_category(), CT_ERROR_CAT_BDECODE},
		{&lt::gzip_category(), CT_ERROR_CAT_GZIP},
		{&lt::socks_category(), CT_ERROR_CAT_SOCKS},
		{&lt::upnp_category(), CT_ERROR_CAT_UPNP},
		{&lt::pcp_category(), CT_ERROR_CAT_PCP},
#if TORRENT_USE_I2P
		{&lt::i2p_category(), CT_ERROR_CAT_I2P},
#endif
		{&boost::system::generic_category(), CT_ERROR_CAT_GENERIC},
		{&boost::system::system_category(), CT_ERROR_CAT_SYSTEM},
	};
	for (auto const& k : known)
		// operator==, not address identity: shared Windows builds can
		// hold semantically equal category objects at different
		// addresses.
		if (*k.cat == cat) return k.kind;
	return CT_ERROR_CAT_UNKNOWN;
}

} // namespace

namespace ct {

void set_error(ct_error* err, lt::error_code const& ec) noexcept
{
	if (err == nullptr) return;
	if (!ec) {
		clear_error(err);
		return;
	}
	err->value = ec.value();
	err->category = category_kind(ec.category());
	err->category_ptr = &ec.category();
}

void set_error_current_exception(ct_error* err) noexcept
{
	if (err == nullptr) return;
	try {
		throw;
	} catch (boost::system::system_error const& e) {
		set_error(err, e.code());
	} catch (std::bad_alloc const&) {
		set_error(err, lt::error_code(
			boost::system::errc::not_enough_memory,
			boost::system::generic_category()));
	} catch (std::exception const& e) {
		try {
			t_exception_message = e.what();
		} catch (...) {
			t_exception_message.clear();
		}
		err->value = 1;
		err->category = CT_ERROR_CAT_UNKNOWN;
		err->category_ptr = nullptr;
	} catch (...) {
		t_exception_message.clear();
		err->value = 1;
		err->category = CT_ERROR_CAT_UNKNOWN;
		err->category_ptr = nullptr;
	}
}

ct_str box_string(std::string s) noexcept
{
	if (s.empty()) return ct_str{nullptr, 0, nullptr};
	try {
		auto* box = new std::string(std::move(s));
		return ct_str{box->data(), box->size(), box};
	} catch (...) {
		return ct_str{nullptr, 0, nullptr};
	}
}

ct_buf box_buffer(std::vector<char> v)
{
	if (v.empty()) return ct_buf{nullptr, 0, nullptr};
	auto* box = new std::vector<char>(std::move(v));
	return ct_buf{
		reinterpret_cast<std::uint8_t const*>(box->data()), box->size(), box};
}

} // namespace ct

extern "C" {

void ct_str_free(ct_str* s)
{
	if (s == nullptr) return;
	delete static_cast<std::string*>(s->box_);
	*s = ct_str{nullptr, 0, nullptr};
}

void ct_buf_free(ct_buf* b)
{
	if (b == nullptr) return;
	delete static_cast<std::vector<char>*>(b->box_);
	*b = ct_buf{nullptr, 0, nullptr};
}

void ct_error_message(const ct_error* err, ct_str* out)
{
	if (out == nullptr) return;
	*out = ct_str{nullptr, 0, nullptr};
	if (err == nullptr || err->category == CT_ERROR_CAT_NONE) return;
	if (err->category_ptr != nullptr) {
		auto const& cat = *static_cast<boost::system::error_category const*>(
			err->category_ptr);
		try {
			*out = ct::box_string(cat.message(err->value));
		} catch (...) {}
		return;
	}
	*out = ct::box_string(t_exception_message);
}

ct_str_view ct_error_category_name(const ct_error* err)
{
	char const* name = "none";
	if (err != nullptr && err->category != CT_ERROR_CAT_NONE) {
		if (err->category_ptr != nullptr)
			name = static_cast<boost::system::error_category const*>(
				err->category_ptr)->name();
		else
			name = "exception";
	}
	return ct_str_view{name, std::strlen(name)};
}

} // extern "C"
