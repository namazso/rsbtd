// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Engine-level integration tests: add/transfer/persist/restore without
//! the HTTP layer. Fully hermetic: loopback only, DHT/LSD/UPnP disabled.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use rbtorrent::{AddTorrentParams, SettingsPack, TorrentFlags};
use rsbtd::config::{Config, Listen};
use rsbtd::engine::events::{Event, EventKind};
use rsbtd::engine::{Engine, EngineError};
use tokio::sync::broadcast;
use tokio::time::timeout;
use uuid::Uuid;

fn hermetic_settings() -> SettingsPack {
    let mut pack = SettingsPack::new();
    pack.enable_dht(false)
        .enable_lsd(false)
        .enable_upnp(false)
        .enable_natpmp(false)
        .listen_interfaces(&[rbtorrent::ListenEndpoint::new("127.0.0.1", 0)])
        .unwrap();
    pack
}

fn test_config(state_dir: &Path) -> Config {
    Config {
        state_dir: state_dir.to_path_buf(),
        listen: Listen::Tcp("127.0.0.1:0".parse().unwrap()),
        token: None,
        graphiql: false,
        serve_root: None,
        cors: Vec::new(),
        shutdown_grace_secs: 15,
    }
}

// Built from components: libtorrent opens paths natively, and native
// Windows path handling accepts neither `..` nor forward slashes.
fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("rbtorrent")
        .join("tests")
        .join("fixtures")
}

fn fixture_path() -> PathBuf {
    fixtures_dir().join("transfer.torrent")
}

/// Regenerates the fixture content (same xorshift PRNG as gen_fixtures.cpp).
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

/// Writes the fixture's payload files under `dir` so a torrent added with
/// `SEED_MODE` can seed them.
fn write_fixture_content(dir: &Path) -> Vec<u8> {
    let content = generate_fixture_content();
    let fixture_dir = dir.join("fixture");
    std::fs::create_dir_all(&fixture_dir).unwrap();
    std::fs::write(fixture_dir.join("a.bin"), &content[..40960]).unwrap();
    std::fs::write(fixture_dir.join("b.txt"), &content[40960..]).unwrap();
    content
}

async fn wait_for_event(
    rx: &mut broadcast::Receiver<Arc<Event>>,
    mut pred: impl FnMut(&Event) -> bool,
) {
    timeout(Duration::from_secs(30), async {
        loop {
            let event = rx.recv().await.expect("event bus closed");
            if pred(&event) {
                return;
            }
        }
    })
    .await
    .expect("timed out waiting for event");
}

