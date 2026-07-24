// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Simple CLI torrent downloader example.
//!
//! Usage: cargo run --example cli_download <torrent-file> [save-path]

use rbtorrent::*;
use std::env;
use std::time::Duration;

#[tokio::main]
async fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <torrent-file> [save-path]", args[0]);
        std::process::exit(1);
    }

    let torrent_path = &args[1];
    let save_path = args.get(2).map(|s| s.as_str()).unwrap_or(".");

    println!("Loading torrent from {}", torrent_path);

    let mut params = AddTorrentParams::from_torrent_file(torrent_path)?;
    params.set_save_path(save_path);

    let mut settings = SettingsPack::new();
    settings
        .enable_dht(true)
        .enable_lsd(true)
        .enable_upnp(true)
        .enable_natpmp(true);

    let session_params = SessionParams::new().settings(&settings);

    println!("Starting session...");
    let session = Session::new(session_params)?;

    println!("Adding torrent...");

    // Get alert stream before adding torrent (required for futures to resolve)
    let mut alerts = session.alerts();

    // Popping batches is what drives the add future, but a popped batch may
    // already carry terminal alerts (a tiny or already-complete torrent can
    // finish before the add future resolves), so inspect every batch and
    // latch what we see. Errors break with a result instead of returning so
    // the session is always shut down gracefully via Session::close at the
    // end (Drop would block the async runtime). The add future and the
    // handle borrow the session, so both live inside this block, which
    // completes before the close below.
    let outcome: std::result::Result<(), Box<dyn std::error::Error>> = async {
        // Ctrl+C returns from this block so the graceful close below
        // still runs.
        let ctrl_c = tokio::signal::ctrl_c();
        tokio::pin!(ctrl_c);

        let add_future = session.add_torrent(&params);
        tokio::pin!(add_future);
        let mut finished = false;
        let mut torrent_error: Option<String> = None;
        let handle = loop {
            tokio::select! {
                _ = &mut ctrl_c => {
                    println!("\nInterrupted; stopping.");
                    return Ok(());
                }
                result = &mut add_future => {
                    break result?;
                }
                batch = alerts.next_batch() => {
                    for alert in batch?.iter() {
                        match &alert {
                            &Alert::TorrentFinished(_) => finished = true,
                            Alert::TorrentError(a) => {
                                torrent_error = Some(
                                    a.error()
                                        .map(|e| e.message().to_string())
                                        .unwrap_or_else(|| "torrent error".into()),
                                );
                            }
                            _ => {}
                        }
                    }
                }
            }
        };

        println!("Torrent added successfully");
        println!("Info hash: {:?}", handle.info_hashes());

        if let Some(msg) = torrent_error {
            eprintln!("\n✗ Error: {}", msg);
            return Err("torrent errored".into());
        }

        println!("\nDownloading... Press Ctrl+C to stop.");

        let mut last_progress = 0;
        loop {
            let batch_result = tokio::select! {
                _ = &mut ctrl_c => {
                    println!("\nInterrupted; stopping.");
                    return Ok(());
                }
                result = tokio::time::timeout(Duration::from_secs(1), alerts.next_batch()) => {
                    result
                }
            };

            let mut done = finished;
            if let Ok(Ok(ref batch)) = batch_result {
                let mut errored = false;
                for alert in batch.iter() {
                    match &alert {
                        &Alert::TorrentFinished(_) => done = true,
                        Alert::TorrentError(a) => {
                            if let Some(err) = a.error() {
                                eprintln!("\n✗ Error: {}", err.message());
                            } else {
                                eprintln!("\n✗ Torrent error");
                            }
                            errored = true;
                        }
                        _ => {}
                    }
                }
                if errored {
                    return Err("torrent errored".into());
                }
            }

            let status = handle.status(0)?;
            let progress = (status.progress() * 100.0) as u32;

            if progress != last_progress || progress == 0 {
                print_status(&status);
                last_progress = progress;
            }

            // `done` covers the alert; is_finished covers a missed alert.
            if done || status.is_finished() {
                println!("\n✓ Download complete!");
                println!("Total downloaded: {} bytes", status.total_done());
                println!("Saved to: {}", save_path);
                return Ok(());
            }
        }
    }
    .await;

    // Graceful shutdown: release the alert stream, then close the session
    // without blocking the runtime (see the crate-level docs).
    drop(alerts);
    session.close().await;

    outcome
}

fn print_status(status: &TorrentStatus) {
    let progress = (status.progress() * 100.0) as u32;
    let down_rate = status.download_rate() as f64 / 1024.0; // KB/s
    let up_rate = status.upload_rate() as f64 / 1024.0;

    print!(
        "\r[{:3}%] ↓ {:.1} KB/s ↑ {:.1} KB/s | Peers: {} | Seeds: {} | State: {:?}",
        progress,
        down_rate,
        up_rate,
        status.num_peers(),
        status.num_seeds(),
        status.state()
    );

    use std::io::Write;
    std::io::stdout().flush().unwrap();
}
