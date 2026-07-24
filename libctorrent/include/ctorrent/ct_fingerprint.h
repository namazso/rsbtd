/* Copyright (C) 2026  namazso <admin@namazso.eu>
 * SPDX-License-Identifier: MPL-2.0
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/.
 */

#ifndef CT_FINGERPRINT_H
#define CT_FINGERPRINT_H

#include <ctorrent/ct_types.h>

#ifdef __cplusplus
extern "C" {
#endif

// Generate a client ID fingerprint string.
// name must be exactly 2 characters.
// Returns a ct_str (owned string) that must be freed with ct_str_free().
ct_str ct_generate_fingerprint(const char* name,
	int32_t major, int32_t minor, int32_t revision, int32_t tag,
	ct_error* err);

#ifdef __cplusplus
}
#endif

#endif // CT_FINGERPRINT_H