async fn wait_until(mut pred: impl FnMut() -> bool) {
    timeout(Duration::from_secs(30), async {
        while !pred() {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("timed out waiting for condition");
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

#[tokio::test(flavor = "multi_thread")]
async fn transfer_persist_restore() {
    let seed_state = tempfile::tempdir().unwrap();
    let seed_data = tempfile::tempdir().unwrap();
    let leech_state = tempfile::tempdir().unwrap();
    let leech_data = tempfile::tempdir().unwrap();

    let content = write_fixture_content(seed_data.path());

    // --- seed engine -------------------------------------------------------
    let seed = Engine::start(&test_config(seed_state.path()), Some(hermetic_settings()))
        .await
        .unwrap();
    let mut seed_atp = AddTorrentParams::from_torrent_file(fixture_path()).unwrap();
    seed_atp.set_save_path(seed_data.path().to_str().unwrap());
    seed_atp.set_flags(seed_atp.flags() | TorrentFlags::SEED_MODE | TorrentFlags::UPDATE_SUBSCRIBE);
    let seed_entry = seed.add_torrent(&mut seed_atp).await.unwrap();
    assert!(seed.with_handle(&seed_entry, |h| h.is_valid()).unwrap());
    assert_eq!(seed.registry().len(), 1);

    // Duplicate adds are an error.
    let mut dup = AddTorrentParams::from_torrent_file(fixture_path()).unwrap();
    dup.set_save_path(seed_data.path().to_str().unwrap());
    assert!(seed.add_torrent(&mut dup).await.is_err());

    // --- leech engine ------------------------------------------------------
    let leech = Engine::start(&test_config(leech_state.path()), Some(hermetic_settings()))
        .await
        .unwrap();
    let mut leech_events = leech.subscribe_events();
    let mut leech_atp = AddTorrentParams::from_torrent_file(fixture_path()).unwrap();
    leech_atp.set_save_path(leech_data.path().to_str().unwrap());
    let leech_entry = leech.add_torrent(&mut leech_atp).await.unwrap();
    let uuid = leech_entry.uuid;
    let v1 = leech
        .with_handle(&leech_entry, |h| h.info_hashes())
        .unwrap()
        .v1();

    // The initial resume snapshot lands on disk shortly after the add.
    wait_for_event(&mut leech_events, |e| {
        matches!(e.kind, EventKind::ResumeDataSaved)
    })
    .await;
    let resume_file = leech_state
        .path()
        .join("torrents")
        .join(format!("{uuid}.resume"));
    assert!(resume_file.exists(), "initial resume snapshot missing");
    let bytes = std::fs::read(&resume_file).unwrap();
    assert!(
        contains(&bytes, b"8:rbt-data"),
        "initial resume snapshot lacks the client-data key"
    );
    assert!(
        contains(&bytes, uuid.as_bytes()),
        "initial resume snapshot lacks the uuid"
    );

    // --- transfer ----------------------------------------------------------
    let seed_port = seed.listen_port().unwrap();
    leech
        .with_handle(&leech_entry, |h| {
            h.connect_peer(format!("127.0.0.1:{seed_port}").parse().unwrap())
        })
        .unwrap()
        .unwrap();
    wait_for_event(&mut leech_events, |e| {
        matches!(e.kind, EventKind::TorrentFinished)
    })
    .await;

    // Settings applied through the engine persist across restarts.
    let mut tweak = SettingsPack::new();
    tweak.upload_rate_limit(123_456);
    leech.apply_settings(&mut tweak).await.unwrap();

    leech.save_resume_data(&leech_entry).await.unwrap();

    // --- restart the leech and verify restore ------------------------------
    drop(leech_events);
    leech.shutdown().await;
    drop(leech);

    let leech2 = Engine::start(&test_config(leech_state.path()), Some(hermetic_settings()))
        .await
        .unwrap();
    assert_eq!(leech2.registry().len(), 1, "torrent was not restored");
    // The uuid is the durable identity: same key finds it after restart.
    let restored = leech2.registry().find(&uuid).expect("restored entry");
    assert_eq!(
        leech2
            .with_handle(&restored, |h| h.info_hashes())
            .unwrap()
            .v1(),
        v1
    );
    assert_eq!(
        leech2.settings().unwrap().get_upload_rate_limit(),
        Some(123_456),
        "settings did not survive the restart"
    );
    // The save-alert writer preserved the client-data key.
    assert!(
        contains(&std::fs::read(&resume_file).unwrap(), b"8:rbt-data"),
        "saved resume data lacks the client-data key"
    );

    // The restored torrent has its data and reaches seeding state.
    wait_until(|| {
        leech2
            .with_handle(&restored, |h| {
                h.status(0).map(|s| s.is_seeding()).unwrap_or(false)
            })
            .unwrap_or(false)
    })
    .await;
    let a = std::fs::read(leech_data.path().join("fixture/a.bin")).unwrap();
    let b = std::fs::read(leech_data.path().join("fixture/b.txt")).unwrap();
    assert_eq!(a, content[..40960]);
    assert_eq!(b, content[40960..]);

    // --- remove ------------------------------------------------------------
    leech2.remove_torrent(&uuid, false).await.unwrap();
    assert_eq!(leech2.registry().len(), 0);
    wait_until(|| !resume_file.exists()).await;
    // Files stay on disk without delete_files.
    assert!(leech_data.path().join("fixture/a.bin").exists());

    // Removing an unknown torrent reports NotFound.
    assert!(leech2.remove_torrent(&Uuid::new_v4(), false).await.is_err());

    leech2.shutdown().await;
    seed.shutdown().await;

    // Shutdown leaves a session state behind.
    assert!(seed_state.path().join("session.state").exists());
}

/// The alert mask and queue size are daemon-owned constants: no initial
/// pack, persisted session state, or runtime settings delta may move
/// them (the no-drop invariant depends on the queue size, and events
/// depend on exactly the pinned mask categories).
#[tokio::test(flavor = "multi_thread")]
async fn alert_settings_are_pinned() {
    use rbtorrent::{SaveStateFlags, Session, SessionParams};
    use rsbtd::engine::{ALERT_MASK, ALERT_QUEUE_SIZE};

    let state = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    write_fixture_content(data.path());

    // Forge a session state carrying foreign values for both settings.
    let mut old = hermetic_settings();
    old.alert_queue_size(8192).unwrap().alert_mask(0x7fff_ffff);
    let session = Session::new(SessionParams::new().settings(&old)).unwrap();
    let bytes = session.save_state(SaveStateFlags::SETTINGS).unwrap();
    session.close().await;
    std::fs::write(state.path().join("session.state"), bytes).unwrap();

    // Neither a hostile initial pack nor the restored blob can move them.
    let mut settings = hermetic_settings();
    settings.alert_queue_size(16).unwrap().alert_mask(0);
    let engine = Engine::start(&test_config(state.path()), Some(settings))
        .await
        .unwrap();
    let effective = engine.settings().unwrap();
    assert_eq!(effective.get_alert_mask(), Some(ALERT_MASK.bits_i32()));
    assert_eq!(effective.get_alert_queue_size(), Some(ALERT_QUEUE_SIZE));

    // Nor can a runtime settings delta (exact pin: no more, no less).
    let mut hostile = SettingsPack::new();
    hostile.alert_queue_size(16).unwrap().alert_mask(0);
    engine.apply_settings(&mut hostile).await.unwrap();
    let effective = engine.settings().unwrap();
    assert_eq!(effective.get_alert_mask(), Some(ALERT_MASK.bits_i32()));
    assert_eq!(effective.get_alert_queue_size(), Some(ALERT_QUEUE_SIZE));

    // The pinned mask keeps events flowing: seed-mode add reaches
    // seeding and resume persistence still works.
    let mut events = engine.subscribe_events();
    let mut atp = AddTorrentParams::from_torrent_file(fixture_path()).unwrap();
    atp.set_save_path(data.path().to_str().unwrap());
    atp.set_flags(atp.flags() | TorrentFlags::SEED_MODE);
    let entry = engine.add_torrent(&mut atp).await.unwrap();
    engine.save_resume_data(&entry).await.unwrap();
    wait_for_event(&mut events, |e| {
        matches!(e.kind, EventKind::ResumeDataSaved)
    })
    .await;

    engine.shutdown().await;
}

/// applySettings promises durability: a failed session-state write must
/// fail the mutation and roll the live session back, instead of
/// acknowledging and merely logging.
#[tokio::test(flavor = "multi_thread")]
async fn apply_settings_surfaces_failed_persistence() {
    let state = tempfile::tempdir().unwrap();
    let engine = Engine::start(&test_config(state.path()), Some(hermetic_settings()))
        .await
        .unwrap();

    // Sanity: with a writable state directory a real change is accepted
    // and observable.
    let mut pack = SettingsPack::new();
    pack.upload_rate_limit(111_111);
    engine.apply_settings(&mut pack).await.unwrap();
    assert_eq!(
        engine.settings().unwrap().get_upload_rate_limit(),
        Some(111_111)
    );

    // Occupy session.state's path with a non-empty directory: the atomic
    // write's rename then fails on every platform and for every user
    // (unlike a chmod, which root bypasses).
    let target = state.path().join("session.state");
    std::fs::remove_file(&target).unwrap();
    std::fs::create_dir_all(target.join("obstruction")).unwrap();

    let mut pack = SettingsPack::new();
    pack.upload_rate_limit(222_222);
    let result = engine.apply_settings(&mut pack).await;
    assert!(
        result.is_err(),
        "a failed durable write must fail applySettings"
    );
    assert_eq!(
        engine.settings().unwrap().get_upload_rate_limit(),
        Some(111_111),
        "the live session must roll back to the acknowledged state"
    );

    engine.shutdown().await;
}

/// A legacy resume file (pre-uuid: hash stem, no rbt-data key) is
/// migrated at restore time: a uuid is minted, spliced into the record,
/// and the file is renamed to the uuid stem — durably, so the identity
/// survives every later restart. Removal then leaves no stale file to
/// resurrect the torrent.
#[tokio::test(flavor = "multi_thread")]
async fn restore_migrates_legacy_resume_files() {
    let state = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();

    let hybrid = fixtures_dir().join("hybrid.torrent");
    let mut atp = AddTorrentParams::from_torrent_file(&hybrid).unwrap();
    atp.set_save_path(data.path().to_str().unwrap());
    // The fixture embeds example.com trackers and a web seed the
    // restored torrent would try to contact; drop them (the test is
    // about migration, not endpoints).
    atp.clear_trackers();
    atp.clear_url_seeds();
    let v2 = atp
        .info_hashes()
        .v2()
        .expect("hybrid fixture has a v2 hash");

    // A pre-uuid record: hash stem, no client data.
    let torrents_dir = state.path().join("torrents");
    std::fs::create_dir_all(&torrents_dir).unwrap();
    let bytes = rbtorrent::Session::write_resume_data(&atp).unwrap();
    let stale = torrents_dir.join(format!("{v2}.resume"));
    std::fs::write(&stale, &bytes).unwrap();

    let engine = Engine::start(&test_config(state.path()), Some(hermetic_settings()))
        .await
        .unwrap();
    assert_eq!(engine.registry().len(), 1, "torrent was not restored");
    let uuid = engine.registry().list()[0].uuid;

    let canonical = torrents_dir.join(format!("{uuid}.resume"));
    assert!(
        canonical.exists(),
        "resume file was not renamed to the uuid"
    );
    assert!(!stale.exists(), "stale legacy resume file left behind");
    let migrated = std::fs::read(&canonical).unwrap();
    assert!(
        contains(&migrated, b"8:rbt-data"),
        "migration did not splice the client-data key"
    );
    assert!(
        contains(&migrated, uuid.as_bytes()),
        "migrated record does not carry the minted uuid"
    );
    engine.shutdown().await;

    // The minted identity is durable: a second restart reads it back.
    let engine2 = Engine::start(&test_config(state.path()), Some(hermetic_settings()))
        .await
        .unwrap();
    assert_eq!(engine2.registry().len(), 1);
    assert!(
        engine2.registry().find(&uuid).is_some(),
        "uuid changed across a restart"
    );
    assert!(canonical.exists());

    // Removal therefore leaves no resume file to resurrect the torrent.
    engine2.remove_torrent(&uuid, false).await.unwrap();
    wait_until(|| !canonical.exists()).await;
    engine2.shutdown().await;

    let engine3 = Engine::start(&test_config(state.path()), Some(hermetic_settings()))
        .await
        .unwrap();
    assert_eq!(
        engine3.registry().len(),
        0,
        "removed torrent was resurrected by a stale resume file"
    );
    engine3.shutdown().await;
}

/// A restore round-trip must leave uuid-bearing resume records
/// byte-identical: the add alert's params are libtorrent's
/// selected-field snapshot, and rewriting a full record from them
/// discards trackers, web seeds, limits and piece state.
#[tokio::test(flavor = "multi_thread")]
async fn restore_leaves_resume_files_untouched() {
    let state = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();

    // A full-fidelity record: endpoints beyond what the add alert copies.
    let mut atp = AddTorrentParams::from_torrent_file(fixture_path()).unwrap();
    atp.set_save_path(data.path().to_str().unwrap());
    atp.add_tracker("http://127.0.0.1:9/announce", 0);
    atp.add_url_seed("http://127.0.0.1:9/seed");

    let rsbt_data = rsbtd::engine::client_data::RsbtData::new();
    let torrents_dir = state.path().join("torrents");
    std::fs::create_dir_all(&torrents_dir).unwrap();
    let bytes = rbtorrent::Session::write_resume_data_with(&atp, &rsbt_data).unwrap();
    let file = torrents_dir.join(format!("{}.resume", rsbt_data.uuid));
    std::fs::write(&file, &bytes).unwrap();

    // Engine::start awaits every restore add, so the pump has processed
    // the add alerts by the time it returns.
    let engine = Engine::start(&test_config(state.path()), Some(hermetic_settings()))
        .await
        .unwrap();
    assert_eq!(engine.registry().len(), 1, "torrent was not restored");
    assert_eq!(
        engine.registry().list()[0].uuid,
        rsbt_data.uuid,
        "restore did not adopt the persisted uuid"
    );
    assert_eq!(
        std::fs::read(&file).unwrap(),
        bytes,
        "restore rewrote the resume record"
    );
    engine.shutdown().await;
}

/// addTorrent acknowledges success only after the initial resume record
/// is durably on disk; a failed write unwinds the add entirely.
#[tokio::test(flavor = "multi_thread")]
async fn failed_initial_persist_unwinds_the_add() {
    let state = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();

    let engine = Engine::start(&test_config(state.path()), Some(hermetic_settings()))
        .await
        .unwrap();
    let mut atp = AddTorrentParams::from_torrent_file(fixture_path()).unwrap();
    atp.set_save_path(data.path().to_str().unwrap());

    // The record's filename is a uuid minted inside the add, so it cannot
    // be pre-obstructed; replace the whole torrents directory with a
    // regular file instead — every resume write then fails regardless of
    // privileges (unlike a chmod, which root bypasses).
    let torrents_dir = state.path().join("torrents");
    std::fs::remove_dir_all(&torrents_dir).unwrap();
    std::fs::write(&torrents_dir, b"obstruction").unwrap();

    let mut rx = engine.subscribe_events();
    let err = engine.add_torrent(&mut atp).await.unwrap_err();
    assert!(matches!(err, EngineError::Io(_)), "unexpected error: {err}");
    assert_eq!(
        engine.registry().len(),
        0,
        "failed add left a registry entry"
    );

    // The session drops the torrent too: once its removal is processed,
    // the same torrent can be added again.
    wait_for_event(&mut rx, |e| matches!(e.kind, EventKind::TorrentRemoved)).await;
    std::fs::remove_file(&torrents_dir).unwrap();
    std::fs::create_dir_all(&torrents_dir).unwrap();
    let entry = engine.add_torrent(&mut atp).await.unwrap();
    assert_eq!(engine.registry().len(), 1);
    assert!(
        torrents_dir
            .join(format!("{}.resume", entry.uuid))
            .is_file()
    );
    engine.shutdown().await;
}

/// A storage move of a metadata-less magnet must reach the resume file:
/// libtorrent's metadata-less move branch never marks resume data
/// dirty, so without an explicit save the move would be undone by the
/// next restart.
#[tokio::test(flavor = "multi_thread")]
async fn metadata_less_move_is_persisted() {
    let state = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let moved = tempfile::tempdir().unwrap();

    let engine = Engine::start(&test_config(state.path()), Some(hermetic_settings()))
        .await
        .unwrap();
    // No peers ever supply metadata for a made-up hash.
    let mut atp = AddTorrentParams::from_magnet_uri(
        "magnet:?xt=urn:btih:00112233445566778899aabbccddeeff00112233&dn=meta-less",
    )
    .unwrap();
    atp.set_save_path(data.path().to_str().unwrap());
    let entry = engine.add_torrent(&mut atp).await.unwrap();

    let mut rx = engine.subscribe_events();
    let new_path = engine
        .move_storage(
            &entry,
            moved.path().to_str().unwrap(),
            rbtorrent::TorrentHandle::MOVE_ALWAYS_REPLACE_FILES,
        )
        .await
        .unwrap();
    assert_eq!(new_path, moved.path().to_str().unwrap());
    wait_for_event(&mut rx, |e| matches!(e.kind, EventKind::ResumeDataSaved)).await;

    let resume = state
        .path()
        .join("torrents")
        .join(format!("{}.resume", entry.uuid));
    let (restored, _) =
        rbtorrent::Session::read_resume_data(&std::fs::read(&resume).unwrap(), None).unwrap();
    assert_eq!(
        restored.save_path(),
        moved.path().to_str().unwrap(),
        "moved path was not persisted"
    );
    engine.shutdown().await;
}
