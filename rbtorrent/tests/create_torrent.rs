// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use rbtorrent::*;
use std::fs;
use tempfile::TempDir;

#[test]
fn create_torrent_basic() {
    let temp_dir = TempDir::new().unwrap();
    let base_path = temp_dir.path();

    let content_dir = base_path.join("test_torrent");
    fs::create_dir(&content_dir).unwrap();

    let file_path = content_dir.join("test_file.txt");
    let content = b"Hello, this is a test file for torrent creation!";
    fs::write(&file_path, content).unwrap();

    let files = list_files(&content_dir, CreateFlags::empty()).unwrap();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].size, content.len() as i64);

    // piece size 0 = auto
    let mut ct = CreateTorrent::new(&files, 0, CreateFlags::empty()).unwrap();

    ct.set_comment("Test torrent").unwrap();
    ct.set_creator("rbtorrent test suite").unwrap();
    ct.add_tracker("http://tracker.example.com:8080/announce", 0)
        .unwrap();

    // base_path is the parent of content_dir
    set_piece_hashes(&mut ct, base_path, None::<fn(u32) -> bool>).unwrap();

    let torrent_data = ct.generate().unwrap();
    assert!(!torrent_data.is_empty());

    let params = AddTorrentParams::from_torrent_buffer(&torrent_data).unwrap();
    let info = params.ti().unwrap();
    assert_eq!(info.num_files(), 1);
    assert_eq!(info.total_size(), content.len() as i64);
}

#[test]
fn create_torrent_multi_file() {
    let temp_dir = TempDir::new().unwrap();
    let base_path = temp_dir.path();

    let content_dir = base_path.join("my_torrent");
    fs::create_dir(&content_dir).unwrap();
    let subdir = content_dir.join("subdir");
    fs::create_dir(&subdir).unwrap();

    fs::write(content_dir.join("file1.txt"), b"File 1 content").unwrap();
    fs::write(content_dir.join("file2.txt"), b"File 2 content").unwrap();
    fs::write(subdir.join("file3.txt"), b"File 3 content in subdirectory").unwrap();

    let files = list_files(&content_dir, CreateFlags::empty()).unwrap();
    assert!(
        files.len() >= 3,
        "Expected at least 3 files, got {}",
        files.len()
    );

    let mut ct = CreateTorrent::new(&files, 16384, CreateFlags::empty()).unwrap();

    ct.set_comment("Multi-file test").unwrap();
    ct.add_tracker("udp://tracker.example.com:6969/announce", 0)
        .unwrap();
    ct.set_priv(true);

    assert!(ct.is_private());
    assert!(!ct.is_v2_only());
    assert!(!ct.is_v1_only());

    let mut progress_count = 0;
    set_piece_hashes(
        &mut ct,
        base_path,
        Some(|_idx| {
            progress_count += 1;
            true
        }),
    )
    .unwrap();

    assert!(progress_count > 0);
    assert_eq!(ct.num_pieces(), progress_count);
    assert_eq!(ct.piece_length(), 16384);

    let torrent_data = ct.generate().unwrap();
    let params = AddTorrentParams::from_torrent_buffer(&torrent_data).unwrap();
    let info = params.ti().unwrap();

    // may include pad files
    assert!(
        info.num_files() >= 3,
        "Expected at least 3 files in torrent"
    );
    assert!(info.name().contains("my_torrent"));
}

#[test]
fn set_piece_hashes_abort_stops_hashing() {
    let temp_dir = TempDir::new().unwrap();
    let base_path = temp_dir.path();

    let file_path = base_path.join("abort_test.bin");
    fs::write(&file_path, vec![3u8; 100_000]).unwrap();

    let files = vec![FileEntry::new("abort_test.bin", 100_000).unwrap()];
    let mut ct = CreateTorrent::new(&files, 16384, CreateFlags::empty()).unwrap();
    assert!(ct.num_pieces() > 1);

    let mut calls = 0u32;
    let err = set_piece_hashes(
        &mut ct,
        base_path,
        Some(|_idx| {
            calls += 1;
            false
        }),
    )
    .unwrap_err();
    assert!(err.is_cancelled(), "unexpected error: {err}");
    assert_eq!(calls, 1);
}

