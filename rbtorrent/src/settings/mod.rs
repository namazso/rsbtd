// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Session configuration via libtorrent's `settings_pack`.
//!
//! [`SettingsPack`] is a *delta*: it holds only the settings explicitly set
//! on it. Applying a pack to a session updates just those values.
//!
//! Every known setting is written through a typed method (e.g.
//! [`SettingsPack::upload_rate_limit`]); these are the only write path,
//! and they enforce each setting's safe value domain (fallible setters
//! return [`SettingsError`]). Interdependent settings are written as
//! groups ([`SettingsPack::proxy`], [`SettingsPack::i2p`],
//! [`SettingsPack::outgoing_ports`], ...) whose config types make
//! invalid combinations unrepresentable. The [`SettingKey`]-based
//! getters and [`setting_by_name`] remain available for reads and
//! diagnostics, including settings newer than these bindings.
//!
//! The typed methods live in one module per functional area, mirroring the
//! sections of `docs/libtorrent-settings-constraints.md`.

mod bandwidth;
mod connections;
mod dht;
#[cfg(test)]
mod domain_tests;
mod enums;
mod error;
mod net;
mod storage;
mod table;
mod tokens;
mod tracker;

use std::ptr::NonNull;

use libctorrent_sys as sys;

pub use connections::ListenEndpoint;
pub use dht::HostPort;
pub use enums::{
    BandwidthMixedAlgo, ChokingAlgorithm, EncLevel, EncPolicy, IoBufferMode, MmapWriteMode,
    ProxyType, SeedChokingAlgorithm, SuggestMode,
};
pub use error::SettingsError;
pub use net::{Credentials, I2pConfig, I2pTunnels, ProxyConfig, ProxyProtocol};

/// The value type of a setting key.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SettingKind {
    Str,
    Int,
    Bool,
}

/// A validated libtorrent setting key.
///
/// Every `SettingKey` is in range for the linked libtorrent, so it can be
/// passed to the key-based [`SettingsPack`] methods freely. Obtain one from
/// [`setting_by_name`] — which also covers settings newer than these
/// bindings — or from [`SettingKey::from_raw`]. The typed `SettingsPack`
/// methods don't need keys.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct SettingKey(i32);

impl SettingKey {
    /// Validates a raw `lt::settings_pack::*` enum value (type bits and
    /// index bounds, against the linked libtorrent).
    pub fn from_raw(raw: i32) -> Option<SettingKey> {
        // SAFETY: pure validation, no preconditions.
        unsafe { sys::ct_setting_is_valid(raw) }.then_some(SettingKey(raw))
    }

    /// Wraps a raw key without validating it.
    ///
    /// # Safety
    /// `raw` must be a key the linked libtorrent considers valid: its type
    /// bits must be one of the three type bases and its index must be below
    /// that type's setting count ([`SettingKey::from_raw`] checks exactly
    /// this).
    pub const unsafe fn from_raw_unchecked(raw: i32) -> SettingKey {
        SettingKey(raw)
    }

    /// Wraps a generated `CT_SET_*` constant; these are static_asserted
    /// against the `lt::settings_pack` enum values at shim compile time.
    pub(crate) const fn from_generated(raw: u32) -> SettingKey {
        SettingKey(raw as i32)
    }

    /// The raw `lt::settings_pack::*` enum value.
    pub const fn raw(self) -> i32 {
        self.0
    }

    /// The value type of this setting.
    pub fn kind(self) -> SettingKind {
        match self.0 as u32 & sys::CT_SETTINGS_TYPE_MASK {
            sys::CT_SETTINGS_STR_BASE => SettingKind::Str,
            sys::CT_SETTINGS_INT_BASE => SettingKind::Int,
            sys::CT_SETTINGS_BOOL_BASE => SettingKind::Bool,
            _ => unreachable!("SettingKey holds a validated key"),
        }
    }

