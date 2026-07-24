// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Async alert stream and request-registry tests.

use std::time::Duration;

use rbtorrent::{Alert, SaveStateFlags, Session, SessionParams, SettingsPack};

fn test_params() -> SessionParams {
    let mut settings = SettingsPack::new();
    settings
        .listen_interfaces(&[rbtorrent::ListenEndpoint::new("127.0.0.1", 0)])
        .unwrap()
        .enable_dht(false)
        .enable_lsd(false)
        .enable_upnp(false)
        .enable_natpmp(false)
        .alert_mask(rbtorrent::AlertCategory::ALL.bits_i32());
    SessionParams::new().settings(&settings)
}

#[tokio::test]
async fn await_listen_alert() {
    let session = Session::new(test_params()).expect("session");
    let mut alerts = session.alerts();

    let found = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let batch = alerts.next_batch().await.expect("pop");
            for alert in batch.iter() {
                if let Alert::ListenSucceeded(listen) = alert {
                    let endpoint = listen.endpoint();
                    assert!(endpoint.ip().is_loopback());
                    assert!(endpoint.port() > 0);
                    // base accessors through Deref
                    assert_eq!(listen.what(), "listen_succeeded");
                    assert!(!listen.message().is_empty());
                    return;
                }
            }
        }
    })
    .await;
    assert!(found.is_ok(), "no listen_succeeded within timeout");

    drop(alerts);
    session.close().await;
}

#[tokio::test]
async fn session_stats_future_resolves_while_polling() {
    let session = Session::new(test_params()).expect("session");
    let mut stats = std::pin::pin!(session.session_stats());

    let mut alerts = session.alerts();
    let counters = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            tokio::select! {
                // resolution happens as a side effect of the polling arm
                res = &mut stats => return res.expect("stats"),
                batch = alerts.next_batch() => drop(batch.expect("pop")),
            }
        }
    })
    .await
    .expect("stats future did not resolve");
    assert!(!counters.is_empty());
    // num counters is stable within a libtorrent version and > 100
    assert!(counters.len() > 100, "{}", counters.len());

    drop(alerts);
    session.close().await;
}

#[tokio::test]
async fn session_stats_fails_after_close() {
    let session = Session::new(test_params()).expect("session");
    let stats = session.session_stats();
    session.close().await;
    let err = stats.await.expect_err("must fail after close");
    assert_eq!(err.category(), rbtorrent::Category::Bindings);
}

