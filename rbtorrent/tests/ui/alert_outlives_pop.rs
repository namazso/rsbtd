// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

// An Alert view must not survive the next pop (which would dangle).

async fn misuse(alerts: &mut rbtorrent::Alerts<'_>) {
    let batch = alerts.next_batch().await.unwrap();
    let first = batch.get(0);
    let _batch2 = alerts.next_batch().await.unwrap();
    drop(first);
}

fn main() {}