    /// The setting's name, or `None` for the (valid but nameless) slot of
    /// a setting this libtorrent build has removed.
    pub fn name(self) -> Option<&'static str> {
        // SAFETY: the shim validates the key and libtorrent returns a
        // pointer into a static name table.
        unsafe {
            let view = sys::ct_name_for_setting(self.0);
            if view.ptr.is_null() || view.len == 0 {
                return None;
            }
            str::from_utf8(std::slice::from_raw_parts(view.ptr.cast(), view.len)).ok()
        }
    }
}

impl std::fmt::Debug for SettingKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SettingKey({:#06x}", self.0)?;
        if let Some(name) = self.name() {
            write!(f, " {name:?}")?;
        }
        write!(f, ")")
    }
}

/// A set of libtorrent settings (delta semantics; see the module docs).
pub struct SettingsPack {
    ptr: NonNull<sys::ct_settings_pack>,
}

// The underlying settings_pack is plain data; mutation requires &mut and
// reads are const-only.
unsafe impl Send for SettingsPack {}
unsafe impl Sync for SettingsPack {}

impl SettingsPack {
    /// An empty pack (no settings explicitly set).
    pub fn new() -> SettingsPack {
        // SAFETY: constructor returns an owned pointer (or null on OOM).
        let ptr = unsafe { sys::ct_settings_pack_new() };
        SettingsPack {
            ptr: NonNull::new(ptr).expect("allocation failed"),
        }
    }

    /// A pack holding libtorrent's default value for every setting.
    pub fn defaults() -> SettingsPack {
        // SAFETY: as above.
        let ptr = unsafe { sys::ct_settings_pack_default() };
        SettingsPack {
            ptr: NonNull::new(ptr).expect("allocation failed"),
        }
    }

    /// Sets a string setting by key. Ignored if `key` is not a string
    /// setting.
    #[inline]
    pub(crate) fn set_str(&mut self, key: SettingKey, value: &str) {
        // SAFETY: pack is valid; the view borrows `value` only for the call.
        unsafe {
            sys::ct_settings_pack_set_str(
                self.ptr.as_ptr(),
                key.raw(),
                sys::ct_str_view {
                    ptr: value.as_ptr().cast(),
                    len: value.len(),
                },
            );
        }
    }

    /// Sets an integer setting by key. Ignored if `key` is not an integer
    /// setting.
    #[inline]
    pub(crate) fn set_int(&mut self, key: SettingKey, value: i32) {
        // SAFETY: pack is valid.
        unsafe { sys::ct_settings_pack_set_int(self.ptr.as_ptr(), key.raw(), value) }
    }

    /// Sets a boolean setting by key. Ignored if `key` is not a boolean
    /// setting.
    #[inline]
    pub(crate) fn set_bool(&mut self, key: SettingKey, value: bool) {
        // SAFETY: pack is valid.
        unsafe { sys::ct_settings_pack_set_bool(self.ptr.as_ptr(), key.raw(), value) }
    }

    /// Reads a string setting if this pack contains it.
    #[inline]
    pub fn get_str(&self, key: SettingKey) -> Option<String> {
        // SAFETY: pack is valid; on `true`, `out` owns a string we must free.
        unsafe {
            let mut out = sys::ct_str::default();
            if !sys::ct_settings_pack_get_str(self.ptr.as_ptr(), key.raw(), &mut out) {
                return None;
            }
            let text = if out.ptr.is_null() {
                String::new()
            } else {
                String::from_utf8_lossy(std::slice::from_raw_parts(out.ptr.cast(), out.len))
                    .into_owned()
            };
            sys::ct_str_free(&mut out);
            Some(text)
        }
    }

    /// Reads an integer setting if this pack contains it.
    #[inline]
    pub fn get_int(&self, key: SettingKey) -> Option<i32> {
        // SAFETY: pack is valid; out is only read after a `true` return.
        unsafe {
            let mut out = 0i32;
            sys::ct_settings_pack_get_int(self.ptr.as_ptr(), key.raw(), &mut out).then_some(out)
        }
    }

    /// Reads a boolean setting if this pack contains it.
    #[inline]
    pub fn get_bool(&self, key: SettingKey) -> Option<bool> {
        // SAFETY: as above.
        unsafe {
            let mut out = false;
            sys::ct_settings_pack_get_bool(self.ptr.as_ptr(), key.raw(), &mut out).then_some(out)
        }
    }