#[tokio::test]
async fn state_save_load_roundtrip() {
    let session = Session::new(test_params()).expect("session");
    let mut delta = SettingsPack::new();
    delta.user_agent("rbtorrent-state-test");
    session.apply_settings(&delta).unwrap();
    // give the setting a moment to apply on the session thread
    let state = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let state = session.save_state(SaveStateFlags::all()).unwrap();
            if !state.is_empty() {
                let effective = session.settings().unwrap();
                if effective.get_user_agent().as_deref() == Some("rbtorrent-state-test") {
                    return session.save_state(SaveStateFlags::all()).unwrap();
                }
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("state never reflected the setting");
    session.close().await;

    // SAFETY: the blob was produced by save_state just above.
    let params =
        unsafe { test_params().load_state(&state, SaveStateFlags::all()) }.expect("load_state");
    let restored = Session::new(params).expect("restored session");
    assert_eq!(
        restored.settings().unwrap().get_user_agent().as_deref(),
        Some("rbtorrent-state-test")
    );
    restored.close().await;

    // load_state replaces only the fields selected by the flags: loading
    // without SETTINGS must not clobber settings applied to the params.
    let mut local = SettingsPack::new();
    local
        .listen_interfaces(&[rbtorrent::ListenEndpoint::new("127.0.0.1", 0)])
        .unwrap()
        .enable_dht(false)
        .enable_lsd(false)
        .enable_upnp(false)
        .enable_natpmp(false)
        .user_agent("rbtorrent-local");
    // SAFETY: the blob was produced by save_state just above.
    let params = unsafe {
        SessionParams::new()
            .settings(&local)
            .load_state(&state, SaveStateFlags::DHT_STATE)
    }
    .expect("load_state");
    let session = Session::new(params).expect("session");
    assert_eq!(
        session.settings().unwrap().get_user_agent().as_deref(),
        Some("rbtorrent-local")
    );
    session.close().await;
}

#[tokio::test]
async fn alerts_can_be_retaken_after_drop() {
    let session = Session::new(test_params()).expect("session");
    let alerts = session.alerts();
    drop(alerts);
    let alerts = session.alerts();
    drop(alerts);
    session.close().await;
}

#[tokio::test]
#[should_panic(expected = "another Alerts receiver exists")]
async fn double_take_panics() {
    let session = Session::new(test_params()).expect("session");
    let _first = session.alerts();
    let _second = session.alerts();
}

/// The fire-and-forget request/response workflow advertised by the crate
/// docs is real: `save_resume_data` and every `post_*` request has a typed
/// response view, and each response can be attributed to its torrent via
/// `RawAlert::torrent_handle()`.
#[tokio::test]
async fn post_responses_have_typed_views() {
    let fixtures = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let dir = tempfile::tempdir().unwrap();
    let mut atp =
        rbtorrent::AddTorrentParams::from_torrent_file(fixtures.join("transfer.torrent")).unwrap();
    atp.set_save_path(dir.path().to_str().unwrap());

    let session = Session::new(test_params()).expect("session");
    let mut alerts = session.alerts();

    let handle = {
        let add = session.add_torrent(&atp);
        tokio::pin!(add);
        tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                tokio::select! {
                    r = &mut add => return r.unwrap(),
                    batch = alerts.next_batch() => { batch.unwrap(); }
                }
            }
        })
        .await
        .expect("add did not resolve")
    };

    // Fire all requests, then collect the responses off the stream.
    assert!(handle.save_resume_data(rbtorrent::TorrentHandle::RESUME_SAVE_INFO_DICT));
    session.post_torrent_updates(0).unwrap();
    handle.post_file_progress(0);
    handle.add_tracker("http://tracker.invalid/announce", 3);
    handle.post_trackers();
    handle.post_peer_info();

    let mut resume: Option<rbtorrent::AddTorrentParams> = None;
    let (mut update, mut progress, mut trackers, mut peers) = (false, false, false, false);
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let batch = alerts.next_batch().await.unwrap();
            for alert in batch.iter() {
                match alert {
                    Alert::SaveResumeData(a) => {
                        assert_eq!(
                            a.torrent_handle().unwrap().info_hashes(),
                            atp.info_hashes(),
                            "response attributable to its torrent"
                        );
                        resume = Some(a.params());
                    }
                    Alert::SaveResumeDataFailed(a) => {
                        panic!("save_resume_data failed: {:?}", a.error())
                    }
                    Alert::StateUpdate(a) => {
                        // the freshly added torrent counts as changed
                        let statuses = a.statuses();
                        assert_eq!(statuses.len(), a.len());
                        assert!(
                            statuses
                                .iter()
                                .any(|s| { s.info_hashes() == atp.info_hashes() })
                        );
                        update = true;
                    }
                    Alert::FileProgress(a) => {
                        assert_eq!(a.progress().len(), 2, "fixture has two files");
                        assert_eq!(a.torrent_handle().unwrap().info_hashes(), atp.info_hashes());
                        progress = true;
                    }
                    Alert::TrackerList(a) => {
                        assert_eq!(a.iter().count(), a.len());
                        let entry = a
                            .iter()
                            .find(|t| t.url == "http://tracker.invalid/announce")
                            .expect("added tracker in the list");
                        assert_eq!(entry.tier, 3);
                        trackers = true;
                    }
                    Alert::PeerInfo(a) => {
                        // no peers connected; the response still decodes
                        assert_eq!(a.len(), 0);
                        assert!(a.peers().is_empty());
                        peers = true;
                    }
                    _ => {}
                }
            }
            if resume.is_some() && update && progress && trackers && peers {
                return;
            }
        }
    })
    .await
    .expect("not every response alert arrived");

    // The resume data extracted from the alert survives the bencode
    // round-trip with its identity intact.
    let params = resume.unwrap();
    assert_eq!(params.info_hashes(), atp.info_hashes());
    let blob = Session::write_resume_data(&params).unwrap();
    let restored = Session::read_resume_data(&blob, None).unwrap();
    assert_eq!(restored.info_hashes(), atp.info_hashes());

    drop(handle);
    drop(alerts);
    session.close().await;
}

/// Two adds in flight on one session must resolve to their own torrents:
/// pins the client_data_t token round-trip (a mistyped `get<void*>()` in the
/// shim used to make every token read back as 0, cross-wiring concurrent
/// adds).
#[tokio::test]
async fn concurrent_add_torrents_resolve_independently() {
    let fixtures = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let dir_a = tempfile::tempdir().unwrap();
    let dir_b = tempfile::tempdir().unwrap();

    let mut atp_a =
        rbtorrent::AddTorrentParams::from_torrent_file(fixtures.join("transfer.torrent")).unwrap();
    atp_a.set_save_path(dir_a.path().to_str().unwrap());
    let mut atp_b =
        rbtorrent::AddTorrentParams::from_torrent_file(fixtures.join("v2.torrent")).unwrap();
    atp_b.set_save_path(dir_b.path().to_str().unwrap());
    // The v2 fixture embeds example.com endpoints; keep the session
    // hermetic (the test only needs a second, distinct torrent).
    atp_b.clear_trackers();
    atp_b.clear_url_seeds();

    let session = Session::new(test_params()).expect("session");
    let mut alerts = session.alerts();

    let (a, b) = {
        let add_a = session.add_torrent(&atp_a);
        let add_b = session.add_torrent(&atp_b);
        tokio::pin!(add_a, add_b);

        let (mut got_a, mut got_b) = (None, None);
        let done = tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                tokio::select! {
                    r = &mut add_a, if got_a.is_none() => got_a = Some(r.unwrap()),
                    r = &mut add_b, if got_b.is_none() => got_b = Some(r.unwrap()),
                    batch = alerts.next_batch() => { batch.unwrap(); }
                }
                if got_a.is_some() && got_b.is_some() {
                    return;
                }
            }
        })
        .await;
        assert!(done.is_ok(), "concurrent adds did not both resolve");
        (got_a.unwrap(), got_b.unwrap())
    };
    assert_eq!(a.info_hashes(), atp_a.info_hashes(), "handle A mismatched");
    assert_eq!(b.info_hashes(), atp_b.info_hashes(), "handle B mismatched");

    drop(a);
    drop(b);
    drop(alerts);
    session.close().await;
}
