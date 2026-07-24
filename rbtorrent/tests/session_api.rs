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
        let add = session.add_torrent(&atp);
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

    let restored = Session::read_resume_data(&resume, None).unwrap();
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
        let add = session.add_torrent(&atp);
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
async fn find_torrent_and_status_identity() {
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
        let add = session.add_torrent(&atp);
        tokio::pin!(add);
        loop {
            tokio::select! {
                result = &mut add => break result.unwrap(),
                batch = alerts.next_batch() => { batch.unwrap(); }
            }
        }
    };

    // find_torrent: a hit yields the same torrent.
    let found = session
        .find_torrent(atp.info_hashes())
        .expect("added torrent is findable");
    assert_eq!(found.id(), handle.id());
    assert_eq!(found.info_hashes(), handle.info_hashes());

    // Status snapshots carry the torrent's identity.
    let status = handle.status(0).unwrap();
    assert_eq!(status.id(), handle.id());
    assert_eq!(status.info_hashes(), handle.info_hashes());

    // Misses: empty and unknown hashes.
    assert!(
        session
            .find_torrent(rbtorrent::InfoHash::new(None, None))
            .is_none()
    );
    let unknown = rbtorrent::InfoHash::from_v1(rbtorrent::Sha1Hash([0xab; 20]));
    assert!(session.find_torrent(unknown).is_none());

    // A hybrid torrent stays findable when the v1 lookup misses: the v2
    // hash is tried next.
    let hybrid_path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hybrid.torrent");
    let mut hybrid_atp = AddTorrentParams::from_torrent_file(&hybrid_path).unwrap();
    hybrid_atp.set_save_path(save_dir.path().to_str().unwrap());
    let hybrid = {
        let add = session.add_torrent(&hybrid_atp);
        tokio::pin!(add);
        loop {
            tokio::select! {
                result = &mut add => break result.unwrap(),
                batch = alerts.next_batch() => { batch.unwrap(); }
            }
        }
    };
    let v2 = hybrid.info_hashes().v2().expect("hybrid has a v2 hash");
    let mixed = rbtorrent::InfoHash::new(Some(rbtorrent::Sha1Hash([0xab; 20])), Some(v2));
    let via_fallback = session.find_torrent(mixed).expect("v2 fallback finds it");
    assert_eq!(via_fallback.id(), hybrid.id());

    drop(via_fallback);
    drop(hybrid);
    drop(found);
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
