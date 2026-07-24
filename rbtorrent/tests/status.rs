// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use rbtorrent::{AddTorrentParams, Session, SessionParams, SettingsPack};
use std::path::PathBuf;

#[tokio::test]
async fn torrent_status_accessors() {
    // transfer.torrent is trackerless and web-seedless, so the live session
    // never contacts external hosts.
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

    let handle = {
        let add_future = session.add_torrent(&atp);
        tokio::pin!(add_future);
        loop {
            tokio::select! {
                result = &mut add_future => break result.unwrap(),
                batch = alerts.next_batch() => { let _ = batch.unwrap(); }
            }
        }
    };

    let status = handle.status(0).unwrap();

    assert!(status.has_metadata());
    assert!(!status.is_seeding());
    assert!(!status.is_finished());

    // full name/save_path population needs status query flags
    let name = status.name();
    let save_path = status.save_path();
    println!("Name: {}, Save path: {}", name, save_path);

    let progress = status.progress();
    assert!((0.0..=1.0).contains(&progress));
    println!("Progress: {:.2}%", progress * 100.0);

    println!(
        "Seeds: {}, Peers: {}",
        status.num_seeds(),
        status.num_peers()
    );
    println!("Queue position: {}", status.queue_position());

    use rbtorrent::TorrentState;
    let state = status.state();
    println!("State: {:?}", state);
    assert!(matches!(
        state,
        TorrentState::CheckingFiles
            | TorrentState::DownloadingMetadata
            | TorrentState::Downloading
            | TorrentState::Finished
            | TorrentState::Seeding
            | TorrentState::CheckingResumeData
    ));

    use rbtorrent::StatusStorageMode;
    let storage_mode = status.storage_mode();
    println!("Storage mode: {:?}", storage_mode);
    assert!(matches!(
        storage_mode,
        StatusStorageMode::Allocate | StatusStorageMode::Sparse
    ));

    println!(
        "Downloaded: {} bytes, Uploaded: {} bytes",
        status.total_download(),
        status.total_upload()
    );
    println!(
        "Download rate: {} B/s, Upload rate: {} B/s",
        status.download_rate(),
        status.upload_rate()
    );

    println!(
        "Total size: {} bytes, Done: {} bytes",
        status.total(),
        status.total_done()
    );

    println!(
        "Upload limit: {}, Download limit: {}",
        status.upload_limit(),
        status.download_limit()
    );
    println!(
        "Connections: {} / {}",
        status.num_connections(),
        status.connections_limit()
    );

    drop(handle);
    drop(alerts);
    session.close().await;
}

