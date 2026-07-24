/* Copyright (C) 2026  namazso <admin@namazso.eu>
 * SPDX-License-Identifier: MPL-2.0
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/.
 */

#ifndef CT_STATS_H
#define CT_STATS_H

#include <ctorrent/ct_types.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef enum ct_metric_type {
	CT_METRIC_COUNTER = 0,  // Monotonically increasing
	CT_METRIC_GAUGE = 1     // Fluctuates up and down
} ct_metric_type;

typedef struct ct_stats_metric {
	const char* name;       // Metric name (borrowed, valid for program lifetime)
	int32_t value_index;    // Index into session_stats_alert counters array
	ct_metric_type type;
} ct_stats_metric;

// Get the list of all available session statistics metrics.
// Returns an array; out_count receives the number of metrics.
// Caller must free with ct_stats_metrics_free().
ct_stats_metric* ct_session_stats_metrics(size_t* out_count, ct_error* err);

// Free the array returned by ct_session_stats_metrics
void ct_stats_metrics_free(ct_stats_metric* metrics);

// Find the value_index of a metric by name. Returns -1 if not found.
int32_t ct_find_metric_idx(const char* name, ct_error* err);

#ifdef __cplusplus
}
#endif

#endif // CT_STATS_H
