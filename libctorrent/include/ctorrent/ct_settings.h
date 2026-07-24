/* Copyright (C) 2026  namazso <admin@namazso.eu>
 * SPDX-License-Identifier: MPL-2.0
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/.
 */

/* settings_pack: typed session configuration. Keys are the CT_SET_*
 * constants from ct_settings_generated.h; a key encodes its type in the top
 * bits (CT_SETTINGS_TYPE_MASK).
 */
#ifndef CT_SETTINGS_H_INCLUDED
#define CT_SETTINGS_H_INCLUDED

#include "ct_settings_generated.h"
#include "ct_types.h"

#ifdef __cplusplus
extern "C" {
#endif

/* Opaque lt::settings_pack. A pack is a delta: it only contains explicitly
 * set values. Not thread-safe for concurrent mutation. */
typedef struct ct_settings_pack ct_settings_pack;

/* Constructors return NULL on allocation failure. */
ct_settings_pack* ct_settings_pack_new(void);
/* Pre-populated with libtorrent's default for every setting. */
ct_settings_pack* ct_settings_pack_default(void);
ct_settings_pack* ct_settings_pack_clone(const ct_settings_pack* pack);
void ct_settings_pack_free(ct_settings_pack* pack);

/* Every function taking a key validates it (type bits and index bounds,
 * against the libtorrent build these bindings were compiled with); invalid
 * keys are silently ignored by setters and unknown to getters/lookups.
 * ct_setting_is_valid exposes the check directly. */
bool ct_setting_is_valid(int32_t key);

/* Setters silently ignore invalid keys and keys whose type does not match
 * the function, matching libtorrent's behavior. */
void ct_settings_pack_set_str(ct_settings_pack* pack, int32_t key,
	ct_str_view value);
void ct_settings_pack_set_int(ct_settings_pack* pack, int32_t key,
	int32_t value);
void ct_settings_pack_set_bool(ct_settings_pack* pack, int32_t key,
	bool value);

/* Getters return false when the pack does not contain the key (or on type
 * mismatch); *out is untouched then. */
bool ct_settings_pack_get_str(const ct_settings_pack* pack, int32_t key,
	ct_str* out);
bool ct_settings_pack_get_int(const ct_settings_pack* pack, int32_t key,
	int32_t* out);
bool ct_settings_pack_get_bool(const ct_settings_pack* pack, int32_t key,
	bool* out);

bool ct_settings_pack_has(const ct_settings_pack* pack, int32_t key);
void ct_settings_pack_clear(ct_settings_pack* pack);
void ct_settings_pack_clear_one(ct_settings_pack* pack, int32_t key);

/* Copies every value present in src into dst; keys absent from src keep
 * their dst values. */
void ct_settings_pack_merge(ct_settings_pack* dst,
	const ct_settings_pack* src);

/* Name lookup, resolved by the linked libtorrent: also covers settings
 * newer than these bindings. Returns -1 / an empty view for unknowns. */
int32_t ct_setting_by_name(ct_str_view name);
ct_str_view ct_name_for_setting(int32_t key);

#ifdef __cplusplus
}
#endif

#endif
