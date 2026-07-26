// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

#include <ctorrent/ct_session.h>
#include <ctorrent/ct_peer_class.h>
#include <ctorrent/ct_torrent_handle.h>

#include "ct_common.hpp"

#include <libtorrent/bdecode.hpp>
#include <libtorrent/bencode.hpp>
#include <libtorrent/disabled_disk_io.hpp>
#include <libtorrent/entry.hpp>
#include <libtorrent/extensions/i2p_pex.hpp>
#include <libtorrent/extensions/smart_ban.hpp>
#include <libtorrent/extensions/ut_metadata.hpp>
#include <libtorrent/extensions/ut_pex.hpp>
#include <libtorrent/ip_filter.hpp>
#include <libtorrent/mmap_disk_io.hpp>
#include <libtorrent/peer_class.hpp>
#include <libtorrent/peer_class_type_filter.hpp>
#include <libtorrent/posix_disk_io.hpp>
#include <libtorrent/pread_disk_io.hpp>
#include <libtorrent/read_resume_data.hpp>
#include <libtorrent/session.hpp>
#include <libtorrent/session_params.hpp>
#include <libtorrent/settings_pack.hpp>
#include <libtorrent/write_resume_data.hpp>

#include "handles.hpp"

#include <cstring>
#include <iterator>
#include <new>
#include <utility>
#include <vector>

namespace {

// Top-level resume-data key for the caller's opaque byte string; unknown to
// libtorrent (read_resume_data skips it), owned end-to-end by the bindings.
constexpr char const* rbt_data_key = "rbt-data";

// ct_session_params carries shim-side state next to the lt::session_params:
// the default-extension toggles can only be applied when the session is
// constructed (libtorrent has no public way to build the individual default
// plugin wrappers).
struct params_impl {
	lt::session_params params;
	bool ut_metadata = true;
	bool ut_pex = true;
	bool smart_ban = true;
};

params_impl* unwrap(ct_session_params* p)
{
	return reinterpret_cast<params_impl*>(p);
}

params_impl const* unwrap(ct_session_params const* p)
{
	return reinterpret_cast<params_impl const*>(p);
}

lt::session* unwrap(ct_session* s)
{
	return reinterpret_cast<lt::session*>(s);
}

lt::session const* unwrap(ct_session const* s)
{
	return reinterpret_cast<lt::session const*>(s);
}

lt::session_proxy* proxy_ptr(ct_session_proxy* p)
{
	return std::launder(reinterpret_cast<lt::session_proxy*>(p->data_));
}

// Shared body of the ct_session_find_torrent_* pair: *hash* is one of the
// ct_sha1/ct_sha256 PODs, and Hash the lt hash type it is layout-compatible
// with (asserted in abi_asserts.cpp).
template <typename Hash, typename CtHash>
bool find_torrent_impl(ct_session const* session, CtHash const* hash,
	ct_torrent_handle* out)
{
	if (!session || !hash || !out) return false;
	try {
		lt::torrent_handle th = unwrap(session)->find_torrent(
			Hash(reinterpret_cast<char const*>(hash->data)));
		if (!th.is_valid()) return false;
		new (out->data_) lt::torrent_handle(std::move(th));
		return true;
	} catch (...) {
		return false;
	}
}

static_assert(CT_SAVE_SETTINGS
	== static_cast<uint32_t>(lt::session_handle::save_settings));
static_assert(CT_SAVE_DHT_STATE
	== static_cast<uint32_t>(lt::session_handle::save_dht_state));
static_assert(CT_SAVE_EXTENSION_STATE
	== static_cast<uint32_t>(lt::session_handle::save_extension_state));
static_assert(CT_SAVE_IP_FILTER
	== static_cast<uint32_t>(lt::session_handle::save_ip_filter));

} // namespace

