// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Parsing .torrent files (v1 / v2 / hybrid fixtures), magnet links, and
//! AddTorrentParams field access.

use std::net::SocketAddr;
use std::path::PathBuf;

use rbtorrent::{
    AddTorrentParams, DownloadPriority, InfoHash, Session, Sha1Hash, TorrentFlags, TorrentInfo,
};

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

fn fixture(name: &str) -> AddTorrentParams {
    AddTorrentParams::from_torrent_file(fixture_path(name)).expect(name)
}

/// Checks everything the three fixtures have in common.
fn check_common(atp: &AddTorrentParams) -> TorrentInfo {
    assert_eq!(atp.comment(), "rbtorrent test fixture");
    assert_eq!(atp.created_by(), "rbtorrent gen_fixtures");
    assert_eq!(atp.creation_date(), 1234567890);

    let trackers: Vec<(String, i32)> = atp
        .trackers()
        .map(|(url, tier)| (url.into_owned(), tier))
        .collect();
    assert_eq!(
        trackers,
        vec![
            ("http://tracker.example.com/announce".to_owned(), 0),
            ("http://backup.example.com/announce".to_owned(), 1),
        ]
    );

    let seeds: Vec<String> = atp.url_seeds().map(|s| s.into_owned()).collect();
    // the parser appends a trailing slash for multi-file torrents
    assert_eq!(seeds, vec!["http://seed.example.com/fixture/".to_owned()]);

    let ti = atp.ti().expect("metadata is present");
    assert!(ti.is_valid());
    assert_eq!(ti.name(), "fixture");
    assert_eq!(ti.piece_length(), 16384);
    assert!(!ti.is_private());
    assert!(!ti.info_section().is_empty());
    assert_eq!(ti.info_hashes(), atp.info_hashes());

    // the two content files (v2/hybrid layouts interleave pad files)
    let content: Vec<_> = ti.files().filter(|f| !f.flags().is_pad_file()).collect();
    assert_eq!(content.len(), 2);
    let a = content[0];
    assert_eq!(a.name(), "a.bin");
    // file paths come back with native separators
    assert_eq!(a.path().replace('\\', "/"), "fixture/a.bin");
    assert_eq!(a.size(), 40960);
    assert_eq!(a.offset(), 0);
    assert_eq!(a.index(), 0);
    let b = content[1];
    assert_eq!(b.name(), "b.txt");
    assert_eq!(b.size(), 137);

    // 40960-byte a.bin at 16 KiB pieces spans pieces 0..=2
    assert_eq!(a.first_piece(), 0);
    assert_eq!(a.last_piece(), 2);
    assert_eq!(ti.file_index_at_offset(0), Some(0));

    // block 0 of piece 0 lies entirely in a.bin
    let req = ti.map_file(0, 0, 16384).expect("map_file");
    assert_eq!((req.piece, req.start, req.length), (0, 0, 16384));
    let slices = ti.map_block(0, 0, 16384);
    assert_eq!(slices.len(), 1);
    assert_eq!(slices[0].file_index, 0);
    assert_eq!(slices[0].offset, 0);
    assert_eq!(slices[0].size, 16384);

    // out-of-range accesses are refused
    assert!(ti.file(ti.num_files()).is_none());
    assert!(ti.piece_size(ti.num_pieces()).is_none());
    assert!(ti.map_file(0, 40960, 1).is_none());

    // hostile ranges whose naive end-of-range arithmetic overflows
    assert!(ti.map_file(1, i64::MAX, 1).is_none());
    assert!(ti.map_file(0, i64::MAX, i32::MAX).is_none());
    assert!(ti.map_block(2, i64::MAX, 1).is_empty());
    assert!(ti.map_block(0, i64::MAX, i32::MAX).is_empty());

    ti
}

