// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

// A TorrentHandle must not survive its Session: close() consumes the
// session, which cannot move while a handle borrows it.

async fn misuse(session: rbtorrent::Session) {
    let handle = session.find_torrent_by_token(1);
    session.close().await;
    drop(handle);
}

fn main() {}
