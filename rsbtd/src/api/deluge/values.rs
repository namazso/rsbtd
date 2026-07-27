// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Parameter coercion and the Deluge/libtorrent unit conversions the
//! handlers share. The `*_of` coercions turn a raw
//! [`positional`](super::registry::positional) argument into a typed
//! one, naming the offending parameter in a code-3 error.
//!
//! Rates and peer limits are spelled differently on each side: Deluge
//! speaks KiB/s floats with -1 for unlimited, libtorrent bytes/s with
//! unlimited spelled 0 in a settings pack, 0 or saturated on a handle.

use rbtorrent::DownloadPriority;
use serde_json::Value;

use super::proto::RpcError;

pub(super) fn bool_of(key: &str, value: &Value) -> Result<bool, RpcError> {
    value
        .as_bool()
        .ok_or_else(|| RpcError::call_error(format!("{key} must be a boolean")))
}

pub(super) fn str_of(key: &str, value: &Value) -> Result<String, RpcError> {
    Ok(value
        .as_str()
        .ok_or_else(|| RpcError::call_error(format!("{key} must be a string")))?
        .to_owned())
}

pub(super) fn f64_of(key: &str, value: &Value) -> Result<f64, RpcError> {
    value
        .as_f64()
        .ok_or_else(|| RpcError::call_error(format!("{key} must be a number")))
}

pub(super) fn i32_of(key: &str, value: &Value) -> Result<i32, RpcError> {
    value
        .as_i64()
        .and_then(|v| i32::try_from(v).ok())
        .ok_or_else(|| RpcError::call_error(format!("{key} must be an integer")))
}

pub(super) fn priorities_of(key: &str, value: &Value) -> Result<Vec<DownloadPriority>, RpcError> {
    let list = value
        .as_array()
        .ok_or_else(|| RpcError::call_error(format!("{key} must be a list of integers")))?;
    list.iter()
        .map(|p| {
            p.as_u64()
                .and_then(|p| u8::try_from(p).ok())
                .and_then(DownloadPriority::new)
                .ok_or_else(|| RpcError::call_error(format!("file priority {p} is outside 0..=7")))
        })
        .collect()
}

/// Truncated, and kept below libtorrent's `i32::MAX` sentinel.
fn to_bytes(kib: f64) -> i32 {
    (kib * 1024.0).min(f64::from(i32::MAX - 1)) as i32
}

/// Both 0 and any negative mean unlimited, which the engine spells -1.
/// Validated here, so a whole options delta fails before it is applied.
pub(super) fn torrent_rate_of(key: &str, value: &Value) -> Result<i32, RpcError> {
    let kib = f64_of(key, value)?;
    let rate = match to_bytes(kib) {
        rate if kib < 0.0 || rate == 0 => -1,
        rate => rate,
    };
    rbtorrent::check_rate_limit(rate)
        .map_err(|e| RpcError::call_error(format!("{key}: {}", e.message())))?;
    Ok(rate)
}

/// A session-wide speed: the same float, but a settings pack spells
/// unlimited 0.
pub(super) fn session_rate_of(key: &str, value: &Value) -> Result<i32, RpcError> {
    let kib = f64_of(key, value)?;
    Ok(if kib < 0.0 { 0 } else { to_bytes(kib) })
}

/// Both libtorrent spellings of unlimited — 0 and a saturated limit —
/// report as -1.
pub(super) fn rate_out(raw: i64) -> f64 {
    if raw <= 0 || raw >= i64::from(i32::MAX) {
        -1.0
    } else {
        raw as f64 / 1024.0
    }
}

pub(super) fn peer_limit_of(key: &str, value: &Value) -> Result<i32, RpcError> {
    let limit = i32_of(key, value)?;
    rbtorrent::check_peer_limit(limit)
        .map_err(|e| RpcError::call_error(format!("{key}: {}", e.message())))?;
    Ok(limit)
}

/// [`peer_limit_of`] with normalization: 0, the web UI's spelling of
/// unlimited, becomes the -1 sentinel.
pub(super) fn upload_slots_of(key: &str, value: &Value) -> Result<i32, RpcError> {
    let normalized = match i32_of(key, value)? {
        0 => -1,
        limit => limit,
    };
    peer_limit_of(key, &Value::from(normalized))
}

/// [`upload_slots_of`], additionally raising 1 to 2, the smallest count
/// libtorrent accepts.
pub(super) fn connection_limit_of(key: &str, value: &Value) -> Result<i32, RpcError> {
    let normalized = match i32_of(key, value)? {
        1 => 2,
        limit => limit,
    };
    upload_slots_of(key, &Value::from(normalized))
}

/// libtorrent's 24-bit all-ones sentinel, and anything nonpositive,
/// reports as -1.
pub(super) fn peer_limit_out(raw: i32) -> i32 {
    if raw <= 0 || raw >= (1 << 24) - 1 {
        -1
    } else {
        raw
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn coercions_name_the_parameter() {
        assert!(bool_of("k", &json!(true)).unwrap());
        assert_eq!(
            bool_of("k", &json!(1)).unwrap_err().message,
            "k must be a boolean"
        );
        assert_eq!(str_of("k", &json!("v")).unwrap(), "v");
        assert_eq!(f64_of("k", &json!(1)).unwrap(), 1.0);
        assert_eq!(i32_of("k", &json!(7)).unwrap(), 7);
        assert!(i32_of("k", &json!(i64::from(i32::MAX) + 1)).is_err());
    }

    #[test]
    fn rates_round_trip_through_deluge_units() {
        assert_eq!(torrent_rate_of("k", &json!(1.5)).unwrap(), 1536);
        assert_eq!(torrent_rate_of("k", &json!(-1)).unwrap(), -1);
        assert_eq!(torrent_rate_of("k", &json!(0)).unwrap(), -1);
        assert_eq!(session_rate_of("k", &json!(-1)).unwrap(), 0);
        assert_eq!(session_rate_of("k", &json!(2)).unwrap(), 2048);
        // Fractional bytes truncate.
        assert_eq!(session_rate_of("k", &json!(0.9)).unwrap(), 921);
        assert_eq!(rate_out(1536), 1.5);
        assert_eq!(rate_out(0), -1.0);
        assert_eq!(rate_out(i64::from(i32::MAX)), -1.0);
    }

    #[test]
    fn peer_limits_use_the_unlimited_sentinel() {
        assert_eq!(peer_limit_of("k", &json!(10)).unwrap(), 10);
        assert!(peer_limit_of("k", &json!(1)).is_err());
        assert_eq!(peer_limit_out(10), 10);
        assert_eq!(peer_limit_out(-1), -1);
        assert_eq!(peer_limit_out((1 << 24) - 1), -1);
        assert_eq!(upload_slots_of("k", &json!(0)).unwrap(), -1);
        assert!(upload_slots_of("k", &json!(1)).is_err());
        assert_eq!(upload_slots_of("k", &json!(10)).unwrap(), 10);
        assert_eq!(connection_limit_of("k", &json!(0)).unwrap(), -1);
        assert_eq!(connection_limit_of("k", &json!(1)).unwrap(), 2);
        assert_eq!(connection_limit_of("k", &json!(10)).unwrap(), 10);
    }
}
