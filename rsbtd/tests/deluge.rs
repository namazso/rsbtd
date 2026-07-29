// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Deluge-compatible JSON API integration tests: envelope and session
//! behavior over TCP, the stub method surface, the `core.*` and `web.*`
//! torrent lifecycles, and open (tokenless) mode.

use std::collections::BTreeSet;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::Duration;

use http_body_util::{BodyExt, Full};
use hyper::body::Bytes;
use hyper::{Request, StatusCode};
use hyper_util::rt::TokioIo;
use rbtorrent::SettingsPack;
use rsbtd::Daemon;
use rsbtd::api::deluge::HOST_ID;
use rsbtd::config::{Config, Listen};
use serde_json::{Value, json};

/// Every registered `core.*` method: the catalog minus the four
/// ADMIN-level account methods.
const CORE_METHODS: [&str; 66] = [
    "core.add_torrent_file",
    "core.add_torrent_file_async",
    "core.add_torrent_files",
    "core.add_torrent_magnet",
    "core.add_torrent_url",
    "core.connect_peer",
    "core.create_torrent",
    "core.disable_plugin",
    "core.enable_plugin",
    "core.force_reannounce",
    "core.force_recheck",
    "core.get_auth_levels_mappings",
    "core.get_available_plugins",
    "core.get_completion_paths",
    "core.get_config",
    "core.get_config_value",
    "core.get_config_values",
    "core.get_enabled_plugins",
    "core.get_external_ip",
    "core.get_filter_tree",
    "core.get_free_space",
    "core.get_libtorrent_version",
    "core.get_listen_port",
    "core.get_magnet_uri",
    "core.get_path_size",
    "core.get_proxy",
    "core.get_session_state",
    "core.get_session_status",
    "core.get_torrent_status",
    "core.get_torrents_status",
    "core.glob",
    "core.is_session_paused",
    "core.move_storage",
    "core.pause_session",
    "core.pause_torrent",
    "core.pause_torrents",
    "core.prefetch_magnet_metadata",
    "core.queue_bottom",
    "core.queue_down",
    "core.queue_top",
    "core.queue_up",
    "core.remove_torrent",
    "core.remove_torrents",
    "core.rename_files",
    "core.rename_folder",
    "core.rescan_plugins",
    "core.resume_session",
    "core.resume_torrent",
    "core.resume_torrents",
    "core.set_config",
    "core.set_torrent_auto_managed",
    "core.set_torrent_file_priorities",
    "core.set_torrent_max_connections",
    "core.set_torrent_max_download_speed",
    "core.set_torrent_max_upload_slots",
    "core.set_torrent_max_upload_speed",
    "core.set_torrent_move_completed",
    "core.set_torrent_move_completed_path",
    "core.set_torrent_options",
    "core.set_torrent_prioritize_first_last",
    "core.set_torrent_remove_at_ratio",
    "core.set_torrent_stop_at_ratio",
    "core.set_torrent_stop_ratio",
    "core.set_torrent_trackers",
    "core.test_listen_port",
    "core.upload_plugin",
];

/// The session cookie `auth.login` hands out for the `s3cret` token:
/// the token, base64url-encoded.
const SESSION: Option<&str> = Some("czNjcmV0");

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

fn test_config(state_dir: &Path, token: Option<&str>) -> Config {
    Config {
        state_dir: state_dir.to_path_buf(),
        listen: Listen::Tcp("127.0.0.1:0".parse().unwrap()),
        token: token.map(str::to_owned),
        graphiql: false,
        serve_root: None,
        cors: Vec::new(),
        shutdown_grace_secs: 15,
    }
}

async fn raw_request(
    addr: SocketAddr,
    method: &str,
    cookie: Option<&str>,
    body: &str,
) -> (StatusCode, Option<String>, String) {
    raw_request_typed(addr, method, cookie, Some("application/json"), None, body).await
}

/// [`raw_request`] with a custom (or no) `Content-Type` header, and an
/// optional `Origin`.
async fn raw_request_typed(
    addr: SocketAddr,
    method: &str,
    cookie: Option<&str>,
    content_type: Option<&str>,
    origin: Option<&str>,
    body: &str,
) -> (StatusCode, Option<String>, String) {
    let stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    let (mut sender, conn) = hyper::client::conn::http1::handshake(TokioIo::new(stream))
        .await
        .unwrap();
    tokio::spawn(conn);
    let mut builder = Request::builder()
        .method(method)
        .uri("/json")
        .header("host", "rsbtd");
    if let Some(content_type) = content_type {
        builder = builder.header("content-type", content_type);
    }
    if let Some(cookie) = cookie {
        builder = builder.header("cookie", format!("_session_id={cookie}"));
    }
    if let Some(origin) = origin {
        builder = builder.header("origin", origin);
    }
    let req = builder
        .body(Full::new(Bytes::from(body.to_owned())))
        .unwrap();
    let resp = sender.send_request(req).await.unwrap();
    let status = resp.status();
    let set_cookie = resp
        .headers()
        .get("set-cookie")
        .map(|v| v.to_str().unwrap().to_owned());
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        set_cookie,
        String::from_utf8_lossy(&body).into_owned(),
    )
}

async fn call(addr: SocketAddr, cookie: Option<&str>, method: &str, params: Value) -> Value {
    let body = json!({"method": method, "params": params, "id": 1}).to_string();
    let (status, _, body) = raw_request(addr, "POST", cookie, &body).await;
    assert_eq!(status, StatusCode::OK, "{method}: {body}");
    serde_json::from_str(&body).unwrap()
}

async fn call_ok(addr: SocketAddr, cookie: Option<&str>, method: &str, params: Value) -> Value {
    let mut envelope = call(addr, cookie, method, params).await;
    assert_eq!(envelope["error"], Value::Null, "{method}: {envelope}");
    assert_eq!(envelope["id"], json!(1), "{method}: {envelope}");
    envelope["result"].take()
}

async fn call_err(addr: SocketAddr, cookie: Option<&str>, method: &str, params: Value) -> i64 {
    let envelope = call(addr, cookie, method, params).await;
    assert_eq!(envelope["result"], Value::Null, "{method}: {envelope}");
    assert_eq!(envelope["id"], json!(1), "{method}: {envelope}");
    envelope["error"]["code"].as_i64().expect("error code")
}

