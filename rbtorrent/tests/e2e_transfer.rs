// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use rbtorrent::{AddTorrentParams, Session, SessionParams, SettingsPack, TorrentFlags};
use std::path::PathBuf;
use std::time::Duration;
use tokio::time::timeout;

/// Regenerates the fixture content using the same xorshift PRNG from
/// gen_fixtures.cpp.
fn generate_fixture_content() -> Vec<u8> {
    fn xorshift(state: &mut u32) -> u8 {
        *state ^= *state << 13;
        *state ^= *state >> 17;
        *state ^= *state << 5;
        (*state & 0xff) as u8
    }

    let mut content = Vec::with_capacity(40960 + 137);
    let mut state = 0xdecafbad_u32;
    for _ in 0..40960 {
        content.push(xorshift(&mut state));
    }
    state = 0xb0bafe77_u32;
    for _ in 0..137 {
        content.push(xorshift(&mut state));
    }
    content
}

#[tokio::test]
async fn localhost_transfer() {
    // transfer.torrent is the hermetic fixture: no trackers, no web seeds,
    // so neither session has any reason to touch the network beyond the
    // explicit localhost peer connection below.
    let fixture_path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/transfer.torrent");
    let ti = AddTorrentParams::from_torrent_file(&fixture_path)
        .unwrap()
        .ti()
        .unwrap();

    // Seed session: write the content to a temp directory and add the torrent.
    let seed_tmp = tempfile::tempdir().unwrap();
    let seed_dir = seed_tmp.path().to_path_buf();
    let fixture_dir = seed_dir.join("fixture");
    std::fs::create_dir_all(&fixture_dir).unwrap();
    let content = generate_fixture_content();
    std::fs::write(fixture_dir.join("a.bin"), &content[..40960]).unwrap();
    std::fs::write(fixture_dir.join("b.txt"), &content[40960..]).unwrap();

    let mut seed_settings = SettingsPack::new();
    seed_settings
        .enable_dht(false)
        .enable_lsd(false)
        .enable_upnp(false)
        .enable_natpmp(false)
        .listen_interfaces(&[rbtorrent::ListenEndpoint::new("127.0.0.1", 0)])
        .unwrap()
        .alert_mask(
            (rbtorrent::AlertCategory::ERROR
                | rbtorrent::AlertCategory::STATUS
                | rbtorrent::AlertCategory::PEER
                | rbtorrent::AlertCategory::CONNECT
                | rbtorrent::AlertCategory::STORAGE)
                .bits_i32(),
        );

    let seed_params = SessionParams::new().settings(&seed_settings);
    let seed_session = Session::new(seed_params).unwrap();
    let mut seed_alerts = seed_session.alerts();

    let mut seed_atp = AddTorrentParams::new();
    seed_atp
        .set_ti(&ti)
        .set_save_path(seed_dir.to_str().unwrap())
        .set_flags(
            TorrentFlags::UPDATE_SUBSCRIBE
                | TorrentFlags::AUTO_MANAGED
                | TorrentFlags::SEED_MODE
                | TorrentFlags::APPLY_IP_FILTER
                | TorrentFlags::NEED_SAVE_RESUME,
        );

    let _seed_handle = {
        let seed_add_future = seed_session.add_torrent(&seed_atp);
        tokio::pin!(seed_add_future);
        loop {
            tokio::select! {
                result = &mut seed_add_future => {
                    eprintln!("[seed] add_torrent completed");
                    break result.unwrap();
                }
                batch = seed_alerts.next_batch() => {
                    let batch = batch.unwrap();
                    for alert in batch.iter() {
                        eprintln!("[seed-setup] {}", alert.raw().message());
                    }
                }
            }
        }
    };
    eprintln!(
        "[seed] Listening on port {}",
        seed_session.listen_port().unwrap()
    );

    // Leech session: download to another temp directory.
    let leech_tmp = tempfile::tempdir().unwrap();
    let leech_dir = leech_tmp.path().to_path_buf();

    let mut leech_settings = SettingsPack::new();
    leech_settings
        .enable_dht(false)
        .enable_lsd(false)
        .enable_upnp(false)
        .enable_natpmp(false)
        .listen_interfaces(&[rbtorrent::ListenEndpoint::new("127.0.0.1", 0)])
        .unwrap()
        .alert_mask(
            (rbtorrent::AlertCategory::ERROR
                | rbtorrent::AlertCategory::STATUS
                | rbtorrent::AlertCategory::PEER
                | rbtorrent::AlertCategory::CONNECT
                | rbtorrent::AlertCategory::STORAGE)
                .bits_i32(),
        );

    let leech_params = SessionParams::new().settings(&leech_settings);
    let leech_session = Session::new(leech_params).unwrap();
    let mut leech_alerts = leech_session.alerts();

    let mut leech_atp = AddTorrentParams::new();
    leech_atp
        .set_ti(&ti)
        .set_save_path(leech_dir.to_str().unwrap())
        .set_flags(
            TorrentFlags::UPDATE_SUBSCRIBE
                | TorrentFlags::AUTO_MANAGED
                | TorrentFlags::APPLY_IP_FILTER
                | TorrentFlags::NEED_SAVE_RESUME,
        );

    let leech_handle = {
        let leech_add_future = leech_session.add_torrent(&leech_atp);
        tokio::pin!(leech_add_future);
        loop {
            tokio::select! {
                result = &mut leech_add_future => {
                    eprintln!("[leech] add_torrent completed");
                    break result.unwrap();
                }
                batch = leech_alerts.next_batch() => {
                    let batch = batch.unwrap();
                    for alert in batch.iter() {
                        eprintln!("[leech-setup] {}", alert.raw().message());
                    }
                }
            }
        }
    };
    eprintln!(
        "[leech] Listening on port {}",
        leech_session.listen_port().unwrap()
    );

    let seed_port = seed_session.listen_port().unwrap();
    leech_handle
        .connect_peer(format!("127.0.0.1:{}", seed_port).parse().unwrap())
        .unwrap();

    let result = timeout(Duration::from_secs(30), async {
        loop {
            tokio::select! {
                batch = seed_alerts.next_batch() => {
                    let batch = batch.unwrap();
                    for alert in batch.iter() {
                        eprintln!("[seed] {} (cat={:?})", alert.raw().message(), alert.raw().category());
                    }
                }
                batch = leech_alerts.next_batch() => {
                    let batch = batch.unwrap();
                    for alert in batch.iter() {
                        eprintln!("[leech] {} (cat={:?})", alert.raw().message(), alert.raw().category());
                        if let rbtorrent::Alert::TorrentFinished(_) = alert {
                            eprintln!("[leech] ✓ Transfer complete!");
                            return;
                        }
                    }
                }
            }
        }
    })
    .await;

    assert!(result.is_ok(), "Transfer timed out");

    // The leech now has every piece; QUERY_PIECES exposes the bitfield.
    let status = leech_handle
        .status(rbtorrent::TorrentHandle::QUERY_PIECES)
        .unwrap();
    let pieces = status.pieces().expect("pieces bitfield");
    assert_eq!(pieces.len() as i32, ti.num_pieces());
    assert_eq!(pieces.count_ones(), pieces.len());

    eprintln!("[test] Files in leech_dir:");
    for entry in std::fs::read_dir(&leech_dir).unwrap() {
        let entry = entry.unwrap();
        eprintln!("[test]   {:?}", entry.path());
    }
    if leech_dir.join("fixture").exists() {
        eprintln!("[test] Files in fixture/:");
        for entry in std::fs::read_dir(leech_dir.join("fixture")).unwrap() {
            let entry = entry.unwrap();
            eprintln!("[test]   {:?}", entry.path());
        }
    }

    // Disk writes are asynchronous, so poll (bounded) for the content to
    // match instead of hoping a fixed sleep outlasts the flush.
    let expected_a = &content[..40960];
    let expected_b = &content[40960..];
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        let a = std::fs::read(leech_dir.join("fixture/a.bin")).unwrap_or_default();
        let b = std::fs::read(leech_dir.join("fixture/b.txt")).unwrap_or_default();
        if a == expected_a && b == expected_b {
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "downloaded files did not match on disk within 10s (a: {} bytes, b: {} bytes)",
            a.len(),
            b.len()
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    // Obtain real resume data through the alert workflow: request it, pull
    // the response off the stream, and check it reflects the completed
    // download (not just the params we added with).
    assert!(leech_handle.save_resume_data(rbtorrent::TorrentHandle::RESUME_SAVE_INFO_DICT));
    let resume_params = timeout(Duration::from_secs(10), async {
        loop {
            let batch = leech_alerts.next_batch().await.unwrap();
            for alert in batch.iter() {
                match alert {
                    rbtorrent::Alert::SaveResumeData(a) => return a.params(),
                    rbtorrent::Alert::SaveResumeDataFailed(a) => {
                        panic!("save_resume_data failed: {:?}", a.error())
                    }
                    _ => {}
                }
            }
        }
    })
    .await
    .expect("no save_resume_data alert");

    let have = resume_params.have_pieces();
    assert_eq!(have.len() as i32, ti.num_pieces());
    assert!(have.iter().all(|&b| b), "resume data records all pieces");

    // And it survives the bencode roundtrip. (Compare against the torrent
    // metadata: the original atp only carried `ti`, so its own info_hashes
    // field is empty. The bencoded piece bitmask is byte-granular, so the
    // decoded bitfield may carry up to 7 trailing false bits.)
    let resume = Session::write_resume_data(&resume_params).unwrap();
    let restored = Session::read_resume_data(&resume, None).unwrap();
    assert_eq!(restored.info_hashes(), ti.info_hashes());
    let restored_have = restored.have_pieces();
    assert!(restored_have.len() >= have.len());
    assert_eq!(restored_have[..have.len()], have[..]);
    assert!(restored_have[have.len()..].iter().all(|&b| !b));

    // Drop handles and alerts before closing sessions (both borrow them)
    drop(_seed_handle);
    drop(leech_handle);
    drop(seed_alerts);
    drop(leech_alerts);

    seed_session.close().await;
    leech_session.close().await;

    // seed_tmp/leech_tmp clean up on drop, only after the sessions have
    // released the files.
}
