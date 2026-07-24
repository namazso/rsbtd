// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

#include <ctorrent/ct_stats.h>

#include "ct_common.hpp"

#include <libtorrent/session_stats.hpp>

#include <new>
#include <cstring>

extern "C" {

ct_stats_metric* ct_session_stats_metrics(size_t* out_count, ct_error* err)
{
	// Zeroed up front so a caller that ignores *err never reads garbage.
	*out_count = 0;
	return ct::guard(err, [&]() -> ct_stats_metric* {
		std::vector<lt::stats_metric> metrics = lt::session_stats_metrics();

		if (metrics.empty()) {
			*out_count = 0;
			return nullptr;
		}

		auto* result = new ct_stats_metric[metrics.size()];
		for (size_t i = 0; i < metrics.size(); ++i) {
			result[i].name = metrics[i].name;
			result[i].value_index = metrics[i].value_index;
			result[i].type = (metrics[i].type == lt::metric_type_t::counter)
				? CT_METRIC_COUNTER
				: CT_METRIC_GAUGE;
		}
		*out_count = metrics.size();
		return result;
	});
}

void ct_stats_metrics_free(ct_stats_metric* metrics)
{
	delete[] metrics;
}

int32_t ct_find_metric_idx(const char* name, ct_error* err)
{
	return ct::guard(err, [&] {
		return lt::find_metric_idx(name);
	});
}

} // extern "C"