#[tokio::test(flavor = "multi_thread")]
async fn protocol_and_sessions() {
    let state = tempfile::tempdir().unwrap();
    let daemon = Daemon::start(
        test_config(state.path(), Some("s3cret")),
        Some(hermetic_settings()),
    )
    .await
    .unwrap();
    let addr = daemon.tcp_addr().unwrap();

    let (status, _, _) = raw_request(addr, "GET", None, "").await;
    assert_eq!(status, StatusCode::METHOD_NOT_ALLOWED);

    // The CSRF barrier: a non-JSON (or missing) content type is
    // rejected before the body is looked at.
    let ping = json!({"method": "auth.check_session", "params": [], "id": 1}).to_string();
    for content_type in [None, Some("text/plain"), Some("multipart/form-data")] {
        let (status, _, body) =
            raw_request_typed(addr, "POST", None, content_type, None, &ping).await;
        assert_eq!(status, StatusCode::OK);
        let envelope: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(envelope["error"]["code"], json!(5), "{content_type:?}");
        assert_eq!(envelope["id"], Value::Null);
    }
    // Media-type parameters are tolerated.
    let (_, _, body) = raw_request_typed(
        addr,
        "POST",
        None,
        Some("application/json; charset=utf-8"),
        None,
        &ping,
    )
    .await;
    let envelope: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(envelope["error"], Value::Null);

    // A body that is not a valid envelope is code 5 with a null id.
    for bad in [
        "not json",
        r#"{"params":[],"id":1}"#,
        r#"{"method":"web.connected","params":{},"id":1}"#,
        r#"{"method":"web.connected","params":[]}"#,
    ] {
        let (status, _, body) = raw_request(addr, "POST", None, bad).await;
        assert_eq!(status, StatusCode::OK);
        let envelope: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(envelope["error"]["code"], json!(5), "{bad}: {envelope}");
        assert_eq!(envelope["id"], Value::Null);
        assert_eq!(envelope["result"], Value::Null);
    }

    // A body past the public cap is refused before it is parsed; a
    // session lifts the cap.
    let big = json!({
        "method": "auth.check_session",
        "params": ["x".repeat(128 * 1024)],
        "id": 1,
    })
    .to_string();
    let (_, _, body) = raw_request(addr, "POST", None, &big).await;
    let envelope: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(envelope["error"]["code"], json!(5), "{envelope}");
    let (_, _, body) = raw_request(addr, "POST", SESSION, &big).await;
    let envelope: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(envelope["result"], json!(true), "{envelope}");

    // Unknown methods are code 2 even without a session, the ADMIN-only
    // account methods included.
    assert_eq!(
        call_err(addr, None, "core.create_account", json!([])).await,
        2
    );
    assert_eq!(call_err(addr, None, "system.nope", json!([])).await, 2);
    // Known session-only methods are code 1.
    assert_eq!(call_err(addr, None, "web.connected", json!([])).await, 1);
    assert_eq!(call_err(addr, None, "core.get_config", json!([])).await, 1);
    let code = call_err(addr, Some("wrong"), "web.connected", json!([])).await;
    assert_eq!(code, 1);

    // listMethods and check_session are public.
    let methods = call_ok(addr, None, "system.listMethods", json!([])).await;
    assert!(
        methods
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m == "auth.login")
    );
    let session = call_ok(addr, None, "auth.check_session", json!([])).await;
    assert_eq!(session, json!(false));

    // A wrong password is a plain false — no error, no cookie.
    let body = json!({"method": "auth.login", "params": ["wrong"], "id": 2}).to_string();
    let (_, set_cookie, body) = raw_request(addr, "POST", None, &body).await;
    let envelope: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(envelope["result"], json!(false));
    assert_eq!(envelope["error"], Value::Null);
    assert!(set_cookie.is_none(), "{set_cookie:?}");

    // The right password sets the base64url token as the session
    // cookie.
    let body = json!({"method": "auth.login", "params": ["s3cret"], "id": 3}).to_string();
    let (_, set_cookie, body) = raw_request(addr, "POST", None, &body).await;
    let envelope: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(envelope["result"], json!(true));
    assert_eq!(envelope["id"], json!(3));
    assert_eq!(
        set_cookie.as_deref(),
        Some("_session_id=czNjcmV0; Path=/; HttpOnly")
    );

    // Only a cross-site login gets the attributes a browser needs to
    // keep the cookie; the same origin, or one that could not store a
    // Secure cookie anyway, keeps the plain form.
    let login = json!({"method": "auth.login", "params": ["s3cret"], "id": 4}).to_string();
    for (origin, want) in [
        (
            "https://ui.example.com",
            "_session_id=czNjcmV0; Path=/; HttpOnly; SameSite=None; Secure",
        ),
        ("https://rsbtd", "_session_id=czNjcmV0; Path=/; HttpOnly"),
        (
            "http://ui.example.com",
            "_session_id=czNjcmV0; Path=/; HttpOnly",
        ),
    ] {
        let (_, set_cookie, _) = raw_request_typed(
            addr,
            "POST",
            None,
            Some("application/json"),
            Some(origin),
            &login,
        )
        .await;
        assert_eq!(set_cookie.as_deref(), Some(want), "{origin}");
    }

    let session = call_ok(addr, SESSION, "auth.check_session", json!([])).await;
    assert_eq!(session, json!(true));
    // The optional session_id parameter is ignored; the cookie counts.
    let session = call_ok(addr, SESSION, "auth.check_session", json!(["bogus"])).await;
    assert_eq!(session, json!(true));
    // The raw token is not a session cookie.
    assert_eq!(
        call_err(addr, Some("s3cret"), "web.connected", json!([])).await,
        1
    );

    // Arity errors are code 3.
    assert_eq!(call_err(addr, None, "auth.login", json!([])).await, 3);
    let code = call_err(addr, SESSION, "web.connected", json!([1, 2])).await;
    assert_eq!(code, 3);

    // Any id value is echoed verbatim.
    let body = json!({"method": "web.connected", "params": [], "id": {"a": [1, "x"]}}).to_string();
    let (_, _, body) = raw_request(addr, "POST", SESSION, &body).await;
    let envelope: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(envelope["id"], json!({"a": [1, "x"]}));
    assert_eq!(envelope["result"], json!(true));

    daemon.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn stub_method_surface() {
    let state = tempfile::tempdir().unwrap();
    let daemon = Daemon::start(
        test_config(state.path(), Some("s3cret")),
        Some(hermetic_settings()),
    )
    .await
    .unwrap();
    let addr = daemon.tcp_addr().unwrap();
    let c = SESSION;

    let methods = call_ok(addr, c, "system.listMethods", json!([])).await;
    let names: BTreeSet<&str> = methods
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m.as_str().unwrap())
        .collect();
    let expected: BTreeSet<&str> = [
        "system.listMethods",
        "auth.login",
        "auth.check_session",
        "auth.delete_session",
        "auth.change_password",
        "daemon.authorized_call",
        "daemon.get_method_list",
        "daemon.get_version",
        "daemon.shutdown",
        "web.connected",
        "web.connect",
        "web.disconnect",
        "web.get_hosts",
        "web.get_host_status",
        "web.add_host",
        "web.edit_host",
        "web.remove_host",
        "web.start_daemon",
        "web.stop_daemon",
        "web.update_ui",
        "web.get_torrent_status",
        "web.get_torrent_files",
        "web.get_torrent_info",
        "web.get_magnet_info",
        "web.add_torrents",
        "web.download_torrent_from_url",
        "web.get_config",
        "web.set_config",
        "web.get_plugins",
        "web.get_plugin_info",
        "web.get_plugin_resources",
        "web.upload_plugin",
        "web.register_event_listener",
        "web.deregister_event_listener",
        "web.get_events",
        "web.set_theme",
        "webutils.get_languages",
        "webutils.get_themes",
    ]
    .into_iter()
    .chain(CORE_METHODS)
    .collect();
    assert_eq!(names, expected);

    // Daemon scope is daemon.* plus core.*, which "connecting" to the
    // fake host resolves with; any other host resolves with null.
    let list = call_ok(addr, c, "daemon.get_method_list", json!([])).await;
    let daemon_names: BTreeSet<&str> = list
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m.as_str().unwrap())
        .collect();
    let expected: BTreeSet<&str> = [
        "daemon.authorized_call",
        "daemon.get_method_list",
        "daemon.get_version",
        "daemon.shutdown",
    ]
    .into_iter()
    .chain(CORE_METHODS)
    .collect();
    assert_eq!(daemon_names, expected);
    let connect = call_ok(addr, c, "web.connect", json!([HOST_ID])).await;
    assert_eq!(connect, list);
    let connect = call_ok(
        addr,
        c,
        "web.connect",
        json!(["ffffffffffffffffffffffffffffffff"]),
    )
    .await;
    assert_eq!(connect, Value::Null);

    assert_eq!(
        call_ok(addr, c, "web.connected", json!([])).await,
        json!(true)
    );
    assert_eq!(
        call_ok(addr, c, "web.disconnect", json!([])).await,
        json!("Connection was closed cleanly.")
    );
    assert_eq!(
        call_ok(addr, c, "web.get_hosts", json!([])).await,
        json!([[HOST_ID, "127.0.0.1", 58846, ""]])
    );
    assert_eq!(
        call_ok(addr, c, "web.get_host_status", json!([HOST_ID])).await,
        json!([HOST_ID, "Connected", env!("CARGO_PKG_VERSION")])
    );
    assert_eq!(
        call_ok(addr, c, "web.get_host_status", json!(["beef"])).await,
        json!(["beef", "Offline", ""])
    );
    assert_eq!(
        call_ok(addr, c, "web.add_host", json!(["127.0.0.1", 58846])).await,
        json!([false, "Host details already in hostlist"])
    );
    assert_eq!(
        call_ok(addr, c, "web.add_host", json!(["h", 1, "u", "p"])).await,
        json!([false, "Host details already in hostlist"])
    );
    assert_eq!(
        call_ok(addr, c, "web.edit_host", json!([HOST_ID, "h", 1])).await,
        json!(false)
    );
    assert_eq!(
        call_ok(addr, c, "web.remove_host", json!([HOST_ID])).await,
        json!(false)
    );
    assert_eq!(
        call_ok(addr, c, "web.start_daemon", json!([58846])).await,
        Value::Null
    );
    assert_eq!(
        call_ok(addr, c, "web.stop_daemon", json!([HOST_ID])).await,
        Value::Null
    );

    assert_eq!(
        call_ok(addr, c, "webutils.get_languages", json!([])).await,
        json!([])
    );
    assert_eq!(
        call_ok(addr, c, "webutils.get_themes", json!([])).await,
        json!([])
    );

    for (rpc, authorized) in [
        ("daemon.get_version", true),
        ("core.get_config", true),
        ("web.connected", false),
        ("core.nope", false),
        ("core.create_account", false),
    ] {
        assert_eq!(
            call_ok(addr, c, "daemon.authorized_call", json!([rpc])).await,
            json!(authorized),
            "{rpc}"
        );
    }
    assert_eq!(
        call_ok(addr, c, "daemon.get_version", json!([])).await,
        json!(env!("CARGO_PKG_VERSION"))
    );

    assert_eq!(
        call_ok(addr, c, "auth.change_password", json!(["s3cret", "new"])).await,
        json!(false)
    );
    let body = json!({"method": "auth.delete_session", "params": [], "id": 4}).to_string();
    let (_, set_cookie, body) = raw_request(addr, "POST", c, &body).await;
    let envelope: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(envelope["result"], json!(true));
    let set_cookie = set_cookie.unwrap();
    assert!(set_cookie.starts_with("_session_id=;"), "{set_cookie}");
    assert!(set_cookie.contains("Max-Age=0"), "{set_cookie}");
    // With no session store, a client that replays the token cookie
    // still holds a valid session.
    assert_eq!(
        call_ok(addr, c, "auth.check_session", json!([])).await,
        json!(true)
    );

    assert_eq!(
        call_ok(addr, c, "daemon.shutdown", json!([])).await,
        Value::Null
    );
    assert_eq!(
        call_ok(addr, c, "web.connected", json!([])).await,
        json!(true)
    );

    daemon.stop().await;
}

