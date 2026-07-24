// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Create a .torrent file from a directory or file.
//!
//! Usage: cargo run --example make_torrent <input-path> <output.torrent> [options]

use rbtorrent::*;
use std::env;
use std::fs;
use std::path::Path;

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 3 {
        eprintln!(
            "Usage: {} <input-path> <output.torrent> [tracker-url]",
            args[0]
        );
        eprintln!("\nExample:");
        eprintln!(
            "  {} ./my_files output.torrent http://tracker.example.com:8080/announce",
            args[0]
        );
        std::process::exit(1);
    }

    let input_path = Path::new(&args[1]);
    let output_path = &args[2];
    let tracker_url = args.get(3).map(|s| s.as_str());

    if !input_path.exists() {
        eprintln!(
            "Error: Input path '{}' does not exist",
            input_path.display()
        );
        std::process::exit(1);
    }

    println!("Creating torrent from: {}", input_path.display());

    let files = list_files(input_path, CreateFlags::empty())?;
    println!("Found {} file(s)", files.len());

    let total_size: i64 = files.iter().map(|f| f.size).sum();
    println!(
        "Total size: {} bytes ({:.2} MB)",
        total_size,
        total_size as f64 / 1_048_576.0
    );

    // Use automatic piece size (0)
    let mut ct = CreateTorrent::new(&files, 0, CreateFlags::empty())?;

    println!("Piece length: {} bytes", ct.piece_length());
    println!("Number of pieces: {}", ct.num_pieces());

    ct.set_creator("rbtorrent example")?;
    ct.set_comment(&format!("Created from {}", input_path.display()))?;

    if let Some(url) = tracker_url {
        println!("Adding tracker: {}", url);
        ct.add_tracker(url, 0)?;
    }

    println!("\nHashing pieces...");
    let parent_dir = input_path.parent().unwrap_or(Path::new("."));

    let total_pieces = ct.num_pieces();
    set_piece_hashes(
        &mut ct,
        parent_dir,
        Some(|idx| {
            let progress = ((idx + 1) as f64 / total_pieces as f64 * 100.0) as u32;
            print!("\rProgress: {}% ({}/{})", progress, idx + 1, total_pieces);
            use std::io::Write;
            std::io::stdout().flush().unwrap();
            true
        }),
    )?;

    println!("\n\nGenerating .torrent file...");

    let torrent_data = ct.generate()?;

    fs::write(output_path, &torrent_data)?;

    println!("✓ Successfully created: {}", output_path);
    println!("  Size: {} bytes", torrent_data.len());
    println!("  Pieces: {}", ct.num_pieces());
    println!("  Piece size: {} bytes", ct.piece_length());

    if ct.is_v1_only() {
        println!("  Format: BitTorrent v1 only");
    } else if ct.is_v2_only() {
        println!("  Format: BitTorrent v2 only");
    } else {
        println!("  Format: Hybrid (v1 + v2)");
    }

    Ok(())
}
