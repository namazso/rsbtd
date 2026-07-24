// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

// An Alert view must not survive the Alerts receiver it came from.

async fn misuse(session: rbtorrent::Session) {
    let kept;
    {
        let mut alerts = session.alerts();
        let batch = alerts.next_batch().await.unwrap();
        kept = batch.get(0);
    }
    drop(kept);
}

fn main() {}
