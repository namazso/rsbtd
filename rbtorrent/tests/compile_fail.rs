// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Lifetime soundness: misuses of the alert batch API must not compile.

#[test]
fn ui() {
    // The .stderr expectations are recorded against the pinned toolchain
    // (rust-toolchain.toml); borrowck diagnostics drift across rustc
    // releases. Builds on a floating rustc (the Alpine image) set this.
    if std::env::var_os("RSBTD_SKIP_TRYBUILD").is_some() {
        eprintln!("skipping trybuild ui tests: RSBTD_SKIP_TRYBUILD is set");
        return;
    }
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/ui/*.rs");
}