#[test]
fn create_torrent_v2_only() {
    let temp_dir = TempDir::new().unwrap();
    let base_path = temp_dir.path();

    let content_dir = base_path.join("v2_test");
    fs::create_dir(&content_dir).unwrap();

    let file_path = content_dir.join("v2_file.dat");
    let content = vec![0u8; 32768]; // 32 KiB
    fs::write(&file_path, &content).unwrap();

    let files = list_files(&content_dir, CreateFlags::empty()).unwrap();
    let mut ct = CreateTorrent::new(&files, 16384, CreateFlags::V2_ONLY).unwrap();

    assert!(ct.is_v2_only());
    assert!(!ct.is_v1_only());

    ct.add_tracker("https://tracker.example.com/announce", 0)
        .unwrap();

    set_piece_hashes(&mut ct, base_path, None::<fn(u32) -> bool>).unwrap();

    let torrent_data = ct.generate().unwrap();
    let params = AddTorrentParams::from_torrent_buffer(&torrent_data).unwrap();
    let info = params.ti().unwrap();

    assert!(info.has_v2());
}

#[test]
fn create_torrent_properties() {
    let temp_dir = TempDir::new().unwrap();
    let base_path = temp_dir.path();

    let content_dir = base_path.join("prop_test");
    fs::create_dir(&content_dir).unwrap();

    fs::write(content_dir.join("test.txt"), b"test").unwrap();

    let files = list_files(&content_dir, CreateFlags::empty()).unwrap();
    let mut ct = CreateTorrent::new(&files, 16384, CreateFlags::empty()).unwrap();

    ct.set_comment("A test comment").unwrap();
    ct.set_creator("test creator").unwrap();
    ct.add_url_seed("http://seed.example.com/files/").unwrap();
    ct.add_tracker("http://tracker1.example.com/announce", 0)
        .unwrap();
    ct.add_tracker("http://tracker2.example.com/announce", 1)
        .unwrap();
    ct.add_node("dht.example.com", 6881).unwrap();
    ct.set_priv(false);

    assert!(!ct.is_private());

    let total = ct.total_size();
    assert_eq!(total, 4);

    set_piece_hashes(&mut ct, base_path, None::<fn(u32) -> bool>).unwrap();

    let torrent_data = ct.generate().unwrap();
    assert!(!torrent_data.is_empty());
}

#[test]
fn set_hash_out_of_range_is_rejected() {
    // A v1-only torrent (no canonical layout, so no pad files): 50 000 bytes
    // over 16 KiB pieces = 4 pieces. Every out-of-range index must come back
    // as Err, never a memory write.
    let files = vec![
        FileEntry::new("hashes/a.bin", 40_000).unwrap(),
        FileEntry::new("hashes/b.txt", 10_000).unwrap(),
    ];
    let mut ct = CreateTorrent::new(&files, 16384, CreateFlags::V1_ONLY).unwrap();
    let pieces = ct.num_pieces();
    assert!(pieces > 0);

    let sha1 = [0xabu8; 20];
    ct.set_hash(0, &sha1).unwrap();
    ct.set_hash(pieces - 1, &sha1).unwrap();
    assert!(ct.set_hash(pieces, &sha1).is_err());
    assert!(ct.set_hash(1_000_000, &sha1).is_err());
    assert!(ct.set_hash(u32::MAX, &sha1).is_err());

    // v1-only torrent rejects v2 hashes outright
    let sha256 = [0xcdu8; 32];
    assert!(ct.set_hash2(0, 0, &sha256).is_err());
}

#[test]
fn set_hash2_out_of_range_is_rejected() {
    let files = vec![
        FileEntry::new("hashes2/a.bin", 40_000).unwrap(),
        FileEntry::new("hashes2/b.txt", 10_000).unwrap(),
    ];
    let mut ct = CreateTorrent::new(&files, 16384, CreateFlags::V2_ONLY).unwrap();

    let sha256 = [0xcdu8; 32];
    // file 0 is a.bin: 40_000 bytes / 16 KiB pieces = 3 pieces
    ct.set_hash2(0, 0, &sha256).unwrap();
    ct.set_hash2(0, 2, &sha256).unwrap();
    assert!(ct.set_hash2(0, 3, &sha256).is_err());
    assert!(ct.set_hash2(0, u32::MAX, &sha256).is_err());
    // out-of-range file index
    assert!(ct.set_hash2(1_000_000, 0, &sha256).is_err());
    assert!(ct.set_hash2(u32::MAX, 0, &sha256).is_err());
    // Caller indices address the original entries only; the pad files the
    // v2 canonical layout interleaves consume no indices.
    ct.set_hash2(1, 0, &sha256).unwrap();
    assert!(ct.set_hash2(2, 0, &sha256).is_err());

    // v2-only torrent rejects v1 hashes outright
    let sha1 = [0xabu8; 20];
    assert!(ct.set_hash(0, &sha1).is_err());
}