#[test]
fn parse_v1_fixture() {
    let atp = fixture("v1.torrent");
    let ti = check_common(&atp);

    assert!(ti.has_v1() && !ti.has_v2());
    assert!(ti.info_hashes().v1().is_some());
    assert!(ti.info_hashes().v2().is_none());
    assert_eq!(ti.num_files(), 2);
    assert_eq!(ti.total_size(), 41097);
    assert_eq!(ti.size_on_disk(), 41097);
    assert_eq!(ti.num_pieces(), 3);

    // per-piece SHA-1 hashes exist and differ
    let h0 = ti.hash_for_piece(0).expect("piece 0 hash");
    let h1 = ti.hash_for_piece(1).expect("piece 1 hash");
    assert_ne!(h0, h1);
    assert!(!h0.is_all_zeros());

    // no v2 data; file dimensions need v2 piece alignment
    assert!(ti.file(0).unwrap().root().is_none());
    assert_eq!(atp.num_merkle_trees(), 0);
    assert_eq!(ti.file(0).unwrap().num_pieces(), None);
    assert_eq!(ti.file(0).unwrap().num_blocks(), None);

    // last piece is short: 41097 - 2*16384 = 8329
    assert_eq!(ti.piece_size(2), Some(8329));
}

#[test]
fn parse_v2_fixture() {
    let atp = fixture("v2.torrent");
    let ti = check_common(&atp);

    assert!(!ti.has_v1() && ti.has_v2());
    assert!(ti.info_hashes().v1().is_none());
    assert!(ti.info_hashes().v2().is_some());
    // a.bin, pad to the piece boundary, b.txt, tail pad
    assert_eq!(ti.num_files(), 4);

    // v2: no SHA-1 piece hashes, but per-file merkle roots
    assert!(ti.hash_for_piece(0).is_none());
    let root_a = ti.file(0).unwrap().root().expect("a.bin root");
    let root_b = ti.file(2).unwrap().root().expect("b.txt root");
    assert_ne!(root_a, root_b);
    assert_eq!(ti.file_index_for_root(&root_a), Some(0));
    // pad files have no merkle root and no piece/block dimensions
    assert!(ti.file(1).unwrap().root().is_none());
    assert_eq!(ti.file(1).unwrap().num_pieces(), None);
    assert_eq!(ti.file(1).unwrap().num_blocks(), None);
    assert!(ti.file(0).unwrap().num_pieces().unwrap() > 0);
    assert!(ti.file(0).unwrap().num_blocks().unwrap() > 0);

    // a.bin (40960 > 16384) has a piece layer; b.txt fits in one piece
    assert_eq!(atp.num_merkle_trees(), 4);
    assert!(!atp.merkle_tree(0).is_empty());
    assert!(atp.merkle_tree(2).is_empty());
    assert!(!atp.merkle_tree_mask(0).is_empty());
}

#[test]
fn parse_hybrid_fixture() {
    let atp = fixture("hybrid.torrent");
    let ti = check_common(&atp);

    assert!(ti.has_v1() && ti.has_v2());
    assert!(ti.info_hashes().v1().is_some());
    assert!(ti.info_hashes().v2().is_some());

    // hybrid layout pads a.bin to a piece boundary and pads the tail
    assert_eq!(ti.num_files(), 4);
    let pad = ti.file(1).unwrap();
    assert!(pad.flags().is_pad_file());
    assert_eq!(pad.size(), 8192);
    let tail = ti.file(3).unwrap();
    assert!(tail.flags().is_pad_file());
    assert_eq!(tail.size(), 16384 - 137);
    assert_eq!(ti.total_size(), 65536);
    assert_eq!(ti.size_on_disk(), 41097);
    assert_eq!(ti.num_pieces(), 4);

    // both hash schemes are present
    assert!(ti.hash_for_piece(0).is_some());
    assert!(ti.file(0).unwrap().root().is_some());
    assert!(ti.file(0).unwrap().num_pieces().unwrap() > 0);
}

#[test]
fn buffer_and_file_loads_agree() {
    let from_file = fixture("hybrid.torrent");
    let bytes = std::fs::read(fixture_path("hybrid.torrent")).unwrap();
    let from_buffer = AddTorrentParams::from_torrent_buffer(&bytes).unwrap();
    assert_eq!(from_file.info_hashes(), from_buffer.info_hashes());
    assert_eq!(
        from_file.ti().unwrap().info_section(),
        from_buffer.ti().unwrap().info_section()
    );
}

#[test]
fn load_errors_are_reported() {
    let err = AddTorrentParams::from_torrent_buffer(b"not bencoded").unwrap_err();
    assert_ne!(err.message(), "");

    let err = AddTorrentParams::from_torrent_file(fixture_path("missing.torrent")).unwrap_err();
    assert_ne!(err.message(), "");

    // a valid bencoded dict that is not a torrent
    let err = AddTorrentParams::from_torrent_buffer(b"de").unwrap_err();
    assert_eq!(err.category(), rbtorrent::Category::Libtorrent);
}

