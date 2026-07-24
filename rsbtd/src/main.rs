// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The rsbtd binary. On unix-y platforms it is a foreground daemon
//! configured by a TOML file; on Windows it is a tray application
//! configured from the registry (see [`rsbtd::windows`]).
#![cfg_attr(windows, windows_subsystem = "windows")]

use std::process::ExitCode;

#[cfg(windows)]
fn main() -> ExitCode {
    rsbtd::windows::tray_main()
}

#[cfg(not(windows))]
fn main() -> ExitCode {
    use clap::Parser;
    use tracing_subscriber::EnvFilter;

    /// rsbtd — torrent client daemon with a GraphQL API.
    #[derive(Parser)]
    #[command(version, about)]
    struct Args {
        /// Path to the TOML configuration file.
        #[arg(short, long)]
        config: std::path::PathBuf,
    }

    let args = Args::parse();

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    let config = match rsbtd::Config::load(&args.config) {
        Ok(config) => config,
        Err(e) => {
            eprintln!("rsbtd: {e}");
            return ExitCode::FAILURE;
        }
    };

    let runtime = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("rsbtd: cannot start runtime: {e}");
            return ExitCode::FAILURE;
        }
    };

    let result = runtime.block_on(async {
        // Install the signal handlers before startup: a signal during a
        // long state restore is buffered until a clean shutdown is possible.
        #[cfg(unix)]
        let shutdown = {
            use tokio::signal::unix::{SignalKind, signal};
            let mut interrupt =
                signal(SignalKind::interrupt()).expect("cannot install SIGINT handler");
            let mut terminate =
                signal(SignalKind::terminate()).expect("cannot install SIGTERM handler");
            async move {
                tokio::select! {
                    _ = interrupt.recv() => {}
                    _ = terminate.recv() => {}
                }
                tracing::info!("shutdown signal received");
            }
        };
        #[cfg(not(unix))]
        let shutdown = async {
            let _ = tokio::signal::ctrl_c().await;
            tracing::info!("shutdown signal received");
        };
        rsbtd::run(config, shutdown).await
    });

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            tracing::error!("{e}");
            ExitCode::FAILURE
        }
    }
}
