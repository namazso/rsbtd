// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! File logging for the tray app: a GUI-subsystem process has no console,
//! so tracing goes to a daily-rolling file under `%LOCALAPPDATA%\rsbtd\logs`.

use std::path::PathBuf;

use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::EnvFilter;

/// Where log files live; also shown in error dialogs so users can find them.
pub fn log_dir() -> Option<PathBuf> {
    let base = std::env::var_os("LOCALAPPDATA")?;
    Some(PathBuf::from(base).join("rsbtd").join("logs"))
}

/// Initializes tracing. The returned guard must live for the whole
/// process; dropping it stops the background log writer.
pub fn init(filter: &str) -> Result<WorkerGuard, String> {
    let dir = log_dir().ok_or("LOCALAPPDATA is not set; cannot place log files")?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    let appender = tracing_appender::rolling::Builder::new()
        .rotation(tracing_appender::rolling::Rotation::DAILY)
        .filename_prefix("rsbtd")
        .filename_suffix("log")
        .max_log_files(7)
        .build(&dir)
        .map_err(|e| format!("cannot open log file in {}: {e}", dir.display()))?;
    let (writer, guard) = tracing_appender::non_blocking(appender);
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_new(filter).unwrap_or_else(|_| EnvFilter::new("info")))
        .with_writer(writer)
        .with_ansi(false)
        .init();
    Ok(guard)
}