fn fixture_path() -> PathBuf {
    // libtorrent opens paths natively, and native Windows path handling
    // accepts neither `..` nor forward slashes.
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("rbtorrent")
        .join("tests")
        .join("fixtures")
        .join("transfer.torrent")
}

/// Writes the fixture's payload files (same xorshift PRNG as
/// gen_fixtures.cpp) so a `seed_mode` add can seed them.
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

async fn torrent_status(addr: SocketAddr, cookie: Option<&str>, id: &str, keys: Value) -> Value {
    call_ok(addr, cookie, "core.get_torrent_status", json!([id, keys])).await
}

/// Handle mutations are asynchronous posts to the session thread.
async fn wait_torrent_status(
    addr: SocketAddr,
    cookie: Option<&str>,
    id: &str,
    key: &str,
    want: &Value,
) {
    for _ in 0..300 {
        let status = torrent_status(addr, cookie, id, json!([key])).await;
        if &status[key] == want {
            return;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("torrent {id} never reached {key} == {want}");
}

/// The `core.*` surface that needs no torrents.
#[tokio::test(flavor = "multi_thread")]
async fn core_stubs_and_config() {
    let state = tempfile::tempdir().unwrap();
    let daemon = Daemon::start(test_config(state.path(), None), Some(hermetic_settings()))
        .await
        .unwrap();
    let addr = daemon.tcp_addr().unwrap();
    let c = None;

    assert_eq!(
        call_ok(addr, c, "core.get_auth_levels_mappings", json!([])).await,
        json!([
            {"NONE": 0, "READONLY": 1, "DEFAULT": 5, "NORMAL": 5, "ADMIN": 10},
            {"0": "NONE", "1": "READONLY", "5": "NORMAL", "10": "ADMIN"},
        ])
    );

    // Enabling fails softly, disabling is idempotently true.
    assert_eq!(
        call_ok(addr, c, "core.get_available_plugins", json!([])).await,
        json!([])
    );
    assert_eq!(
        call_ok(addr, c, "core.get_enabled_plugins", json!([])).await,
        json!([])
    );
    assert_eq!(
        call_ok(addr, c, "core.enable_plugin", json!(["Label"])).await,
        json!(false)
    );
    assert_eq!(
        call_ok(addr, c, "core.disable_plugin", json!(["Label"])).await,
        json!(true)
    );
    assert_eq!(
        call_ok(addr, c, "core.rescan_plugins", json!([])).await,
        Value::Null
    );
    assert_eq!(
        call_ok(addr, c, "core.upload_plugin", json!(["p.egg", ""])).await,
        Value::Null
    );

    // The "nothing there" values.
    assert_eq!(
        call_ok(addr, c, "core.get_free_space", json!([])).await,
        json!(-1)
    );
    assert_eq!(
        call_ok(addr, c, "core.get_free_space", json!(["/tmp"])).await,
        json!(-1)
    );
    assert_eq!(
        call_ok(addr, c, "core.get_path_size", json!(["/tmp"])).await,
        json!(-1)
    );
    assert_eq!(
        call_ok(addr, c, "core.glob", json!(["/tmp/*"])).await,
        json!([])
    );
    assert_eq!(
        call_ok(
            addr,
            c,
            "core.get_completion_paths",
            json!([{"completion_text": "/t", "show_hidden_files": false}])
        )
        .await,
        json!({"completion_text": "/t", "show_hidden_files": false, "paths": []})
    );
    assert_eq!(
        call_ok(addr, c, "core.get_external_ip", json!([])).await,
        Value::Null
    );
    assert_eq!(
        call_ok(addr, c, "core.test_listen_port", json!([])).await,
        Value::Null
    );

    let code = call_err(addr, c, "core.add_torrent_url", json!(["http://x", {}])).await;
    assert_eq!(code, 3);
    let code = call_err(addr, c, "core.create_torrent", json!(["/x", null, 0])).await;
    assert_eq!(code, 3);
    let code = call_err(
        addr,
        c,
        "core.prefetch_magnet_metadata",
        json!(["magnet:?"]),
    )
    .await;
    assert_eq!(code, 3);

    let version = call_ok(addr, c, "core.get_libtorrent_version", json!([])).await;
    assert!(version.as_str().is_some_and(|v| !v.is_empty()));
    assert!(
        call_ok(addr, c, "core.get_listen_port", json!([]))
            .await
            .is_number()
    );
    assert_eq!(
        call_ok(addr, c, "core.get_session_state", json!([])).await,
        json!([])
    );
    assert_eq!(
        call_ok(addr, c, "core.get_torrents_status", json!([{}, []])).await,
        json!({})
    );
    let tree = call_ok(addr, c, "core.get_filter_tree", json!([])).await;
    assert_eq!(tree["state"][0], json!(["All", 0]));
    assert!(tree["tracker_host"].is_array());
    assert!(tree["owner"].is_array());
    // Hidden categories are dropped; zero hits keep only "All".
    let tree = call_ok(
        addr,
        c,
        "core.get_filter_tree",
        json!([false, ["tracker_host"]]),
    )
    .await;
    assert!(tree.get("tracker_host").is_none());
    assert_eq!(tree["state"], json!([["All", 0]]));

    assert_eq!(
        call_ok(addr, c, "core.is_session_paused", json!([])).await,
        json!(false)
    );
    assert_eq!(
        call_ok(addr, c, "core.pause_session", json!([])).await,
        Value::Null
    );
    assert_eq!(
        call_ok(addr, c, "core.is_session_paused", json!([])).await,
        json!(true)
    );
    assert_eq!(
        call_ok(addr, c, "core.resume_session", json!([])).await,
        Value::Null
    );
    assert_eq!(
        call_ok(addr, c, "core.is_session_paused", json!([])).await,
        json!(false)
    );

    // Keys without engine backing are absent.
    let config = call_ok(addr, c, "core.get_config", json!([])).await;
    assert_eq!(config["dht"], json!(false));
    assert_eq!(config["upnp"], json!(false));
    assert_eq!(config["lsd"], json!(false));
    assert!(config.get("download_location").is_none());
    assert!(config["max_download_speed"].is_number());
    assert!(config["max_connections_global"].is_number());
    if let Some(ports) = config.get("listen_ports") {
        assert_eq!(ports.as_array().unwrap().len(), 2);
    }

    assert_eq!(
        call_ok(addr, c, "core.get_config_value", json!(["dht"])).await,
        json!(false)
    );
    assert_eq!(
        call_ok(addr, c, "core.get_config_value", json!(["nonexistent"])).await,
        Value::Null
    );
    assert_eq!(
        call_ok(addr, c, "core.get_config_values", json!([["dht", "bogus"]])).await,
        json!({"dht": false, "bogus": null})
    );

    // set_config applies supported keys and silently skips the rest.
    assert_eq!(
        call_ok(
            addr,
            c,
            "core.set_config",
            json!([{
                "max_download_speed": 100.0,
                "max_active_downloading": 7,
                "download_location": "/ignored",
                "totally_unknown": 1,
            }])
        )
        .await,
        Value::Null
    );
    assert_eq!(
        call_ok(
            addr,
            c,
            "core.get_config_value",
            json!(["max_download_speed"])
        )
        .await,
        json!(100.0)
    );
    assert_eq!(
        call_ok(
            addr,
            c,
            "core.get_config_value",
            json!(["max_active_downloading"])
        )
        .await,
        json!(7)
    );

    let proxy = call_ok(addr, c, "core.get_proxy", json!([])).await;
    assert_eq!(proxy["type"], json!(0));
    assert_eq!(proxy["hostname"], json!(""));
    assert_eq!(proxy["anonymous_mode"], json!(false));
    // anonymous_mode applies even with proxy type none.
    for anonymous in [true, false] {
        call_ok(
            addr,
            c,
            "core.set_config",
            json!([{"proxy": {"type": 0, "anonymous_mode": anonymous}}]),
        )
        .await;
        let proxy = call_ok(addr, c, "core.get_proxy", json!([])).await;
        assert_eq!(proxy["anonymous_mode"], json!(anonymous));
    }

    // Aliases resolve; unknown and derived rate keys drop out silently.
    let all = call_ok(addr, c, "core.get_session_status", json!([[]])).await;
    let all = all.as_object().unwrap();
    assert!(all.len() > 100, "{}", all.len());
    assert!(all.contains_key("net.recv_payload_bytes"));
    assert!(!all.contains_key("payload_download_rate"));
    assert_eq!(all["write_hit_ratio"], json!(0.0));
    let named = call_ok(
        addr,
        c,
        "core.get_session_status",
        json!([["dht_nodes", "dht.dht_nodes", "upload_rate", "no_such_key"]]),
    )
    .await;
    let named = named.as_object().unwrap();
    assert!(named.contains_key("dht_nodes"));
    assert!(named.contains_key("dht.dht_nodes"));
    assert!(!named.contains_key("upload_rate"));
    assert!(!named.contains_key("no_such_key"));

    let nil = "00000000-0000-0000-0000-000000000000";
    assert_eq!(
        call_err(addr, c, "core.get_torrent_status", json!([nil, []])).await,
        3
    );
    assert_eq!(
        call_err(
            addr,
            c,
            "core.get_torrent_status",
            json!(["not-a-uuid", []])
        )
        .await,
        3
    );

    daemon.stop().await;
}

/// The `web.*` surface that needs no torrents.
#[tokio::test(flavor = "multi_thread")]
async fn web_stubs_and_config() {
    let state = tempfile::tempdir().unwrap();
    let daemon = Daemon::start(test_config(state.path(), None), Some(hermetic_settings()))
        .await
        .unwrap();
    let addr = daemon.tcp_addr().unwrap();
    let c = None;

    let config = call_ok(addr, c, "web.get_config", json!([])).await;
    assert_eq!(config["default_daemon"], json!(HOST_ID));
    assert_eq!(config["first_login"], json!(false));
    assert_eq!(config["theme"], json!("gray"));
    assert_eq!(config["session_timeout"], json!(3600));
    assert!(config.get("sessions").is_none());
    assert!(config.get("pwd_sha1").is_none());
    assert_eq!(
        call_ok(addr, c, "web.set_config", json!([{"theme": "dark"}])).await,
        Value::Null
    );
    let config = call_ok(addr, c, "web.get_config", json!([])).await;
    assert_eq!(config["theme"], json!("gray"));
    assert_eq!(call_err(addr, c, "web.set_config", json!([42])).await, 3);

    assert_eq!(
        call_ok(addr, c, "web.get_plugins", json!([])).await,
        json!({"available_plugins": [], "enabled_plugins": []})
    );
    let info = call_ok(addr, c, "web.get_plugin_info", json!(["Label"])).await;
    assert_eq!(info["Name"], json!("not available"));
    assert_eq!(info["Version"], json!("not available"));
    assert_eq!(info["Author"], json!(""));
    assert_eq!(
        call_ok(addr, c, "web.get_plugin_resources", json!(["Label"])).await,
        Value::Null
    );
    assert_eq!(
        call_ok(addr, c, "web.upload_plugin", json!(["p.egg", "/tmp/p.egg"])).await,
        json!(false)
    );
    assert_eq!(
        call_ok(addr, c, "web.set_theme", json!(["dark"])).await,
        Value::Null
    );
    assert_eq!(
        call_ok(
            addr,
            c,
            "web.register_event_listener",
            json!(["TorrentAddedEvent"])
        )
        .await,
        Value::Null
    );
    assert_eq!(
        call_ok(
            addr,
            c,
            "web.deregister_event_listener",
            json!(["TorrentAddedEvent"])
        )
        .await,
        Value::Null
    );
    // get_events keeps Deluge's ~5 s long-poll pause, then null.
    assert_eq!(
        call_ok(addr, c, "web.get_events", json!([])).await,
        Value::Null
    );

    let code = call_err(
        addr,
        c,
        "web.download_torrent_from_url",
        json!(["http://x/f.torrent"]),
    )
    .await;
    assert_eq!(code, 3);

    // An unreadable .torrent path is the literal false.
    assert_eq!(
        call_ok(addr, c, "web.get_magnet_info", json!(["junk"])).await,
        json!({})
    );
    let magnet = "magnet:?xt=urn:btih:c0ffeec0ffeec0ffeec0ffeec0ffeec0ffeec0ff\
                  &dn=Cup&tr=udp%3A%2F%2Ft.example%3A6969%2Fann";
    let info = call_ok(addr, c, "web.get_magnet_info", json!([magnet])).await;
    assert_eq!(
        info,
        json!({
            "name": "Cup",
            "info_hash": "c0ffeec0ffeec0ffeec0ffeec0ffeec0ffeec0ff",
            "files_tree": "",
            "trackers": {"udp://t.example:6969/ann": 0},
        })
    );
    assert_eq!(
        call_ok(addr, c, "web.get_torrent_info", json!(["/no/such.torrent"])).await,
        json!(false)
    );

    // No free_space/external_ip stats.
    let ui = call_ok(addr, c, "web.update_ui", json!([["name"], {}])).await;
    assert_eq!(ui["connected"], json!(true));
    assert_eq!(ui["torrents"], json!({}));
    assert_eq!(ui["filters"]["state"][0], json!(["All", 0]));
    let stats = ui["stats"].as_object().unwrap();
    for key in [
        "max_download",
        "max_upload",
        "max_num_connections",
        "num_connections",
        "dht_nodes",
        "has_incoming_connections",
        "download_rate",
        "upload_rate",
        "download_protocol_rate",
        "upload_protocol_rate",
    ] {
        assert!(stats[key].is_number(), "{key}: {stats:?}");
    }
    assert!(!stats.contains_key("free_space"));
    assert!(!stats.contains_key("external_ip"));

    // A pathless entry fails its slot without failing the batch.
    assert_eq!(
        call_ok(addr, c, "web.add_torrents", json!([[]])).await,
        json!([])
    );
    let results = call_ok(addr, c, "web.add_torrents", json!([[{"options": null}]])).await;
    assert_eq!(results[0][0], json!(false));
    assert!(results[0][1].is_string());

    let nil = "00000000-0000-0000-0000-000000000000";
    assert_eq!(
        call_err(addr, c, "web.get_torrent_status", json!([nil, []])).await,
        3
    );
    assert_eq!(
        call_err(addr, c, "web.get_torrent_files", json!(["not-a-uuid"])).await,
        3
    );

    daemon.stop().await;
}

/// One torrent through the whole `core.*` lifecycle.
#[tokio::test(flavor = "multi_thread")]
async fn core_torrent_lifecycle() {
    use base64::Engine as _;

    let state = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let daemon = Daemon::start(test_config(state.path(), None), Some(hermetic_settings()))
        .await
        .unwrap();
    let addr = daemon.tcp_addr().unwrap();
    let c = None;

    write_fixture_content(data.path());
    let torrent_b64 =
        base64::engine::general_purpose::STANDARD.encode(std::fs::read(fixture_path()).unwrap());
    let save = data.path().to_str().unwrap();

    // download_location is mandatory: rsbtd has no default.
    let code = call_err(
        addr,
        c,
        "core.add_torrent_file",
        json!(["transfer.torrent", torrent_b64, {}]),
    )
    .await;
    assert_eq!(code, 3);

    // Added over the payload written above, so seed_mode can seed it.
    let id = call_ok(
        addr,
        c,
        "core.add_torrent_file",
        json!([
            "transfer.torrent",
            torrent_b64,
            {"download_location": save, "seed_mode": true}
        ]),
    )
    .await;
    let id = id.as_str().unwrap().to_owned();
    uuid::Uuid::parse_str(&id).unwrap();

    let code = call_err(
        addr,
        c,
        "core.add_torrent_file",
        json!(["transfer.torrent", torrent_b64, {"download_location": save}]),
    )
    .await;
    assert_eq!(code, 3);

    assert_eq!(
        call_ok(addr, c, "core.get_session_state", json!([])).await,
        json!([id])
    );

    // Errors-only: successes contribute nothing to the result.
    assert_eq!(
        call_ok(addr, c, "core.add_torrent_files", json!([[]])).await,
        json!([])
    );
    let errors = call_ok(
        addr,
        c,
        "core.add_torrent_files",
        json!([[["transfer.torrent", torrent_b64, {"download_location": save}]]]),
    )
    .await;
    let errors = errors.as_array().unwrap();
    assert_eq!(errors.len(), 1);
    assert!(errors[0].is_string(), "{errors:?}");

    wait_torrent_status(addr, c, &id, "state", &json!("Seeding")).await;
    let st = torrent_status(
        addr,
        c,
        &id,
        json!([
            "name",
            "hash",
            "save_path",
            "download_location",
            "total_size",
            "progress",
            "owner",
            "num_files"
        ]),
    )
    .await;
    // `hash` is the info-hash, not the uuid the torrent is keyed by.
    assert_eq!(
        st["hash"],
        json!("f16f1145c1c8d1c031da83461d9d0a27d243e454")
    );
    assert_eq!(st["name"], json!("fixture"));
    assert_eq!(st["total_size"], json!(40960 + 137));
    assert_eq!(st["progress"], json!(100.0));
    assert_eq!(st["owner"], json!("localclient"));
    assert_eq!(st["num_files"], json!(2));
    assert_eq!(st["download_location"], st["save_path"]);

    // Empty keys return the full built-in catalog, sentinels included.
    let st = torrent_status(addr, c, &id, json!([])).await;
    assert!(st.as_object().unwrap().len() >= 70, "{st}");
    assert_eq!(st["stop_at_ratio"], json!(false));
    assert_eq!(st["is_seed"], json!(true));
    assert_eq!(st["pieces"], Value::Null); // null while seeding
    assert_eq!(st["eta"], json!(0));
    let files = st["files"].as_array().unwrap();
    assert_eq!(files.len(), 2);
    assert_eq!(files[0]["path"], json!("fixture/a.bin"));
    assert_eq!(st["file_progress"], json!([1.0, 1.0]));

    let all = call_ok(addr, c, "core.get_torrents_status", json!([{}, ["state"]])).await;
    assert_eq!(all[id.as_str()]["state"], json!("Seeding"));
    let by_id = call_ok(
        addr,
        c,
        "core.get_torrents_status",
        json!([{"id": [id]}, ["name"]]),
    )
    .await;
    assert_eq!(by_id.as_object().unwrap().len(), 1);
    let none = call_ok(
        addr,
        c,
        "core.get_torrents_status",
        json!([{"state": "Downloading"}, []]),
    )
    .await;
    assert_eq!(none, json!({}));
    // An empty state list is a constraint no torrent satisfies.
    let none = call_ok(
        addr,
        c,
        "core.get_torrents_status",
        json!([{"state": []}, []]),
    )
    .await;
    assert_eq!(none, json!({}));
    let keyword = call_ok(
        addr,
        c,
        "core.get_torrents_status",
        json!([{"keyword": "FIXT"}, ["name"]]),
    )
    .await;
    assert_eq!(keyword.as_object().unwrap().len(), 1);
    // Keywords reach the file paths, not just the name.
    let keyword = call_ok(
        addr,
        c,
        "core.get_torrents_status",
        json!([{"keyword": "b.txt"}, ["name"]]),
    )
    .await;
    assert_eq!(keyword.as_object().unwrap().len(), 1);

    let tree = call_ok(addr, c, "core.get_filter_tree", json!([])).await;
    assert_eq!(tree["state"][0], json!(["All", 1]));
    assert!(
        tree["state"]
            .as_array()
            .unwrap()
            .contains(&json!(["Seeding", 1]))
    );
    assert_eq!(
        tree["owner"].as_array().unwrap().last(),
        Some(&json!(["localclient", 1]))
    );

    // Pause detaches and pauses; resume brings seeding back.
    assert_eq!(
        call_ok(addr, c, "core.pause_torrent", json!([id])).await,
        Value::Null
    );
    wait_torrent_status(addr, c, &id, "state", &json!("Paused")).await;
    assert_eq!(
        call_ok(addr, c, "core.resume_torrent", json!([id])).await,
        Value::Null
    );
    wait_torrent_status(addr, c, &id, "state", &json!("Seeding")).await;

    // The plural forms read an empty (or omitted) list as every torrent.
    assert_eq!(
        call_ok(addr, c, "core.pause_torrents", json!([[]])).await,
        Value::Null
    );
    wait_torrent_status(addr, c, &id, "state", &json!("Paused")).await;
    assert_eq!(
        call_ok(addr, c, "core.resume_torrents", json!([])).await,
        Value::Null
    );
    wait_torrent_status(addr, c, &id, "state", &json!("Seeding")).await;

    // A zero rate is the other unlimited spelling.
    call_ok(
        addr,
        c,
        "core.set_torrent_options",
        json!([[id], {"max_download_speed": 100.0}]),
    )
    .await;
    wait_torrent_status(addr, c, &id, "max_download_speed", &json!(100.0)).await;
    call_ok(
        addr,
        c,
        "core.set_torrent_max_upload_speed",
        json!([id, 200.0]),
    )
    .await;
    wait_torrent_status(addr, c, &id, "max_upload_speed", &json!(200.0)).await;
    call_ok(
        addr,
        c,
        "core.set_torrent_options",
        json!([[id], {"max_download_speed": 0.0}]),
    )
    .await;
    wait_torrent_status(addr, c, &id, "max_download_speed", &json!(-1.0)).await;
    // Deluge normalizes connection limits 0 and 1.
    call_ok(
        addr,
        c,
        "core.set_torrent_options",
        json!([[id], {"max_connections": 0}]),
    )
    .await;
    wait_torrent_status(addr, c, &id, "max_connections", &json!(-1)).await;
    call_ok(
        addr,
        c,
        "core.set_torrent_options",
        json!([[id], {"max_connections": 1}]),
    )
    .await;
    wait_torrent_status(addr, c, &id, "max_connections", &json!(2)).await;
    // Upload slots take the 0 the web UI's menu sends, but not 1.
    call_ok(addr, c, "core.set_torrent_max_upload_slots", json!([id, 0])).await;
    wait_torrent_status(addr, c, &id, "max_upload_slots", &json!(-1)).await;
    let code = call_err(addr, c, "core.set_torrent_max_upload_slots", json!([id, 1])).await;
    assert_eq!(code, 3);

    // Renames answer one [success, result] entry per renamed child; a
    // drive-rooted target is refused before any of them run.
    call_ok(
        addr,
        c,
        "core.rename_files",
        json!([id, [[1, "fixture/b2.txt"]]]),
    )
    .await;
    let st = torrent_status(addr, c, &id, json!(["files"])).await;
    assert_eq!(st["files"][1]["path"], json!("fixture/b2.txt"));
    let code = call_err(
        addr,
        c,
        "core.rename_files",
        json!([id, [[0, "C:\\outside\\a.bin"]]]),
    )
    .await;
    assert_eq!(code, 3);
    let renamed = call_ok(
        addr,
        c,
        "core.rename_folder",
        json!([id, "fixture", "renamed"]),
    )
    .await;
    assert_eq!(renamed, json!([[true, null], [true, null]]));
    let st = torrent_status(addr, c, &id, json!(["files"])).await;
    assert_eq!(st["files"][0]["path"], json!("renamed/a.bin"));
    assert_eq!(st["files"][1]["path"], json!("renamed/b2.txt"));

    // Moves are answered before they finish, but their turn is taken in
    // call order, so the last destination is the one that sticks.
    let first = data.path().join("first");
    let second = data.path().join("second");
    for dest in [&first, &second] {
        call_ok(
            addr,
            c,
            "core.move_storage",
            json!([[id], dest.to_str().unwrap()]),
        )
        .await;
    }
    let second = json!(second.to_str().unwrap());
    wait_torrent_status(addr, c, &id, "save_path", &second).await;

    let magnet = call_ok(addr, c, "core.get_magnet_uri", json!([id])).await;
    assert!(magnet.as_str().unwrap().starts_with("magnet:?xt=urn:btih:"));

    // remove_torrents reports per-id failures; remove_torrent removes.
    let nil = "00000000-0000-0000-0000-000000000000";
    let failures = call_ok(addr, c, "core.remove_torrents", json!([[nil], false])).await;
    assert_eq!(failures.as_array().unwrap().len(), 1);
    assert_eq!(failures[0][0], json!(nil));
    assert_eq!(
        call_ok(addr, c, "core.remove_torrent", json!([id, false])).await,
        json!(true)
    );
    assert_eq!(
        call_ok(addr, c, "core.get_session_state", json!([])).await,
        json!([])
    );
    assert_eq!(
        call_err(addr, c, "core.remove_torrent", json!([id, false])).await,
        3
    );

    daemon.stop().await;
}

/// The file options that only exist at add time.
#[tokio::test(flavor = "multi_thread")]
async fn add_time_file_options() {
    let state = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let daemon = Daemon::start(test_config(state.path(), None), Some(hermetic_settings()))
        .await
        .unwrap();
    let addr = daemon.tcp_addr().unwrap();
    let c = None;
    let save = data.path().to_str().unwrap();

    // mapped_files renames before the add, so the files status reads
    // back the mapped path.
    let results = call_ok(
        addr,
        c,
        "web.add_torrents",
        json!([[{
            "path": fixture_path().to_str().unwrap(),
            "options": {
                "download_location": save,
                "add_paused": true,
                "mapped_files": {"1": "fixture/renamed.txt"},
            },
        }]]),
    )
    .await;
    assert_eq!(results[0][0], json!(true), "{results}");
    let id = results[0][1].as_str().unwrap().to_owned();
    let st = torrent_status(addr, c, &id, json!(["files"])).await;
    assert_eq!(st["files"][0]["path"], json!("fixture/a.bin"));
    assert_eq!(st["files"][1]["path"], json!("fixture/renamed.txt"));

    // The pure-v2 fixture carries a pad after each of its two payload
    // files; the add dialog's preview hides them and numbers what is
    // left contiguously.
    let v2 = fixture_path().with_file_name("v2.torrent");
    let info = call_ok(
        addr,
        c,
        "web.get_torrent_info",
        json!([v2.to_str().unwrap()]),
    )
    .await;
    let contents = &info["files_tree"]["contents"]["fixture"]["contents"];
    assert_eq!(contents.as_object().unwrap().len(), 2);
    assert_eq!(contents["a.bin"]["index"], json!(0));
    assert_eq!(contents["b.txt"]["index"], json!(1));

    // Priorities and renames in those terms land on the four libtorrent
    // files behind them, pads included.
    let results = call_ok(
        addr,
        c,
        "web.add_torrents",
        json!([[{
            "path": v2.to_str().unwrap(),
            "options": {
                "download_location": save,
                "add_paused": true,
                "file_priorities": [0, 7],
                "mapped_files": {"1": "fixture/renamed.txt"},
            },
        }]]),
    )
    .await;
    assert_eq!(results[0][0], json!(true), "{results}");
    let v2_id = results[0][1].as_str().unwrap().to_owned();
    let st = torrent_status(addr, c, &v2_id, json!(["file_priorities", "files"])).await;
    assert_eq!(st["file_priorities"], json!([0, 0, 7, 0]));
    assert_eq!(st["files"][0]["path"], json!("fixture/a.bin"));
    assert_eq!(st["files"][2]["path"], json!("fixture/renamed.txt"));
    // Past the payload files there is nothing to rename; the add fails
    // rather than renaming a pad.
    let hybrid = fixture_path().with_file_name("hybrid.torrent");
    let results = call_ok(
        addr,
        c,
        "web.add_torrents",
        json!([[{
            "path": hybrid.to_str().unwrap(),
            "options": {"download_location": save, "mapped_files": {"2": "fixture/x"}},
        }]]),
    )
    .await;
    assert_eq!(results[0][0], json!(false), "{results}");
    assert_eq!(results[0][1], json!("mapped_files: no file at index 2"));

    // A list that covers neither numbering is ignored outright, rather
    // than applied to the files it does reach.
    let results = call_ok(
        addr,
        c,
        "web.add_torrents",
        json!([[{
            "path": hybrid.to_str().unwrap(),
            "options": {
                "download_location": save,
                "add_paused": true,
                "file_priorities": [0],
            },
        }]]),
    )
    .await;
    assert_eq!(results[0][0], json!(true), "{results}");
    let hybrid_id = results[0][1].as_str().unwrap().to_owned();
    let st = torrent_status(addr, c, &hybrid_id, json!(["file_priorities"])).await;
    assert_eq!(st["file_priorities"], json!([4, 0, 4, 0]));

    daemon.stop().await;
}

/// Torrents through the `web.*` surface.
#[tokio::test(flavor = "multi_thread")]
async fn web_torrent_lifecycle() {
    let state = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let daemon = Daemon::start(test_config(state.path(), None), Some(hermetic_settings()))
        .await
        .unwrap();
    let addr = daemon.tcp_addr().unwrap();
    let c = None;

    write_fixture_content(data.path());
    let fixture = fixture_path();
    let fixture = fixture.to_str().unwrap();
    let save = data.path().to_str().unwrap();

    // A server-side .torrent path, a magnet, and a bogus path, which
    // fails its slot without aborting the rest.
    let magnet = "magnet:?xt=urn:btih:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa&dn=maggy\
                  &ws=http%3A%2F%2Fseed.example%2Fpath";
    let results = call_ok(
        addr,
        c,
        "web.add_torrents",
        json!([[
            {"path": fixture, "options": {"download_location": save, "seed_mode": true}},
            {"path": magnet, "options": {"download_location": save, "auto_managed": false, "add_paused": false}},
            {"path": "/no/such.torrent", "options": {"download_location": save}},
        ]]),
    )
    .await;
    let results = results.as_array().unwrap();
    assert_eq!(results.len(), 3);
    assert_eq!(results[0][0], json!(true));
    assert_eq!(results[1][0], json!(true));
    assert_eq!(results[2][0], json!(false));
    assert!(results[2][1].is_string());
    let id = results[0][1].as_str().unwrap().to_owned();
    let magnet_id = results[1][1].as_str().unwrap().to_owned();
    uuid::Uuid::parse_str(&id).unwrap();
    uuid::Uuid::parse_str(&magnet_id).unwrap();

    wait_torrent_status(addr, c, &id, "state", &json!("Seeding")).await;

    // Fresh libtorrent params default to paused + auto-managed, and
    // without auto-management no queue would ever resume this add.
    wait_torrent_status(addr, c, &magnet_id, "state", &json!("Downloading")).await;
    let st = call_ok(
        addr,
        c,
        "web.get_torrent_status",
        json!([magnet_id, ["auto_managed", "paused"]]),
    )
    .await;
    assert_eq!(st["auto_managed"], json!(false));
    assert_eq!(st["paused"], json!(false));

    let st = call_ok(
        addr,
        c,
        "web.get_torrent_status",
        json!([id, ["name", "state", "hash"]]),
    )
    .await;
    assert_eq!(st["name"], json!("fixture"));
    assert_eq!(st["state"], json!("Seeding"));
    assert_eq!(
        st["hash"],
        json!("f16f1145c1c8d1c031da83461d9d0a27d243e454")
    );

    // Progress stays a 0-1 fraction at both levels.
    let tree = call_ok(addr, c, "web.get_torrent_files", json!([id])).await;
    assert_eq!(tree["type"], json!("dir"));
    let dir = &tree["contents"]["fixture"];
    assert_eq!(dir["type"], json!("dir"));
    assert_eq!(dir["path"], json!("fixture"));
    assert_eq!(dir["size"], json!(40960 + 137));
    assert_eq!(dir["priority"], json!(4));
    assert!((dir["progress"].as_f64().unwrap() - 1.0).abs() < 1e-9);
    assert_eq!(dir["progresses"].as_array().unwrap().len(), 2);
    let a = &dir["contents"]["a.bin"];
    assert_eq!(a["type"], json!("file"));
    assert_eq!(a["index"], json!(0));
    assert_eq!(a["path"], json!("fixture/a.bin"));
    assert_eq!(a["size"], json!(40960));
    assert_eq!(a["offset"], json!(0));
    assert_eq!(a["progress"], json!(1.0));
    assert_eq!(a["priority"], json!(4));
    assert_eq!(dir["contents"]["b.txt"]["size"], json!(137));

    let tree = call_ok(addr, c, "web.get_torrent_files", json!([magnet_id])).await;
    assert_eq!(tree, json!({"type": "dir", "contents": {}}));

    let info = call_ok(addr, c, "web.get_torrent_info", json!([fixture])).await;
    assert_eq!(info["name"], json!("fixture"));
    assert_eq!(
        info["info_hash"],
        json!("f16f1145c1c8d1c031da83461d9d0a27d243e454")
    );
    let ftree = &info["files_tree"];
    assert_eq!(ftree["type"], json!("dir"));
    let fdir = &ftree["contents"]["fixture"];
    assert_eq!(fdir["type"], json!("dir"));
    assert_eq!(fdir["length"], json!(40960 + 137));
    assert_eq!(fdir["download"], json!(true));
    assert_eq!(
        &fdir["contents"]["a.bin"],
        &json!({
            "type": "file",
            "path": "fixture/a.bin",
            "index": 0,
            "length": 40960,
            "download": true,
        })
    );

    let uri = call_ok(addr, c, "core.get_magnet_uri", json!([id])).await;
    let minfo = call_ok(addr, c, "web.get_magnet_info", json!([uri])).await;
    assert_eq!(
        minfo["info_hash"],
        json!("f16f1145c1c8d1c031da83461d9d0a27d243e454")
    );
    assert_eq!(minfo["name"], json!("fixture"));
    assert_eq!(minfo["files_tree"], json!(""));

    // Web seeds survive into generated magnet URIs as ws= params.
    let uri = call_ok(addr, c, "core.get_magnet_uri", json!([magnet_id])).await;
    assert!(
        uri.as_str()
            .unwrap()
            .contains("&ws=http%3A%2F%2Fseed.example%2Fpath"),
        "{uri}"
    );

    let ui = call_ok(addr, c, "web.update_ui", json!([["name", "state"], {}])).await;
    let torrents = ui["torrents"].as_object().unwrap();
    assert_eq!(torrents.len(), 2);
    assert_eq!(torrents[&id]["name"], json!("fixture"));
    assert_eq!(torrents[&id]["state"], json!("Seeding"));
    assert!(torrents.contains_key(&magnet_id));
    assert_eq!(ui["filters"]["state"][0], json!(["All", 2]));
    assert!(
        ui["filters"]["state"]
            .as_array()
            .unwrap()
            .contains(&json!(["Seeding", 1]))
    );
    let ui = call_ok(
        addr,
        c,
        "web.update_ui",
        json!([["name"], {"state": "Seeding"}]),
    )
    .await;
    let torrents = ui["torrents"].as_object().unwrap();
    assert_eq!(torrents.len(), 1);
    assert!(torrents.contains_key(&id));

    daemon.stop().await;
}

async fn queue_order(addr: SocketAddr, cookie: Option<&str>, ids: &[&String]) -> Vec<String> {
    let mut got: Vec<(i64, String)> = Vec::new();
    for id in ids {
        let st = torrent_status(addr, cookie, id, json!(["queue"])).await;
        got.push((st["queue"].as_i64().unwrap(), (*id).clone()));
    }
    got.sort();
    got.into_iter().map(|(_, id)| id).collect()
}

/// Queue moves are asynchronous posts to the session thread.
async fn wait_queue_order(addr: SocketAddr, cookie: Option<&str>, want: &[&String]) {
    let mut got = Vec::new();
    for _ in 0..300 {
        got = queue_order(addr, cookie, want).await;
        if got.iter().zip(want.iter()).all(|(g, w)| &g == w) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("queue never reached {want:?}, last {got:?}");
}

/// Batch queue operations order their moves by current position, so a
/// multi-selection keeps its relative order and a selected block
/// already touching a queue edge does not rotate.
#[tokio::test(flavor = "multi_thread")]
async fn queue_batch_order() {
    let state = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let daemon = Daemon::start(test_config(state.path(), None), Some(hermetic_settings()))
        .await
        .unwrap();
    let addr = daemon.tcp_addr().unwrap();
    let c = None;
    let save = data.path().to_str().unwrap();

    // Three metadata-less magnets, queued 0..2 in add order.
    let mut ids = Vec::new();
    for hash in [
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        "cccccccccccccccccccccccccccccccccccccccc",
    ] {
        let id = call_ok(
            addr,
            c,
            "core.add_torrent_magnet",
            json!([
                format!("magnet:?xt=urn:btih:{hash}"),
                {"download_location": save}
            ]),
        )
        .await;
        ids.push(id.as_str().unwrap().to_owned());
    }
    let [a, b, cc]: [String; 3] = ids.try_into().unwrap();
    wait_queue_order(addr, c, &[&a, &b, &cc]).await;

    // [A,B,C] with [B,C] sent to the top becomes [B,C,A], not [C,B,A].
    call_ok(addr, c, "core.queue_top", json!([[b, cc]])).await;
    wait_queue_order(addr, c, &[&b, &cc, &a]).await;

    call_ok(addr, c, "core.queue_up", json!([[cc, a]])).await;
    wait_queue_order(addr, c, &[&cc, &a, &b]).await;
    // ...and a block already at the top stays put instead of rotating.
    call_ok(addr, c, "core.queue_up", json!([[cc, a]])).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(
        queue_order(addr, c, &[&cc, &a, &b]).await,
        [cc.clone(), a.clone(), b.clone()]
    );

    call_ok(addr, c, "core.queue_bottom", json!([[cc, a]])).await;
    wait_queue_order(addr, c, &[&b, &cc, &a]).await;

    call_ok(addr, c, "core.queue_down", json!([[b, cc]])).await;
    wait_queue_order(addr, c, &[&a, &b, &cc]).await;
    // ...including the no-rotate rule at the bottom.
    call_ok(addr, c, "core.queue_down", json!([[b, cc]])).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(
        queue_order(addr, c, &[&a, &b, &cc]).await,
        [a.clone(), b.clone(), cc.clone()]
    );

    daemon.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn open_mode() {
    let state = tempfile::tempdir().unwrap();
    let daemon = Daemon::start(test_config(state.path(), None), Some(hermetic_settings()))
        .await
        .unwrap();
    let addr = daemon.tcp_addr().unwrap();

    // Without a token, no cookie is needed at all.
    assert_eq!(
        call_ok(addr, None, "web.connected", json!([])).await,
        json!(true)
    );
    assert_eq!(
        call_ok(addr, None, "auth.check_session", json!([])).await,
        json!(true)
    );

    // Any password logs in and is echoed back as the session id.
    let body = json!({"method": "auth.login", "params": ["anything"], "id": 1}).to_string();
    let (_, set_cookie, body) = raw_request(addr, "POST", None, &body).await;
    let envelope: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(envelope["result"], json!(true));
    assert_eq!(
        set_cookie.as_deref(),
        Some("_session_id=YW55dGhpbmc; Path=/; HttpOnly")
    );
    assert_eq!(
        call_ok(addr, Some("YW55dGhpbmc"), "web.connected", json!([])).await,
        json!(true)
    );

    daemon.stop().await;
}

/// The body cap answers in the envelope like every other bad request.
#[tokio::test(flavor = "multi_thread")]
async fn oversized_body() {
    let state = tempfile::tempdir().unwrap();
    let daemon = Daemon::start(test_config(state.path(), None), Some(hermetic_settings()))
        .await
        .unwrap();
    let addr = daemon.tcp_addr().unwrap();

    let padding = "x".repeat(rsbtd::api::BODY_LIMIT);
    let body = format!(r#"{{"method":"web.connected","params":[],"id":1,"pad":"{padding}"}}"#);
    let (status, _, body) = raw_request(addr, "POST", None, &body).await;
    assert_eq!(status, StatusCode::OK);
    let envelope: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(envelope["error"]["code"], json!(5), "{envelope}");
    assert_eq!(envelope["result"], Value::Null);
    assert_eq!(envelope["id"], Value::Null);

    daemon.stop().await;
}

/// A token holding a cookie delimiter still yields a working session.
#[tokio::test(flavor = "multi_thread")]
async fn delimiter_token_session() {
    let state = tempfile::tempdir().unwrap();
    let daemon = Daemon::start(
        test_config(state.path(), Some("ab;cd")),
        Some(hermetic_settings()),
    )
    .await
    .unwrap();
    let addr = daemon.tcp_addr().unwrap();

    let body = json!({"method": "auth.login", "params": ["ab;cd"], "id": 1}).to_string();
    let (_, set_cookie, _) = raw_request(addr, "POST", None, &body).await;
    // What a cookie jar would store, delimiters and all.
    let stored = set_cookie
        .as_deref()
        .and_then(|cookie| cookie.strip_prefix("_session_id="))
        .and_then(|value| value.split(';').next())
        .unwrap()
        .to_owned();
    assert_eq!(stored, "YWI7Y2Q");
    assert_eq!(
        call_ok(addr, Some(&stored), "auth.check_session", json!([])).await,
        json!(true)
    );

    daemon.stop().await;
}
