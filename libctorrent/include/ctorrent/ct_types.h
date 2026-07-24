/* Copyright (C) 2026  namazso <admin@namazso.eu>
 * SPDX-License-Identifier: MPL-2.0
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/.
 */

/* Core value types shared by every section: strings, buffers, errors,
 * hashes, indices, endpoints, and version queries.
 */
#ifndef CT_TYPES_H_INCLUDED
#define CT_TYPES_H_INCLUDED

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#include "ct_abi.h"

#ifdef __cplusplus
extern "C" {
#endif

/* ---- strings and buffers ---------------------------------------------- */

/* Borrowed, non-owning view of a byte string. Not NUL-terminated by
 * contract; the producing function states the lifetime. */
typedef struct ct_str_view {
  const char* ptr;
  size_t len;
} ct_str_view;

/* Owning string. ptr[0..len) is valid until ct_str_free. A zeroed ct_str is
 * valid and empty. box_ is an implementation detail; never touch it. */
typedef struct ct_str {
  const char* ptr;
  size_t len;
  void* box_;
} ct_str;

void ct_str_free(ct_str* s);

/* Borrowed, non-owning view of a binary buffer (input parameter). */
typedef struct ct_span {
  const uint8_t* ptr;
  size_t len;
} ct_span;

/* Owning binary buffer (e.g. bencoded output). A zeroed ct_buf is valid and
 * empty. */
typedef struct ct_buf {
  const uint8_t* ptr;
  size_t len;
  void* box_;
} ct_buf;

void ct_buf_free(ct_buf* b);

/* Owning list of strings (a boxed std::vector<std::string>). */
typedef struct ct_str_list ct_str_list;

size_t ct_str_list_len(const ct_str_list* list);

/* Borrowed from the list; valid until the list is freed. Out-of-range
 * indices return an empty view. */
ct_str_view ct_str_list_get(const ct_str_list* list, size_t i);

void ct_str_list_free(ct_str_list* list);

/* Borrowed view of a bit vector (lt::bitfield layout): bit i is
 * bytes[i / 8] & (0x80 >> (i % 8)). bytes may be NULL when num_bits is 0. */
typedef struct ct_bitfield_view {
  const uint8_t* bytes;
  int32_t num_bits;
} ct_bitfield_view;

/* ---- errors ------------------------------------------------------------ */

/* Identity of the boost::system error category an error belongs to. */
typedef enum ct_error_category {
  CT_ERROR_CAT_NONE = 0,
  CT_ERROR_CAT_GENERIC,
  CT_ERROR_CAT_SYSTEM,
  CT_ERROR_CAT_LIBTORRENT,
  CT_ERROR_CAT_HTTP,
  CT_ERROR_CAT_BDECODE,
  CT_ERROR_CAT_GZIP,
  CT_ERROR_CAT_SOCKS,
  CT_ERROR_CAT_UPNP,
  CT_ERROR_CAT_PCP,
  CT_ERROR_CAT_I2P,
  CT_ERROR_CAT_UNKNOWN,
} ct_error_category;

/* Out-parameter filled by every fallible function. category is a
 * ct_error_category value; CT_ERROR_CAT_NONE means success. category_ptr is
 * the underlying boost category (a stable static) enabling exact identity
 * and message lookup; it may be NULL for errors carried by an exception
 * without an error_code.
 *
 * THREAD SAFETY: ct_error_message must be called on the same thread that
 * produced the error, and before any other libctorrent call on that thread.
 * For exception-without-error_code errors (category_ptr == NULL), the message
 * is stored in thread-local storage and will be lost if the error crosses
 * threads or if another shim call overwrites it. Errors with a category_ptr
 * (most errors from libtorrent) are safe to move across threads. */
typedef struct ct_error {
  int32_t value;
  int32_t category;
  const void* category_ptr;
} ct_error;

/* Human-readable message for an error. Returns an owned string. */
void ct_error_message(const ct_error* err, ct_str* out);

/* Name of the error's category (static storage), e.g. "libtorrent". */
ct_str_view ct_error_category_name(const ct_error* err);

/* ---- hashes ------------------------------------------------------------ */

/* Layout-compatible with lt::sha1_hash / lt::sha256_hash / lt::info_hash_t
 * (verified by static_assert). Byte order matches libtorrent's to_string(). */
typedef struct ct_sha1 {
  uint8_t data[20];
} ct_sha1;

typedef struct ct_sha256 {
  uint8_t data[32];
} ct_sha256;

/* v1 and/or v2 info-hash; an all-zero hash means "absent". */
typedef struct ct_info_hash {
  ct_sha1 v1;
  ct_sha256 v2;
} ct_info_hash;

/* ---- small value types -------------------------------------------------- */

/* Strong index typedefs (lt::piece_index_t / lt::file_index_t). */
typedef int32_t ct_piece_index;
typedef int32_t ct_file_index;

/* 0 = don't download, 1 = lowest, 4 = default, 7 = highest. */
typedef uint8_t ct_download_priority;

/* Layout-compatible with lt::peer_request. */
typedef struct ct_peer_request {
  ct_piece_index piece;
  int32_t start;
  int32_t length;
} ct_peer_request;

/* IP endpoint, converted at the boundary (never a masquerade of asio
 * types). For v4 endpoints addr[0..4) holds the address; addr is in network
 * byte order. scope_id is the v6 scope (link-local zone index), 0 for v4. */
typedef struct ct_endpoint {
  uint8_t addr[16];
  uint32_t scope_id;
  uint16_t port;
  uint8_t is_v6;
} ct_endpoint;

/* ---- version ------------------------------------------------------------ */

/* Version string of the libtorrent shared library actually loaded, e.g.
 * "2.1.0.0". Static storage. */
ct_str_view ct_libtorrent_version(void);

/* LIBTORRENT_VERSION_NUM of the headers the shim was built against. */
uint32_t ct_libtorrent_version_num(void);

/* TORRENT_ABI_VERSION the shim (and thus the linked libtorrent) uses. */
uint32_t ct_libtorrent_abi_version(void);

#ifdef __cplusplus
}
#endif

#endif
