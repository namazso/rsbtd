// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Custom GraphQL scalars.

use async_graphql::{InputValueError, InputValueResult, Scalar, ScalarType, Value};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;

/// A SHA-1 digest.
#[derive(Clone, Copy, Debug)]
pub struct Sha1Sum(pub rbtorrent::Sha1Hash);

/// A SHA-1 digest: 40 hex characters.
#[Scalar]
impl ScalarType for Sha1Sum {
    fn parse(value: Value) -> InputValueResult<Self> {
        match &value {
            Value::String(s) => hex::decode(s)
                .ok()
                .and_then(|b| <[u8; 20]>::try_from(b).ok())
                .map(|b| Sha1Sum(rbtorrent::Sha1Hash(b)))
                .ok_or_else(|| InputValueError::custom("expected 40 hex characters")),
            _ => Err(InputValueError::expected_type(value)),
        }
    }

    fn to_value(&self) -> Value {
        Value::String(self.0.to_string())
    }
}

/// A SHA-256 digest.
#[derive(Clone, Copy, Debug)]
pub struct Sha256Sum(pub rbtorrent::Sha256Hash);

/// A SHA-256 digest: 64 hex characters.
#[Scalar]
impl ScalarType for Sha256Sum {
    fn parse(value: Value) -> InputValueResult<Self> {
        match &value {
            Value::String(s) => hex::decode(s)
                .ok()
                .and_then(|b| <[u8; 32]>::try_from(b).ok())
                .map(|b| Sha256Sum(rbtorrent::Sha256Hash(b)))
                .ok_or_else(|| InputValueError::custom("expected 64 hex characters")),
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
    fn sha1_sum_roundtrip() {
        let hex = "0123456789abcdef0123456789abcdef01234567";
        let parsed = <Sha1Sum as ScalarType>::parse(Value::String(hex.into())).unwrap();
        assert_eq!(parsed.to_value(), Value::String(hex.into()));
        assert!(<Sha1Sum as ScalarType>::parse(Value::String("nope".into())).is_err());
        assert!(<Sha1Sum as ScalarType>::parse(Value::String("01".repeat(32))).is_err());
        assert!(<Sha1Sum as ScalarType>::parse(Value::Number(7.into())).is_err());
    }

    #[test]
    fn sha256_sum_roundtrip() {
        let hex = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        let parsed = <Sha256Sum as ScalarType>::parse(Value::String(hex.into())).unwrap();
        assert_eq!(parsed.to_value(), Value::String(hex.into()));
        assert!(<Sha256Sum as ScalarType>::parse(Value::String("nope".into())).is_err());
        assert!(<Sha256Sum as ScalarType>::parse(Value::String("01".repeat(20))).is_err());
        assert!(<Sha256Sum as ScalarType>::parse(Value::Number(7.into())).is_err());
    }

    #[test]
    fn base64_roundtrip() {
        let parsed = <Base64Bytes as ScalarType>::parse(Value::String("AQID".into())).unwrap();
        assert_eq!(parsed.0, vec![1, 2, 3]);
        assert_eq!(parsed.to_value(), Value::String("AQID".into()));
        assert!(<Base64Bytes as ScalarType>::parse(Value::String("!!".into())).is_err());
    }
}
