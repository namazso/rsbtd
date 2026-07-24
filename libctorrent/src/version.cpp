// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

#include "ct_common.hpp"

#include <libtorrent/version.hpp>

#include <cstring>

extern "C" {

ct_str_view ct_libtorrent_version(void)
{
	char const* v = lt::version();
	return ct_str_view{v, std::strlen(v)};
}

uint32_t ct_libtorrent_version_num(void)
{
	return LIBTORRENT_VERSION_NUM;
}

uint32_t ct_libtorrent_abi_version(void)
{
	return TORRENT_ABI_VERSION;
}

} // extern "C"