extern "C" {

ct_session_params* ct_session_params_new(void)
{
	try {
		return reinterpret_cast<ct_session_params*>(new params_impl());
	} catch (...) {
		return nullptr;
	}
}

void ct_session_params_free(ct_session_params* params)
{
	delete unwrap(params);
}

void ct_session_params_set_settings(ct_session_params* params,
	const ct_settings_pack* pack, ct_error* err)
{
	ct::guard(err, [&] {
		unwrap(params)->params.settings =
			*reinterpret_cast<lt::settings_pack const*>(pack);
	});
}

void ct_session_params_set_default_extensions(ct_session_params* params,
	bool ut_metadata, bool ut_pex, bool smart_ban)
{
	auto* p = unwrap(params);
	p->ut_metadata = ut_metadata;
	p->ut_pex = ut_pex;
	p->smart_ban = smart_ban;
}

bool ct_session_params_set_disk_io(ct_session_params* params,
	int32_t backend)
{
	auto& p = unwrap(params)->params;
	switch (backend) {
		case CT_DISK_IO_DEFAULT:
			p.disk_io_constructor = lt::default_disk_io_constructor;
			return true;
		case CT_DISK_IO_MMAP:
#if TORRENT_HAVE_MMAP || TORRENT_HAVE_MAP_VIEW_OF_FILE
			p.disk_io_constructor = lt::mmap_disk_io_constructor;
			return true;
#else
			return false;
#endif
		case CT_DISK_IO_POSIX:
			p.disk_io_constructor = lt::posix_disk_io_constructor;
			return true;
		case CT_DISK_IO_PREAD:
			p.disk_io_constructor = lt::pread_disk_io_constructor;
			return true;
		case CT_DISK_IO_DISABLED:
			p.disk_io_constructor = lt::disabled_disk_io_constructor;
			return true;
		default:
			return false;
	}
}

void ct_session_params_set_paused(ct_session_params* params, bool paused)
{
	auto& p = unwrap(params)->params;
	if (paused)
		p.flags |= lt::session::paused;
	else
		p.flags &= ~lt::session::paused;
}

ct_session* ct_session_new(const ct_session_params* params, ct_error* err)
{
	return ct::guard(err, [&]() -> ct_session* {
		auto const* impl = unwrap(params);
		lt::session_params sp = impl->params;
		bool const defaults =
			impl->ut_metadata && impl->ut_pex && impl->smart_ban;
		if (!defaults) sp.extensions.clear();
		auto session = std::make_unique<lt::session>(std::move(sp));
		if (!defaults) {
			// Rebuilding the cleared default list: the toggles cover
			// only the trio below, so the other default extensions
			// must be re-added unconditionally.
#if TORRENT_USE_I2P
			session->add_extension(&lt::create_i2p_pex_plugin);
#endif
			if (impl->ut_metadata)
				session->add_extension(&lt::create_ut_metadata_plugin);
			if (impl->ut_pex)
				session->add_extension(&lt::create_ut_pex_plugin);
			if (impl->smart_ban)
				session->add_extension(&lt::create_smart_ban_plugin);
		}
		return reinterpret_cast<ct_session*>(session.release());
	});
}

void ct_session_abort(ct_session* session, ct_session_proxy* out_proxy)
{
	static_assert(sizeof(lt::session_proxy) == sizeof(ct_session_proxy));
	static_assert(alignof(lt::session_proxy) <= alignof(ct_session_proxy));
	new (out_proxy->data_) lt::session_proxy(unwrap(session)->abort());
}

void ct_session_free(ct_session* session)
{
	delete unwrap(session);
}

void ct_session_proxy_drop(ct_session_proxy* proxy)
{
	proxy_ptr(proxy)->~session_proxy();
}

ct_buf ct_session_get_state(const ct_session* session, uint32_t save_flags,
	ct_error* err)
{
	return ct::guard(err, [&]() -> ct_buf {
		auto const flags = lt::save_state_flags_t(save_flags);
		return ct::box_buffer(lt::write_session_params_buf(
			unwrap(session)->session_state(flags), flags));
	});
}

void ct_session_params_load_state(ct_session_params* params,
	ct_span bencoded, uint32_t save_flags, ct_error* err)
{
	ct::guard(err, [&] {
		// read_session_params returns a freshly default-constructed
		// session_params; assigning it wholesale would wipe fields it does
		// not load (disk_io_constructor, flags). Move over only the fields
		// the caller asked to restore.
		auto const flags = lt::save_state_flags_t(save_flags);
		lt::session_params loaded =
			lt::read_session_params(ct::span(bencoded), flags);
		auto& dst = unwrap(params)->params;
		if (flags & lt::session_handle::save_settings)
			dst.settings = std::move(loaded.settings);
		if (flags & lt::session_handle::save_dht_state)
			dst.dht_state = std::move(loaded.dht_state);
		if (flags & lt::session_handle::save_extension_state)
			dst.ext_state = std::move(loaded.ext_state);
		if (flags & lt::session_handle::save_ip_filter)
			dst.ip_filter = std::move(loaded.ip_filter);
	});
}

void ct_session_post_session_stats(ct_session* session, ct_error* err)
{
	ct::guard(err, [&] { unwrap(session)->post_session_stats(); });
}

void ct_session_post_torrent_updates(ct_session* session, uint32_t flags, ct_error* err)
{
	ct::guard(err, [&] {
		unwrap(session)->post_torrent_updates(lt::status_flags_t{flags});
	});
}

void ct_session_apply_settings(ct_session* session,
	const ct_settings_pack* pack, ct_error* err)
{
	ct::guard(err, [&] {
		unwrap(session)->apply_settings(
			*reinterpret_cast<lt::settings_pack const*>(pack));
	});
}

ct_settings_pack* ct_session_get_settings(const ct_session* session,
	ct_error* err)
{
	return ct::guard(err, [&]() -> ct_settings_pack* {
		return reinterpret_cast<ct_settings_pack*>(
			new lt::settings_pack(unwrap(session)->get_settings()));
	});
}

uint16_t ct_session_listen_port(const ct_session* session, ct_error* err)
{
	return ct::guard(err, [&] { return unwrap(session)->listen_port(); });
}

uint16_t ct_session_ssl_listen_port(const ct_session* session, ct_error* err)
{
	return ct::guard(err, [&] { return unwrap(session)->ssl_listen_port(); });
}

bool ct_session_is_listening(const ct_session* session, ct_error* err)
{
	return ct::guard(err, [&] { return unwrap(session)->is_listening(); });
}

bool ct_session_is_paused(const ct_session* session, ct_error* err)
{
	return ct::guard(err, [&] { return unwrap(session)->is_paused(); });
}

void ct_session_pause(ct_session* session, ct_error* err)
{
	ct::guard(err, [&] { unwrap(session)->pause(); });
}

void ct_session_resume(ct_session* session, ct_error* err)
{
	ct::guard(err, [&] { unwrap(session)->resume(); });
}

bool ct_session_is_dht_running(const ct_session* session, ct_error* err)
{
	return ct::guard(err, [&] { return unwrap(session)->is_dht_running(); });
}

void ct_session_set_ip_filter(ct_session* session,
	const ct_ip_filter* filter, ct_error* err)
{
	ct::guard(err, [&] {
		unwrap(session)->set_ip_filter(
			*reinterpret_cast<const lt::ip_filter*>(filter));
	});
}

ct_ip_filter* ct_session_get_ip_filter(const ct_session* session,
	ct_error* err)
{
	return ct::guard(err, [&]() -> ct_ip_filter* {
		return reinterpret_cast<ct_ip_filter*>(
			new lt::ip_filter(unwrap(session)->get_ip_filter()));
	});
}

void ct_session_set_port_filter(ct_session* session,
	const ct_port_filter* filter, ct_error* err)
{
	ct::guard(err, [&] {
		unwrap(session)->set_port_filter(
			*reinterpret_cast<const lt::port_filter*>(filter));
	});
}

ct_peer_class_t ct_session_create_peer_class(ct_session* session,
	const char* name, ct_error* err)
{
	return ct::guard(err, [&]() -> ct_peer_class_t {
		return static_cast<ct_peer_class_t>(unwrap(session)->create_peer_class(name));
	});
}

void ct_session_delete_peer_class(ct_session* session,
	ct_peer_class_t cid, ct_error* err)
{
	ct::guard(err, [&] {
		unwrap(session)->delete_peer_class(lt::peer_class_t{cid});
	});
}

void ct_session_get_peer_class(ct_session* session, ct_peer_class_t cid,
	ct_peer_class_info* out, ct_str* out_label, ct_error* err)
{
	ct::guard(err, [&] {
		lt::peer_class_info pci = unwrap(session)->get_peer_class(lt::peer_class_t{cid});
		out->ignore_unchoke_slots = pci.ignore_unchoke_slots;
		out->connection_limit_factor = pci.connection_limit_factor;
		out->upload_limit = pci.upload_limit;
		out->download_limit = pci.download_limit;
		out->upload_priority = pci.upload_priority;
		out->download_priority = pci.download_priority;
		// label is an input-only field; the name is returned as an owned
		// string (pci is a stack local, so no pointer into it may escape).
		out->label = nullptr;
		if (out_label != nullptr)
			*out_label = ct::box_string(std::move(pci.label));
	});
}

void ct_session_set_peer_class(ct_session* session, ct_peer_class_t cid,
	const ct_peer_class_info* info, ct_error* err)
{
	ct::guard(err, [&] {
		// Seed from the current class: libtorrent's set_info copies the
		// label unconditionally, so a default-constructed pci would clear
		// the name whenever the caller passes label == NULL (which the
		// contract defines as "leave the name unchanged").
		lt::peer_class_info pci = unwrap(session)->get_peer_class(lt::peer_class_t{cid});
		pci.ignore_unchoke_slots = info->ignore_unchoke_slots;
		pci.connection_limit_factor = info->connection_limit_factor;
		pci.upload_limit = info->upload_limit;
		pci.download_limit = info->download_limit;
		pci.upload_priority = info->upload_priority;
		pci.download_priority = info->download_priority;
		if (info->label) {
			pci.label = info->label;
		}
		unwrap(session)->set_peer_class(lt::peer_class_t{cid}, pci);
	});
}

void ct_session_set_peer_class_filter(ct_session* session,
	const ct_ip_filter* filter, ct_error* err)
{
	ct::guard(err, [&] {
		unwrap(session)->set_peer_class_filter(
			*reinterpret_cast<const lt::ip_filter*>(filter));
	});
}

ct_ip_filter* ct_session_get_peer_class_filter(ct_session* session,
	ct_error* err)
{
	return ct::guard(err, [&]() -> ct_ip_filter* {
		return reinterpret_cast<ct_ip_filter*>(
			new lt::ip_filter(unwrap(session)->get_peer_class_filter()));
	});
}

void ct_session_set_peer_class_type_filter(ct_session* session,
	const ct_peer_class_type_filter* filter, ct_error* err)
{
	ct::guard(err, [&] {
		unwrap(session)->set_peer_class_type_filter(
			*reinterpret_cast<const lt::peer_class_type_filter*>(filter));
	});
}

ct_peer_class_type_filter* ct_session_get_peer_class_type_filter(
	ct_session* session, ct_error* err)
{
	return ct::guard(err, [&]() -> ct_peer_class_type_filter* {
		return reinterpret_cast<ct_peer_class_type_filter*>(
			new lt::peer_class_type_filter(unwrap(session)->get_peer_class_type_filter()));
	});
}

ct_port_mapping_t* ct_session_add_port_mapping(ct_session* session,
	ct_portmap_protocol protocol, int32_t external_port, int32_t local_port,
	size_t* out_count, ct_error* err)
{
	// Zeroed up front so a caller that ignores *err never reads garbage.
	*out_count = 0;
	return ct::guard(err, [&]() -> ct_port_mapping_t* {
		lt::portmap_protocol proto = (protocol == CT_PORTMAP_TCP)
			? lt::portmap_protocol::tcp
			: lt::portmap_protocol::udp;
		std::vector<lt::port_mapping_t> mappings =
			unwrap(session)->add_port_mapping(proto, external_port, local_port);

		if (mappings.empty()) {
			*out_count = 0;
			return nullptr;
		}

		auto* result = new ct_port_mapping_t[mappings.size()];
		for (size_t i = 0; i < mappings.size(); ++i) {
			result[i] = static_cast<ct_port_mapping_t>(mappings[i]);
		}
		*out_count = mappings.size();
		return result;
	});
}

void ct_port_mapping_array_free(ct_port_mapping_t* mappings)
{
	delete[] mappings;
}

void ct_session_delete_port_mapping(ct_session* session,
	ct_port_mapping_t handle, ct_error* err)
{
	ct::guard(err, [&] {
		unwrap(session)->delete_port_mapping(lt::port_mapping_t{handle});
	});
}

void ct_session_reopen_network_sockets(ct_session* session,
	uint32_t options, ct_error* err)
{
	ct::guard(err, [&] {
		unwrap(session)->reopen_network_sockets(
			static_cast<lt::reopen_network_flags_t>(options));
	});
}

void ct_session_async_add_torrent(ct_session* session,
	const ct_add_torrent_params* params, uint64_t userdata, ct_error* err)
{
	ct::guard(err, [&] {
		lt::add_torrent_params atp = ct::unwrap(params);
		// Lossless only because void* is 64-bit; asserted in abi_asserts.cpp
		// (32-bit platforms are unsupported).
		atp.userdata = lt::client_data_t{reinterpret_cast<void*>(userdata)};
		unwrap(session)->async_add_torrent(std::move(atp));
	});
}

void ct_session_remove_torrent(ct_session* session,
	const ct_torrent_handle* handle, uint32_t flags)
{
	try {
		unwrap(session)->remove_torrent(
			*reinterpret_cast<const lt::torrent_handle*>(handle),
			static_cast<lt::remove_flags_t>(flags)
		);
	} catch (...) {
		// Non-fallible; invalid handles are silent no-ops.
	}
}

bool ct_session_find_torrent_v1(const ct_session* session,
	const ct_sha1* hash, ct_torrent_handle* out)
{
	return find_torrent_impl<lt::sha1_hash>(session, hash, out);
}

bool ct_session_find_torrent_v2(const ct_session* session,
	const ct_sha256* hash, ct_torrent_handle* out)
{
	return find_torrent_impl<lt::sha256_hash>(session, hash, out);
}

ct_buf ct_write_resume_data_buf(const ct_add_torrent_params* atp,
	ct_error* err)
{
	return ct::guard(err, [&]() -> ct_buf {
		std::vector<char> buf = lt::write_resume_data_buf(ct::unwrap(atp));
		return ct::box_buffer(std::move(buf));
	});
}

ct_add_torrent_params* ct_read_resume_data(ct_span buf,
	const ct_load_torrent_limits* limits, ct_error* err)
{
	return ct::guard(err, [&]() -> ct_add_torrent_params* {
		lt::error_code ec;
		lt::load_torrent_limits lt_lim = ct::to_lt_load_limits(limits);
		lt::add_torrent_params atp = lt::read_resume_data(
			lt::span<const char>(reinterpret_cast<const char*>(buf.ptr), buf.len),
			ec, lt_lim);
		if (ec) {
			ct::set_error(err, ec);
			return nullptr;
		}
		return ct::box_atp(std::move(atp));
	});
}

ct_buf ct_write_resume_data_buf_ex(const ct_add_torrent_params* atp,
	ct_span extra, ct_error* err)
{
	return ct::guard(err, [&]() -> ct_buf {
		if (extra.len == 0) {
			return ct::box_buffer(lt::write_resume_data_buf(ct::unwrap(atp)));
		}
		// The blob is spliced verbatim as the key's value (preformatted),
		// so it must be exactly one well-formed bencode value: anything
		// else would corrupt the whole file for every reader, old builds
		// included. Reject rather than emit a broken file.
		lt::error_code ec;
		lt::bdecode_node blob = lt::bdecode(ct::span(extra), ec);
		if (ec) {
			ct::set_error(err, ec);
			return ct_buf{};
		}
		if (blob.data_section().size()
			!= static_cast<std::ptrdiff_t>(extra.len))
		{
			ct::set_error(err, lt::error_code(lt::errors::invalid_bencoding));
			return ct_buf{};
		}
		// The entry form so the extra key sorts canonically with the rest;
		// write_resume_data_buf is bencode-of-write_resume_data, so the
		// output is byte-identical to the plain function modulo this key.
		lt::entry e = lt::write_resume_data(ct::unwrap(atp));
		const char* p = reinterpret_cast<const char*>(extra.ptr);
		e[rbt_data_key] = lt::entry::preformatted_type(p, p + extra.len);
		std::vector<char> buf;
		lt::bencode(std::back_inserter(buf), e);
		return ct::box_buffer(std::move(buf));
	});
}

ct_add_torrent_params* ct_read_resume_data_ex(ct_span buf,
	const ct_load_torrent_limits* limits, ct_buf* extra_out, ct_error* err)
{
	if (extra_out != nullptr) *extra_out = ct_buf{};
	return ct::guard(err, [&]() -> ct_add_torrent_params* {
		lt::error_code ec;
		lt::load_torrent_limits lt_lim = ct::to_lt_load_limits(limits);
		lt::add_torrent_params atp = lt::read_resume_data(
			ct::span(buf), ec, lt_lim);
		if (ec) {
			ct::set_error(err, ec);
			return nullptr;
		}
		if (extra_out != nullptr) {
			// read_resume_data drops keys it does not know, so re-walk the
			// top level for the bindings' own key. The buffer already
			// bdecoded successfully above. The value was spliced verbatim
			// (preformatted), so hand back its raw section whatever its
			// bencode type.
			lt::error_code ignore;
			lt::bdecode_node root = lt::bdecode(ct::span(buf), ignore);
			if (!ignore && root.type() == lt::bdecode_node::dict_t) {
				lt::bdecode_node v = root.dict_find(rbt_data_key);
				if (v) {
					lt::span<char const> sec = v.data_section();
					if (!sec.empty()) {
						*extra_out = ct::box_buffer(
							std::vector<char>(sec.begin(), sec.end()));
					}
				}
			}
		}
		try {
			return ct::box_atp(std::move(atp));
		} catch (...) {
			if (extra_out != nullptr) ct_buf_free(extra_out);
			throw;
		}
	});
}

} // extern "C"
