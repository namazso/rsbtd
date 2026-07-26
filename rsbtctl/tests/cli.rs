// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Daemon-free CLI behavior: argument handling and error paths. (This
//! test also forces the `rsbtctl` binary to be built during workspace
//! test runs, which the rsbtd end-to-end tests depend on.)

use std::process::Command;

fn ctl() -> Command {
    Command::new(env!("CARGO_BIN_EXE_rsbtctl"))
}

#[test]
fn help_lists_subcommands() {
    let output = ctl().arg("--help").output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    for cmd in [
        "add", "list", "status", "remove", "wait", "settings", "create",
    ] {
        assert!(stdout.contains(cmd), "--help is missing {cmd}");
    }
}

#[test]
fn status_takes_a_uuid_or_one_hash() {
    let help = ctl().args(["status", "--help"]).output().unwrap();
    assert!(help.status.success());
    let stdout = String::from_utf8_lossy(&help.stdout);
    for flag in ["--hash-v1", "--hash-v2"] {
        assert!(stdout.contains(flag), "status --help is missing {flag}");
    }

    // The three keys are mutually exclusive, and one is required.
    for args in [
        vec!["status"],
        vec!["status", "--hash-v1", "ab", "--hash-v2", "cd"],
        vec![
            "status",
            "0f1e2d3c-0000-0000-0000-000000000000",
            "--hash-v1",
            "ab",
        ],
    ] {
        let output = ctl()
            .args(["--url", "http://127.0.0.1:1"])
            .args(&args)
            .output()
            .unwrap();
        assert!(!output.status.success(), "accepted {args:?}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("cannot be used with") || stderr.contains("required arguments"),
            "unexpected stderr for {args:?}: {stderr}"
        );
    }
}

#[test]
fn requires_a_target() {
    let output = ctl()
        // On Windows an installed rsbtd's registry config would provide
        // a target; point the lookup at a key that cannot exist.
        .env("RSBTCTL_REGISTRY_KEY", r"Software\rsbtd-cli-test-absent")
        .arg("version")
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("--url"), "unexpected stderr: {stderr}");
}

#[test]
fn rejects_non_http_urls() {
    let output = ctl()
        .args(["--url", "ftp://127.0.0.1:1", "version"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("use http:// or https://"),
        "unexpected stderr: {stderr}"
    );
}

#[test]
fn unreachable_daemon_is_a_transport_error() {
    // Port 1 on loopback is essentially guaranteed closed.
    let output = ctl()
        .args(["--url", "http://127.0.0.1:1", "version"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("cannot connect"),
        "unexpected stderr: {stderr}"
    );
}

#[test]
fn https_urls_are_accepted() {
    // Reaches the connect stage (and fails there — port 1 is closed)
    // instead of being rejected at URL parsing.
    let output = ctl()
        .args(["--url", "https://127.0.0.1:1", "version"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("cannot connect"),
        "unexpected stderr: {stderr}"
    );
}