    /// Whether this pack explicitly contains `key`.
    #[inline]
    pub fn has(&self, key: SettingKey) -> bool {
        // SAFETY: pack is valid.
        unsafe { sys::ct_settings_pack_has(self.ptr.as_ptr(), key.raw()) }
    }

    /// Removes every setting from the pack.
    pub fn clear(&mut self) {
        // SAFETY: pack is valid.
        unsafe { sys::ct_settings_pack_clear(self.ptr.as_ptr()) }
    }

    /// Removes one setting from the pack.
    // Currently exercised only by tests; kept as the natural inverse of
    // the key-based setters.
    #[allow(dead_code)]
    pub(crate) fn clear_key(&mut self, key: SettingKey) {
        // SAFETY: pack is valid.
        unsafe { sys::ct_settings_pack_clear_one(self.ptr.as_ptr(), key.raw()) }
    }

    /// Copies every value present in `delta` into this pack; settings
    /// absent from `delta` keep their values here.
    pub fn apply(&mut self, delta: &SettingsPack) {
        // SAFETY: both packs are valid; the shim only copies values.
        unsafe { sys::ct_settings_pack_merge(self.ptr.as_ptr(), delta.as_ptr()) }
    }

    /// Sets `key` to libtorrent's default value for that setting.
    pub fn set_default(&mut self, key: SettingKey) {
        static DEFAULTS: std::sync::OnceLock<SettingsPack> = std::sync::OnceLock::new();
        let defaults = DEFAULTS.get_or_init(SettingsPack::defaults);
        match key.kind() {
            SettingKind::Str => self.set_str(key, &defaults.get_str(key).unwrap_or_default()),
            SettingKind::Int => self.set_int(key, defaults.get_int(key).unwrap_or(0)),
            SettingKind::Bool => self.set_bool(key, defaults.get_bool(key).unwrap_or(false)),
        }
    }

    pub(crate) fn as_ptr(&self) -> *const sys::ct_settings_pack {
        self.ptr.as_ptr()
    }

    /// Takes ownership of a pack returned by the shim.
    ///
    /// # Safety
    /// `ptr` must be a valid, owned `ct_settings_pack` not freed elsewhere.
    pub(crate) unsafe fn from_owned_ptr(ptr: *mut sys::ct_settings_pack) -> SettingsPack {
        SettingsPack {
            ptr: NonNull::new(ptr).expect("null settings pack"),
        }
    }
}

impl Default for SettingsPack {
    fn default() -> Self {
        SettingsPack::new()
    }
}

impl Clone for SettingsPack {
    fn clone(&self) -> Self {
        // SAFETY: clone of a valid pack; returns an owned pointer.
        let ptr = unsafe { sys::ct_settings_pack_clone(self.ptr.as_ptr()) };
        SettingsPack {
            ptr: NonNull::new(ptr).expect("allocation failed"),
        }
    }
}

impl Drop for SettingsPack {
    fn drop(&mut self) {
        // SAFETY: we own the pack.
        unsafe { sys::ct_settings_pack_free(self.ptr.as_ptr()) }
    }
}

impl std::fmt::Debug for SettingsPack {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SettingsPack").finish_non_exhaustive()
    }
}

/// Enumerates every setting known to these bindings, as
/// `(key, name, kind)` tuples in libtorrent's declaration order.
///
/// The names match each key's `lt::settings_pack` enum constant. Settings
/// newer than the bindings are not listed (resolve those with
/// [`setting_by_name`]).
pub fn all_settings() -> impl Iterator<Item = (SettingKey, &'static str, SettingKind)> {
    table::ALL_SETTINGS.iter().copied()
}

/// Resolves a setting name (e.g. `"upload_rate_limit"`) to its key,
/// including settings newer than these bindings.
pub fn setting_by_name(name: &str) -> Option<SettingKey> {
    // Removed settings keep a nameless ("") slot in libtorrent's tables;
    // an empty name must not resolve to one.
    if name.is_empty() {
        return None;
    }
    // SAFETY: the view borrows `name` only for the call; the shim validates
    // the resolved key before returning it.
    let key = unsafe {
        sys::ct_setting_by_name(sys::ct_str_view {
            ptr: name.as_ptr().cast(),
            len: name.len(),
        })
    };
    (key >= 0).then_some(SettingKey(key))
}

