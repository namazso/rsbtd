// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! HTTP/GraphQL API integration tests: auth, the read surface over TCP,
//! and unix-domain-socket serving. Fully hermetic (loopback, DHT off).

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use http_body_util::{BodyExt, Full};
use hyper::body::Bytes;
use hyper::{Request, StatusCode};
use hyper_util::rt::TokioIo;
use rbtorrent::{AddTorrentParams, SettingsPack, TorrentFlags};
use rsbtd::Daemon;
use rsbtd::config::{Config, Listen};
use rsbtd::engine::registry::TorrentEntry;
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

fn test_config(state_dir: &Path, listen: Listen, token: Option<&str>, graphiql: bool) -> Config {
    Config {
        state_dir: state_dir.to_path_buf(),
        listen,
        token: token.map(str::to_owned),
        graphiql,
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
fn gql_path(path: &Path) -> String {
    serde_json::to_string(path.to_str().unwrap()).unwrap()
}

/// Writes the fixture's payload files (same xorshift PRNG as
/// gen_fixtures.cpp) under `dir` so a `SEED_MODE` add can seed them.
fn write_fixture_content(dir: &Path) {
    fn xorshift(state: &mut u32) -> u8 {
        *state ^= *state << 13;
        *state ^= *state >> 17;
        *state ^= *state << 5;
        (*state & 0xff) as u8
    }

    let fixture_dir = dir.join("fixture");
    std::fs::create_dir_all(&fixture_dir).unwrap();
    let mut state = 0xdecafbad_u32;
    let a: Vec<u8> = (0..40960).map(|_| xorshift(&mut state)).collect();
    std::fs::write(fixture_dir.join("a.bin"), &a).unwrap();
    state = 0xb0bafe77_u32;
    let b: Vec<u8> = (0..137).map(|_| xorshift(&mut state)).collect();
    std::fs::write(fixture_dir.join("b.txt"), &b).unwrap();
}

/// The fixture torrent's v1 info-hash, hex-encoded.
fn fixture_v1_hex() -> String {
    AddTorrentParams::from_torrent_file(fixture_path())
        .unwrap()
        .info_hashes()
        .v1()
        .unwrap()
        .to_string()
}

async fn add_seeding_fixture(daemon: &Daemon, data_dir: &Path) -> Arc<TorrentEntry> {
    write_fixture_content(data_dir);
    let mut atp = AddTorrentParams::from_torrent_file(fixture_path()).unwrap();
    atp.set_save_path(data_dir.to_str().unwrap());
    atp.set_flags(atp.flags() | TorrentFlags::SEED_MODE);
    let entry = daemon.engine().add_torrent(&mut atp).await.unwrap();
    timeout(Duration::from_secs(30), async {
        loop {
            if daemon
                .engine()
                .with_handle(&entry, |h| {
                    h.status(0).map(|s| s.is_seeding()).unwrap_or(false)
                })
                .unwrap_or(false)
            {
                return;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("fixture torrent did not reach seeding");
    entry
}

/// Sends one HTTP/1.1 request over an established stream.
async fn raw_request<S>(stream: S, req: Request<Full<Bytes>>) -> (StatusCode, String)
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    let (mut sender, conn) = hyper::client::conn::http1::handshake(TokioIo::new(stream))
        .await
        .unwrap();
    tokio::spawn(conn);
    let resp = sender.send_request(req).await.unwrap();
    let status = resp.status();
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    (status, String::from_utf8_lossy(&body).into_owned())
}

fn graphql_request(token: Option<&str>, query: &str) -> Request<Full<Bytes>> {
    let payload = json!({ "query": query }).to_string();
    let mut builder = Request::builder()
        .method("POST")
        .uri("/graphql")
        .header("host", "rsbtd")
        .header("content-type", "application/json");
    if let Some(token) = token {
        builder = builder.header("authorization", format!("Bearer {token}"));
    }
    builder.body(Full::new(Bytes::from(payload))).unwrap()
}

/// Runs a GraphQL query over TCP and returns the `data` field, panicking
/// on transport or GraphQL errors.
async fn graphql(addr: SocketAddr, token: Option<&str>, query: &str) -> Value {
    let stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    let (status, body) = raw_request(stream, graphql_request(token, query)).await;
    assert_eq!(status, StatusCode::OK, "unexpected status; body: {body}");
    let mut json: Value = serde_json::from_str(&body).unwrap();
    assert!(
        json.get("errors").is_none(),
        "graphql errors: {}",
        json["errors"]
    );
    json["data"].take()
}

#[tokio::test(flavor = "multi_thread")]
async fn read_surface_over_tcp() {
    let state = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let config = test_config(
        state.path(),
        Listen::Tcp("127.0.0.1:0".parse().unwrap()),
        Some("s3cret"),
        false,
    );
    let daemon = Daemon::start(config, Some(hermetic_settings()))
        .await
        .unwrap();
    let addr = daemon.tcp_addr().expect("tcp listener");
    let token = Some("s3cret");

    // healthz needs no auth.
    let stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    let req = Request::builder()
        .uri("/healthz")
        .header("host", "rsbtd")
        .body(Full::new(Bytes::new()))
        .unwrap();
    let (status, body) = raw_request(stream, req).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, "ok");

    // GraphiQL is disabled by default.
    let stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    let req = Request::builder()
        .uri("/")
        .header("host", "rsbtd")
        .body(Full::new(Bytes::new()))
        .unwrap();
    let (status, _) = raw_request(stream, req).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    for bad in [None, Some("wrong")] {
        let stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        let (status, _) = raw_request(stream, graphql_request(bad, "{ version { daemon } }")).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    // Bodies above the 64 MiB cap are rejected before GraphQL parsing
    // (the GraphQL extractor reads the raw body, which DefaultBodyLimit
    // would not restrict). The declared length alone triggers the
    // rejection, before any body bytes are sent.
    {
        use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        stream
            .write_all(
                format!(
                    "POST /graphql HTTP/1.1\r\nhost: rsbtd\r\n\
                     content-type: application/json\r\n\
                     authorization: Bearer s3cret\r\n\
                     content-length: {}\r\n\r\n",
                    64 * 1024 * 1024 + 1
                )
                .as_bytes(),
            )
            .await
            .unwrap();
        let mut buf = vec![0u8; 1024];
        let n = stream.read(&mut buf).await.unwrap();
        let response = String::from_utf8_lossy(&buf[..n]);
        assert!(response.starts_with("HTTP/1.1 413"), "{response}");
    }

    let data_json = graphql(
        addr,
        token,
        "{ version { daemon libtorrent } \
           session { isPaused isListening isDhtRunning listenPort sslListenPort torrentCount } \
           torrents { name } ipFilter { first last blocked } }",
    )
    .await;
    assert_eq!(data_json["version"]["daemon"], env!("CARGO_PKG_VERSION"));
    assert!(
        data_json["version"]["libtorrent"]
            .as_str()
            .unwrap()
            .starts_with("2.")
    );
    assert_eq!(data_json["session"]["isPaused"], false);
    assert_eq!(data_json["session"]["isDhtRunning"], false);
    assert_eq!(data_json["session"]["sslListenPort"], 0);
    assert_eq!(data_json["session"]["torrentCount"], 0);
    assert_eq!(data_json["torrents"], json!([]));
    // An unset filter exports the default allow-all ranges (v4 + v6).
    let rules = data_json["ipFilter"].as_array().unwrap();
    assert!(!rules.is_empty());
    assert!(rules.iter().all(|r| r["blocked"] == false));

    let data_json = graphql(
        addr,
        token,
        r#"{ settings {
            uploadRateLimit enableDht userAgent
            proxy { protocol }
        } }"#,
    )
    .await;
    let settings = &data_json["settings"];
    assert!(settings["uploadRateLimit"].is_i64());
    assert_eq!(settings["enableDht"], false);
    assert!(settings["userAgent"].is_string());
    assert_eq!(settings["proxy"], Value::Null);

    // The schema exposes exactly the reviewed public surface; hidden
    // backing settings and blacklisted settings are absent.
    let all = graphql(
        addr,
        token,
        r#"{ __type(name: "Settings") { fields { name } } }"#,
    )
    .await;
    let names: Vec<&str> = all["__type"]["fields"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["name"].as_str().unwrap())
        .collect();
    // 15 structured settings + 169 scalar passthroughs.
    assert_eq!(names.len(), 184);
    assert!(names.contains(&"proxy"));
    assert!(!names.contains(&"proxyType"));
    assert!(!names.contains(&"alertQueueSize"));
    assert!(!names.contains(&"alertMask"));
    // Settings that are inert in this build are not exposed.
    assert!(!names.contains(&"maxRejects"));
    assert!(!names.contains(&"webtorrentStunServer"));

    // Unknown settings are a GraphQL validation error.
    let stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    let (status, body) = raw_request(
        stream,
        graphql_request(token, r#"{ settings { noSuchSetting } }"#),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("noSuchSetting"), "{body}");

    let data_json = graphql(
        addr,
        token,
        r#"{ sessionStats(names: ["net.recv_payload_bytes"]) { name kind value } }"#,
    )
    .await;
    let stats = data_json["sessionStats"].as_array().unwrap();
    assert_eq!(stats.len(), 1);
    assert_eq!(stats[0]["name"], "net.recv_payload_bytes");

    let entry = add_seeding_fixture(&daemon, data.path()).await;
    let uuid = entry.uuid;
    let v1_hex = fixture_v1_hex();
    let data_json = graphql(
        addr,
        token,
        "{ torrents(state: SEEDING) { \
             uuid name state isSeeding hasMetadata infoHashV1 magnetUri \
             totalSize progressPpm queuePosition flags \
             sizeOnDisk pieceLength isPrivate isI2P \
             pieces(includeBitfield: true) { total have bitfield } \
             files { path size progressBytes priority isPadFile } \
             trackers { url } } }",
    )
    .await;
    let torrents = data_json["torrents"].as_array().unwrap();
    assert_eq!(torrents.len(), 1);
    let t = &torrents[0];
    assert_eq!(t["state"], "SEEDING");
    assert_eq!(t["isSeeding"], true);
    assert_eq!(t["hasMetadata"], true);
    assert_eq!(t["uuid"], uuid.to_string());
    assert_eq!(t["infoHashV1"], v1_hex);
    assert!(
        t["magnetUri"]
            .as_str()
            .unwrap()
            .starts_with(&format!("magnet:?xt=urn:btih:{v1_hex}"))
    );
    assert_eq!(t["totalSize"], 40960 + 137);
    assert_eq!(t["sizeOnDisk"], 40960 + 137);
    assert!(t["pieceLength"].as_i64().unwrap() > 0);
    assert_eq!(t["isPrivate"], false);
    assert_eq!(t["isI2P"], false);
    assert_eq!(t["progressPpm"], 1_000_000);
    // A seed is not in the download queue.
    assert_eq!(t["queuePosition"], Value::Null);
    let pieces = &t["pieces"];
    assert!(pieces["total"].as_i64().unwrap() > 0);
    assert_eq!(pieces["have"], pieces["total"]);
    assert!(pieces["bitfield"].is_string());
    let files = t["files"].as_array().unwrap();
    assert_eq!(files.len(), 2);
    // File paths use the platform's separator (libtorrent joins natively).
    let file_path = |i: usize| files[i]["path"].as_str().unwrap().replace('\\', "/");
    assert_eq!(file_path(0), "fixture/a.bin");
    assert_eq!(files[0]["size"], 40960);
    assert_eq!(files[0]["progressBytes"], 40960);
    assert_eq!(files[0]["priority"], 4);
    assert_eq!(file_path(1), "fixture/b.txt");
    assert!(t["trackers"].is_array());

    let data_json = graphql(
        addr,
        token,
        &format!(
            "{{ torrent(uuid: \"{uuid}\") {{ name peers {{ \
                 client peerId localEndpoint lastRequestUs lastActiveUs \
                 numHashfails failcount downloadRatePeak uploadRatePeak }} }} \
               missing: torrent(uuid: \"{}\") {{ name }} \
               downloading: torrents(state: DOWNLOADING) {{ name }} }}",
            "00000000-0000-0000-0000-000000000000"
        ),
    )
    .await;
    assert!(data_json["torrent"]["name"].is_string());
    assert_eq!(data_json["torrent"]["peers"], json!([]));
    assert_eq!(data_json["missing"], Value::Null);
    assert_eq!(data_json["downloading"], json!([]));

    daemon.stop().await;
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn unix_socket_serving() {
    use std::os::unix::fs::PermissionsExt;

    let state = tempfile::tempdir().unwrap();
    let sock_path = state.path().join("api.sock");

    std::fs::write(&sock_path, b"sentinel").unwrap();
    let config = test_config(state.path(), Listen::Unix(sock_path.clone()), None, true);
    let refused = Daemon::start(config, Some(hermetic_settings())).await;
    assert!(refused.is_err(), "bound over a regular file");
    assert_eq!(
        std::fs::read(&sock_path).unwrap(),
        b"sentinel",
        "startup deleted a non-socket file at the listen path"
    );
    std::fs::remove_file(&sock_path).unwrap();

    // A stale socket file (nothing listening) must be replaced.
    drop(std::os::unix::net::UnixListener::bind(&sock_path).unwrap());
    assert!(sock_path.exists());

    let config = test_config(state.path(), Listen::Unix(sock_path.clone()), None, true);
    let daemon = Daemon::start(config, Some(hermetic_settings()))
        .await
        .unwrap();
    assert!(daemon.tcp_addr().is_none());
    let mode = std::fs::metadata(&sock_path).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o600, "socket must be user-only");

    let dup_state = tempfile::tempdir().unwrap();
    let dup = Daemon::start(
        test_config(
            dup_state.path(),
            Listen::Unix(sock_path.clone()),
            None,
            false,
        ),
        Some(hermetic_settings()),
    )
    .await;
    assert!(dup.is_err(), "bound a socket another daemon is serving");

    // healthz and an unauthenticated query (no token configured).
    let stream = tokio::net::UnixStream::connect(&sock_path).await.unwrap();
    let req = Request::builder()
        .uri("/healthz")
        .header("host", "rsbtd")
        .body(Full::new(Bytes::new()))
        .unwrap();
    let (status, body) = raw_request(stream, req).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, "ok");

    let stream = tokio::net::UnixStream::connect(&sock_path).await.unwrap();
    let (status, body) = raw_request(
        stream,
        graphql_request(None, "{ session { torrentCount } }"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let json: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(json["data"]["session"]["torrentCount"], 0);

    // GraphiQL is enabled on this daemon.
    let stream = tokio::net::UnixStream::connect(&sock_path).await.unwrap();
    let req = Request::builder()
        .uri("/")
        .header("host", "rsbtd")
        .body(Full::new(Bytes::new()))
        .unwrap();
    let (status, body) = raw_request(stream, req).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("GraphiQL"));

    daemon.stop().await;
    assert!(!sock_path.exists(), "socket file left behind");
}

/// `serve_root` serves static files on `/` and `cors` answers preflights
/// for the configured origins (an externally served web UI needs both the
/// preflight and the mirrored origin on actual responses).
#[tokio::test(flavor = "multi_thread")]
async fn serve_root_and_cors() {
    let state = tempfile::tempdir().unwrap();
    let webroot = tempfile::tempdir().unwrap();
    std::fs::write(webroot.path().join("index.html"), "<html>ui</html>").unwrap();
    std::fs::write(webroot.path().join("app.js"), "js").unwrap();

    let mut config = test_config(
        state.path(),
        Listen::Tcp("127.0.0.1:0".parse().unwrap()),
        Some("s3cret"),
        false,
    );
    config.serve_root = Some(webroot.path().to_path_buf());
    config.cors = vec!["https://ui.example.com".into()];
    let daemon = Daemon::start(config, Some(hermetic_settings()))
        .await
        .unwrap();
    let addr = daemon.tcp_addr().expect("tcp listener");

    let get = |path: &str| {
        Request::builder()
            .uri(path)
            .header("host", "rsbtd")
            .body(Full::new(Bytes::new()))
            .unwrap()
    };

    // Static files are served without auth.
    let stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    let (status, body) = raw_request(stream, get("/")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, "<html>ui</html>");
    let stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    let (status, body) = raw_request(stream, get("/app.js")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, "js");
    let stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    let (status, _) = raw_request(stream, get("/missing.js")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // Preflight from an allowed origin passes without a token and grants
    // the method and headers the UI will use.
    let stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    let req = Request::builder()
        .method("OPTIONS")
        .uri("/graphql")
        .header("host", "rsbtd")
        .header("origin", "https://ui.example.com")
        .header("access-control-request-method", "POST")
        .header(
            "access-control-request-headers",
            "authorization,content-type",
        )
        .body(Full::new(Bytes::new()))
        .unwrap();
    let (mut sender, conn) = hyper::client::conn::http1::handshake(TokioIo::new(stream))
        .await
        .unwrap();
    tokio::spawn(conn);
    let resp = sender.send_request(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        resp.headers()["access-control-allow-origin"],
        "https://ui.example.com"
    );
    let allow_headers = resp.headers()["access-control-allow-headers"]
        .to_str()
        .unwrap()
        .to_ascii_lowercase();
    assert!(allow_headers.contains("authorization"));
    assert!(allow_headers.contains("content-type"));

    // An actual cross-origin request carries the mirrored origin; an
    // unlisted origin gets no CORS headers (the browser blocks it).
    for (origin, expect_allowed) in [
        ("https://ui.example.com", true),
        ("https://evil.example", false),
    ] {
        let stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        let mut req = graphql_request(Some("s3cret"), "{ version { daemon } }");
        req.headers_mut().insert("origin", origin.parse().unwrap());
        let (mut sender, conn) = hyper::client::conn::http1::handshake(TokioIo::new(stream))
            .await
            .unwrap();
        tokio::spawn(conn);
        let resp = sender.send_request(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(
            resp.headers()
                .get("access-control-allow-origin")
                .map(|v| v.to_str().unwrap()),
            expect_allowed.then_some(origin),
        );
    }

    daemon.stop().await;

    let state2 = tempfile::tempdir().unwrap();
    let mut config = test_config(
        state2.path(),
        Listen::Tcp("127.0.0.1:0".parse().unwrap()),
        None,
        false,
    );
    config.serve_root = Some(state2.path().join("nonexistent"));
    assert!(
        Daemon::start(config, Some(hermetic_settings()))
            .await
            .is_err(),
        "started with a missing serve_root"
    );
}

/// Exercises the mutation surface end to end on one daemon: add via
/// base64 .torrent, per-torrent ops, correlated ops (rename, readPiece,
/// moveStorage), settings, ip filter, session pause, and removal.
#[tokio::test(flavor = "multi_thread")]
async fn mutation_surface_over_tcp() {
    use base64::Engine as _;

    let state = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let moved = tempfile::tempdir().unwrap();
    let config = test_config(
        state.path(),
        Listen::Tcp("127.0.0.1:0".parse().unwrap()),
        None,
        false,
    );
    let daemon = Daemon::start(config, Some(hermetic_settings()))
        .await
        .unwrap();
    let addr = daemon.tcp_addr().unwrap();

    write_fixture_content(data.path());
    let torrent_b64 =
        base64::engine::general_purpose::STANDARD.encode(std::fs::read(fixture_path()).unwrap());
    let data_json = graphql(
        addr,
        None,
        &format!(
            r#"mutation {{ addTorrent(input: {{
                torrentData: "{torrent_b64}",
                savePath: {},
                flags: [SEED_MODE],
                maxConnections: 77
            }}) {{ uuid infoHashV1 name flags }} }}"#,
            gql_path(data.path())
        ),
    )
    .await;
    let added = &data_json["addTorrent"];
    let uuid = added["uuid"].as_str().unwrap().to_owned();
    assert!(added["infoHashV1"].is_string());
    assert!(
        added["flags"]
            .as_array()
            .unwrap()
            .contains(&json!("SEED_MODE"))
    );
    let entry = daemon
        .engine()
        .registry()
        .find(&uuid.parse().unwrap())
        .unwrap();
    assert_eq!(
        daemon
            .engine()
            .with_handle(&entry, |h| h.max_connections())
            .unwrap(),
        77
    );

    // Wait until it seeds so piece reads and renames have data on disk.
    timeout(Duration::from_secs(30), async {
        while !daemon
            .engine()
            .with_handle(&entry, |h| {
                h.status(0).map(|s| s.is_seeding()).unwrap_or(false)
            })
            .unwrap_or(false)
        {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .unwrap();

    let stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    let (_, body) = raw_request(
        stream,
        graphql_request(
            None,
            r#"mutation { addTorrent(input: {magnetUri: "magnet:?", torrentData: "AA==", savePath: "/tmp"}) { name } }"#,
        ),
    )
    .await;
    assert!(body.contains("exactly one of magnetUri or torrentData"));

    let data_json = graphql(
        addr,
        None,
        &format!(r#"mutation {{ setTorrentFlags(uuid: "{uuid}", set: [SEQUENTIAL_DOWNLOAD]) }}"#),
    )
    .await;
    assert!(
        data_json["setTorrentFlags"]
            .as_array()
            .unwrap()
            .contains(&json!("SEQUENTIAL_DOWNLOAD"))
    );
    let data_json = graphql(
        addr,
        None,
        &format!(r#"mutation {{ setTorrentFlags(uuid: "{uuid}", unset: [SEQUENTIAL_DOWNLOAD]) }}"#),
    )
    .await;
    assert!(
        !data_json["setTorrentFlags"]
            .as_array()
            .unwrap()
            .contains(&json!("SEQUENTIAL_DOWNLOAD"))
    );

    let data_json = graphql(
        addr,
        None,
        &format!(
            r#"mutation {{
                limits: setTorrentLimits(uuid: "{uuid}", uploadLimit: 250000, maxUploads: 9)
                prio: setFilePriorities(uuid: "{uuid}", priorities: [7, 1])
                one: setFilePriority(uuid: "{uuid}", index: 1, priority: 2)
                top: queueTop(uuid: "{uuid}")
                tracker: addTracker(uuid: "{uuid}", url: "http://127.0.0.1:1/announce")
                seed: addUrlSeed(uuid: "{uuid}", url: "http://127.0.0.1:1/seed")
                unseed: removeUrlSeed(uuid: "{uuid}", url: "http://127.0.0.1:1/seed")
                deadline: setPieceDeadline(uuid: "{uuid}", piece: 0, deadlineMs: 5000)
                undeadline: clearPieceDeadlines(uuid: "{uuid}")
            }}"#
        ),
    )
    .await;
    assert_eq!(data_json["limits"], true);
    let limits = daemon
        .engine()
        .with_handle(&entry, |h| (h.upload_limit(), h.max_uploads()))
        .unwrap();
    assert_eq!(limits, (250_000, 9));
    // File priorities apply asynchronously on the disk thread; the getter
    // returns the old value until then, so poll instead of asserting once.
    timeout(Duration::from_secs(30), async {
        while daemon
            .engine()
            .with_handle(&entry, |h| {
                (h.file_priority(0).value(), h.file_priority(1).value())
            })
            .unwrap()
            != (7, 2)
        {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("file priorities were not applied");

    // replaceTrackers swaps the whole list (dropping the metadata's own
    // trackers and the added one), preserving tiers.
    let data_json = graphql(
        addr,
        None,
        &format!(
            r#"mutation {{ replaceTrackers(uuid: "{uuid}",
                trackers: [{{url: "http://127.0.0.1:1/a", tier: 1}}]) }}"#
        ),
    )
    .await;
    assert_eq!(data_json["replaceTrackers"], true);
    let data_json = graphql(
        addr,
        None,
        &format!(r#"{{ torrent(uuid: "{uuid}") {{ trackers {{ url tier }} }} }}"#),
    )
    .await;
    assert_eq!(
        data_json["torrent"]["trackers"],
        json!([{ "url": "http://127.0.0.1:1/a", "tier": 1 }])
    );
    // An empty list removes every tracker.
    let data_json = graphql(
        addr,
        None,
        &format!(r#"mutation {{ replaceTrackers(uuid: "{uuid}", trackers: []) }}"#),
    )
    .await;
    assert_eq!(data_json["replaceTrackers"], true);
    let data_json = graphql(
        addr,
        None,
        &format!(r#"{{ torrent(uuid: "{uuid}") {{ trackers {{ url }} }} }}"#),
    )
    .await;
    assert_eq!(data_json["torrent"]["trackers"], json!([]));
    // Out-of-range tiers reject the whole replacement.
    let stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    let (_, body) = raw_request(
        stream,
        graphql_request(
            None,
            &format!(
                r#"mutation {{ replaceTrackers(uuid: "{uuid}",
                    trackers: [{{url: "http://127.0.0.1:1/b", tier: 300}}]) }}"#
            ),
        ),
    )
    .await;
    assert!(body.contains("0..=255"));

    // Limit validation: rates below -1 and peer limits of 0/1/-2 are
    // assertion-invalid in libtorrent; an invalid value rejects the whole
    // delta without applying any field.
    let stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    let (_, body) = raw_request(
        stream,
        graphql_request(
            None,
            &format!(
                r#"mutation {{ setTorrentLimits(uuid: "{uuid}", uploadLimit: -2, maxUploads: 5) }}"#
            ),
        ),
    )
    .await;
    assert!(body.contains("uploadLimit"));
    assert_eq!(
        daemon
            .engine()
            .with_handle(&entry, |h| h.max_uploads())
            .unwrap(),
        9,
        "no field of a rejected delta may be applied"
    );
    let stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    let (_, body) = raw_request(
        stream,
        graphql_request(
            None,
            &format!(r#"mutation {{ setTorrentLimits(uuid: "{uuid}", maxConnections: 1) }}"#),
        ),
    )
    .await;
    assert!(body.contains("maxConnections"));

    let stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    let (_, body) = raw_request(
        stream,
        graphql_request(
            None,
            &format!(r#"mutation {{ setFilePriorities(uuid: "{uuid}", priorities: [9]) }}"#),
        ),
    )
    .await;
    assert!(body.contains("outside 0..=7"));

    // Out-of-range file indexes are rejected before reaching libtorrent.
    let stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    let (_, body) = raw_request(
        stream,
        graphql_request(
            None,
            &format!(
                r#"mutation {{ setFilePriority(uuid: "{uuid}", index: 2147483646, priority: 2) }}"#
            ),
        ),
    )
    .await;
    assert!(body.contains("file index 2147483646 is outside"));
    let stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    let (_, body) = raw_request(
        stream,
        graphql_request(
            None,
            &format!(r#"mutation {{ renameFile(uuid: "{uuid}", index: -1, name: "x") }}"#),
        ),
    )
    .await;
    assert!(body.contains("file index -1 is outside"));

    let overlong = vec!["4"; 100_000].join(",");
    let stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    let (_, body) = raw_request(
        stream,
        graphql_request(
            None,
            &format!(
                r#"mutation {{ setPiecePriorities(uuid: "{uuid}", priorities: [{overlong}]) }}"#
            ),
        ),
    )
    .await;
    assert!(body.contains("longer than the number of pieces"));

    let data_json = graphql(
        addr,
        None,
        &format!(r#"mutation {{ readPiece(uuid: "{uuid}", piece: 0) }}"#),
    )
    .await;
    let piece = base64::engine::general_purpose::STANDARD
        .decode(data_json["readPiece"].as_str().unwrap())
        .unwrap();
    let a_bin = std::fs::read(data.path().join("fixture/a.bin")).unwrap();
    assert_eq!(&piece[..64], &a_bin[..64], "piece 0 must start like a.bin");

    let data_json = graphql(
        addr,
        None,
        &format!(
            r#"mutation {{ renameFile(uuid: "{uuid}", index: 1, name: "fixture/renamed.txt") }}"#
        ),
    )
    .await;
    assert_eq!(data_json["renameFile"], "fixture/renamed.txt");

    let data_json = graphql(
        addr,
        None,
        &format!(
            r#"mutation {{ moveStorage(uuid: "{uuid}", path: {}) }}"#,
            gql_path(moved.path())
        ),
    )
    .await;
    assert_eq!(data_json["moveStorage"], moved.path().to_str().unwrap());
    assert!(moved.path().join("fixture/a.bin").exists());
    assert!(moved.path().join("fixture/renamed.txt").exists());

    // pause/resume: pausing detaches from auto-management so it sticks.
    graphql(
        addr,
        None,
        &format!(r#"mutation {{ pauseTorrent(uuid: "{uuid}") }}"#),
    )
    .await;
    timeout(Duration::from_secs(30), async {
        while !daemon
            .engine()
            .with_handle(&entry, |h| h.is_paused())
            .unwrap()
        {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("torrent did not pause");
    graphql(
        addr,
        None,
        &format!(r#"mutation {{ resumeTorrent(uuid: "{uuid}") }}"#),
    )
    .await;
    timeout(Duration::from_secs(30), async {
        while daemon
            .engine()
            .with_handle(&entry, |h| h.is_paused())
            .unwrap()
        {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("torrent did not resume");

    let data_json = graphql(
        addr,
        None,
        &format!(
            r#"mutation {{
                recheck: forceRecheck(uuid: "{uuid}")
                reannounce: forceReannounce(uuid: "{uuid}")
                dht: forceDhtAnnounce(uuid: "{uuid}")
                clear: clearError(uuid: "{uuid}")
                flush: flushCache(uuid: "{uuid}")
                save: saveResumeData(uuid: "{uuid}")
                reopen: reopenNetworkSockets(mapPorts: false)
            }}"#
        ),
    )
    .await;
    assert_eq!(data_json["save"], true);

    // applySettings validates and persists; the response is the full new
    // effective settings, so readback comes with the mutation.
    let data_json = graphql(
        addr,
        None,
        r#"mutation { applySettings(input: {uploadRateLimit: 123000, enableLsd: false}) {
            uploadRateLimit enableLsd
        } }"#,
    )
    .await;
    assert_eq!(data_json["applySettings"]["uploadRateLimit"], 123000);
    assert_eq!(data_json["applySettings"]["enableLsd"], false);
    let stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    let (_, body) = raw_request(
        stream,
        graphql_request(
            None,
            r#"mutation { applySettings(input: {enableDht: "yes"}) { enableDht } }"#,
        ),
    )
    .await;
    assert!(body.contains("enableDht"), "{body}");

    // The structured settings: proxy is null by default, applying an
    // object writes the whole group atomically and reads back typed.
    let data_json = graphql(
        addr,
        None,
        r#"mutation { applySettings(input: {
            proxy: {
                protocol: SOCKS5, hostname: "127.0.0.1", port: 1080,
                resolveHostnames: true, peerConnections: true,
                trackerConnections: true
            },
            userAgent: RSBTD
        }) { proxy { protocol hostname password } userAgent } }"#,
    )
    .await;
    let applied = &data_json["applySettings"];
    assert_eq!(applied["proxy"]["protocol"], "SOCKS5");
    assert_eq!(applied["proxy"]["hostname"], "127.0.0.1");
    assert_eq!(applied["proxy"]["password"], "");
    assert_eq!(applied["userAgent"], "RSBTD");

    // Disable it again: null clears the backing fields.
    let data_json = graphql(
        addr,
        None,
        r#"mutation { applySettings(input: {proxy: null}) { proxy { hostname } } }"#,
    )
    .await;
    assert_eq!(data_json["applySettings"]["proxy"], Value::Null);

    // Blacklisted and hidden settings are absent from the schema (a
    // validation error names the offending field); invalid values are
    // rejected by validation.
    for (query, expect) in [
        (r#"{ settings { alertQueueSize } }"#, "alertQueueSize"),
        (r#"{ settings { proxyHostname } }"#, "proxyHostname"),
        (
            r#"mutation { applySettings(input: {disableHashChecks: true}) { userAgent } }"#,
            "disableHashChecks",
        ),
        (
            // Raw enum ints are not accepted for enum settings.
            r#"mutation { applySettings(input: {chokingAlgorithm: 1}) { userAgent } }"#,
            "chokingAlgorithm",
        ),
        (
            // Blacklisted settings are absent from the input type too.
            r#"mutation { applySettings(input: {alertMask: 0}) { userAgent } }"#,
            "alertMask",
        ),
        (
            // Explicit null is only meaningful for the nullable groups.
            r#"mutation { applySettings(input: {uploadRateLimit: null}) { userAgent } }"#,
            "omit the field",
        ),
    ] {
        let stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        let (_, body) = raw_request(stream, graphql_request(None, query)).await;
        assert!(body.contains(expect), "{query}: {body}");
    }

    graphql(
        addr,
        None,
        r#"mutation { setIpFilter(rules: [{first: "10.0.0.0", last: "10.255.255.255", blocked: true}]) }"#,
    )
    .await;
    let data_json = graphql(addr, None, "{ ipFilter { first last blocked } }").await;
    assert!(data_json["ipFilter"].as_array().unwrap().iter().any(|r| {
        r["first"] == "10.0.0.0" && r["last"] == "10.255.255.255" && r["blocked"] == true
    }));

    graphql(addr, None, "mutation { pauseSession }").await;
    let data_json = graphql(addr, None, "{ session { isPaused } }").await;
    assert_eq!(data_json["session"]["isPaused"], true);
    graphql(addr, None, "mutation { resumeSession }").await;
    let data_json = graphql(addr, None, "{ session { isPaused } }").await;
    assert_eq!(data_json["session"]["isPaused"], false);

    let stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    let (_, body) = raw_request(
        stream,
        graphql_request(
            None,
            r#"mutation { resumeTorrent(uuid: "00000000-0000-0000-0000-000000000000") }"#,
        ),
    )
    .await;
    assert!(body.contains("torrent not found"));

    // removeTorrent(deleteFiles) waits for the deletion outcome: the
    // payload is already gone when the mutation returns.
    let data_json = graphql(
        addr,
        None,
        &format!(r#"mutation {{ removeTorrent(uuid: "{uuid}", deleteFiles: true) }}"#),
    )
    .await;
    assert_eq!(data_json["removeTorrent"], true);
    assert!(
        !moved.path().join("fixture/a.bin").exists(),
        "the mutation acknowledged before the files were deleted"
    );
    let data_json = graphql(addr, None, "{ session { torrentCount } }").await;
    assert_eq!(data_json["session"]["torrentCount"], 0);

    daemon.stop().await;
}

/// graphql-ws subscriptions: connection_init auth (payload and header),
/// sessionStats ticks, torrentChanged snapshots, and the events firehose.
#[tokio::test(flavor = "multi_thread")]
async fn websocket_subscriptions() {
    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::tungstenite::Message;
    use tokio_tungstenite::tungstenite::client::IntoClientRequest;

    type Ws = tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >;

    async fn ws_connect(addr: SocketAddr, auth_header: Option<&str>) -> Ws {
        let mut req = format!("ws://{addr}/graphql")
            .into_client_request()
            .unwrap();
        req.headers_mut().insert(
            "sec-websocket-protocol",
            "graphql-transport-ws".parse().unwrap(),
        );
        if let Some(token) = auth_header {
            req.headers_mut()
                .insert("authorization", format!("Bearer {token}").parse().unwrap());
        }
        let (ws, _) = tokio_tungstenite::connect_async(req).await.unwrap();
        ws
    }

    async fn send(ws: &mut Ws, value: Value) {
        ws.send(Message::Text(value.to_string())).await.unwrap();
    }

    /// Next graphql-transport-ws JSON frame (None once the socket closes).
    async fn next_frame(ws: &mut Ws) -> Option<Value> {
        loop {
            match timeout(Duration::from_secs(30), ws.next())
                .await
                .expect("timed out waiting for a ws frame")?
            {
                Ok(Message::Text(text)) => return serde_json::from_str(&text).ok(),
                Ok(Message::Close(_)) | Err(_) => return None,
                Ok(_) => {}
            }
        }
    }

    /// Reads frames until a `next` for `id` arrives; returns its data.
    async fn next_data(ws: &mut Ws, id: &str) -> Value {
        loop {
            let frame = next_frame(ws).await.expect("socket closed while waiting");
            if frame["type"] == "next" && frame["id"] == id {
                return frame["payload"]["data"].clone();
            }
            assert_ne!(frame["type"], "error", "subscription failed: {frame}");
        }
    }

    let state = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let config = test_config(
        state.path(),
        Listen::Tcp("127.0.0.1:0".parse().unwrap()),
        Some("s3cret"),
        false,
    );
    let daemon = Daemon::start(config, Some(hermetic_settings()))
        .await
        .unwrap();
    let addr = daemon.tcp_addr().unwrap();

    // A bad connection_init token is rejected (no ack; the socket dies).
    let mut ws = ws_connect(addr, None).await;
    send(
        &mut ws,
        json!({"type": "connection_init", "payload": {"token": "wrong"}}),
    )
    .await;
    let frame = next_frame(&mut ws).await;
    assert!(
        frame.is_none() || frame.as_ref().unwrap()["type"] != "connection_ack",
        "bad token was acked: {frame:?}"
    );

    // A valid Authorization header authenticates without a payload token.
    let mut ws = ws_connect(addr, Some("s3cret")).await;
    send(&mut ws, json!({"type": "connection_init"})).await;
    assert_eq!(next_frame(&mut ws).await.unwrap()["type"], "connection_ack");
    drop(ws);

    // Payload-token connection carrying all the subscriptions.
    let mut ws = ws_connect(addr, None).await;
    send(
        &mut ws,
        json!({"type": "connection_init", "payload": {"token": "s3cret"}}),
    )
    .await;
    assert_eq!(next_frame(&mut ws).await.unwrap()["type"], "connection_ack");

    send(
        &mut ws,
        json!({"type": "subscribe", "id": "bad", "payload": {"query":
            "subscription { torrentChanged(uuid: \"00000000-0000-0000-0000-000000000000\") { name } }"
        }}),
    )
    .await;
    let frame = next_frame(&mut ws).await.unwrap();
    assert_eq!(frame["id"], "bad");
    let failed = frame["type"] == "error"
        || frame["payload"]["errors"][0]["message"]
            .as_str()
            .is_some_and(|m| m.contains("not found"));
    assert!(failed, "expected a failure frame, got: {frame}");

    send(
        &mut ws,
        json!({"type": "subscribe", "id": "stats", "payload": {"query":
            "subscription { sessionStats(intervalMs: 200, names: [\"net.recv_payload_bytes\"]) { name kind value } }"
        }}),
    )
    .await;
    let stats = next_data(&mut ws, "stats").await;
    assert_eq!(stats["sessionStats"][0]["name"], "net.recv_payload_bytes");

    send(
        &mut ws,
        json!({"type": "subscribe", "id": "events", "payload": {"query":
            "subscription { torrentEvents { __typename ... on TorrentAddedEvent { torrentUuid } } }"
        }}),
    )
    .await;
    send(
        &mut ws,
        json!({"type": "subscribe", "id": "changed", "payload": {"query":
            "subscription { torrentChanged { name infoHashV1 state isSeeding } }"
        }}),
    )
    .await;
    // Give the subscriptions a beat to attach to the buses.
    tokio::time::sleep(Duration::from_millis(200)).await;

    let entry = add_seeding_fixture(&daemon, data.path()).await;
    let v1_hex = fixture_v1_hex();

    let event = next_data(&mut ws, "events").await;
    assert_eq!(event["torrentEvents"]["__typename"], "TorrentAddedEvent");
    assert_eq!(
        event["torrentEvents"]["torrentUuid"],
        entry.uuid.to_string()
    );

    let changed = next_data(&mut ws, "changed").await;
    let snapshot = &changed["torrentChanged"][0];
    assert_eq!(snapshot["infoHashV1"], v1_hex);
    assert!(snapshot["state"].is_string());

    // Completing a subscription stops its frames; the rest keep working.
    send(&mut ws, json!({"type": "complete", "id": "events"})).await;
    let stats = next_data(&mut ws, "stats").await;
    assert!(stats["sessionStats"].is_array());

    drop(ws);
    daemon.stop().await;
}

/// An oversized pre-auth WebSocket message kills the connection: an
/// unauthenticated peer cannot make the daemon buffer and parse a
/// near-transport-default (64 MiB) `connection_init`.
#[tokio::test(flavor = "multi_thread")]
async fn websocket_oversized_message_rejected() {
    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::tungstenite::Message;
    use tokio_tungstenite::tungstenite::client::IntoClientRequest;

    let state = tempfile::tempdir().unwrap();
    let config = test_config(
        state.path(),
        Listen::Tcp("127.0.0.1:0".parse().unwrap()),
        Some("s3cret"),
        false,
    );
    let daemon = Daemon::start(config, Some(hermetic_settings()))
        .await
        .unwrap();
    let addr = daemon.tcp_addr().unwrap();

    let mut req = format!("ws://{addr}/graphql")
        .into_client_request()
        .unwrap();
    req.headers_mut().insert(
        "sec-websocket-protocol",
        "graphql-transport-ws".parse().unwrap(),
    );
    let (mut ws, _) = tokio_tungstenite::connect_async(req).await.unwrap();
    // A valid token padded to 1 MiB: far over the inbound cap, far under
    // the transport default. Only the size cap can reject this — if the
    // message were buffered and parsed, the init would be acked.
    let padding = "x".repeat(1024 * 1024);
    let init = format!(
        "{{\"type\":\"connection_init\",\"payload\":{{\"token\":\"s3cret\",\"pad\":\"{padding}\"}}}}"
    );
    // The server may drop the connection mid-send (a send error is the
    // rejection); if the send lands, no ack may come back.
    if ws.send(Message::Text(init)).await.is_ok() {
        let acked = loop {
            match timeout(Duration::from_secs(30), ws.next())
                .await
                .expect("timed out waiting for the connection to close")
            {
                None | Some(Ok(Message::Close(_))) | Some(Err(_)) => break false,
                Some(Ok(Message::Text(text))) => {
                    break serde_json::from_str::<Value>(&text)
                        .is_ok_and(|frame| frame["type"] == "connection_ack");
                }
                Some(Ok(_)) => {}
            }
        };
        assert!(!acked, "oversized connection_init was accepted");
    }
    daemon.stop().await;
}

/// Shutdown closes open graphql-ws sockets with 1001 (going away)
/// instead of leaving clients frozen on a dead daemon.
#[tokio::test(flavor = "multi_thread")]
async fn websocket_closed_on_shutdown() {
    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::tungstenite::Message;
    use tokio_tungstenite::tungstenite::client::IntoClientRequest;

    let state = tempfile::tempdir().unwrap();
    let config = test_config(
        state.path(),
        Listen::Tcp("127.0.0.1:0".parse().unwrap()),
        None,
        false,
    );
    let daemon = Daemon::start(config, Some(hermetic_settings()))
        .await
        .unwrap();
    let addr = daemon.tcp_addr().unwrap();

    let mut req = format!("ws://{addr}/graphql")
        .into_client_request()
        .unwrap();
    req.headers_mut().insert(
        "sec-websocket-protocol",
        "graphql-transport-ws".parse().unwrap(),
    );
    let (mut ws, _) = tokio_tungstenite::connect_async(req).await.unwrap();
    ws.send(Message::Text(
        json!({"type": "connection_init"}).to_string(),
    ))
    .await
    .unwrap();
    let ack = timeout(Duration::from_secs(30), ws.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let ack: Value = serde_json::from_str(ack.to_text().unwrap()).unwrap();
    assert_eq!(ack["type"], "connection_ack");
    ws.send(Message::Text(
        json!({"type": "subscribe", "id": "stats", "payload": {"query":
            "subscription { sessionStats(intervalMs: 200) { name } }"
        }})
        .to_string(),
    ))
    .await
    .unwrap();

    let stop = tokio::spawn(daemon.stop());
    let close = loop {
        match timeout(Duration::from_secs(30), ws.next())
            .await
            .expect("timed out waiting for the close frame")
        {
            Some(Ok(Message::Close(frame))) => break frame,
            Some(Ok(_)) => {}
            other => panic!("socket ended without a close frame: {other:?}"),
        }
    };
    let close = close.expect("the close frame carries a code");
    assert_eq!(u16::from(close.code), 1001);
    stop.await.unwrap();
}

/// Torrent-creation jobs: create from a directory tree, watch progress
/// over graphql-ws, add the result back, and exercise error/cancel paths.
#[tokio::test(flavor = "multi_thread")]
async fn create_torrent_jobs() {
    let state = tempfile::tempdir().unwrap();
    let source = tempfile::tempdir().unwrap();
    let out = tempfile::tempdir().unwrap();
    let download = tempfile::tempdir().unwrap();

    let content_dir = source.path().join("mytorrent");
    std::fs::create_dir_all(content_dir.join("sub")).unwrap();
    std::fs::write(content_dir.join("one.bin"), vec![0xAB_u8; 100_000]).unwrap();
    std::fs::write(content_dir.join("sub/two.txt"), b"hello torrent").unwrap();

    let config = test_config(
        state.path(),
        Listen::Tcp("127.0.0.1:0".parse().unwrap()),
        None,
        false,
    );
    let daemon = Daemon::start(config, Some(hermetic_settings()))
        .await
        .unwrap();
    let addr = daemon.tcp_addr().unwrap();

    // Inline-result variant (no outputPath).
    let data_json = graphql(
        addr,
        None,
        &format!(
            r#"mutation {{ startCreateTorrent(input: {{
                sourcePath: {},
                trackers: [{{url: "http://127.0.0.1:1/announce"}}],
                comment: "made by rsbtd tests",
                private: true
            }}) {{ id state }} }}"#,
            gql_path(&content_dir)
        ),
    )
    .await;
    let job_id = data_json["startCreateTorrent"]["id"].as_u64().unwrap();

    let torrent_b64 = timeout(Duration::from_secs(30), async {
        loop {
            let data_json = graphql(
                addr,
                None,
                &format!(
                    "{{ createJob(id: {job_id}) {{ state piecesDone piecesTotal error torrentData }} }}"
                ),
            )
            .await;
            let job = &data_json["createJob"];
            match job["state"].as_str().unwrap() {
                "FINISHED" => {
                    assert_eq!(job["piecesDone"], job["piecesTotal"]);
                    return job["torrentData"].as_str().unwrap().to_owned();
                }
                "FAILED" => panic!("creation failed: {}", job["error"]),
                _ => tokio::time::sleep(Duration::from_millis(50)).await,
            }
        }
    })
    .await
    .expect("creation job did not finish");

    // The generated torrent adds back in seed mode over the source parent.
    let data_json = graphql(
        addr,
        None,
        &format!(
            r#"mutation {{ addTorrent(input: {{
                torrentData: "{torrent_b64}",
                savePath: {},
                flags: [SEED_MODE]
            }}) {{ name totalSize files {{ path }} }} }}"#,
            gql_path(source.path())
        ),
    )
    .await;
    let added = &data_json["addTorrent"];
    assert_eq!(added["name"], "mytorrent");
    assert_eq!(added["totalSize"], 100_000 + 13);
    let data_json = graphql(addr, None, "{ createJobs { id state } }").await;
    assert_eq!(data_json["createJobs"][0]["id"], job_id);
    assert_eq!(data_json["createJobs"][0]["state"], "FINISHED");

    // Output-path variant writes the file instead of inlining it.
    let out_file = out.path().join("made.torrent");
    let data_json = graphql(
        addr,
        None,
        &format!(
            r#"mutation {{ startCreateTorrent(input: {{
                sourcePath: {},
                outputPath: {}
            }}) {{ id }} }}"#,
            gql_path(&content_dir),
            gql_path(&out_file)
        ),
    )
    .await;
    let job2 = data_json["startCreateTorrent"]["id"].as_u64().unwrap();
    timeout(Duration::from_secs(30), async {
        loop {
            let data_json = graphql(
                addr,
                None,
                &format!("{{ createJob(id: {job2}) {{ state torrentData outputPath error }} }}"),
            )
            .await;
            let job = &data_json["createJob"];
            match job["state"].as_str().unwrap() {
                "FINISHED" => {
                    assert_eq!(job["torrentData"], Value::Null);
                    assert_eq!(job["outputPath"], out_file.to_str().unwrap());
                    return;
                }
                "FAILED" => panic!("creation failed: {}", job["error"]),
                _ => tokio::time::sleep(Duration::from_millis(50)).await,
            }
        }
    })
    .await
    .unwrap();
    let written = std::fs::read(&out_file).unwrap();
    rbtorrent::AddTorrentParams::from_torrent_buffer(&written).unwrap();
    let _ = download;

    // Bad source path fails fast.
    let stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    let (_, body) = raw_request(
        stream,
        graphql_request(
            None,
            r#"mutation { startCreateTorrent(input: {sourcePath: "/does/not/exist"}) { id } }"#,
        ),
    )
    .await;
    assert!(body.contains("does not exist"));

    // Cancelling a finished job is a no-op; unknown jobs return false.
    let data_json = graphql(
        addr,
        None,
        &format!("mutation {{ a: cancelCreateJob(id: {job_id}) b: cancelCreateJob(id: 999999) }}"),
    )
    .await;
    assert_eq!(data_json["a"], false);
    assert_eq!(data_json["b"], false);

    // createJobProgress streams snapshots until the terminal state.
    {
        use futures_util::{SinkExt, StreamExt};
        use tokio_tungstenite::tungstenite::Message;
        use tokio_tungstenite::tungstenite::client::IntoClientRequest;

        let mut req = format!("ws://{addr}/graphql")
            .into_client_request()
            .unwrap();
        req.headers_mut().insert(
            "sec-websocket-protocol",
            "graphql-transport-ws".parse().unwrap(),
        );
        let (mut ws, _) = tokio_tungstenite::connect_async(req).await.unwrap();
        ws.send(Message::Text(
            json!({"type": "connection_init"}).to_string(),
        ))
        .await
        .unwrap();
        // ack
        ws.next().await.unwrap().unwrap();

        let data_json = graphql(
            addr,
            None,
            &format!(
                r#"mutation {{ startCreateTorrent(input: {{ sourcePath: {} }}) {{ id }} }}"#,
                gql_path(&content_dir)
            ),
        )
        .await;
        let job3 = data_json["startCreateTorrent"]["id"].as_u64().unwrap();
        ws.send(Message::Text(
            json!({"type": "subscribe", "id": "job", "payload": {"query":
                format!("subscription {{ createJobProgress(id: {job3}) {{ state piecesDone piecesTotal }} }}")
            }})
            .to_string(),
        ))
        .await
        .unwrap();

        let mut saw_terminal = false;
        while let Some(Ok(Message::Text(text))) =
            timeout(Duration::from_secs(30), ws.next()).await.unwrap()
        {
            let frame: Value = serde_json::from_str(&text).unwrap();
            match frame["type"].as_str().unwrap() {
                "next" => {
                    let job = &frame["payload"]["data"]["createJobProgress"];
                    if job["state"] == "FINISHED" {
                        saw_terminal = true;
                    }
                }
                "complete" => break,
                other => panic!("unexpected frame type {other}: {frame}"),
            }
        }
        assert!(saw_terminal, "never saw the FINISHED snapshot");
    }

    daemon.stop().await;
}