#[test]
fn set_hash2_rejects_all_zero_hash() {
    // libtorrent reserves the all-zero SHA-256 as "unset" and only asserts
    // against it; the binding must reject it with an error instead.
    let files = vec![FileEntry::new("zero/a.bin", 40_000).unwrap()];
    let mut ct = CreateTorrent::new(&files, 16384, CreateFlags::V2_ONLY).unwrap();
    let err = ct.set_hash2(0, 0, &[0u8; 32]).unwrap_err();
    assert!(err.message().contains("all-zero"), "{:?}", err.message());
    ct.set_hash2(0, 0, &[0xcdu8; 32]).unwrap();
}

#[test]
fn v1_only_plus_v2_only_is_rejected() {
    let files = vec![FileEntry::new("both/a.bin", 1024).unwrap()];
    let err = match CreateTorrent::new(&files, 16384, CreateFlags::V1_ONLY | CreateFlags::V2_ONLY) {
        Ok(_) => panic!("mutually exclusive flags accepted"),
        Err(e) => e,
    };
    assert!(
        err.message().contains("mutually exclusive"),
        "{:?}",
        err.message()
    );
}

#[test]
fn file_entry_builder() {
    let entry = FileEntry::new("path/to/file.txt", 1024).unwrap();
    assert_eq!(entry.size, 1024);
    assert_eq!(entry.flags, FileFlags::empty());

    let entry = entry
        .with_flags(FileFlags::EXECUTABLE)
        .with_mtime(1234567890);

    assert!(entry.flags.contains(FileFlags::EXECUTABLE));
    assert_eq!(entry.mtime, Some(1234567890));
}

#[test]
fn out_of_range_piece_size_is_rejected() {
    // 2^31 would wrap negative through the C ABI's i32 and abort inside
    // libtorrent's asserts; the safe layer must reject it up front.
    let files = vec![FileEntry::new("f/a.bin", 1024).unwrap()];
    let err = match CreateTorrent::new(&files, 2_147_483_648, CreateFlags::V1_ONLY) {
        Ok(_) => panic!("oversized piece_size accepted"),
        Err(e) => e,
    };
    assert!(err.message().contains("piece_size"), "{:?}", err.message());
    assert!(
        CreateTorrent::new(&files, CreateTorrent::MAX_PIECE_SIZE, CreateFlags::V1_ONLY).is_ok()
    );
}

#[test]
fn excessive_piece_count_is_rejected() {
    // 4 x 2^43 bytes at 16 KiB pieces is 2^31 pieces, which exceeds
    // libtorrent's max_num_pieces of 2^30 - 1 (the numeric_cast into the
    // int piece count only asserts).
    let files: Vec<_> = (0..4)
        .map(|i| FileEntry::new(format!("f/{i}.bin"), 1 << 43).unwrap())
        .collect();
    let err = match CreateTorrent::new(&files, 16 * 1024, CreateFlags::V1_ONLY) {
        Ok(_) => panic!("piece count above max_num_pieces accepted"),
        Err(e) => e,
    };
    assert!(err.message().contains("piece count"), "{:?}", err.message());

    // The same total with a proportionate piece size is fine.
    assert!(CreateTorrent::new(&files, 32 * 1024 * 1024, CreateFlags::V1_ONLY).is_ok());
}

#[test]
fn negative_file_sizes_are_rejected() {
    assert!(FileEntry::new("f/a.bin", -1).is_err());
    assert!(FileEntry::new("f/a.bin", i64::MIN).is_err());

    // `size` is a public field, so CreateTorrent::new re-validates it.
    let mut entry = FileEntry::new("f/a.bin", 1).unwrap();
    entry.size = -1;
    assert!(CreateTorrent::new(&[entry], 0, CreateFlags::empty()).is_err());
}