#[cfg(test)]
mod tests {
    use super::table::ALL_SETTINGS;
    use super::*;

    #[test]
    fn roundtrip_every_setting() {
        let mut pack = SettingsPack::new();
        for &(key, name, kind) in ALL_SETTINGS {
            assert!(!pack.has(key), "{name} set in fresh pack");
            match kind {
                SettingKind::Str => {
                    pack.set_str(key, "value");
                    assert_eq!(pack.get_str(key).as_deref(), Some("value"), "{name}");
                }
                SettingKind::Int => {
                    pack.set_int(key, 7);
                    assert_eq!(pack.get_int(key), Some(7), "{name}");
                }
                SettingKind::Bool => {
                    pack.set_bool(key, true);
                    assert_eq!(pack.get_bool(key), Some(true), "{name}");
                }
            }
            assert!(pack.has(key), "{name}");
            assert_eq!(key.kind(), kind, "{name}");
            assert_eq!(SettingKey::from_raw(key.raw()), Some(key), "{name}");
        }
        let mut cloned = pack.clone();
        cloned.clear();
        for &(key, name, _) in ALL_SETTINGS {
            assert!(pack.has(key), "{name} lost from original");
            assert!(!cloned.has(key), "{name} survived clear");
        }
    }

    #[test]
    fn all_settings_matches_generated_table() {
        let listed: Vec<_> = all_settings().collect();
        assert_eq!(listed.len(), ALL_SETTINGS.len());
        assert!(listed.len() > 200, "expected the full settings table");
        for (key, name, kind) in listed {
            assert_eq!(key.kind(), kind, "{name}");
            assert!(!name.is_empty());
        }
    }

    #[test]
    fn name_lookup_roundtrip() {
        // Upstream quirk (libtorrent 2.1.0): announce_to_all_tiers and
        // announce_to_all_trackers are swapped between the settings enum and
        // the runtime name table (settings_pack.cpp), so name-based lookup
        // is cross-wired for exactly this pair. Our constants follow the
        // enum, which is what libtorrent's own behavior uses. If this
        // assertion starts failing, upstream fixed the table; re-check both
        // orders and update.
        let swapped = ["announce_to_all_tiers", "announce_to_all_trackers"];
        for &(key, name, _) in ALL_SETTINGS {
            if swapped.contains(&name) {
                let partner_key = setting_by_name(name).expect(name);
                assert_ne!(partner_key, key, "{name}: upstream swap resolved?");
                let partner = key.name().expect(name);
                assert!(
                    swapped.contains(&partner) && partner != name,
                    "{name} resolved to {partner}"
                );
                continue;
            }
            assert_eq!(setting_by_name(name), Some(key), "{name}");
            assert_eq!(key.name(), Some(name), "{name}");
        }
        assert_eq!(setting_by_name("no_such_setting"), None);
        assert_eq!(setting_by_name(""), None);
    }

    #[test]
    fn invalid_keys_rejected() {
        // In-range type bits, out-of-range indices.
        assert_eq!(SettingKey::from_raw(0x3fff), None);
        assert_eq!(SettingKey::from_raw(0x7fff), None);
        assert_eq!(SettingKey::from_raw(0xbfff_u32 as i32), None);
        // Invalid type bits, negative, and out-of-encoding values.
        assert_eq!(SettingKey::from_raw(0xc000_u32 as i32), None);
        assert_eq!(SettingKey::from_raw(-1), None);
        assert_eq!(SettingKey::from_raw(0x1_0000), None);
        assert_eq!(SettingKey::from_raw(i32::MAX), None);
        assert_eq!(SettingKey::from_raw(i32::MIN), None);

        // The shim revalidates below the Rust API, so even raw C callers
        // (and the unsafe from_raw_unchecked escape hatch) cannot reach
        // libtorrent's unchecked table indexing, which would abort or
        // segfault.
        // SAFETY: exercising the shim's rejection paths with plain values.
        unsafe {
            assert!(!sys::ct_setting_is_valid(0x7fff));
            let view = sys::ct_name_for_setting(0x7fff);
            assert!(view.ptr.is_null());

            let defaults = SettingsPack::defaults();
            let mut out = 0i32;
            assert!(!sys::ct_settings_pack_get_int(
                defaults.as_ptr(),
                0x7fff,
                &mut out
            ));
            assert!(!sys::ct_settings_pack_has(defaults.as_ptr(), 0x7fff));
        }
    }

