// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Monitor and print all alerts from a session.
//!
//! Usage: cargo run --example alert_dump

use rbtorrent::*;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<()> {
    println!("Starting session...");

    let mut settings = SettingsPack::new();
    settings.enable_dht(true).enable_lsd(true);

    let session_params = SessionParams::new().settings(&settings);
    let session = Session::new(session_params)?;

    let mut alerts = session.alerts();

    println!("Monitoring alerts... Press Ctrl+C to stop.\n");

    // Pinned once outside the loop: re-creating ctrl_c() per iteration
    // could drop a signal arriving between registrations.
    let ctrl_c = tokio::signal::ctrl_c();
    tokio::pin!(ctrl_c);

    loop {
        tokio::select! {
            _ = &mut ctrl_c => break,
            batch_result = tokio::time::timeout(Duration::from_secs(1), alerts.next_batch()) => {
                match batch_result {
                    Ok(Ok(batch)) => {
                        for alert in batch.iter() {
                            print_alert(&alert);
                        }
                    }
                    Ok(Err(e)) => return Err(e),
                    // No alerts within the timeout; keep waiting.
                    Err(_elapsed) => {}
                }
            }
        }
    }

    println!("Shutting down...");
    drop(alerts);
    session.close().await;
    Ok(())
}

fn print_alert(alert: &Alert) {
    use std::time::{SystemTime, UNIX_EPOCH};

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let raw = alert.raw();

    // Print basic alert info available on all alerts
    if let Some(ty) = raw.alert_type() {
        println!("[{}] {:?}: {}", timestamp, ty, raw.message());
    } else {
        println!("[{}] {}: {}", timestamp, raw.what(), raw.message());
    }
}
