// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

#include <ctorrent/ct_settings.h>

#include "ct_common.hpp"

#include <libtorrent/settings_pack.hpp>

#include <cstring>
#include <new>

#include "settings_asserts.inc"

namespace {

lt::settings_pack* unwrap(ct_settings_pack* p)
{
	return reinterpret_cast<lt::settings_pack*>(p);
}

lt::settings_pack const* unwrap(ct_settings_pack const* p)
{
	return reinterpret_cast<lt::settings_pack const*>(p);
}

ct_settings_pack* wrap(lt::settings_pack* p)
{
	return reinterpret_cast<ct_settings_pack*>(p);
}

// libtorrent's settings functions only assert (or don't check at all) that a
// key's index is within the name tables; an out-of-range index is a
// heap/static OOB access. Validate against the bounds of the libtorrent
// build these bindings were compiled against.
bool valid_key(int32_t key)
{
	if (key < 0 || key > 0xffff) return false;
	int const index = key & CT_SETTINGS_INDEX_MASK;
	switch (key & CT_SETTINGS_TYPE_MASK) {
	case CT_SETTINGS_STR_BASE:
		return index < lt::settings_pack::num_string_settings;
	case CT_SETTINGS_INT_BASE:
		return index < lt::settings_pack::num_int_settings;
	case CT_SETTINGS_BOOL_BASE:
		return index < lt::settings_pack::num_bool_settings;
	default:
		return false;
	}
}

bool valid_key_of(int32_t key, int32_t base)
{
	return (key & CT_SETTINGS_TYPE_MASK) == base && valid_key(key);
}

} // namespace

extern "C" {

bool ct_setting_is_valid(int32_t key)
{
	return valid_key(key);
}

ct_settings_pack* ct_settings_pack_new(void)
{
	return wrap(new (std::nothrow) lt::settings_pack());
}

ct_settings_pack* ct_settings_pack_default(void)
{
	try {
		auto pack = lt::default_settings();
		// Upstream default_settings() omits string settings whose default
		// is empty; densify so the pack covers every known setting.
		for (int i = 0; i < lt::settings_pack::num_string_settings; ++i) {
			int const key = lt::settings_pack::string_type_base + i;
			if (!pack.has_val(key)) pack.set_str(key, std::string());
		}
		return wrap(new lt::settings_pack(std::move(pack)));
	} catch (...) {
		return nullptr;
	}
}

ct_settings_pack* ct_settings_pack_clone(const ct_settings_pack* pack)
{
	try {
		return wrap(new lt::settings_pack(*unwrap(pack)));
	} catch (...) {
		return nullptr;
	}
}

void ct_settings_pack_free(ct_settings_pack* pack)
{
	delete unwrap(pack);
}

void ct_settings_pack_set_str(ct_settings_pack* pack, int32_t key,
	ct_str_view value)
{
	if (!valid_key_of(key, CT_SETTINGS_STR_BASE)) return;
	ct::infallible([&] {
		unwrap(pack)->set_str(key, std::string(value.ptr, value.len));
	});
}

void ct_settings_pack_set_int(ct_settings_pack* pack, int32_t key,
	int32_t value)
{
	if (!valid_key_of(key, CT_SETTINGS_INT_BASE)) return;
	ct::infallible([&] { unwrap(pack)->set_int(key, value); });
}

void ct_settings_pack_set_bool(ct_settings_pack* pack, int32_t key,
	bool value)
{
	if (!valid_key_of(key, CT_SETTINGS_BOOL_BASE)) return;
	ct::infallible([&] { unwrap(pack)->set_bool(key, value); });
}

bool ct_settings_pack_get_str(const ct_settings_pack* pack, int32_t key,
	ct_str* out)
{
	if (!valid_key_of(key, CT_SETTINGS_STR_BASE)) return false;
	auto const& p = *unwrap(pack);
	if (!p.has_val(key)) return false;
	try {
		*out = ct::box_string(p.get_str(key));
	} catch (...) {
		*out = ct_str{nullptr, 0, nullptr};
	}
	return true;
}

bool ct_settings_pack_get_int(const ct_settings_pack* pack, int32_t key,
	int32_t* out)
{
	if (!valid_key_of(key, CT_SETTINGS_INT_BASE)) return false;
	auto const& p = *unwrap(pack);
	if (!p.has_val(key)) return false;
	*out = p.get_int(key);
	return true;
}

bool ct_settings_pack_get_bool(const ct_settings_pack* pack, int32_t key,
	bool* out)
{
	if (!valid_key_of(key, CT_SETTINGS_BOOL_BASE)) return false;
	auto const& p = *unwrap(pack);
	if (!p.has_val(key)) return false;
	*out = p.get_bool(key);
	return true;
}

bool ct_settings_pack_has(const ct_settings_pack* pack, int32_t key)
{
	if (!valid_key(key)) return false;
	return unwrap(pack)->has_val(key);
}

void ct_settings_pack_clear(ct_settings_pack* pack)
{
	unwrap(pack)->clear();
}

void ct_settings_pack_clear_one(ct_settings_pack* pack, int32_t key)
{
	if (!valid_key(key)) return;
	unwrap(pack)->clear(key);
}

void ct_settings_pack_merge(ct_settings_pack* dst, const ct_settings_pack* src)
{
	auto const& s = *unwrap(src);
	auto& d = *unwrap(dst);
	ct::infallible([&] {
		for (int i = 0; i < lt::settings_pack::num_string_settings; ++i) {
			int const key = lt::settings_pack::string_type_base + i;
			if (s.has_val(key)) d.set_str(key, s.get_str(key));
		}
		for (int i = 0; i < lt::settings_pack::num_int_settings; ++i) {
			int const key = lt::settings_pack::int_type_base + i;
			if (s.has_val(key)) d.set_int(key, s.get_int(key));
		}
		for (int i = 0; i < lt::settings_pack::num_bool_settings; ++i) {
			int const key = lt::settings_pack::bool_type_base + i;
			if (s.has_val(key)) d.set_bool(key, s.get_bool(key));
		}
	});
}

int32_t ct_setting_by_name(ct_str_view name)
{
	// Removed settings keep a nameless ("") slot in libtorrent's tables; an
	// empty query would resolve to the first such slot instead of failing.
	if (name.len == 0) return -1;
	int const key = lt::setting_by_name(ct::view(name));
	// A key resolved by the linked libtorrent can still exceed the tables
	// of the headers we compiled against (never with the vendored build,
	// but possible with an unusual system pairing); reject it rather than
	// hand out a key the other functions would refuse or misuse.
	if (key < 0 || !valid_key(key)) return -1;
	return key;
}

ct_str_view ct_name_for_setting(int32_t key)
{
	if (!valid_key(key)) return ct_str_view{nullptr, 0};
	char const* name = lt::name_for_setting(key);
	if (name == nullptr) return ct_str_view{nullptr, 0};
	return ct_str_view{name, std::strlen(name)};
}

} // extern "C"
