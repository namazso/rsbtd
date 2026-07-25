// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Per-torrent [`ClientData`]: attach, retrieve, swap, persistence in
//! resume data, and lifecycle cleanup.

use std::path::PathBuf;
use std::sync::Arc;

use rbtorrent::{
    AddTorrentParams, Alerts, ClientData, RemoveFlags, Session, SessionParams, SettingsPack,
    TorrentHandle,
};

/// Encodes the label as a bencode string (`<len>:<label>`): the blob must
/// be one well-formed bencode value, since it is spliced verbatim.
#[derive(Debug, Default, PartialEq)]
struct TestData {
    label: String,
}

impl TestData {
    fn new(label: &str) -> TestData {
        TestData {
            label: label.to_owned(),
        }
    }
}

impl ClientData for TestData {
    fn to_bencode(&self) -> Vec<u8> {
        format!("{}:{}", self.label.len(), self.label).into_bytes()
    }

    fn from_bencode(bytes: Option<&[u8]>) -> rbtorrent::Result<TestData> {
        let Some(bytes) = bytes else {
            return Ok(TestData::default());
        };
        let bad = || rbtorrent::Error::client("not a bencode string");
        let text = str::from_utf8(bytes).map_err(|_| bad())?;
        let (len, label) = text.split_once(':').ok_or_else(bad)?;
        if len.parse::<usize>().map_err(|_| bad())? != label.len() {
            return Err(bad());
        }
        Ok(TestData::new(label))
    }
}

/// Violates the contract: not bencode. The writers must reject it.
struct BadData;

impl ClientData for BadData {
    fn to_bencode(&self) -> Vec<u8> {
        b"}}not bencode{{".to_vec()
    }

    fn from_bencode(_bytes: Option<&[u8]>) -> rbtorrent::Result<BadData> {
        Ok(BadData)
    }
}

fn hermetic_session() -> Session {
    let mut settings = SettingsPack::new();
    settings
        .enable_dht(false)
        .enable_lsd(false)
        .enable_upnp(false)
        .enable_natpmp(false)
        .listen_interfaces(&[rbtorrent::ListenEndpoint::new("127.0.0.1", 0)])
        .unwrap();
    Session::new(SessionParams::new().settings(&settings)).unwrap()
}

fn fixture_atp(name: &str, save_dir: &tempfile::TempDir) -> AddTorrentParams {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!("tests/fixtures/{name}"));
    let mut atp = AddTorrentParams::from_torrent_file(&path).unwrap();
    atp.set_save_path(save_dir.path().to_str().unwrap());
    atp
}

/// Adds a torrent while polling the alert stream (see the crate docs).
async fn add<'s>(
    session: &'s Session,
    alerts: &mut Alerts<'s>,
    atp: &AddTorrentParams,
    data: Arc<dyn ClientData>,
) -> TorrentHandle<'s> {
    let add = session.add_torrent(atp, data);
    tokio::pin!(add);
    loop {
        tokio::select! {
            result = &mut add => break result.unwrap(),
            batch = alerts.next_batch() => { batch.unwrap(); }
        }
    }
}

