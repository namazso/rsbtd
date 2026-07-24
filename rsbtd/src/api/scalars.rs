// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Custom GraphQL scalars.

use async_graphql::{InputValueError, InputValueResult, Scalar, ScalarType, Value};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;

use crate::engine::registry::parse_info_hash;

/// A torrent info-hash: 40 hex characters (v1/SHA-1) or 64 hex characters
/// (v2/SHA-256). Serializes as the torrent's best hash (v1 preferred).
#[derive(Clone, Copy, Debug)]
pub struct InfoHash(pub rbtorrent::InfoHash);

/// A torrent info-hash: 40 hex characters (v1/SHA-1) or 64 hex
/// characters (v2/SHA-256). Input accepts either case and either hash
/// of a hybrid torrent; output is lowercase and prefers v1.
#[Scalar(name = "InfoHash")]
impl ScalarType for InfoHash {
    fn parse(value: Value) -> InputValueResult<Self> {
        match &value {
            Value::String(s) => parse_info_hash(s).map(InfoHash).ok_or_else(|| {
                InputValueError::custom("expected 40 (v1) or 64 (v2) hex characters")
            }),
            _ => Err(InputValueError::expected_type(value)),
        }
    }

    fn to_value(&self) -> Value {
        Value::String(self.0.to_string())
    }
}

/// Binary data as standard base64 (RFC 4648, with padding).
#[derive(Clone, Debug)]
pub struct Base64Bytes(pub Vec<u8>);

/// Binary data as a standard base64 string (RFC 4648: `+`/`/` alphabet
/// with `=` padding, not base64url).
#[Scalar(name = "Base64")]
impl ScalarType for Base64Bytes {
    fn parse(value: Value) -> InputValueResult<Self> {
        match &value {
            Value::String(s) => BASE64
                .decode(s)
                .map(Base64Bytes)
                .map_err(|e| InputValueError::custom(format!("invalid base64: {e}"))),
            _ => Err(InputValueError::expected_type(value)),
        }
    }

    fn to_value(&self) -> Value {
        Value::String(BASE64.encode(&self.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn info_hash_roundtrip() {
        let hex = "0123456789abcdef0123456789abcdef01234567";
        let parsed = <InfoHash as ScalarType>::parse(Value::String(hex.into())).unwrap();
        assert_eq!(parsed.to_value(), Value::String(hex.into()));
        assert!(<InfoHash as ScalarType>::parse(Value::String("nope".into())).is_err());
        assert!(<InfoHash as ScalarType>::parse(Value::Number(7.into())).is_err());
    }

    #[test]
    fn hybrid_info_hash_serializes_v1() {
        let hybrid = rbtorrent::InfoHash::new(
            Some(rbtorrent::Sha1Hash([0x01; 20])),
            Some(rbtorrent::Sha256Hash([0x02; 32])),
        );
        assert_eq!(InfoHash(hybrid).to_value(), Value::String("01".repeat(20)));
    }

    #[test]
    fn base64_roundtrip() {
        let parsed = <Base64Bytes as ScalarType>::parse(Value::String("AQID".into())).unwrap();
        assert_eq!(parsed.0, vec![1, 2, 3]);
        assert_eq!(parsed.to_value(), Value::String("AQID".into()));
        assert!(<Base64Bytes as ScalarType>::parse(Value::String("!!".into())).is_err());
    }
}
