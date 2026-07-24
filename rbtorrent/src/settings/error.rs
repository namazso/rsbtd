// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! [`SettingsError`]: a value rejected by a validating settings setter.

/// A value (or combination of values) outside a setting's safe domain,
/// rejected before anything reaches libtorrent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SettingsError {
    setting: &'static str,
    message: String,
}

impl SettingsError {
    pub(crate) fn new(setting: &'static str, message: impl Into<String>) -> SettingsError {
        SettingsError {
            setting,
            message: message.into(),
        }
    }

    /// The snake_case libtorrent setting name — or, for the grouped
    /// setters, the group name (e.g. `"proxy"`).
    pub fn setting(&self) -> &'static str {
        self.setting
    }

    /// What was wrong with the value. Value-centric and free of the
    /// setting name, so callers can prefix their own field path.
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl std::fmt::Display for SettingsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.setting, self.message)
    }
}

impl std::error::Error for SettingsError {}

impl From<SettingsError> for crate::Error {
    fn from(e: SettingsError) -> crate::Error {
        crate::Error::binding(&e.to_string())
    }
}

/// Checks a value against a setting's accepted range, producing the
/// standard out-of-domain error.
pub(crate) fn in_range(
    setting: &'static str,
    value: i32,
    range: std::ops::RangeInclusive<i32>,
) -> Result<(), SettingsError> {
    if range.contains(&value) {
        Ok(())
    } else {
        Err(SettingsError::new(
            setting,
            format!(
                "{value} is outside the valid range {}..={}",
                range.start(),
                range.end()
            ),
        ))
    }
}