#[test]
fn aggregate_file_size_overflow_is_rejected() {
    let a = FileEntry::new("f/a.bin", i64::MAX).unwrap();
    let b = FileEntry::new("f/b.bin", i64::MAX).unwrap();
    assert!(CreateTorrent::new(&[a, b], 0, CreateFlags::empty()).is_err());
}

#[test]
fn list_files_on_empty_dir_returns_empty() {
    let dir = TempDir::new().unwrap();
    let entries = list_files(dir.path(), CreateFlags::empty()).unwrap();
    assert!(entries.is_empty());
}

#[test]
fn set_hash2_uses_original_entry_order() {
    // Entries deliberately out of canonical order: v2 construction sorts
    // the list by path and interleaves pad entries before indices are
    // interpreted, so manual hashes must be translated.
    let files = vec![
        FileEntry::new("root/bbb.bin", 100).unwrap(),
        FileEntry::new("root/aaa.bin", 200).unwrap(),
    ];
    let mut ct = CreateTorrent::new(&files, 16_384, CreateFlags::V2_ONLY).unwrap();

    let hash_b = [0xBB_u8; 32];
    let hash_a = [0xAA_u8; 32];
    // Original index 0 = bbb.bin, 1 = aaa.bin; each file fits one piece.
    ct.set_hash2(0, 0, &hash_b).unwrap();
    ct.set_hash2(1, 0, &hash_a).unwrap();
    // Pad entries never consume caller indices.
    assert!(ct.set_hash2(2, 0, &hash_a).is_err());

    let data = ct.generate().unwrap();
    let params = AddTorrentParams::from_torrent_buffer(&data).unwrap();
    let ti = params.ti().unwrap();

    // Single-piece files: the file's merkle root is its one piece hash,
    // so a swapped mapping would swap the roots between the names.
    let mut roots = std::collections::HashMap::new();
    for f in ti.files() {
        if !f.flags().is_pad_file() {
            roots.insert(f.path().replace('\\', "/"), f.root().expect("root"));
        }
    }
    assert_eq!(roots["root/aaa.bin"].0, hash_a);
    assert_eq!(roots["root/bbb.bin"].0, hash_b);
}

#[test]
fn duplicate_paths_are_rejected() {
    let files = vec![
        FileEntry::new("root/same.bin", 100).unwrap(),
        FileEntry::new("root/same.bin", 200).unwrap(),
    ];
    assert!(CreateTorrent::new(&files, 16_384, CreateFlags::V2_ONLY).is_err());
}

#[test]
fn multi_file_requires_one_common_root() {
    // libtorrent derives the torrent name from the first entry's root and
    // silently rewrites disagreeing paths under it when generating.
    let a = FileEntry::new("a/foo.bin", 100).unwrap();
    let b = FileEntry::new("b/bar.bin", 100).unwrap();
    let err = match CreateTorrent::new(&[a, b], 16_384, CreateFlags::empty()) {
        Err(e) => e,
        Ok(_) => panic!("disjoint roots must be rejected"),
    };
    assert!(err.to_string().contains("root"), "{err}");

    let rooted = FileEntry::new("a/foo.bin", 100).unwrap();
    let bare = FileEntry::new("bare.bin", 100).unwrap();
    assert!(CreateTorrent::new(&[rooted, bare], 16_384, CreateFlags::empty()).is_err());

    let traversal = FileEntry::new("a/../foo.bin", 100).unwrap();
    assert!(CreateTorrent::new(&[traversal], 16_384, CreateFlags::empty()).is_err());
    let empty = FileEntry::new("", 100).unwrap();
    assert!(CreateTorrent::new(&[empty], 16_384, CreateFlags::empty()).is_err());

    // A one-file directory torrent and a bare single file both stand.
    let single_rooted = [FileEntry::new("a/foo.bin", 1).unwrap()];
    assert!(CreateTorrent::new(&single_rooted, 16_384, CreateFlags::empty()).is_ok());
    let single_bare = [FileEntry::new("foo.bin", 1).unwrap()];
    assert!(CreateTorrent::new(&single_bare, 16_384, CreateFlags::empty()).is_ok());
}