#[tokio::test]
async fn client_data_attach_swap_persist() {
    let save_dir = tempfile::tempdir().unwrap();
    let atp = fixture_atp("transfer.torrent", &save_dir);
    let session = hermetic_session();
    let mut alerts = session.alerts();

    let handle = add(
        &session,
        &mut alerts,
        &atp,
        Arc::new(TestData::new("first")),
    )
    .await;

    // Retrieval, typed and untyped; a wrong downcast is an error.
    assert_eq!(*handle.client_data_as::<TestData>().unwrap(), TestData::new("first"));
    assert!(handle.client_data().is_ok());
    assert!(handle.client_data_as::<()>().is_err());

    // set_client_data is an Arc swap, visible to the next read.
    handle
        .set_client_data(Arc::new(TestData::new("second")))
        .unwrap();
    assert_eq!(
        *handle.client_data_as::<TestData>().unwrap(),
        TestData::new("second")
    );

    // The handle-aware writer embeds the blob under "rbt-data"; the plain
    // writer does not.
    let spliced = handle.write_resume_data(&atp).unwrap();
    assert!(
        contains(&spliced, b"8:rbt-data6:second"),
        "resume data carries the blob"
    );
    let plain = Session::write_resume_data(&atp).unwrap();
    assert!(!contains(&plain, b"8:rbt-data"));

    // The round-trip law, through real resume data.
    let (restored, data) = Session::read_resume_data_with::<TestData>(&spliced, None).unwrap();
    assert_eq!(restored.info_hashes(), atp.info_hashes());
    assert_eq!(data, TestData::new("second"));
    // The raw form yields the exact bytes.
    let (_, raw) = Session::read_resume_data(&spliced, None).unwrap();
    assert_eq!(raw.as_deref(), Some(&b"6:second"[..]));

    // A blob that is not one well-formed bencode value is rejected at
    // write time instead of corrupting the file.
    handle.set_client_data(Arc::new(BadData)).unwrap();
    assert!(handle.write_resume_data(&atp).is_err());
    handle
        .set_client_data(Arc::new(TestData::new("second")))
        .unwrap();

    // Blob-less resume data decodes to defaults (legacy migration path).
    let (_, data) = Session::read_resume_data_with::<TestData>(&plain, None).unwrap();
    assert_eq!(data, TestData::default());
    let (_, raw) = Session::read_resume_data(&plain, None).unwrap();
    assert_eq!(raw, None);

    drop(handle);
    drop(alerts);
    session.close().await;
}

#[tokio::test]
async fn null_client_data_writes_no_key() {
    let save_dir = tempfile::tempdir().unwrap();
    let atp = fixture_atp("transfer.torrent", &save_dir);
    let session = hermetic_session();
    let mut alerts = session.alerts();

    let handle = add(&session, &mut alerts, &atp, Arc::new(())).await;

    // () serializes to nothing => no key, byte-identical to the plain form.
    let via_handle = handle.write_resume_data(&atp).unwrap();
    assert_eq!(via_handle, Session::write_resume_data(&atp).unwrap());
    assert!(!contains(&via_handle, b"8:rbt-data"));
    handle.client_data_as::<()>().unwrap();

    drop(handle);
    drop(alerts);
    session.close().await;
}

#[tokio::test]
async fn client_data_reclaimed_on_removal() {
    let save_dir = tempfile::tempdir().unwrap();
    let atp = fixture_atp("transfer.torrent", &save_dir);
    let session = hermetic_session();
    let mut alerts = session.alerts();

    let handle = add(
        &session,
        &mut alerts,
        &atp,
        Arc::new(TestData::new("doomed")),
    )
    .await;
    let keep = handle.clone();
    handle.remove(RemoveFlags::empty());

    let mut removed = false;
    for _ in 0..10 {
        let batch =
            tokio::time::timeout(std::time::Duration::from_millis(500), alerts.next_batch()).await;
        if let Ok(batch) = batch {
            for alert in batch.unwrap().iter() {
                if matches!(alert, rbtorrent::Alert::TorrentRemoved(_)) {
                    removed = true;
                }
            }
            if removed {
                break;
            }
        }
    }
    assert!(removed, "should see TorrentRemoved");

    // The sweep ran while the batch was popped: the data is gone, and the
    // handle-aware writer degrades to the plain form.
    assert!(keep.client_data().is_err());
    assert!(keep.set_client_data(Arc::new(TestData::default())).is_err());
    let plain = keep.write_resume_data(&atp).unwrap();
    assert!(!contains(&plain, b"8:rbt-data"));

    drop(keep);
    drop(alerts);
    session.close().await;
}

#[tokio::test]
async fn duplicate_add_keeps_original_data() {
    let save_dir = tempfile::tempdir().unwrap();
    let atp = fixture_atp("transfer.torrent", &save_dir);
    let session = hermetic_session();
    let mut alerts = session.alerts();

    let first = add(
        &session,
        &mut alerts,
        &atp,
        Arc::new(TestData::new("original")),
    )
    .await;
    // Without DUPLICATE_IS_ERROR the add resolves to the existing torrent;
    // its attempted data is discarded, the original survives.
    let second = add(
        &session,
        &mut alerts,
        &atp,
        Arc::new(TestData::new("usurper")),
    )
    .await;
    assert_eq!(second.id(), first.id());
    assert_eq!(
        *second.client_data_as::<TestData>().unwrap(),
        TestData::new("original")
    );

    drop(second);
    drop(first);
    drop(alerts);
    session.close().await;
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}
