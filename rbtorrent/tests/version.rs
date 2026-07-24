// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

#[test]
fn libtorrent_is_2_1_or_newer() {
    assert!(rbtorrent::libtorrent_version_num() >= 20100);
    assert!(rbtorrent::libtorrent_version().starts_with("2."));
    let abi = rbtorrent::libtorrent_abi_version();
    assert!(abi >= 2, "unexpected TORRENT_ABI_VERSION {abi}");
}
