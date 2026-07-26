// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use rbtorrent::{
    AddTorrentParams, RemoveFlags, Session, SessionParams, SettingsPack, TorrentFlags,
};
use std::path::PathBuf;

#[tokio::test]
async fn add_remove_torrent() {
    let fixture_path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/transfer.torrent");
    let save_dir = tempfile::tempdir().unwrap();
    let mut atp = AddTorrentParams::from_torrent_file(&fixture_path).unwrap();
    atp.set_save_path(save_dir.path().to_str().unwrap());

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

    // Add a torrent; the future resolves only while the alert stream is
    // polled, so poll both concurrently. The pinned future borrows the
    // session; the block scopes that borrow to end with the loop.
    let handle = {
        let add = session.add_torrent(&atp, std::sync::Arc::new(()));
        tokio::pin!(add);
        loop {
            tokio::select! {
                result = &mut add => break result.unwrap(),
                batch = alerts.next_batch() => { batch.unwrap(); }
            }
        }
    };
    assert!(handle.is_valid());
    assert_eq!(handle.info_hashes(), atp.info_hashes());

    // remove() consumes the handle
    handle.remove(RemoveFlags::empty());

    let mut removed = false;
    for _ in 0..10 {
        let batch =
            tokio::time::timeout(std::time::Duration::from_millis(500), alerts.next_batch()).await;
        if let Ok(batch) = batch {
            let batch = batch.unwrap();
            for alert in batch.iter() {
                if let rbtorrent::Alert::TorrentRemoved(_) = alert {
                    removed = true;
                    break;
                }
            }
            if removed {
                break;
            }
        }
    }

    assert!(removed, "Should see TorrentRemoved alert");

    drop(alerts);
    session.close().await;
}

#[tokio::test]
async fn resume_data_roundtrip() {
    let fixture_path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hybrid.torrent");
    let mut atp = AddTorrentParams::from_torrent_file(&fixture_path).unwrap();

    atp.set_save_path("/tmp/test")
        .set_name("test torrent")
        .set_max_uploads(10)
        .set_max_connections(50);

    let resume = Session::write_resume_data(&atp).unwrap();
    assert!(!resume.is_empty());

    let (restored, _) = Session::read_resume_data(&resume, None).unwrap();
    assert_eq!(restored.name(), atp.name());
    assert_eq!(restored.save_path(), atp.save_path());
    assert_eq!(restored.info_hashes(), atp.info_hashes());
    assert_eq!(restored.max_uploads(), atp.max_uploads());
    assert_eq!(restored.max_connections(), atp.max_connections());
}

#[tokio::test]
async fn torrent_handle_operations() {
    let fixture_path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/transfer.torrent");
    let save_dir = tempfile::tempdir().unwrap();
    let mut atp = AddTorrentParams::from_torrent_file(&fixture_path).unwrap();
    atp.set_save_path(save_dir.path().to_str().unwrap());

    let mut settings = SettingsPack::new();
    settings
        .listen_interfaces(&[rbtorrent::ListenEndpoint::new("127.0.0.1", 0)])
        .unwrap();

    let params = SessionParams::new().settings(&settings);
    let session = Session::new(params).unwrap();
    let mut alerts = session.alerts();

    let handle = {
        let add = session.add_torrent(&atp, std::sync::Arc::new(()));
        tokio::pin!(add);
        loop {
            tokio::select! {
                result = &mut add => break result.unwrap(),
                batch = alerts.next_batch() => { batch.unwrap(); }
            }
        }
    };

    assert!(handle.is_valid());
    assert_eq!(handle.id(), handle.id()); // Stable
    assert_eq!(handle.info_hashes(), atp.info_hashes());

    let flags = TorrentFlags::from_bits(handle.flags());
    assert!(flags.contains(TorrentFlags::AUTO_MANAGED));

    handle.set_flags(TorrentFlags::PAUSED.bits(), TorrentFlags::PAUSED.bits());
    let new_flags = TorrentFlags::from_bits(handle.flags());
    assert!(new_flags.contains(TorrentFlags::PAUSED));

    handle.unset_flags(TorrentFlags::PAUSED.bits());
    let final_flags = TorrentFlags::from_bits(handle.flags());
    assert!(!final_flags.contains(TorrentFlags::PAUSED));

    // won't actually connect, but must not error
    handle
        .connect_peer("127.0.0.1:9999".parse().unwrap())
        .unwrap();

    // Drop handle before closing session
    drop(handle);
    drop(alerts);
    session.close().await;
}

