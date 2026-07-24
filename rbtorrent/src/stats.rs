// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The session statistics metrics table: names the counters returned by
//! [`Session::session_stats`](crate::Session::session_stats) and carried by
//! [`SessionStatsAlert`](crate::alerts::SessionStatsAlert). The table is a
//! static property of the linked libtorrent: an index obtained once is
//! valid for every stats snapshot of the process.

use std::ffi::CString;

use libctorrent_sys as sys;

use crate::error::{Error, Result, with_error};

/// Whether a metric only ever increases or fluctuates.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MetricKind {
    /// Monotonically increasing.
    Counter,
    /// Fluctuates up and down.
    Gauge,
}

/// One entry of the stats metrics table.
#[derive(Clone, Debug)]
pub struct StatsMetric {
    /// The metric's name, e.g. `"net.recv_payload_bytes"`.
    pub name: String,
    /// Index into the counters array of a stats snapshot.
    pub value_index: i32,
    /// Counter or gauge.
    pub kind: MetricKind,
}

/// The full stats metrics table of the linked libtorrent.
pub fn session_stats_metrics() -> Result<Vec<StatsMetric>> {
    let mut count = 0usize;
    // SAFETY: on success we own the returned array and must free it; the
    // name pointers are libtorrent statics valid for the program lifetime,
    // copied out before the free.
    unsafe {
        let ptr = with_error(|err| sys::ct_session_stats_metrics(&mut count, err))?;
        if ptr.is_null() {
            return Ok(Vec::new());
        }
        let metrics = std::slice::from_raw_parts(ptr, count)
            .iter()
            .map(|m| StatsMetric {
                name: std::ffi::CStr::from_ptr(m.name)
                    .to_string_lossy()
                    .into_owned(),
                value_index: m.value_index,
                kind: if m.type_ == sys::CT_METRIC_COUNTER {
                    MetricKind::Counter
                } else {
                    MetricKind::Gauge
                },
            })
            .collect();
        sys::ct_stats_metrics_free(ptr);
        Ok(metrics)
    }
}

/// Looks up a metric's counters-array index by name; `Ok(None)` if the
/// linked libtorrent has no such metric.
pub fn find_metric_index(name: &str) -> Result<Option<i32>> {
    let name =
        CString::new(name).map_err(|_| Error::binding("metric names cannot contain NUL bytes"))?;
    // SAFETY: name is a valid NUL-terminated string for the call.
    let idx = with_error(|err| unsafe { sys::ct_find_metric_idx(name.as_ptr(), err) })?;
    Ok((idx >= 0).then_some(idx))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metrics_table_is_populated_and_consistent() {
        let metrics = session_stats_metrics().unwrap();
        assert!(!metrics.is_empty());
        // Every index is unique and covered by the table.
        let mut indices: Vec<_> = metrics.iter().map(|m| m.value_index).collect();
        indices.sort_unstable();
        indices.dedup();
        assert_eq!(indices.len(), metrics.len());

        // find_metric_index agrees with the table.
        let by_name = &metrics[metrics.len() / 2];
        assert_eq!(
            find_metric_index(&by_name.name).unwrap(),
            Some(by_name.value_index)
        );
        assert_eq!(find_metric_index("no.such.metric").unwrap(), None);
        assert!(find_metric_index("nul\0byte").is_err());
    }
}