    #[test]
    fn key_based_access_via_name() {
        let key = setting_by_name("upload_rate_limit").unwrap();
        assert_eq!(key.kind(), SettingKind::Int);
        let mut pack = SettingsPack::new();
        pack.set_int(key, 4096);
        assert_eq!(pack.get_int(key), Some(4096));
        assert_eq!(pack.get_upload_rate_limit(), Some(4096));
        pack.clear_key(key);
        assert!(!pack.has(key));
    }

    #[test]
    fn defaults_are_populated() {
        let defaults = SettingsPack::defaults();
        let agent = defaults.get_user_agent().expect("user_agent default");
        assert!(agent.contains("libtorrent"), "{agent:?}");
        assert!(defaults.get_alert_mask().is_some());
        assert_eq!(defaults.get_enable_dht(), Some(true));
        // The pack is dense: every known setting is present, including
        // strings whose default is empty.
        for &(key, name, _) in ALL_SETTINGS {
            assert!(defaults.has(key), "{name} missing from defaults");
        }
        assert_eq!(defaults.get_proxy_hostname().as_deref(), Some(""));
    }

    #[test]
    fn typed_enum_settings() {
        let mut pack = SettingsPack::new();
        pack.proxy_type(ProxyType::Socks5)
            .choking_algorithm(ChokingAlgorithm::RateBasedChoker)
            .allowed_enc_level(EncLevel::PeBoth);
        assert_eq!(pack.get_proxy_type(), Some(ProxyType::Socks5));
        assert_eq!(
            pack.get_choking_algorithm(),
            Some(ChokingAlgorithm::RateBasedChoker)
        );
        assert_eq!(pack.get_allowed_enc_level(), Some(EncLevel::PeBoth));
    }

    #[test]
    fn apply_merges_only_present_values() {
        let mut base = SettingsPack::new();
        base.upload_rate_limit(1000).download_rate_limit(2000);
        let mut delta = SettingsPack::new();
        delta.download_rate_limit(3000).user_agent("merged/1.0");
        base.apply(&delta);
        assert_eq!(base.get_upload_rate_limit(), Some(1000));
        assert_eq!(base.get_download_rate_limit(), Some(3000));
        assert_eq!(base.get_user_agent().as_deref(), Some("merged/1.0"));
        // The delta itself is untouched.
        assert!(!delta.has(setting_by_name("upload_rate_limit").unwrap()));
    }

    #[test]
    fn set_default_writes_libtorrent_defaults() {
        let defaults = SettingsPack::defaults();
        let mut pack = SettingsPack::new();
        for name in ["user_agent", "connections_limit", "enable_dht"] {
            let key = setting_by_name(name).unwrap();
            pack.set_default(key);
            assert!(pack.has(key), "{name}");
            match key.kind() {
                SettingKind::Str => assert_eq!(pack.get_str(key), defaults.get_str(key), "{name}"),
                SettingKind::Int => assert_eq!(pack.get_int(key), defaults.get_int(key), "{name}"),
                SettingKind::Bool => {
                    assert_eq!(pack.get_bool(key), defaults.get_bool(key), "{name}");
                }
            }
        }
    }

    #[test]
    fn builder_chaining() {
        let mut pack = SettingsPack::new();
        pack.upload_rate_limit(1000)
            .download_rate_limit(2000)
            .user_agent("rbtorrent-test/1.0");
        assert_eq!(pack.get_upload_rate_limit(), Some(1000));
        assert_eq!(pack.get_download_rate_limit(), Some(2000));
        assert_eq!(pack.get_user_agent().as_deref(), Some("rbtorrent-test/1.0"));
    }
}
