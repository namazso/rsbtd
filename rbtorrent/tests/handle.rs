// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use rbtorrent::{AddTorrentParams, DownloadPriority, Session, SessionParams, SettingsPack};
use std::path::PathBuf;

#[tokio::test]
async fn torrent_handle_accessors() {
    let fixture_path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/transfer.torrent");
    let save_dir = tempfile::tempdir().unwrap();
    let save_path_str = save_dir.path().to_str().unwrap().to_owned();
    let mut atp = AddTorrentParams::from_torrent_file(&fixture_path).unwrap();
    atp.set_save_path(&save_path_str);

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

    let handle = {
        let add_future = session.add_torrent(&atp, std::sync::Arc::new(()));
        tokio::pin!(add_future);
        loop {
            tokio::select! {
                result = &mut add_future => break result.unwrap(),
                batch = alerts.next_batch() => { let _ = batch.unwrap(); }
            }
        }
    };

    assert!(handle.is_valid());
    assert!(handle.in_session());

    let name = handle.name().unwrap();
    assert!(!name.is_empty());

    let save_path = handle.save_path().unwrap();
    assert_eq!(save_path, save_path_str);

    // completion state lives on the status snapshot
    let status = handle.status(0).unwrap();
    assert!(!status.is_seeding());
    assert!(!status.is_finished());
    assert!(status.has_metadata()); // Loaded from .torrent
    assert!(handle.is_paused()); // Starts paused by default
    assert!(handle.is_auto_managed());

    // defaults can be -1 (unlimited) or 0 (use session default)
    let initial_upload = handle.upload_limit();
    let initial_download = handle.download_limit();
    assert!(initial_upload == 0 || initial_upload == -1);
    assert!(initial_download == 0 || initial_download == -1);

    handle.set_upload_limit(1024 * 100).unwrap(); // 100 KB/s
    handle.set_download_limit(1024 * 200).unwrap(); // 200 KB/s
    assert_eq!(handle.upload_limit(), 1024 * 100);
    assert_eq!(handle.download_limit(), 1024 * 200);

    handle.set_max_uploads(10).unwrap();
    handle.set_max_connections(50).unwrap();
    assert_eq!(handle.max_uploads(), 10);
    assert_eq!(handle.max_connections(), 50);

    // Unlimited round-trips as the -1 sentinel, and out-of-domain
    // values are rejected.
    handle.set_max_uploads(-1).unwrap();
    handle.set_max_connections(-1).unwrap();
    assert_eq!(handle.max_uploads(), -1);
    assert_eq!(handle.max_connections(), -1);
    assert!(handle.set_max_uploads(1).is_err());
    assert!(handle.set_max_connections(0).is_err());
    assert!(handle.set_upload_limit(-2).is_err());
    assert!(handle.set_download_limit(-2).is_err());

    let pos = handle.queue_position();
    assert!(pos >= 0 || pos == -1); // Valid or not queued

    let num_pieces = atp.ti().unwrap().num_pieces();
    if num_pieces > 0 {
        // We don't have the pieces, so have_piece should be false
        assert!(!handle.have_piece(0));

        handle
            .set_piece_priority(0, DownloadPriority::TOP)
            .expect("piece 0 exists");
        assert_eq!(
            handle.piece_priority(0).expect("piece 0 exists"),
            DownloadPriority::TOP
        );
        assert!(handle.piece_priority(num_pieces).is_err());
        assert!(
            handle
                .set_piece_priority(-1, DownloadPriority::TOP)
                .is_err()
        );
        assert!(handle.set_piece_deadline(num_pieces, 0, 0).is_err());
    }

    let num_files = atp.ti().unwrap().num_files();
    if num_files > 0 {
        handle
            .set_file_priority(0, DownloadPriority::DEFAULT)
            .unwrap();
        assert_eq!(handle.file_priority(0), DownloadPriority::DEFAULT);
    }

    // smoke: these must not crash
    handle.post_status(0);
    handle.post_trackers();
    handle.post_peer_info();
    assert!(handle.save_resume_data(0));

    let needs_save = handle.need_save_resume_data();
    println!("Needs save resume data: {}", needs_save);

    drop(handle);
    drop(alerts);
    session.close().await;
}