#[test]
fn buffer_loads_enforce_max_buffer_size() {
    let bytes = std::fs::read(fixture_path("hybrid.torrent")).unwrap();
    let len = i32::try_from(bytes.len()).unwrap();
    let too_small = rbtorrent::LoadTorrentLimits {
        max_buffer_size: len - 1,
        ..rbtorrent::LoadTorrentLimits::default()
    };
    let err = AddTorrentParams::from_torrent_buffer_with_limits(&bytes, &too_small).unwrap_err();
    assert!(err.message().contains("max_buffer_size"));

    let exact = rbtorrent::LoadTorrentLimits {
        max_buffer_size: len,
        ..rbtorrent::LoadTorrentLimits::default()
    };
    AddTorrentParams::from_torrent_buffer_with_limits(&bytes, &exact).unwrap();
}

#[test]
fn download_priority_domain_is_closed() {
    assert_eq!(DownloadPriority::new(7), Some(DownloadPriority::TOP));
    assert_eq!(DownloadPriority::new(8), None);
    assert_eq!(u8::from(DownloadPriority::LOW), 1);
    assert_eq!(DownloadPriority::try_from(4u8).unwrap().value(), 4);
    assert!(DownloadPriority::try_from(8u8).is_err());
}

#[test]
fn magnet_roundtrip() {
    let mut atp = fixture("hybrid.torrent");
    // make_magnet_uri only reads atp.name, which load_torrent leaves
    // empty (the name lives in the metadata)
    let name = atp.ti().unwrap().name().into_owned();
    atp.set_name(&name);
    let uri = atp.make_magnet_uri().expect("magnet uri");
    assert!(uri.starts_with("magnet:?"));

    let parsed = AddTorrentParams::from_magnet_uri(&uri).expect("parse");
    assert_eq!(parsed.info_hashes(), atp.info_hashes());
    assert_eq!(parsed.name(), "fixture");
    let trackers: Vec<String> = parsed.trackers().map(|(url, _)| url.into_owned()).collect();
    assert!(trackers.contains(&"http://tracker.example.com/announce".to_owned()));
}

#[test]
fn magnet_parse_by_hash() {
    let uri = "magnet:?xt=urn:btih:0123456789abcdef0123456789abcdef01234567&dn=demo";
    let atp = AddTorrentParams::from_magnet_uri(uri).unwrap();
    let v1 = atp.info_hashes().v1().expect("v1 hash");
    assert_eq!(
        v1,
        Sha1Hash([
            0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef, 0x01, 0x23, 0x45, 0x67, 0x89, 0xab,
            0xcd, 0xef, 0x01, 0x23, 0x45, 0x67,
        ])
    );
    assert_eq!(atp.name(), "demo");
    assert!(atp.ti().is_none());

    assert!(AddTorrentParams::from_magnet_uri("magnet:?xt=urn:btih:zz").is_err());
    assert!(AddTorrentParams::from_magnet_uri("http://x.example/").is_err());
}

#[test]
fn magnet_requires_info_hash() {
    let empty = AddTorrentParams::new();
    assert!(empty.make_magnet_uri().is_err());
}

