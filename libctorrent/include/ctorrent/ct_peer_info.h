/* Copyright (C) 2026  namazso <admin@namazso.eu>
 * SPDX-License-Identifier: MPL-2.0
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/.
 */

/* Peer info API (lt::peer_info snapshot). */
#ifndef CTORRENT_PEER_INFO_H
#define CTORRENT_PEER_INFO_H

#include "ct_types.h"

#ifdef __cplusplus
extern "C" {
#endif

/* Opaque handle to an owned peer_info snapshot. */
typedef struct ct_peer_info ct_peer_info;

/* Peer flags (bitfield). */
typedef uint32_t ct_peer_flags_t;

/* Peer source flags (bitfield). */
typedef uint8_t ct_peer_source_flags_t;

/* Connection type. */
typedef uint8_t ct_connection_type_t;

/* Peer flag bits. */
#define CT_PEER_INTERESTING          (1u << 0)
#define CT_PEER_CHOKED               (1u << 1)
#define CT_PEER_REMOTE_INTERESTED    (1u << 2)
#define CT_PEER_REMOTE_CHOKED        (1u << 3)
#define CT_PEER_SUPPORTS_EXTENSIONS  (1u << 4)
#define CT_PEER_OUTGOING_CONNECTION  (1u << 5)
#define CT_PEER_HANDSHAKE            (1u << 6)
#define CT_PEER_CONNECTING           (1u << 7)
#define CT_PEER_ON_PAROLE            (1u << 9)
#define CT_PEER_SEED                 (1u << 10)
#define CT_PEER_OPTIMISTIC_UNCHOKE   (1u << 11)
#define CT_PEER_SNUBBED              (1u << 12)
#define CT_PEER_UPLOAD_ONLY          (1u << 13)
#define CT_PEER_ENDGAME_MODE         (1u << 14)
#define CT_PEER_HOLEPUNCHED          (1u << 15)
#define CT_PEER_I2P_SOCKET           (1u << 16)
#define CT_PEER_UTP_SOCKET           (1u << 17)
#define CT_PEER_SSL_SOCKET           (1u << 18)
#define CT_PEER_RC4_ENCRYPTED        (1u << 19)
#define CT_PEER_PLAINTEXT_ENCRYPTED  (1u << 20)

/* Peer source bits. */
#define CT_PEER_SOURCE_TRACKER      (1u << 0)
#define CT_PEER_SOURCE_DHT          (1u << 1)
#define CT_PEER_SOURCE_PEX          (1u << 2)
#define CT_PEER_SOURCE_LSD          (1u << 3)
#define CT_PEER_SOURCE_RESUME_DATA  (1u << 4)
#define CT_PEER_SOURCE_INCOMING     (1u << 5)

/* Connection type. */
#define CT_CONNECTION_STANDARD_BITTORRENT  (1u << 0)
#define CT_CONNECTION_WEB_SEED             (1u << 1)
#define CT_CONNECTION_HTTP_SEED            (1u << 2)

/* ---- Lifecycle ----------------------------------------------------------- */

void ct_peer_info_free(ct_peer_info* pi);

/* ---- Accessors ----------------------------------------------------------- */

/* Client identification string (borrowed, UTF-8). */
ct_str_view ct_peer_info_client(const ct_peer_info* pi);

/* Total bytes downloaded from and uploaded to this peer (payload only). */
int64_t ct_peer_info_total_download(const ct_peer_info* pi);
int64_t ct_peer_info_total_upload(const ct_peer_info* pi);

/* Time since last request and last activity (microseconds). */
int64_t ct_peer_info_last_request_us(const ct_peer_info* pi);
int64_t ct_peer_info_last_active_us(const ct_peer_info* pi);

/* Estimated time until download queue is empty (microseconds). */
int64_t ct_peer_info_download_queue_time_us(const ct_peer_info* pi);

/* Peer flags (combination of CT_PEER_* bits). */
ct_peer_flags_t ct_peer_info_flags(const ct_peer_info* pi);

/* Peer source flags (combination of CT_PEER_SOURCE_* bits). */
ct_peer_source_flags_t ct_peer_info_source(const ct_peer_info* pi);

/* Current transfer rates (bytes per second, including protocol overhead). */
int32_t ct_peer_info_up_speed(const ct_peer_info* pi);
int32_t ct_peer_info_down_speed(const ct_peer_info* pi);

/* Payload-only transfer rates (bytes per second). */
int32_t ct_peer_info_payload_up_speed(const ct_peer_info* pi);
int32_t ct_peer_info_payload_down_speed(const ct_peer_info* pi);

/* Peer ID (20 bytes). */
void ct_peer_info_pid(const ct_peer_info* pi, uint8_t out_pid[20]);

/* Queue and buffer statistics. */
int32_t ct_peer_info_queue_bytes(const ct_peer_info* pi);
int32_t ct_peer_info_request_timeout(const ct_peer_info* pi);
int32_t ct_peer_info_send_buffer_size(const ct_peer_info* pi);
int32_t ct_peer_info_used_send_buffer(const ct_peer_info* pi);
int32_t ct_peer_info_receive_buffer_size(const ct_peer_info* pi);
int32_t ct_peer_info_used_receive_buffer(const ct_peer_info* pi);
int32_t ct_peer_info_receive_buffer_watermark(const ct_peer_info* pi);

/* Failure and queue counts. */
int32_t ct_peer_info_num_hashfails(const ct_peer_info* pi);
int32_t ct_peer_info_download_queue_length(const ct_peer_info* pi);
int32_t ct_peer_info_timed_out_requests(const ct_peer_info* pi);
int32_t ct_peer_info_busy_requests(const ct_peer_info* pi);
int32_t ct_peer_info_requests_in_buffer(const ct_peer_info* pi);
int32_t ct_peer_info_target_dl_queue_length(const ct_peer_info* pi);
int32_t ct_peer_info_upload_queue_length(const ct_peer_info* pi);
int32_t ct_peer_info_failcount(const ct_peer_info* pi);

/* Currently downloading piece/block info. */
int32_t ct_peer_info_downloading_piece_index(const ct_peer_info* pi);
int32_t ct_peer_info_downloading_block_index(const ct_peer_info* pi);
int32_t ct_peer_info_downloading_progress(const ct_peer_info* pi);
int32_t ct_peer_info_downloading_total(const ct_peer_info* pi);

/* Connection type (CT_CONNECTION_* bits). */
ct_connection_type_t ct_peer_info_connection_type(const ct_peer_info* pi);

/* Disk I/O statistics. */
int32_t ct_peer_info_pending_disk_bytes(const ct_peer_info* pi);
int32_t ct_peer_info_pending_disk_read_bytes(const ct_peer_info* pi);

/* Bandwidth quota. */
int32_t ct_peer_info_send_quota(const ct_peer_info* pi);
int32_t ct_peer_info_receive_quota(const ct_peer_info* pi);

/* Round-trip time estimate (milliseconds, 0 for incoming). */
int32_t ct_peer_info_rtt(const ct_peer_info* pi);

/* Number of pieces this peer has. */
int32_t ct_peer_info_num_pieces(const ct_peer_info* pi);

/* Peak transfer rates (bytes per second). */
int32_t ct_peer_info_download_rate_peak(const ct_peer_info* pi);
int32_t ct_peer_info_upload_rate_peak(const ct_peer_info* pi);

/* Progress (0.0 to 1.0 and ppm 0 to 1000000). */
float ct_peer_info_progress(const ct_peer_info* pi);
int32_t ct_peer_info_progress_ppm(const ct_peer_info* pi);

/* Remote and local endpoints (IP:port). Returns false if not available (e.g. i2p). */
bool ct_peer_info_remote_endpoint(const ct_peer_info* pi, ct_endpoint* out);
bool ct_peer_info_local_endpoint(const ct_peer_info* pi, ct_endpoint* out);

/* Hash of the peer's i2p destination. Returns false unless this is an i2p
 * peer (and libtorrent was built with i2p support). */
bool ct_peer_info_i2p_destination(const ct_peer_info* pi, ct_sha256* out);

/* Piece bitfield (NULL if not available; len = num_pieces in torrent, owned by peer_info). */
const uint8_t* ct_peer_info_pieces(const ct_peer_info* pi, size_t* out_len);

#ifdef __cplusplus
}
#endif

#endif /* CTORRENT_PEER_INFO_H */
