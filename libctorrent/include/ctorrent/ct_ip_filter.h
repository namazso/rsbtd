/* Copyright (C) 2026  namazso <admin@namazso.eu>
 * SPDX-License-Identifier: MPL-2.0
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/.
 */

#ifndef CT_IP_FILTER_H
#define CT_IP_FILTER_H

#include <ctorrent/ct_types.h>
#include <stdint.h>
#include <stdbool.h>

#ifdef __cplusplus
extern "C" {
#endif

// Opaque handle to ip_filter
typedef struct ct_ip_filter ct_ip_filter;

// Opaque handle to port_filter
typedef struct ct_port_filter ct_port_filter;

// Access flags for ip_filter and port_filter
#define CT_IP_FILTER_BLOCKED 1
#define CT_PORT_FILTER_BLOCKED 1

// IP address representation (POD)
typedef struct ct_address {
	uint8_t bytes[16];  // IPv4 or IPv6 bytes
	uint16_t port;      // unused for ip_filter, kept for alignment with ct_endpoint
	uint8_t is_v6;      // 0 = IPv4, 1 = IPv6
	uint8_t _pad[5];
} ct_address;

// IP range result (for export_filter)
typedef struct ct_ip_range {
	ct_address first;
	ct_address last;
	uint32_t flags;
} ct_ip_range;

// ip_filter functions

// Create a new ip_filter (default: allows all addresses)
ct_ip_filter* ct_ip_filter_new(ct_error* err);

void ct_ip_filter_free(ct_ip_filter* filter);

// Returns true if the filter does not contain any rules
bool ct_ip_filter_empty(const ct_ip_filter* filter, ct_error* err);

// Add a rule to the filter
// first and last must be the same IP version (both v4 or both v6)
// flags: 0 = allowed, CT_IP_FILTER_BLOCKED = blocked
void ct_ip_filter_add_rule(ct_ip_filter* filter,
	const ct_address* first, const ct_address* last,
	uint32_t flags, ct_error* err);

// Query access permissions for an address
// Returns 0 (allowed) or CT_IP_FILTER_BLOCKED
uint32_t ct_ip_filter_access(const ct_ip_filter* filter,
	const ct_address* addr, ct_error* err);

// Export filter state as minimal set of ranges
// Returns an array of ct_ip_range; caller must free with ct_ip_filter_export_free()
// out_len receives the number of ranges (combined IPv4 + IPv6)
ct_ip_range* ct_ip_filter_export(const ct_ip_filter* filter,
	size_t* out_len, ct_error* err);

// Free the array returned by ct_ip_filter_export
void ct_ip_filter_export_free(ct_ip_range* ranges);

// port_filter functions

// Create a new port_filter (default: allows all ports)
ct_port_filter* ct_port_filter_new(ct_error* err);

void ct_port_filter_free(ct_port_filter* filter);

// Add a rule to the port_filter
// first and last define inclusive port range
// flags: 0 = allowed, CT_PORT_FILTER_BLOCKED = blocked
void ct_port_filter_add_rule(ct_port_filter* filter,
	uint16_t first, uint16_t last,
	uint32_t flags, ct_error* err);

// Query access permissions for a port
// Returns 0 (allowed) or CT_PORT_FILTER_BLOCKED
uint32_t ct_port_filter_access(const ct_port_filter* filter,
	uint16_t port, ct_error* err);

// Applying filters to a session (ct_session_set_ip_filter,
// ct_session_set_port_filter) is declared in ct_session.h.

#ifdef __cplusplus
}
#endif

#endif // CT_IP_FILTER_H