#[test]
fn atp_field_roundtrip() {
    let mut atp = AddTorrentParams::new();
    assert_eq!(atp.flags(), TorrentFlags::DEFAULT);
    assert!(atp.version() > 0);

    atp.set_name("example")
        .set_save_path("/tmp/downloads")
        .set_part_file_dir("parts")
        .set_trackerid("tid")
        .set_comment("a comment")
        .set_created_by("a creator")
        .set_root_certificate("PEM");
    assert_eq!(atp.name(), "example");
    assert_eq!(atp.save_path(), "/tmp/downloads");
    assert_eq!(atp.part_file_dir(), "parts");
    assert_eq!(atp.trackerid(), "tid");
    assert_eq!(atp.comment(), "a comment");
    assert_eq!(atp.created_by(), "a creator");
    assert_eq!(atp.root_certificate(), "PEM");

    let flags = TorrentFlags::DEFAULT | TorrentFlags::SEQUENTIAL_DOWNLOAD;
    atp.set_flags(flags);
    assert_eq!(atp.flags(), flags);
    assert!(atp.flags().contains(TorrentFlags::SEQUENTIAL_DOWNLOAD));
    atp.set_flags(atp.flags() & !TorrentFlags::PAUSED);
    assert!(!atp.flags().contains(TorrentFlags::PAUSED));

    atp.set_storage_mode(rbtorrent::StorageMode::Allocate);
    assert_eq!(atp.storage_mode(), rbtorrent::StorageMode::Allocate);

    let hash = InfoHash::from_v1(Sha1Hash([0x42; 20]));
    atp.set_info_hashes(hash);
    assert_eq!(atp.info_hashes(), hash);

    atp.set_max_uploads(7)
        .set_max_connections(77)
        .set_upload_limit(1024)
        .set_download_limit(2048)
        .set_total_uploaded(1)
        .set_total_downloaded(2)
        .set_active_time(3)
        .set_finished_time(4)
        .set_seeding_time(5)
        .set_added_time(6)
        .set_completed_time(7)
        .set_last_seen_complete(8)
        .set_num_complete(9)
        .set_num_incomplete(10)
        .set_num_downloaded(11)
        .set_last_download(12)
        .set_last_upload(13)
        .set_creation_date(14);
    assert_eq!(atp.max_uploads(), 7);
    assert_eq!(atp.max_connections(), 77);
    assert_eq!(atp.upload_limit(), 1024);
    assert_eq!(atp.download_limit(), 2048);
    assert_eq!(atp.total_uploaded(), 1);
    assert_eq!(atp.total_downloaded(), 2);
    assert_eq!(atp.active_time(), 3);
    assert_eq!(atp.finished_time(), 4);
    assert_eq!(atp.seeding_time(), 5);
    assert_eq!(atp.added_time(), 6);
    assert_eq!(atp.completed_time(), 7);
    assert_eq!(atp.last_seen_complete(), 8);
    assert_eq!(atp.num_complete(), 9);
    assert_eq!(atp.num_incomplete(), 10);
    assert_eq!(atp.num_downloaded(), 11);
    assert_eq!(atp.last_download(), 12);
    assert_eq!(atp.last_upload(), 13);
    assert_eq!(atp.creation_date(), 14);

    atp.add_tracker("http://one.example/announce", 0)
        .add_tracker("http://two.example/announce", 3);
    let trackers: Vec<(String, i32)> = atp
        .trackers()
        .map(|(url, tier)| (url.into_owned(), tier))
        .collect();
    assert_eq!(
        trackers,
        vec![
            ("http://one.example/announce".to_owned(), 0),
            ("http://two.example/announce".to_owned(), 3),
        ]
    );
    atp.clear_trackers();
    assert_eq!(atp.trackers().count(), 0);

    atp.add_dht_node("router.example.com", 6881);
    assert_eq!(
        atp.dht_nodes(),
        vec![("router.example.com".to_owned(), 6881)]
    );
    atp.clear_dht_nodes();
    assert!(atp.dht_nodes().is_empty());

    atp.add_url_seed("http://seed.example/");
    assert_eq!(atp.url_seeds().count(), 1);
    atp.clear_url_seeds();
    assert_eq!(atp.url_seeds().count(), 0);

    let prios = [
        DownloadPriority::TOP,
        DownloadPriority::DONT_DOWNLOAD,
        DownloadPriority::DEFAULT,
    ];
    atp.set_file_priorities(&prios);
    assert_eq!(atp.file_priorities(), &prios);
    atp.set_piece_priorities(&[DownloadPriority::LOW]);
    assert_eq!(atp.piece_priorities(), &[DownloadPriority::LOW]);

    let pieces = [true, false, true, true, false, false, true, false, true];
    atp.set_have_pieces(&pieces);
    assert_eq!(atp.have_pieces(), pieces);
    atp.set_verified_pieces(&pieces[..3]);
    assert_eq!(atp.verified_pieces(), pieces[..3]);

    atp.add_unfinished_piece(2, &[true, false, true]);
    atp.add_unfinished_piece(0, &[false, true]);
    let unfinished = atp.unfinished_pieces();
    assert_eq!(
        unfinished,
        vec![(0, vec![false, true]), (2, vec![true, false, true])]
    );
    atp.clear_unfinished_pieces();
    assert!(atp.unfinished_pieces().is_empty());

    atp.add_renamed_file(1, "renamed.bin").unwrap();
    assert_eq!(atp.renamed_files(), vec![(1, "renamed.bin".to_owned())]);
    atp.clear_renamed_files();

    let v4: SocketAddr = "127.0.0.1:6881".parse().unwrap();
    let v6: SocketAddr = "[2001:db8::1]:16881".parse().unwrap();
    // scope ids survive the boundary (link-local peers need them)
    let v6_scoped: SocketAddr = "[fe80::1%7]:16881".parse().unwrap();
    atp.add_peer(v4).add_peer(v6).add_peer(v6_scoped);
    assert_eq!(atp.peers(), vec![v4, v6, v6_scoped]);
    atp.clear_peers();
    assert!(atp.peers().is_empty());
    atp.add_banned_peer(v4);
    assert_eq!(atp.banned_peers(), vec![v4]);
    atp.clear_banned_peers();

    atp.set_num_merkle_trees(2);
    assert_eq!(atp.num_merkle_trees(), 2);
    let hashes = [
        rbtorrent::Sha256Hash([0xaa; 32]),
        rbtorrent::Sha256Hash([0xbb; 32]),
    ];
    atp.set_merkle_tree(1, &hashes);
    assert_eq!(atp.merkle_tree(1), hashes);
    atp.set_merkle_tree_mask(1, &[true, false, true]);
    assert_eq!(atp.merkle_tree_mask(1), vec![true, false, true]);
    atp.set_verified_leaf_hashes(1, &[false, true]);
    assert_eq!(atp.verified_leaf_hashes(1), vec![false, true]);
    assert!(atp.merkle_tree(0).is_empty());
    assert!(atp.merkle_tree(5).is_empty());
}

