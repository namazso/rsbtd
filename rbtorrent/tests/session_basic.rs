// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use rbtorrent::{AddTorrentParams, Session, SessionParams, SettingsPack};
use std::path::PathBuf;

#[test]
fn resume_data_roundtrip() {
    let fixture_path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hybrid.torrent");
    let mut atp = AddTorrentParams::from_torrent_file(&fixture_path).unwrap();

    atp.set_save_path("/tmp/test")
        .set_name("test torrent")
        .set_max_uploads(10)
        .set_max_connections(50);

    let resume = Session::write_resume_data(&atp).unwrap();
    assert!(!resume.is_empty());

    let restored = Session::read_resume_data(&resume, None).unwrap();
    assert_eq!(restored.name(), atp.name());
    assert_eq!(restored.save_path(), atp.save_path());
    assert_eq!(restored.info_hashes(), atp.info_hashes());
    assert_eq!(restored.max_uploads(), atp.max_uploads());
    assert_eq!(restored.max_connections(), atp.max_connections());
}

#[tokio::test]
async fn add_torrent_with_alert_polling() {
    let fixture_path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/transfer.torrent");
    let mut atp = AddTorrentParams::from_torrent_file(&fixture_path).unwrap();
    atp.set_save_path("/tmp");

    let mut settings = SettingsPack::new();
    settings
        .enable_dht(false)
        .enable_lsd(false)
        .enable_upnp(false)
        .enable_natpmp(false)
        .listen_interfaces(&[rbtorrent::ListenEndpoint::new("127.0.0.1", 0)])
        .unwrap();

    let params = SessionParams::new().settings(&settings);
    let session = Session::new(params).unwrap();
    let mut alerts = session.alerts();

    // Add the torrent and concurrently poll for the alert. The pinned
    // future borrows the session; the block scopes that borrow.
    let handle = {
        let add_future = session.add_torrent(&atp);
        tokio::pin!(add_future);
        loop {
            tokio::select! {
                result = &mut add_future => break result.unwrap(),
                batch_result = alerts.next_batch() => {
                    // Just consume alerts to drive the future forward
                    batch_result.unwrap();
                }
            }
        }
    };
    assert!(handle.is_valid());
    assert_eq!(handle.info_hashes(), atp.info_hashes());

    drop(handle);
    drop(alerts);
    session.close().await;
}
