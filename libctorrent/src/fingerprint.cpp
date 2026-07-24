// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

#include <ctorrent/ct_fingerprint.h>

#include "ct_common.hpp"

#include <libtorrent/fingerprint.hpp>

extern "C" {

ct_str ct_generate_fingerprint(const char* name,
	int32_t major, int32_t minor, int32_t revision, int32_t tag,
	ct_error* err)
{
	return ct::guard(err, [&]() -> ct_str {
		std::string fp = lt::generate_fingerprint(name, major, minor, revision, tag);
		return ct::box_string(std::move(fp));
	});
}

} // extern "C"
