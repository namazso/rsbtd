/* Copyright (C) 2026  namazso <admin@namazso.eu>
 * SPDX-License-Identifier: MPL-2.0
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/.
 */

/* Session lifecycle and session-wide operations. */
#ifndef CT_SESSION_H_INCLUDED
#define CT_SESSION_H_INCLUDED

#include "ct_add_torrent_params.h"
#include "ct_settings.h"
#include "ct_types.h"

struct ct_torrent_handle;

#ifdef __cplusplus
extern "C" {
#endif

/* Opaque owning lt::session. All operations are thread-safe (libtorrent
 * marshals to its network thread). */
typedef struct ct_session ct_session;

/* Opaque construction parameters for a session. */
typedef struct ct_session_params ct_session_params;

/* Built-in disk I/O backends for ct_session_params_set_disk_io. */
typedef enum ct_disk_io_backend {
  CT_DISK_IO_DEFAULT = 0,
  CT_DISK_IO_MMAP,
  CT_DISK_IO_POSIX,
  CT_DISK_IO_PREAD,
  CT_DISK_IO_DISABLED,
} ct_disk_io_backend;

/* Returns NULL on allocation failure. Starts with libtorrent's defaults:
 * default settings, the three default extensions enabled, default disk io. */
ct_session_params* ct_session_params_new(void);
void ct_session_params_free(ct_session_params* params);

/* Copies the pack into the params. */
void ct_session_params_set_settings(ct_session_params* params,
	const ct_settings_pack* pack, ct_error* err);

/* Enables/disables the built-in extensions (all enabled by default). */
void ct_session_params_set_default_extensions(ct_session_params* params,
	bool ut_metadata, bool ut_pex, bool smart_ban);

/* Selects a built-in disk I/O backend. Returns false if this build of
 * libtorrent does not provide the requested backend. */
bool ct_session_params_set_disk_io(ct_session_params* params,
	int32_t backend);

/* Starts the session in paused state. */
void ct_session_params_set_paused(ct_session_params* params, bool paused);

/* ---- lifecycle --------------------------------------------------------- */

/* Spawns the network thread. `params` is copied and stays owned by the
 * caller. Returns NULL and fills *err on failure. */
ct_session* ct_session_new(const ct_session_params* params, ct_error* err);

/* Storage for an lt::session_proxy (masquerade; treat as opaque). Movable
 * by memcpy, never duplicate. Bytewise relocatability is an accepted extra
 * platform requirement; see abi_asserts.cpp ("masquerade relocatability"). */
typedef struct ct_session_proxy {
  CT_ALIGNAS(CT_ALIGNOF_LT_SESSION_PROXY)
  uint8_t data_[CT_SIZEOF_LT_SESSION_PROXY];
} ct_session_proxy;

/* Non-blocking teardown protocol (in this exact order):
 *   ct_session_abort(s, &proxy);   // initiate shutdown, returns at once
 *   ct_session_free(s);            // destroy session object, no blocking
 *   ct_session_proxy_drop(&proxy); // BLOCKS until the session is torn down
 * Calling ct_session_free without abort blocks in place instead. */
void ct_session_abort(ct_session* session, ct_session_proxy* out_proxy);
void ct_session_free(ct_session* session);
void ct_session_proxy_drop(ct_session_proxy* proxy);

/* ---- settings ----------------------------------------------------------- */

void ct_session_apply_settings(ct_session* session,
	const ct_settings_pack* pack, ct_error* err);

/* The session's full effective settings (every key present). NULL on
 * failure. */
ct_settings_pack* ct_session_get_settings(const ct_session* session,
	ct_error* err);

/* ---- state persistence --------------------------------------------------- */

/* Bits of lt::save_state_flags_t (verified by static_assert). */
#define CT_SAVE_SETTINGS (1u << 0)
#define CT_SAVE_DHT_STATE (1u << 2)
#define CT_SAVE_EXTENSION_STATE (1u << 11)
#define CT_SAVE_IP_FILTER (1u << 12)
#define CT_SAVE_ALL 0xffffffffu

/* Serializes the session state (settings, DHT state, ...) as a bencoded
 * buffer suitable for ct_session_params_load_state. Briefly blocking. */
ct_buf ct_session_get_state(const ct_session* session, uint32_t save_flags,
	ct_error* err);

/* Restores a previously saved state into session params, replacing exactly
 * the fields selected by save_flags (settings, DHT state, extension state,
 * ip filter). Everything else - extension toggles, disk io selection,
 * session flags, unselected fields - is preserved. A selected field is
 * replaced even if the blob does not contain it (it resets to the
 * default), so pass the flags the state was saved with. */
void ct_session_params_load_state(ct_session_params* params,
	ct_span bencoded, uint32_t save_flags, ct_error* err);

/* ---- stats ---------------------------------------------------------------- */

/* Triggers posting of a session_stats_alert (not subject to the alert
 * mask). */
void ct_session_post_session_stats(ct_session* session, ct_error* err);

/* Triggers posting of a state_update_alert containing status snapshots for
 * all torrents. The flags parameter controls which fields are populated. */
void ct_session_post_torrent_updates(ct_session* session, uint32_t flags, ct_error* err);

/* ---- torrent management --------------------------------------------------- */

/* Asynchronously adds a torrent to the session. The parameters are copied; the
 * caller retains ownership of *params*. The *userdata* token is echoed in the
 * resulting add_torrent_alert and (if the add fails or the torrent is later
 * removed) torrent_removed_alert, allowing futures to correlate responses.
 * Returns immediately; poll the alert queue for the result. */
void ct_session_async_add_torrent(
  ct_session* session,
  const ct_add_torrent_params* params,
  uint64_t userdata,
  ct_error* err
);

/* Remove flags for ct_session_remove_torrent. */
#define CT_REMOVE_DELETE_FILES    (1u << 0)  /* delete all files on disk */
#define CT_REMOVE_DELETE_PARTFILE (1u << 1)  /* delete .!ut part file */

/* Removes a torrent from the session. The torrent stops and all associated
 * state is cleaned up asynchronously; a torrent_removed_alert is posted when
 * complete. *flags* is a bitwise OR of CT_REMOVE_* values (0 = no deletion). */
void ct_session_remove_torrent(
  ct_session* session,
  const struct ct_torrent_handle* handle,
  uint32_t flags
);

/* Resume data: write an add_torrent_params to a bencoded buffer (typically for
 * saving to disk), or read it back. The buffer format is libtorrent's internal
 * resume-data encoding; use ct_load_torrent_* for .torrent files. */

/* Writes the add_torrent_params to a bencoded buffer. The returned ct_buf must
 * be freed with ct_buf_free. */
ct_buf ct_write_resume_data_buf(
  const ct_add_torrent_params* atp,
  ct_error* err
);

/* Reads an add_torrent_params from a bencoded buffer (e.g. one previously
 * written by ct_write_resume_data_buf). Returns NULL on error. The returned
 * handle must be freed with ct_atp_free. */
ct_add_torrent_params* ct_read_resume_data(
  ct_span buf,
  const ct_load_torrent_limits* limits,
  ct_error* err
);

/* As ct_write_resume_data_buf, additionally splicing *extra* verbatim as
 * the value of the top-level "rbt-data" key. *extra* must be exactly one
 * well-formed bencode value (validated; a malformed blob is an error, not
 * a corrupt file). extra.len == 0 writes no key (identical output to
 * ct_write_resume_data_buf). libtorrent ignores the key when the buffer
 * is read back. */
ct_buf ct_write_resume_data_buf_ex(
  const ct_add_torrent_params* atp,
  ct_span extra,
  ct_error* err
);

/* As ct_read_resume_data, additionally copying the raw bytes of the
 * top-level "rbt-data" value into *extra_out (free with ct_buf_free).
 * *extra_out is zeroed on error and when the key is absent. extra_out may
 * be NULL to skip extraction. */
ct_add_torrent_params* ct_read_resume_data_ex(
  ct_span buf,
  const ct_load_torrent_limits* limits,
  ct_buf* extra_out,
  ct_error* err
);

/* ---- session queries ------------------------------------------------------ */

uint16_t ct_session_listen_port(const ct_session* session, ct_error* err);
uint16_t ct_session_ssl_listen_port(const ct_session* session, ct_error* err);
bool ct_session_is_listening(const ct_session* session, ct_error* err);
bool ct_session_is_paused(const ct_session* session, ct_error* err);
void ct_session_pause(ct_session* session, ct_error* err);
void ct_session_resume(ct_session* session, ct_error* err);
bool ct_session_is_dht_running(const ct_session* session, ct_error* err);

/* ---- ip and port filtering ------------------------------------------------ */

struct ct_ip_filter;
struct ct_port_filter;

/* Set the ip_filter for the session. The filter is copied. */
void ct_session_set_ip_filter(ct_session* session,
	const struct ct_ip_filter* filter, ct_error* err);

/* Get a copy of the current ip_filter. Caller must free with ct_ip_filter_free(). */
struct ct_ip_filter* ct_session_get_ip_filter(const ct_session* session,
	ct_error* err);

/* Set the port_filter for the session. The filter is copied. */
void ct_session_set_port_filter(ct_session* session,
	const struct ct_port_filter* filter, ct_error* err);

/* ---- peer classes --------------------------------------------------------- */

struct ct_peer_class_info;
struct ct_peer_class_type_filter;

/* peer_class_t is a 32-bit identifier */
typedef uint32_t ct_peer_class_t;

/* Creates a new peer class with the given name. Returns the peer class ID. */
ct_peer_class_t ct_session_create_peer_class(ct_session* session,
	const char* name, ct_error* err);

/* Deletes a peer class by ID. */
void ct_session_delete_peer_class(ct_session* session,
	ct_peer_class_t cid, ct_error* err);

/* Gets the settings for a peer class. out->label is left NULL (that field is
   an input for ct_session_set_peer_class); the class name is returned as an
   owned string in *out_label (free with ct_str_free; may be NULL to skip). */
void ct_session_get_peer_class(ct_session* session, ct_peer_class_t cid,
	struct ct_peer_class_info* out, ct_str* out_label, ct_error* err);

/* Sets the settings for a peer class. The label is copied. */
void ct_session_set_peer_class(ct_session* session, ct_peer_class_t cid,
	const struct ct_peer_class_info* info, ct_error* err);

/* Set/get the peer class filter (ip_filter used to assign peer classes).
   Addresses matching a rule are assigned to the corresponding peer class. */
void ct_session_set_peer_class_filter(ct_session* session,
	const struct ct_ip_filter* filter, ct_error* err);
struct ct_ip_filter* ct_session_get_peer_class_filter(ct_session* session,
	ct_error* err);

/* Set/get the peer class type filter (socket type-based peer class assignment). */
void ct_session_set_peer_class_type_filter(ct_session* session,
	const struct ct_peer_class_type_filter* filter, ct_error* err);
struct ct_peer_class_type_filter* ct_session_get_peer_class_type_filter(
	ct_session* session, ct_error* err);

/* ---- port mapping and network -------------------------------------------- */

/* Port mapping protocols */
typedef enum ct_portmap_protocol {
	CT_PORTMAP_TCP = 1,
	CT_PORTMAP_UDP = 2
} ct_portmap_protocol;

/* Port mapping handle (returned by add_port_mapping) */
typedef int32_t ct_port_mapping_t;

/* Reopen network sockets flags */
#define CT_REOPEN_MAP_PORTS 1

/* Add a port mapping via UPnP/NAT-PMP. Returns array of port_mapping_t handles
   (one per active port mapper). Caller must free with ct_port_mapping_array_free(). */
ct_port_mapping_t* ct_session_add_port_mapping(ct_session* session,
	ct_portmap_protocol protocol, int32_t external_port, int32_t local_port,
	size_t* out_count, ct_error* err);

/* Free the array returned by ct_session_add_port_mapping */
void ct_port_mapping_array_free(ct_port_mapping_t* mappings);

/* Delete a port mapping */
void ct_session_delete_port_mapping(ct_session* session,
	ct_port_mapping_t handle, ct_error* err);

/* Reopen network sockets (e.g., after network change) */
void ct_session_reopen_network_sockets(ct_session* session,
	uint32_t options, ct_error* err);

#ifdef __cplusplus
}
#endif

#endif
