/* Copyright (C) 2026  namazso <admin@namazso.eu>
 * SPDX-License-Identifier: MPL-2.0
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/.
 */

/* Alerts. Alerts are owned by the session; pointers obtained from a batch
 * are valid only until the next ct_session_pop_alerts call on the same
 * session (any batch), matching libtorrent's contract. The same applies to
 * every view struct and pointer derived from an alert.
 *
 * Per-type access pattern: ct_alert_as_<name>(alert, &view) fills a plain
 * view struct and returns true iff the alert has that type. View string/
 * buffer members share the alert's batch lifetime.
 */
#ifndef CT_ALERTS_H_INCLUDED
#define CT_ALERTS_H_INCLUDED

#include "ct_alerts_generated.h"
#include "ct_session.h"
#include "ct_types.h"

#ifdef __cplusplus
extern "C" {
#endif

/* Borrowed polymorphic lt::alert. */
typedef struct ct_alert ct_alert;

/* Reusable container for popped alerts (an opaque std::vector<alert*>). */
typedef struct ct_alert_batch ct_alert_batch;

/* Owned snapshot types some accessors below return copies of. */
typedef struct ct_torrent_status ct_torrent_status;
typedef struct ct_peer_info ct_peer_info;

/* ---- alert categories (bits of lt::alert_category_t) ------------------- */

#define CT_ALERT_CAT_ERROR (1u << 0)
#define CT_ALERT_CAT_PEER (1u << 1)
#define CT_ALERT_CAT_PORT_MAPPING (1u << 2)
#define CT_ALERT_CAT_STORAGE (1u << 3)
#define CT_ALERT_CAT_TRACKER (1u << 4)
#define CT_ALERT_CAT_CONNECT (1u << 5)
#define CT_ALERT_CAT_STATUS (1u << 6)
#define CT_ALERT_CAT_IP_BLOCK (1u << 8)
#define CT_ALERT_CAT_PERFORMANCE_WARNING (1u << 9)
#define CT_ALERT_CAT_DHT (1u << 10)
#define CT_ALERT_CAT_STATS (1u << 11)
#define CT_ALERT_CAT_SESSION_LOG (1u << 13)
#define CT_ALERT_CAT_TORRENT_LOG (1u << 14)
#define CT_ALERT_CAT_PEER_LOG (1u << 15)
#define CT_ALERT_CAT_INCOMING_REQUEST (1u << 16)
#define CT_ALERT_CAT_DHT_LOG (1u << 17)
#define CT_ALERT_CAT_DHT_OPERATION (1u << 18)
#define CT_ALERT_CAT_PORT_MAPPING_LOG (1u << 19)
#define CT_ALERT_CAT_PICKER_LOG (1u << 20)
#define CT_ALERT_CAT_FILE_PROGRESS (1u << 21)
#define CT_ALERT_CAT_PIECE_PROGRESS (1u << 22)
#define CT_ALERT_CAT_UPLOAD (1u << 23)
#define CT_ALERT_CAT_BLOCK_PROGRESS (1u << 24)
#define CT_ALERT_CAT_ALL 0xffffffffu

/* ---- popping ------------------------------------------------------------ */

ct_alert_batch* ct_alert_batch_new(void);
void ct_alert_batch_free(ct_alert_batch* batch);

/* Fills `batch` with all pending alerts, transferring "until next pop"
 * validity to them. */
void ct_session_pop_alerts(ct_session* session, ct_alert_batch* batch,
	ct_error* err);

/* Blocks up to timeout_ms; true if an alert is pending. */
bool ct_session_wait_for_alert(ct_session* session, int64_t timeout_ms,
	ct_error* err);

/* Registers a callback invoked (from an internal libtorrent thread) when
 * the alert queue transitions empty -> non-empty; it will not be invoked
 * again until after the next pop. The callback must only wake another
 * thread - never block, and never call back into the session. Passing NULL
 * unregisters (synchronized with the queue: after return, no further calls
 * happen). Unregistering cannot fail, so err may be NULL in that case. */
void ct_session_set_alert_notify(ct_session* session,
	void (*callback)(void* userdata), void* userdata, ct_error* err);

size_t ct_alert_batch_len(const ct_alert_batch* batch);
const ct_alert* ct_alert_batch_get(const ct_alert_batch* batch, size_t i);

/* ---- base accessors ------------------------------------------------------ */

int32_t ct_alert_type(const ct_alert* alert);
uint32_t ct_alert_category(const ct_alert* alert);
/* Static storage (alert type name). */
ct_str_view ct_alert_what(const ct_alert* alert);
/* Human-readable message; owned string. */
void ct_alert_message(const ct_alert* alert, ct_str* out);
/* Microseconds since the Unix epoch. */
int64_t ct_alert_timestamp_us(const ct_alert* alert);

/* For torrent_alert-derived alerts: the handle of the affected torrent
 * (borrowed; clone to own with ct_torrent_handle_clone). NULL if the alert
 * is not torrent-related. Batch lifetime. Present at every ABI, so this is
 * the portable way to attribute an alert to a torrent (the handle may
 * already be invalid if the torrent was removed). */
const struct ct_torrent_handle* ct_alert_torrent_handle(const ct_alert* alert);

/* For torrent_alert-derived alerts: the torrent's name (or hash as text).
 * False if the alert is not torrent-related, or when the linked libtorrent
 * uses TORRENT_ABI_VERSION >= 4 (alerts no longer store the name there).
 * Batch lifetime. */
bool ct_alert_torrent_name(const ct_alert* alert, ct_str_view* out);

/* For tracker_alert-derived alerts: the tracker URL. False otherwise. */
bool ct_alert_tracker_url(const ct_alert* alert, ct_str_view* out);

/* For peer_alert-derived alerts: remote endpoint and peer id. False if not
 * peer-related or the peer is an i2p destination. */
bool ct_alert_peer_endpoint(const ct_alert* alert, ct_endpoint* out_ep,
	ct_sha1* out_pid);

/* ---- session / network views -------------------------------------------- */

typedef struct ct_listen_succeeded_view {
  ct_endpoint endpoint;
  int32_t socket_type; /* lt::socket_type_t */
} ct_listen_succeeded_view;
bool ct_alert_as_listen_succeeded(const ct_alert* alert,
	ct_listen_succeeded_view* out);

typedef struct ct_listen_failed_view {
  ct_str_view interface_name;
  ct_endpoint endpoint;
  ct_error error;
  int32_t operation; /* lt::operation_t */
  int32_t socket_type;
} ct_listen_failed_view;
bool ct_alert_as_listen_failed(const ct_alert* alert,
	ct_listen_failed_view* out);

typedef struct ct_external_ip_view {
  ct_endpoint address; /* port unused */
} ct_external_ip_view;
bool ct_alert_as_external_ip(const ct_alert* alert, ct_external_ip_view* out);

typedef struct ct_udp_error_view {
  ct_endpoint endpoint;
  int32_t operation;
  ct_error error;
} ct_udp_error_view;
bool ct_alert_as_udp_error(const ct_alert* alert, ct_udp_error_view* out);

typedef struct ct_session_stats_view {
  const int64_t* counters;
  size_t len;
} ct_session_stats_view;
bool ct_alert_as_session_stats(const ct_alert* alert,
	ct_session_stats_view* out);

typedef struct ct_session_error_view {
  ct_error error;
} ct_session_error_view;
bool ct_alert_as_session_error(const ct_alert* alert,
	ct_session_error_view* out);

typedef struct ct_alerts_dropped_view {
  /* bit i set = alerts of type i were dropped; 128 bits, LSB first */
  uint8_t dropped[16];
} ct_alerts_dropped_view;
bool ct_alert_as_alerts_dropped(const ct_alert* alert,
	ct_alerts_dropped_view* out);

typedef struct ct_incoming_connection_view {
  int32_t socket_type;
  ct_endpoint endpoint;
} ct_incoming_connection_view;
bool ct_alert_as_incoming_connection(const ct_alert* alert,
	ct_incoming_connection_view* out);

typedef struct ct_portmap_view {
  int32_t mapping;
  int32_t external_port;
  int32_t protocol;  /* lt::portmap_protocol */
  int32_t transport; /* lt::portmap_transport */
  ct_endpoint local_address;
} ct_portmap_view;
bool ct_alert_as_portmap(const ct_alert* alert, ct_portmap_view* out);

typedef struct ct_portmap_error_view {
  int32_t mapping;
  int32_t transport;
  ct_error error;
  ct_endpoint local_address;
} ct_portmap_error_view;
bool ct_alert_as_portmap_error(const ct_alert* alert,
	ct_portmap_error_view* out);

typedef struct ct_socks5_view {
  ct_error error;
  int32_t operation;
  ct_endpoint ip;
} ct_socks5_view;
bool ct_alert_as_socks5(const ct_alert* alert, ct_socks5_view* out);

typedef struct ct_i2p_view {
  ct_error error;
} ct_i2p_view;
bool ct_alert_as_i2p(const ct_alert* alert, ct_i2p_view* out);

typedef struct ct_lsd_error_view {
  ct_error error;
  ct_endpoint local_address; /* port unused */
} ct_lsd_error_view;
bool ct_alert_as_lsd_error(const ct_alert* alert, ct_lsd_error_view* out);

typedef struct ct_log_view {
  ct_str_view message;
} ct_log_view;
/* log_alert(79), torrent_log_alert(80), portmap_log_alert(52),
 * dht_log_alert(85: also fills module) */
bool ct_alert_as_log(const ct_alert* alert, ct_log_view* out);
bool ct_alert_as_torrent_log(const ct_alert* alert, ct_log_view* out);
bool ct_alert_as_portmap_log(const ct_alert* alert, ct_log_view* out);

typedef struct ct_dht_log_view {
  ct_str_view message;
  int32_t module; /* lt::dht_log_alert::dht_module_t */
} ct_dht_log_view;
bool ct_alert_as_dht_log(const ct_alert* alert, ct_dht_log_view* out);

typedef struct ct_peer_log_view {
  ct_str_view message;
  int32_t event_type; /* lt::peer_log_alert::event_t */
  int32_t direction;  /* lt::peer_log_alert::direction_t */
} ct_peer_log_view;
bool ct_alert_as_peer_log(const ct_alert* alert, ct_peer_log_view* out);

/* ---- torrent state views -------------------------------------------------- */

typedef struct ct_state_changed_view {
  int32_t state;      /* lt::torrent_status::state_t */
  int32_t prev_state;
} ct_state_changed_view;
bool ct_alert_as_state_changed(const ct_alert* alert,
	ct_state_changed_view* out);

typedef struct ct_torrent_error_view {
  ct_error error;
  ct_str_view filename;
} ct_torrent_error_view;
bool ct_alert_as_torrent_error(const ct_alert* alert,
	ct_torrent_error_view* out);

typedef struct ct_add_torrent_view {
  const struct ct_torrent_handle* handle; /* borrowed; clone to own */
  const ct_add_torrent_params* params;    /* borrowed; clone to own */
  ct_error error;
  void* userdata;
} ct_add_torrent_view;
bool ct_alert_as_add_torrent(const ct_alert* alert, ct_add_torrent_view* out);

typedef struct ct_torrent_removed_view {
  ct_info_hash info_hashes;
  void* userdata; /* client_data_t as stored by the bindings */
} ct_torrent_removed_view;
bool ct_alert_as_torrent_removed(const ct_alert* alert,
	ct_torrent_removed_view* out);

typedef struct ct_torrent_finished_view {
  /* no fields beyond the base torrent alert; empty structs are not valid
   * ISO C, so keep a placeholder */
  uint8_t _unused;
} ct_torrent_finished_view;
bool ct_alert_as_torrent_finished(const ct_alert* alert,
	ct_torrent_finished_view* out);

typedef struct ct_torrent_deleted_view {
  ct_info_hash info_hashes;
} ct_torrent_deleted_view;
bool ct_alert_as_torrent_deleted(const ct_alert* alert,
	ct_torrent_deleted_view* out);

typedef struct ct_torrent_delete_failed_view {
  ct_error error;
  ct_info_hash info_hashes;
} ct_torrent_delete_failed_view;
bool ct_alert_as_torrent_delete_failed(const ct_alert* alert,
	ct_torrent_delete_failed_view* out);

typedef struct ct_performance_view {
  int32_t warning_code; /* lt::performance_alert::performance_warning_t */
} ct_performance_view;
bool ct_alert_as_performance(const ct_alert* alert, ct_performance_view* out);

typedef struct ct_metadata_failed_view {
  ct_error error;
} ct_metadata_failed_view;
bool ct_alert_as_metadata_failed(const ct_alert* alert,
	ct_metadata_failed_view* out);

typedef struct ct_fastresume_rejected_view {
  ct_error error;
  ct_str_view file_path;
  int32_t operation;
} ct_fastresume_rejected_view;
bool ct_alert_as_fastresume_rejected(const ct_alert* alert,
	ct_fastresume_rejected_view* out);

/* save_resume_data_alert: the response to
 * ct_torrent_handle_save_resume_data. `params` is the resume data itself,
 * ready for ct_write_resume_data_buf or re-adding to a session. */
typedef struct ct_save_resume_data_view {
  const ct_add_torrent_params* params; /* borrowed; clone to own */
} ct_save_resume_data_view;
bool ct_alert_as_save_resume_data(const ct_alert* alert,
	ct_save_resume_data_view* out);

typedef struct ct_save_resume_data_failed_view {
  ct_error error;
} ct_save_resume_data_failed_view;
bool ct_alert_as_save_resume_data_failed(const ct_alert* alert,
	ct_save_resume_data_failed_view* out);

/* state_update_alert: the response to ct_session_post_torrent_updates /
 * ct_torrent_handle_post_status, one status snapshot per updated torrent.
 * The view only carries the count; each status is fetched as an owned
 * copy. */
typedef struct ct_state_update_view {
  size_t count;
} ct_state_update_view;
bool ct_alert_as_state_update(const ct_alert* alert,
	ct_state_update_view* out);

/* Returns an owned copy of status *i* (free with ct_torrent_status_free).
 * NULL if the alert is not a state_update_alert, i is out of range, or
 * allocation fails. */
ct_torrent_status* ct_alert_state_update_status(const ct_alert* alert,
	size_t i);

/* peer_info_alert: the response to ct_torrent_handle_post_peer_info. Same
 * count + owned-copy-per-index pattern as state_update. */
typedef struct ct_peer_info_list_view {
  size_t count;
} ct_peer_info_list_view;
bool ct_alert_as_peer_info(const ct_alert* alert,
	ct_peer_info_list_view* out);

/* Returns an owned copy of peer *i* (free with ct_peer_info_free). NULL if
 * the alert is not a peer_info_alert, i is out of range, or allocation
 * fails. */
ct_peer_info* ct_alert_peer_info(const ct_alert* alert, size_t i);

/* file_progress_alert: the response to
 * ct_torrent_handle_post_file_progress. */
typedef struct ct_file_progress_view {
  const int64_t* progress; /* bytes completed per file; batch lifetime */
  size_t len;
} ct_file_progress_view;
bool ct_alert_as_file_progress(const ct_alert* alert,
	ct_file_progress_view* out);

/* tracker_list_alert: the response to ct_torrent_handle_post_trackers. */
typedef struct ct_tracker_list_view {
  size_t count;
} ct_tracker_list_view;
bool ct_alert_as_tracker_list(const ct_alert* alert,
	ct_tracker_list_view* out);

typedef struct ct_tracker_list_entry {
  ct_str_view url;       /* batch lifetime */
  ct_str_view trackerid; /* batch lifetime */
  int32_t tier;
  int32_t fail_limit;    /* announce failures before giving up; 0=unlimited */
  uint32_t source;       /* lt::announce_entry::tracker_source bits */
  bool verified;         /* responded to an announce at least once */
} ct_tracker_list_entry;
/* Fills *out with tracker *i*; false if the alert is not a
 * tracker_list_alert or i is out of range. */
bool ct_alert_tracker_list_entry(const ct_alert* alert, size_t i,
	ct_tracker_list_entry* out);

/* ---- piece / block / file views ------------------------------------------ */

typedef struct ct_read_piece_view {
  ct_error error;
  const uint8_t* buffer; /* NULL on failure; batch lifetime */
  ct_piece_index piece;
  int32_t size;
} ct_read_piece_view;
bool ct_alert_as_read_piece(const ct_alert* alert, ct_read_piece_view* out);

typedef struct ct_piece_finished_view {
  ct_piece_index piece_index;
} ct_piece_finished_view;
bool ct_alert_as_piece_finished(const ct_alert* alert,
	ct_piece_finished_view* out);

typedef struct ct_hash_failed_view {
  ct_piece_index piece_index;
} ct_hash_failed_view;
bool ct_alert_as_hash_failed(const ct_alert* alert, ct_hash_failed_view* out);

/* Shared by block_finished(30), block_downloading(31), block_timeout(29),
 * unwanted_block(32), request_dropped(28), block_uploaded(94). */
typedef struct ct_block_view {
  int32_t block_index;
  ct_piece_index piece_index;
} ct_block_view;
bool ct_alert_as_block(const ct_alert* alert, ct_block_view* out);

typedef struct ct_invalid_request_view {
  ct_peer_request request;
  bool we_have;
  bool peer_interested;
  bool withheld;
} ct_invalid_request_view;
bool ct_alert_as_invalid_request(const ct_alert* alert,
	ct_invalid_request_view* out);

typedef struct ct_incoming_request_view {
  ct_peer_request request;
} ct_incoming_request_view;
bool ct_alert_as_incoming_request(const ct_alert* alert,
	ct_incoming_request_view* out);

typedef struct ct_file_completed_view {
  ct_file_index index;
} ct_file_completed_view;
bool ct_alert_as_file_completed(const ct_alert* alert,
	ct_file_completed_view* out);

typedef struct ct_file_renamed_view {
  ct_file_index index;
  ct_str_view new_name;
  ct_str_view old_name;
} ct_file_renamed_view;
bool ct_alert_as_file_renamed(const ct_alert* alert,
	ct_file_renamed_view* out);

typedef struct ct_file_rename_failed_view {
  ct_file_index index;
  ct_error error;
} ct_file_rename_failed_view;
bool ct_alert_as_file_rename_failed(const ct_alert* alert,
	ct_file_rename_failed_view* out);

typedef struct ct_file_error_view {
  ct_error error;
  ct_str_view filename;
  int32_t operation;
} ct_file_error_view;
bool ct_alert_as_file_error(const ct_alert* alert, ct_file_error_view* out);

typedef struct ct_file_prio_view {
  /* no fields beyond the base torrent alert: upstream only posts this
   * alert on success and never initializes its op member, so there is
   * no payload worth exposing. Empty structs are not valid ISO C, so
   * keep a placeholder */
  uint8_t _unused;
} ct_file_prio_view;
bool ct_alert_as_file_prio(const ct_alert* alert, ct_file_prio_view* out);

typedef struct ct_storage_moved_view {
  ct_str_view storage_path;
  ct_str_view old_path;
} ct_storage_moved_view;
bool ct_alert_as_storage_moved(const ct_alert* alert,
	ct_storage_moved_view* out);

typedef struct ct_storage_moved_failed_view {
  ct_error error;
  ct_str_view file_path;
  int32_t operation;
} ct_storage_moved_failed_view;
bool ct_alert_as_storage_moved_failed(const ct_alert* alert,
	ct_storage_moved_failed_view* out);

/* ---- peer views ------------------------------------------------------------ */

typedef struct ct_peer_connect_view {
  int32_t direction; /* lt::peer_connect_alert::direction_t */
  int32_t socket_type;
} ct_peer_connect_view;
bool ct_alert_as_peer_connect(const ct_alert* alert,
	ct_peer_connect_view* out);

typedef struct ct_peer_disconnected_view {
  int32_t socket_type;
  int32_t operation;
  ct_error error;
  int32_t reason; /* lt::close_reason_t */
} ct_peer_disconnected_view;
bool ct_alert_as_peer_disconnected(const ct_alert* alert,
	ct_peer_disconnected_view* out);

typedef struct ct_peer_error_view {
  int32_t operation;
  ct_error error;
} ct_peer_error_view;
bool ct_alert_as_peer_error(const ct_alert* alert, ct_peer_error_view* out);

typedef struct ct_peer_blocked_view {
  int32_t reason; /* lt::peer_blocked_alert::reason_t */
} ct_peer_blocked_view;
bool ct_alert_as_peer_blocked(const ct_alert* alert,
	ct_peer_blocked_view* out);

/* ---- tracker views --------------------------------------------------------- */

typedef struct ct_tracker_error_view {
  int32_t times_in_row;
  ct_error error;
  int32_t operation;
  ct_str_view failure_reason;
  int32_t version; /* lt::protocol_version */
} ct_tracker_error_view;
bool ct_alert_as_tracker_error(const ct_alert* alert,
	ct_tracker_error_view* out);

typedef struct ct_tracker_warning_view {
  ct_str_view warning_message;
} ct_tracker_warning_view;
bool ct_alert_as_tracker_warning(const ct_alert* alert,
	ct_tracker_warning_view* out);

typedef struct ct_tracker_reply_view {
  int32_t num_peers;
  int32_t version;
} ct_tracker_reply_view;
bool ct_alert_as_tracker_reply(const ct_alert* alert,
	ct_tracker_reply_view* out);

typedef struct ct_tracker_announce_view {
  int32_t event; /* lt::event_t */
  int32_t version;
} ct_tracker_announce_view;
bool ct_alert_as_tracker_announce(const ct_alert* alert,
	ct_tracker_announce_view* out);

typedef struct ct_scrape_reply_view {
  int32_t incomplete;
  int32_t complete;
} ct_scrape_reply_view;
bool ct_alert_as_scrape_reply(const ct_alert* alert,
	ct_scrape_reply_view* out);

typedef struct ct_scrape_failed_view {
  ct_error error;
  ct_str_view error_message;
} ct_scrape_failed_view;
bool ct_alert_as_scrape_failed(const ct_alert* alert,
	ct_scrape_failed_view* out);

typedef struct ct_dht_reply_view {
  int32_t num_peers;
} ct_dht_reply_view;
bool ct_alert_as_dht_reply(const ct_alert* alert, ct_dht_reply_view* out);

typedef struct ct_trackerid_view {
  ct_str_view trackerid;
} ct_trackerid_view;
bool ct_alert_as_trackerid(const ct_alert* alert, ct_trackerid_view* out);

typedef struct ct_url_seed_view {
  ct_str_view server_url;
  ct_str_view error_message;
  ct_error error;
} ct_url_seed_view;
bool ct_alert_as_url_seed(const ct_alert* alert, ct_url_seed_view* out);

/* ---- DHT views -------------------------------------------------------------- */

typedef struct ct_dht_announce_view {
  ct_endpoint ip; /* port = announced port */
  ct_sha1 info_hash;
} ct_dht_announce_view;
bool ct_alert_as_dht_announce(const ct_alert* alert,
	ct_dht_announce_view* out);

typedef struct ct_dht_get_peers_view {
  ct_sha1 info_hash;
} ct_dht_get_peers_view;
bool ct_alert_as_dht_get_peers(const ct_alert* alert,
	ct_dht_get_peers_view* out);

typedef struct ct_dht_error_view {
  ct_error error;
  int32_t operation;
} ct_dht_error_view;
bool ct_alert_as_dht_error(const ct_alert* alert, ct_dht_error_view* out);

typedef struct ct_dht_put_view {
  ct_sha1 target;
  uint8_t public_key[32];
  uint8_t signature[64];
  ct_str_view salt;
  int64_t seq;
  int32_t num_success;
} ct_dht_put_view;
bool ct_alert_as_dht_put(const ct_alert* alert, ct_dht_put_view* out);

typedef struct ct_dht_outgoing_get_peers_view {
  ct_sha1 info_hash;
  ct_sha1 obfuscated_info_hash;
  ct_endpoint endpoint;
} ct_dht_outgoing_get_peers_view;
bool ct_alert_as_dht_outgoing_get_peers(const ct_alert* alert,
	ct_dht_outgoing_get_peers_view* out);

typedef struct ct_dht_pkt_view {
  ct_span packet;
  int32_t direction; /* lt::dht_pkt_alert::direction_t */
  ct_endpoint node;
} ct_dht_pkt_view;
bool ct_alert_as_dht_pkt(const ct_alert* alert, ct_dht_pkt_view* out);

#ifdef __cplusplus
}
#endif

#endif