/// Renamed-file indices are validated: negative always, and against the
/// file count when metadata is present. A negative index previously
/// reached resume-data serialization, where it indexes past the
/// mapped-files list.
#[test]
fn renamed_file_index_is_validated() {
    let mut atp = AddTorrentParams::new();
    atp.add_renamed_file(-1, "x").unwrap_err();
    // Without metadata only the sign can be checked.
    atp.add_renamed_file(7, "renamed.bin").unwrap();
    assert_eq!(atp.renamed_files(), vec![(7, "renamed.bin".to_owned())]);

    let mut atp = fixture("hybrid.torrent");
    let count = atp.ti().unwrap().num_files();
    atp.add_renamed_file(-1, "x").unwrap_err();
    atp.add_renamed_file(count, "x").unwrap_err();
    atp.add_renamed_file(0, "renamed.bin").unwrap();
    assert_eq!(atp.renamed_files(), vec![(0, "renamed.bin".to_owned())]);
    Session::write_resume_data(&atp).unwrap();
}

#[test]
fn atp_clone_is_deep() {
    let mut original = AddTorrentParams::new();
    original.set_name("original");
    let mut copy = original.clone();
    copy.set_name("copy");
    assert_eq!(original.name(), "original");
    assert_eq!(copy.name(), "copy");
}

#[test]
fn ti_shares_between_clones_and_atp() {
    let atp = fixture("v1.torrent");
    let ti = atp.ti().unwrap();
    let ti2 = ti.clone();
    drop(atp);
    // the metadata outlives the params object that produced it
    assert_eq!(ti.name(), "fixture");
    assert_eq!(ti2.info_hashes(), ti.info_hashes());

    let mut fresh = AddTorrentParams::new();
    fresh.set_ti(&ti);
    assert_eq!(fresh.ti().unwrap().info_hashes(), ti.info_hashes());
    fresh.clear_ti();
    assert!(fresh.ti().is_none());
}

#[test]
fn info_hash_only_torrent_info() {
    let hash = InfoHash::from_v1(Sha1Hash([0x11; 20]));
    let ti = TorrentInfo::from_info_hash(&hash);
    assert!(!ti.is_valid());
    assert_eq!(ti.info_hashes(), hash);
    assert_eq!(ti.num_files(), 0);
    assert_eq!(ti.num_pieces(), 0);
    assert!(ti.hash_for_piece(0).is_none());
}

#[test]
fn similar_and_collections_empty_for_fixtures() {
    let ti = fixture("v1.torrent").ti().unwrap();
    assert!(ti.similar_torrents().is_empty());
    assert!(ti.collections().is_empty());
}