#[tokio::test]
async fn torrent_handle_torrent_file() {
    let fixture_path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/transfer.torrent");
    let save_dir = tempfile::tempdir().unwrap();
    let save_path_str = save_dir.path().to_str().unwrap().to_owned();
    let mut atp = AddTorrentParams::from_torrent_file(&fixture_path).unwrap();
    atp.set_save_path(&save_path_str);

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

    let handle = {
        let add_future = session.add_torrent(&atp, std::sync::Arc::new(()));
        tokio::pin!(add_future);
        loop {
            tokio::select! {
                result = &mut add_future => break result.unwrap(),
                batch = alerts.next_batch() => { let _ = batch.unwrap(); }
            }
        }
    };

    // Metadata came from the .torrent file, so torrent_file() returns it.
    let ti = handle
        .torrent_file()
        .unwrap()
        .expect("metadata was loaded from the fixture");
    assert!(ti.is_valid());
    let original = atp.ti().unwrap();
    assert_eq!(ti.name(), original.name());
    assert_eq!(ti.num_files(), original.num_files());
    assert_eq!(ti.num_pieces(), original.num_pieces());
    assert_eq!(ti.info_hashes(), handle.info_hashes());

    // A magnet-added torrent with no peers has no metadata yet: Ok(None).
    let mut magnet = AddTorrentParams::from_magnet_uri(
        "magnet:?xt=urn:btih:0123456789abcdef0123456789abcdef01234567&dn=meta-less",
    )
    .unwrap();
    magnet.set_save_path(&save_path_str);
    let magnet_handle = {
        let add_future = session.add_torrent(&magnet, std::sync::Arc::new(()));
        tokio::pin!(add_future);
        loop {
            tokio::select! {
                result = &mut add_future => break result.unwrap(),
                batch = alerts.next_batch() => { let _ = batch.unwrap(); }
            }
        }
    };
    assert!(magnet_handle.torrent_file().unwrap().is_none());

    drop(handle);
    drop(magnet_handle);
    drop(alerts);
    session.close().await;
}

#[tokio::test]
async fn torrent_handle_trackers_and_seeds() {
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

    let handle = {
        let add_future = session.add_torrent(&atp, std::sync::Arc::new(()));
        tokio::pin!(add_future);
        loop {
            tokio::select! {
                result = &mut add_future => break result.unwrap(),
                batch = alerts.next_batch() => { let _ = batch.unwrap(); }
            }
        }
    };

    handle.add_tracker("http://test.tracker.local/announce", 0);
    handle.replace_trackers(&[
        ("http://a.tracker.local/announce", 0),
        ("http://b.tracker.local/announce", 1),
    ]);
    handle.replace_trackers(&[]);

    handle.add_url_seed("http://test.local/files/");
    handle.remove_url_seed("http://test.local/files/");

    handle.queue_position_top();
    handle.queue_position_bottom();
    handle.queue_position_up();
    handle.queue_position_down();
    handle.set_queue_position(0).unwrap();
    assert!(handle.set_queue_position(-1).is_err());

    handle.clear_piece_deadlines();
    handle.flush_cache();
    handle.clear_error();

    drop(handle);
    drop(alerts);
    session.close().await;
}

