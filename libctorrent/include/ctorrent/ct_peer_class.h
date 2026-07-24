/* Copyright (C) 2026  namazso <admin@namazso.eu>
 * SPDX-License-Identifier: MPL-2.0
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/.
 */

#ifndef CT_PEER_CLASS_H
#define CT_PEER_CLASS_H

#include <ctorrent/ct_types.h>
#include <stdint.h>
#include <stdbool.h>

#ifdef __cplusplus
extern "C" {
#endif

// Opaque handle to peer_class_type_filter
typedef struct ct_peer_class_type_filter ct_peer_class_type_filter;

// peer_class_t is a 32-bit identifier (libtorrent uses strong typedef)
typedef uint32_t ct_peer_class_t;

// Built-in peer class IDs
#define CT_GLOBAL_PEER_CLASS_ID 0
#define CT_TCP_PEER_CLASS_ID    1
#define CT_LOCAL_PEER_CLASS_ID  2

// Socket types for peer_class_type_filter
typedef enum ct_socket_type {
	CT_SOCKET_TCP = 0,
	CT_SOCKET_UTP,
	CT_SOCKET_SSL_TCP,
	CT_SOCKET_SSL_UTP,
	CT_SOCKET_I2P,
	CT_SOCKET_RTC,
	CT_SOCKET_NUM_TYPES
} ct_socket_type;

// Peer class info (settings for a peer class)
typedef struct ct_peer_class_info {
	bool ignore_unchoke_slots;
	int32_t connection_limit_factor;
	int32_t upload_limit;
	int32_t download_limit;
	int32_t upload_priority;
	int32_t download_priority;
	// Input-only (ct_session_set_peer_class): null-terminated class name,
	// owned by the caller; NULL leaves the name unchanged.
	// ct_session_get_peer_class sets this to NULL and returns the name
	// through its ct_str out-parameter instead.
	const char* label;
} ct_peer_class_info;

// peer_class_type_filter functions

ct_peer_class_type_filter* ct_peer_class_type_filter_new(ct_error* err);
void ct_peer_class_type_filter_free(ct_peer_class_type_filter* filter);

void ct_peer_class_type_filter_add(ct_peer_class_type_filter* filter,
	ct_socket_type st, ct_peer_class_t peer_class, ct_error* err);
void ct_peer_class_type_filter_remove(ct_peer_class_type_filter* filter,
	ct_socket_type st, ct_peer_class_t peer_class, ct_error* err);
void ct_peer_class_type_filter_disallow(ct_peer_class_type_filter* filter,
	ct_socket_type st, ct_peer_class_t peer_class, ct_error* err);
void ct_peer_class_type_filter_allow(ct_peer_class_type_filter* filter,
	ct_socket_type st, ct_peer_class_t peer_class, ct_error* err);

uint32_t ct_peer_class_type_filter_apply(const ct_peer_class_type_filter* filter,
	ct_socket_type st, uint32_t peer_class_mask, ct_error* err);

// The session-level peer class methods (create/delete/get/set and the
// filters) are declared in ct_session.h.

#ifdef __cplusplus
}
#endif

#endif // CT_PEER_CLASS_H