#[tokio::test]
async fn token_lookup_and_status_identity() {
    let fixture_path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/transfer.torrent");
    let save_dir = tempfile::tempdir().unwrap();
    let mut atp = AddTorrentParams::from_torrent_file(&fixture_path).unwrap();
    atp.set_save_path(save_dir.path().to_str().unwrap());

    let mut settings = SettingsPack::new();
    settings
        .enable_dht(false)
        .enable_lsd(false)
        .enable_upnp(false)
        .enable_natpmp(false)
        .listen_interfaces(&[rbtorrent::ListenEndpoint::new("127.0.0.1", 0)])
        .unwrap();

    let session = Session::new(SessionParams::new().settings(&settings)).unwrap();
    let mut alerts = session.alerts();

    let handle = {
        let add = session.add_torrent(&atp, std::sync::Arc::new(()));
        tokio::pin!(add);
        loop {
            tokio::select! {
                result = &mut add => break result.unwrap(),
                batch = alerts.next_batch() => { batch.unwrap(); }
            }
        }
    };

    // find_torrent_by_token: a hit yields the same torrent.
    let token = handle.client_data_token().unwrap();
    let found = session
        .find_torrent_by_token(token)
        .expect("added torrent is findable by its token");
    assert_eq!(found.id(), handle.id());
    assert_eq!(found.info_hashes(), handle.info_hashes());

    // Status snapshots carry the torrent's identity.
    let status = handle.status(0).unwrap();
    assert_eq!(status.id(), handle.id());
    assert_eq!(status.info_hashes(), handle.info_hashes());

    // Misses: the null token and a token never minted.
    assert!(session.find_torrent_by_token(0).is_none());
    assert!(session.find_torrent_by_token(u64::MAX).is_none());

    drop(found);
    drop(handle);
    drop(alerts);
    session.close().await;
}

#[tokio::test]
async fn info_hash_lookup() {
    // A hybrid torrent has both hash forms, so one torrent exercises both
    // lookups.
    let fixture_path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hybrid.torrent");
    let save_dir = tempfile::tempdir().unwrap();
    let mut atp = AddTorrentParams::from_torrent_file(&fixture_path).unwrap();
    atp.set_save_path(save_dir.path().to_str().unwrap());

    let mut settings = SettingsPack::new();
    settings
        .enable_dht(false)
        .enable_lsd(false)
        .enable_upnp(false)
        .enable_natpmp(false)
        .listen_interfaces(&[rbtorrent::ListenEndpoint::new("127.0.0.1", 0)])
        .unwrap();

    let session = Session::new(SessionParams::new().settings(&settings)).unwrap();
    let mut alerts = session.alerts();

    let handle = {
        let add = session.add_torrent(&atp, std::sync::Arc::new(()));
        tokio::pin!(add);
        loop {
            tokio::select! {
                result = &mut add => break result.unwrap(),
                batch = alerts.next_batch() => { batch.unwrap(); }
            }
        }
    };

    let hashes = handle.info_hashes();
    let v1 = hashes.v1().expect("hybrid has a v1 hash");
    let v2 = hashes.v2().expect("hybrid has a v2 hash");

    // Either hash form finds the same torrent.
    let by_v1 = session
        .find_torrent_v1(v1)
        .expect("findable by its v1 hash");
    assert_eq!(by_v1.id(), handle.id());
    let by_v2 = session
        .find_torrent_v2(v2)
        .expect("findable by its v2 hash");
    assert_eq!(by_v2.id(), handle.id());
    assert_eq!(by_v2.info_hashes(), hashes);

    // Misses: the zero hash and a hash no torrent has. Each lookup only
    // probes its own hash form, so a v1 hash never matches via v2.
    assert!(
        session
            .find_torrent_v1(rbtorrent::Sha1Hash([0; 20]))
            .is_none()
    );
    assert!(
        session
            .find_torrent_v1(rbtorrent::Sha1Hash([0xab; 20]))
            .is_none()
    );
    assert!(
        session
            .find_torrent_v2(rbtorrent::Sha256Hash([0xab; 32]))
            .is_none()
    );

    drop(by_v1);
    drop(by_v2);
    drop(handle);
    drop(alerts);
    session.close().await;
}

#[tokio::test]
async fn reopen_network_sockets() {
    let mut settings = SettingsPack::new();
    settings
        .enable_dht(false)
        .enable_lsd(false)
        .enable_upnp(false)
        .enable_natpmp(false)
        .listen_interfaces(&[rbtorrent::ListenEndpoint::new("127.0.0.1", 0)])
        .unwrap();

    let session = Session::new(SessionParams::new().settings(&settings)).unwrap();
    session.reopen_network_sockets(0).unwrap();
    session
        .reopen_network_sockets(Session::REOPEN_MAP_PORTS)
        .unwrap();
    // The session keeps listening after a reopen.
    assert!(session.is_listening().unwrap());
    session.close().await;
}
