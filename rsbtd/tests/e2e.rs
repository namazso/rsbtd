// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Full end-to-end tests: a two-daemon transfer driven purely over the
//! GraphQL API (magnet add, metadata exchange, completion), and a
//! binary-level test spawning the real `rsbtd` daemon, driving it with the
//! real `rsbtctl`, and shutting it down with SIGTERM.
//!
//! Fully hermetic: loopback only, DHT/LSD/UPnP/NAT-PMP off.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::Duration;

use http_body_util::{BodyExt, Full};
use hyper::body::Bytes;
use hyper::{Request, StatusCode};
use hyper_util::rt::TokioIo;
use rbtorrent::SettingsPack;
use rsbtd::Daemon;
use rsbtd::config::{Config, Listen};
use serde_json::{Value, json};
use tokio::time::timeout;

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

fn tcp_config(state_dir: &Path) -> Config {
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

fn fixture_path() -> PathBuf {
    // Built from components: libtorrent opens paths natively, and native
    // Windows path handling accepts neither `..` nor forward slashes.
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("rbtorrent")
        .join("tests")
        .join("fixtures")
        .join("transfer.torrent")
}

/// A path as a GraphQL string literal (quotes included): JSON escaping
/// keeps Windows path separators intact inside the query text.
fn gql_path(path: &std::path::Path) -> String {
    serde_json::to_string(path.to_str().unwrap()).unwrap()
}

/// Regenerates the fixture payload (same xorshift PRNG as gen_fixtures.cpp).
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

fn write_fixture_content(dir: &Path) -> Vec<u8> {
    let content = generate_fixture_content();
    let fixture_dir = dir.join("fixture");
    std::fs::create_dir_all(&fixture_dir).unwrap();
    std::fs::write(fixture_dir.join("a.bin"), &content[..40960]).unwrap();
    std::fs::write(fixture_dir.join("b.txt"), &content[40960..]).unwrap();
    content
}

/// Runs a GraphQL document against a TCP daemon, panicking on errors.
async fn graphql(addr: SocketAddr, query: &str) -> Value {
    let stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    let (mut sender, conn) = hyper::client::conn::http1::handshake(TokioIo::new(stream))
        .await
        .unwrap();
    tokio::spawn(conn);
    let request = Request::builder()
        .method("POST")
        .uri("/graphql")
        .header("host", "rsbtd")
        .header("content-type", "application/json")
        .body(Full::new(Bytes::from(
            json!({ "query": query }).to_string(),
        )))
        .unwrap();
    let response = sender.send_request(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let mut parsed: Value = serde_json::from_slice(&body).unwrap();
    assert!(
        parsed.get("errors").is_none(),
        "graphql errors: {}",
        parsed["errors"]
    );
    parsed["data"].take()
}

/// Transfers the fixture between two daemons using nothing but the API:
/// the seed adds from .torrent data, the leech adds the seed's magnet
/// link, fetches metadata from the peer, downloads, and finishes.
#[tokio::test(flavor = "multi_thread")]
async fn two_daemon_transfer_via_api() {
    use base64::Engine as _;

    let seed_state = tempfile::tempdir().unwrap();
    let seed_data = tempfile::tempdir().unwrap();
    let leech_state = tempfile::tempdir().unwrap();
    let leech_data = tempfile::tempdir().unwrap();
    let content = write_fixture_content(seed_data.path());

    let seed = Daemon::start(tcp_config(seed_state.path()), Some(hermetic_settings()))
        .await
        .unwrap();
    let leech = Daemon::start(tcp_config(leech_state.path()), Some(hermetic_settings()))
        .await
        .unwrap();
    let seed_addr = seed.tcp_addr().unwrap();
    let leech_addr = leech.tcp_addr().unwrap();

    let torrent_b64 =
        base64::engine::general_purpose::STANDARD.encode(std::fs::read(fixture_path()).unwrap());
    let data = graphql(
        seed_addr,
        &format!(
            r#"mutation {{ addTorrent(input: {{
                torrentData: "{torrent_b64}",
                savePath: {},
                flags: [SEED_MODE]
            }}) {{ infoHashV1 magnetUri }} }}"#,
            gql_path(seed_data.path())
        ),
    )
    .await;
    let magnet = data["addTorrent"]["magnetUri"].as_str().unwrap().to_owned();
    let v1_hex = data["addTorrent"]["infoHashV1"]
        .as_str()
        .unwrap()
        .to_owned();

    // Wait until the seed actually seeds, and learn its peer port.
    timeout(Duration::from_secs(30), async {
        loop {
            let data = graphql(
                seed_addr,
                &format!(r#"{{ torrent(infoHash: "{v1_hex}") {{ isSeeding }} }}"#),
            )
            .await;
            if data["torrent"]["isSeeding"] == true {
                return;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("seed never reached seeding");
    let data = graphql(seed_addr, "{ session { listenPort } }").await;
    let seed_port = data["session"]["listenPort"].as_i64().unwrap();

    // Leech: add the magnet (no metadata yet) and connect to the seed.
    let data = graphql(
        leech_addr,
        &format!(
            r#"mutation {{ addTorrent(input: {{
                magnetUri: "{magnet}",
                savePath: {}
            }}) {{ infoHashV1 hasMetadata }} }}"#,
            gql_path(leech_data.path())
        ),
    )
    .await;
    assert_eq!(data["addTorrent"]["infoHashV1"], v1_hex);
    assert_eq!(data["addTorrent"]["hasMetadata"], false);
    graphql(
        leech_addr,
        &format!(
            r#"mutation {{ connectPeer(infoHash: "{v1_hex}", address: "127.0.0.1:{seed_port}") }}"#
        ),
    )
    .await;

    // Metadata arrives from the peer, then the payload completes.
    timeout(Duration::from_secs(60), async {
        loop {
            let data = graphql(
                leech_addr,
                &format!(
                    r#"{{ torrent(infoHash: "{v1_hex}") {{ hasMetadata isFinished name state
                          progressPpm savePath totalWanted totalWantedDone
                          files {{ path progressBytes size }} }} }}"#
                ),
            )
            .await;
            let t = &data["torrent"];
            if t["hasMetadata"] == true && t["isFinished"] == true {
                assert_eq!(t["name"], "fixture");
                return;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("transfer did not finish");

    // The payload reaches disk after a cache flush and matches.
    graphql(
        leech_addr,
        &format!(r#"mutation {{ flushCache(infoHash: "{v1_hex}") }}"#),
    )
    .await;
    timeout(Duration::from_secs(30), async {
        loop {
            let a = std::fs::read(leech_data.path().join("fixture/a.bin"));
            let b = std::fs::read(leech_data.path().join("fixture/b.txt"));
            if let (Ok(a), Ok(b)) = (a, b)
                && a == content[..40960]
                && b == content[40960..]
            {
                return;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("payload never appeared on disk");

    leech.stop().await;
    seed.stop().await;
}

/// Spawns the real `rsbtd` binary, drives it with the real `rsbtctl`
/// binary over the unix socket, and terminates it with SIGTERM.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn subprocess_daemon_driven_by_ctl() {
    let ctl_bin = PathBuf::from(env!("CARGO_BIN_EXE_rsbtd"))
        .parent()
        .unwrap()
        .join("rsbtctl");
    assert!(
        ctl_bin.exists(),
        "rsbtctl binary not built at {} — run via `cargo test --workspace`",
        ctl_bin.display()
    );

    let state = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let create_out = tempfile::tempdir().unwrap();
    let sock = state.path().join("api.sock");
    let token = "e2e-t0ken";

    // Pre-seed the state directory with hermetic settings: boot an engine
    // once and shut it down so session.state carries DHT-off/loopback,
    // which the subprocess then loads (settings are API-managed state).
    {
        let engine =
            rsbtd::engine::Engine::start(&tcp_config(state.path()), Some(hermetic_settings()))
                .await
                .unwrap();
        engine.shutdown().await;
    }

    let config_path = state.path().join("rsbtd.toml");
    std::fs::write(
        &config_path,
        format!(
            "state_dir = {:?}\n[api]\nlisten = \"unix:{}\"\ntoken = {token:?}\n",
            state.path(),
            sock.display()
        ),
    )
    .unwrap();

    let mut daemon = std::process::Command::new(env!("CARGO_BIN_EXE_rsbtd"))
        .arg("--config")
        .arg(&config_path)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("cannot spawn rsbtd");

    let ctl = |args: Vec<String>| {
        let ctl_bin = ctl_bin.clone();
        let sock = sock.clone();
        async move {
            tokio::task::spawn_blocking(move || {
                let output = std::process::Command::new(&ctl_bin)
                    .arg("--unix")
                    .arg(&sock)
                    .arg("--token")
                    .arg(token)
                    .args(&args)
                    .output()
                    .expect("cannot run rsbtctl");
                (
                    output.status.success(),
                    String::from_utf8_lossy(&output.stdout).into_owned(),
                    String::from_utf8_lossy(&output.stderr).into_owned(),
                )
            })
            .await
            .unwrap()
        }
    };
    let args = |list: &[&str]| list.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();

    // Wait for the API to come up.
    timeout(Duration::from_secs(30), async {
        loop {
            let (ok, stdout, _) = ctl(args(&["version"])).await;
            if ok {
                assert!(stdout.contains("rsbtd"), "odd version output: {stdout}");
                return;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("daemon API never came up");

    let output = std::process::Command::new(&ctl_bin)
        .args([
            "--unix",
            sock.to_str().unwrap(),
            "--token",
            "wrong",
            "version",
        ])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("unauthorized"));

    let content = write_fixture_content(data.path());
    let (ok, stdout, stderr) = ctl(args(&[
        "add",
        "--file",
        fixture_path().to_str().unwrap(),
        "--save-path",
        data.path().to_str().unwrap(),
        "--seed-mode",
    ]))
    .await;
    assert!(ok, "add failed: {stderr}");
    let v1_hex = stdout
        .split_whitespace()
        .nth(1)
        .expect("no hash in add output")
        .to_owned();
    assert_eq!(v1_hex.len(), 40, "unexpected add output: {stdout}");

    let (ok, _, stderr) = ctl(args(&[
        "wait",
        &v1_hex,
        "--until",
        "seeding",
        "--timeout",
        "60",
    ]))
    .await;
    assert!(ok, "wait failed: {stderr}");
    let (ok, stdout, _) = ctl(args(&["list"])).await;
    assert!(ok && stdout.contains(&v1_hex), "list output: {stdout}");
    let (ok, stdout, _) = ctl(args(&["status", &v1_hex])).await;
    assert!(ok && stdout.contains("state: SEEDING"), "status: {stdout}");
    let (ok, stdout, stderr) = ctl(args(&["settings", "set", "upload_rate_limit=123456"])).await;
    assert!(ok, "settings set failed: {stderr}");
    assert!(stdout.contains("123456"));
    let (ok, stdout, _) = ctl(args(&["settings", "get", "upload_rate_limit"])).await;
    assert!(
        ok && stdout.trim() == "upload_rate_limit = 123456",
        "get: {stdout}"
    );
    // Structured settings take JSON values, coerced via introspection.
    let (ok, stdout, stderr) = ctl(args(&[
        "settings",
        "set",
        r#"outgoing_port_range={"first":6900,"last":6910}"#,
    ]))
    .await;
    assert!(ok, "json settings set failed: {stderr}");
    assert!(stdout.contains("6900"), "set output: {stdout}");
    let (ok, stdout, _) = ctl(args(&["settings", "get", "outgoing_port_range"])).await;
    assert!(ok && stdout.contains("6910"), "json get: {stdout}");
    // Hidden backing settings are not in the schema at all.
    let (ok, _, stderr) = ctl(args(&["settings", "set", "proxy_type=2"])).await;
    assert!(!ok, "hidden setting accepted");
    assert!(stderr.contains("proxy"), "stderr: {stderr}");
    let (ok, stdout, _) = ctl(args(&["stats", "net.recv_payload_bytes"])).await;
    assert!(
        ok && stdout.contains("net.recv_payload_bytes ="),
        "stats: {stdout}"
    );

    // Torrent creation through ctl (daemon-side output path).
    let out_file = create_out.path().join("fixture.torrent");
    let (ok, stdout, stderr) = ctl(args(&[
        "create",
        data.path().join("fixture").to_str().unwrap(),
        "--out",
        out_file.to_str().unwrap(),
        "--tracker",
        "http://127.0.0.1:1/announce",
        "--comment",
        "e2e",
    ]))
    .await;
    assert!(ok, "create failed: {stderr}");
    assert!(stdout.contains("created"), "create output: {stdout}");
    rbtorrent::AddTorrentParams::from_torrent_buffer(&std::fs::read(&out_file).unwrap()).unwrap();

    let (ok, _, _) = ctl(args(&["session-pause"])).await;
    assert!(ok);
    let (ok, stdout, _) = ctl(args(&["session", "--json"])).await;
    assert!(
        ok && stdout.contains("\"isPaused\": true"),
        "session: {stdout}"
    );
    let (ok, _, _) = ctl(args(&["session-resume"])).await;
    assert!(ok);

    let pid = daemon.id().to_string();
    assert!(
        std::process::Command::new("kill")
            .args(["-TERM", &pid])
            .status()
            .unwrap()
            .success()
    );
    let exit = tokio::task::spawn_blocking(move || daemon.wait().unwrap())
        .await
        .unwrap();
    assert!(exit.success(), "daemon exited with {exit}");
    assert!(state.path().join("session.state").exists());
    let resume = state
        .path()
        .join("torrents")
        .join(format!("{v1_hex}.resume"));
    assert!(resume.exists(), "resume file missing after SIGTERM");
    assert!(!sock.exists(), "socket file left behind");
    // Payload untouched.
    assert_eq!(
        std::fs::read(data.path().join("fixture/a.bin")).unwrap(),
        content[..40960]
    );
}