#[tokio::test]
async fn torrent_status_during_transfer() {
    // transfer.torrent is trackerless and web-seedless, so only the
    // explicit localhost peer connection below touches the network.
    let fixture_path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/transfer.torrent");

    // Seeder session, serving a scratch copy of the payload so the
    // checked-in fixtures stay untouched (a recheck in the repo tree
    // would also race parallel tests over the shared path).
    let seed_dir = tempfile::tempdir().unwrap();
    let payload_src = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fixture");
    std::fs::create_dir_all(seed_dir.path().join("fixture")).unwrap();
    for name in ["a.bin", "b.txt"] {
        std::fs::copy(
            payload_src.join(name),
            seed_dir.path().join("fixture").join(name),
        )
        .unwrap();
    }
    let mut seed_atp = AddTorrentParams::from_torrent_file(&fixture_path).unwrap();
    seed_atp.set_save_path(seed_dir.path().to_str().unwrap());

    let mut seed_settings = SettingsPack::new();
    seed_settings
        .enable_dht(false)
        .enable_lsd(false)
        .enable_upnp(false)
        .enable_natpmp(false)
        .alert_mask(i32::MAX)
        .listen_interfaces(&[rbtorrent::ListenEndpoint::new("127.0.0.1", 0)])
        .unwrap();

    let seed_params = SessionParams::new().settings(&seed_settings);
    let seed_session = Session::new(seed_params).unwrap();
    let mut seed_alerts = seed_session.alerts();

    let seed_handle = {
        let add_seed = seed_session.add_torrent(&seed_atp);
        tokio::pin!(add_seed);
        loop {
            tokio::select! {
                result = &mut add_seed => break result.unwrap(),
                batch = seed_alerts.next_batch() => { let _ = batch.unwrap(); }
            }
        }
    };

    // Force recheck so the seeder knows it has all the data, and wait
    // for the torrent_checked alert before resuming.
    seed_handle.force_recheck();
    let checked = async {
        loop {
            let batch = seed_alerts.next_batch().await.unwrap();
            if batch
                .iter()
                .any(|a| a.raw().alert_type() == Some(rbtorrent::AlertType::TorrentChecked))
            {
                break;
            }
        }
    };
    tokio::time::timeout(tokio::time::Duration::from_secs(15), checked)
        .await
        .expect("recheck did not complete");

    seed_handle.resume();
    let seed_port = seed_session.listen_port().unwrap();

    // Leecher session
    let leech_dir = tempfile::tempdir().unwrap();
    let mut leech_atp = AddTorrentParams::from_torrent_file(&fixture_path).unwrap();
    leech_atp.set_save_path(leech_dir.path().to_str().unwrap());

    let mut leech_settings = SettingsPack::new();
    leech_settings
        .enable_dht(false)
        .enable_lsd(false)
        .enable_upnp(false)
        .enable_natpmp(false)
        .alert_mask(i32::MAX)
        .listen_interfaces(&[rbtorrent::ListenEndpoint::new("127.0.0.1", 0)])
        .unwrap();

    let leech_params = SessionParams::new().settings(&leech_settings);
    let leech_session = Session::new(leech_params).unwrap();
    let mut leech_alerts = leech_session.alerts();

    let leech_handle = {
        let add_leech = leech_session.add_torrent(&leech_atp);
        tokio::pin!(add_leech);
        loop {
            tokio::select! {
                result = &mut add_leech => break result.unwrap(),
                batch = leech_alerts.next_batch() => { let _ = batch.unwrap(); }
            }
        }
    };

    let seed_addr = format!("127.0.0.1:{}", seed_port).parse().unwrap();
    leech_handle.connect_peer(seed_addr).unwrap();
    leech_handle.resume();

    let timeout = tokio::time::sleep(tokio::time::Duration::from_secs(30));
    tokio::pin!(timeout);

    let mut finished = false;
    loop {
        tokio::select! {
            _ = &mut timeout => {
                println!("Transfer timeout");
                break;
            }
            batch = seed_alerts.next_batch() => {
                let _ = batch.unwrap();
            }
            batch = leech_alerts.next_batch() => {
                let batch = batch.unwrap();
                use rbtorrent::Alert;
                for alert in batch.iter() {
                    if matches!(alert, Alert::TorrentFinished(_)) {
                        finished = true;
                    }
                }

                if let Ok(status) = leech_handle.status(0) {
                    println!("Progress: {:.1}%, DL: {} B/s, UL: {} B/s, Peers: {}, Seeds: {}",
                        status.progress() * 100.0,
                        status.download_rate(),
                        status.upload_rate(),
                        status.num_peers(),
                        status.num_seeds()
                    );

                    if status.is_finished() {
                        println!("Transfer complete!");
                        finished = true;
                    }
                }

                if finished {
                    break;
                }
            }
        }
    }

    let final_status = leech_handle.status(0).unwrap();
    assert!(final_status.is_finished() || finished);
    assert!(final_status.progress() >= 0.99);
    println!(
        "Final: {} / {} bytes",
        final_status.total_done(),
        final_status.total()
    );

    drop(seed_handle);
    drop(leech_handle);
    drop(seed_alerts);
    drop(leech_alerts);
    seed_session.close().await;
    leech_session.close().await;
}