#[tokio::test]
async fn prioritize_rejects_overlong_lists() {
    let fixture_path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/transfer.torrent");
    let save_dir = tempfile::tempdir().unwrap();
    let save_path_str = save_dir.path().to_str().unwrap().to_owned();
    let mut atp = AddTorrentParams::from_torrent_file(&fixture_path).unwrap();
    atp.set_save_path(&save_path_str);

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

    let handle = {
        let add_future = session.add_torrent(&atp, std::sync::Arc::new(()));
        tokio::pin!(add_future);
        loop {
            tokio::select! {
                result = &mut add_future => break result.unwrap(),
                batch = alerts.next_batch() => { let _ = batch.unwrap(); }
            }
        }
    };

    let ti = atp.ti().unwrap();
    let num_pieces = usize::try_from(ti.num_pieces()).unwrap();
    let num_files = usize::try_from(ti.num_files()).unwrap();

    // Exact-length and shorter lists are accepted.
    handle
        .prioritize_pieces(&vec![DownloadPriority::DEFAULT; num_pieces])
        .unwrap();
    handle.prioritize_pieces(&[DownloadPriority::TOP]).unwrap();
    handle
        .prioritize_files(&vec![DownloadPriority::DEFAULT; num_files])
        .unwrap();

    // Longer-than-the-torrent lists are rejected.
    assert!(
        handle
            .prioritize_pieces(&vec![DownloadPriority::DEFAULT; num_pieces + 1])
            .is_err()
    );
    assert!(
        handle
            .prioritize_files(&vec![DownloadPriority::DEFAULT; num_files + 1])
            .is_err()
    );

    // Without metadata (magnet with no peers), both are rejected.
    let mut magnet = AddTorrentParams::from_magnet_uri(
        "magnet:?xt=urn:btih:0123456789abcdef0123456789abcdef01234567&dn=meta-less",
    )
    .unwrap();
    magnet.set_save_path(&save_path_str);
    let magnet_handle = {
        let add_future = session.add_torrent(&magnet, std::sync::Arc::new(()));
        tokio::pin!(add_future);
        loop {
            tokio::select! {
                result = &mut add_future => break result.unwrap(),
                batch = alerts.next_batch() => { let _ = batch.unwrap(); }
            }
        }
    };
    assert!(
        magnet_handle
            .prioritize_pieces(&[DownloadPriority::DEFAULT])
            .is_err()
    );
    assert!(
        magnet_handle
            .prioritize_files(&[DownloadPriority::DEFAULT])
            .is_err()
    );

    drop(handle);
    drop(magnet_handle);
    drop(alerts);
    session.close().await;
}

#[tokio::test]
async fn file_paths_reflect_renames_and_url_seeds_are_queryable() {
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
        .unwrap()
        .alert_mask(i32::MAX);

    let session = Session::new(SessionParams::new().settings(&settings)).unwrap();
    let mut alerts = session.alerts();

    let handle = {
        let add_future = session.add_torrent(&atp, std::sync::Arc::new(()));
        tokio::pin!(add_future);
        loop {
            tokio::select! {
                result = &mut add_future => break result.unwrap(),
                batch = alerts.next_batch() => { let _ = batch.unwrap(); }
            }
        }
    };

    // Web seeds added at runtime show up in the authoritative list.
    let initial = handle.url_seeds().unwrap();
    assert!(!initial.contains(&"http://seed.example/files/".to_owned()));
    handle.add_url_seed("http://seed.example/files/");
    assert!(
        handle
            .url_seeds()
            .unwrap()
            .contains(&"http://seed.example/files/".to_owned())
    );
    handle.remove_url_seed("http://seed.example/files/");
    assert_eq!(handle.url_seeds().unwrap(), initial);

    // Before any rename, the live paths match the metadata paths.
    let info = handle.torrent_file().unwrap().unwrap();
    let original = info.file(0).unwrap().path();
    let paths = handle.file_paths().unwrap().unwrap();
    assert_eq!(paths.len(), usize::try_from(info.num_files()).unwrap());
    assert_eq!(paths[0], original);

    handle.rename_file(0, "renamed_by_test.bin").unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        'outer: loop {
            let batch = alerts.next_batch().await.unwrap();
            for alert in batch.iter() {
                match alert {
                    rbtorrent::Alert::FileRenamed(a) => {
                        assert_eq!(a.index(), 0);
                        break 'outer;
                    }
                    rbtorrent::Alert::FileRenameFailed(a) => {
                        panic!("rename failed: {:?}", a.error());
                    }
                    _ => {}
                }
            }
        }
    })
    .await
    .expect("file_renamed_alert did not arrive");

    // The live paths carry the rename; the metadata keeps the original.
    let paths = handle.file_paths().unwrap().unwrap();
    assert_eq!(paths[0], "renamed_by_test.bin");
    let info = handle.torrent_file().unwrap().unwrap();
    assert_eq!(info.file(0).unwrap().path(), original);

    drop(handle);
    drop(alerts);
    session.close().await;
}
