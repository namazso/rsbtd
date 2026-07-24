// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Unit tests for the typed settings surface: classification
//! completeness, backing-table agreement, per-group validation, and
//! write→read roundtrips through real packs.

use std::collections::{HashMap, HashSet};

use async_graphql::MaybeUndefined;
use rbtorrent::SettingsPack;
use rbtorrent::settings::all_settings;

use super::*;

/// A dense effective pack, as a real session would produce.
/// `SettingsPack::defaults()` is dense by construction (the shim fills
/// in the string settings whose libtorrent default is omitted).
fn dense_defaults() -> SettingsPack {
    let pack = SettingsPack::defaults();
    for (key, name, _) in all_settings() {
        assert!(pack.has(key), "{name} missing from defaults");
    }
    pack
}

/// Applies a delta on top of an effective pack, mimicking a session.
fn apply_delta(effective: &mut SettingsPack, delta: &SettingsPack) {
    effective.apply(delta);
}

/// An input with every field omitted.
fn undefined_input() -> SettingsInput {
    SettingsInput {
        scalars: catalog::ScalarSettingsInput::undefined(),
        user_agent: MaybeUndefined::Undefined,
        proxy: MaybeUndefined::Undefined,
        i2p: MaybeUndefined::Undefined,
        encryption: MaybeUndefined::Undefined,
        peer_transports: MaybeUndefined::Undefined,
        outgoing_port_range: MaybeUndefined::Undefined,
        disk_io_cache: MaybeUndefined::Undefined,
        mmap_write_mode: MaybeUndefined::Undefined,
        suggest_mode: MaybeUndefined::Undefined,
        choking_algorithm: MaybeUndefined::Undefined,
        seed_choking_algorithm: MaybeUndefined::Undefined,
        mixed_mode_algorithm: MaybeUndefined::Undefined,
        outgoing_interfaces: MaybeUndefined::Undefined,
        listen_interfaces: MaybeUndefined::Undefined,
        dht_bootstrap_nodes: MaybeUndefined::Undefined,
    }
}

/// Validates and stages `input` into a fresh delta.
fn write_one(input: SettingsInput) -> Result<SettingsPack, SettingsError> {
    let mut delta = SettingsPack::new();
    write(&mut delta, input)?;
    Ok(delta)
}

/// Writes `input`, applies it over the dense defaults, returns the new
/// effective pack.
fn apply_input(input: SettingsInput) -> SettingsPack {
    let delta = write_one(input).expect("write");
    let mut effective = dense_defaults();
    apply_delta(&mut effective, &delta);
    effective
}

fn write_err(input: SettingsInput) -> String {
    write_one(input).expect_err("write accepted").to_string()
}

/// Converts a read view into an input that re-states every field, for
/// roundtrip and completeness tests. Nullable groups that read `null`
/// become explicit null (disable), which also stages their backing.
/// The structured types serve both directions, so fields move straight
/// across; only the scalars change representation (`T` →
/// `MaybeUndefined<T>`).
fn settings_to_input(s: Settings) -> SettingsInput {
    fn opt<T>(v: Option<T>) -> MaybeUndefined<T> {
        match v {
            Some(v) => MaybeUndefined::Value(v),
            None => MaybeUndefined::Null,
        }
    }
    SettingsInput {
        scalars: s.scalars.into_input(),
        user_agent: MaybeUndefined::Value(s.user_agent),
        proxy: opt(s.proxy),
        i2p: opt(s.i2p),
        encryption: MaybeUndefined::Value(s.encryption),
        peer_transports: MaybeUndefined::Value(s.peer_transports),
        outgoing_port_range: opt(s.outgoing_port_range),
        disk_io_cache: MaybeUndefined::Value(s.disk_io_cache),
        mmap_write_mode: MaybeUndefined::Value(s.mmap_write_mode),
        suggest_mode: MaybeUndefined::Value(s.suggest_mode),
        choking_algorithm: MaybeUndefined::Value(s.choking_algorithm),
        seed_choking_algorithm: MaybeUndefined::Value(s.seed_choking_algorithm),
        mixed_mode_algorithm: MaybeUndefined::Value(s.mixed_mode_algorithm),
        outgoing_interfaces: MaybeUndefined::Value(s.outgoing_interfaces),
        listen_interfaces: MaybeUndefined::Value(s.listen_interfaces),
        dht_bootstrap_nodes: MaybeUndefined::Value(s.dht_bootstrap_nodes),
    }
}

fn socks5_proxy() -> ProxySettings {
    ProxySettings {
        protocol: ProxyProtocol::Socks5,
        hostname: "proxy.example".into(),
        port: 1080,
        username: String::new(),
        password: String::new(),
        resolve_hostnames: true,
        peer_connections: true,
        tracker_connections: true,
        socks5_udp_send_local_endpoint: false,
        send_hostname_in_connect: false,
    }
}

fn i2p_config() -> I2pSettings {
    I2pSettings {
        hostname: "127.0.0.1".into(),
        port: 7656,
        allow_mixed: false,
        inbound: I2pTunnel {
            tunnels: 3,
            hops: 3,
            hop_variance: 1,
        },
        outbound: I2pTunnel {
            tunnels: 3,
            hops: 3,
            hop_variance: 0,
        },
    }
}

/// One input per structured field, with only that field defined.
fn structured_single_field_inputs() -> Vec<(&'static str, SettingsInput)> {
    let mut out = Vec::new();
    let mut push = |name, f: &dyn Fn(&mut SettingsInput)| {
        let mut input = undefined_input();
        f(&mut input);
        out.push((name, input));
    };
    push("user_agent", &|i| {
        i.user_agent = MaybeUndefined::Value(UserAgent::Rsbtd);
    });
    push("proxy", &|i| {
        i.proxy = MaybeUndefined::Value(socks5_proxy())
    });
    push("i2p", &|i| i.i2p = MaybeUndefined::Value(i2p_config()));
    push("encryption", &|i| {
        i.encryption = MaybeUndefined::Value(EncryptionSettings {
            incoming: EncryptionPolicy::Enabled,
            outgoing: EncryptionPolicy::Enabled,
            methods: EncryptionMethods {
                plaintext: true,
                rc4: true,
            },
            prefer_rc4: false,
            announce_support: true,
        });
    });
    push("peer_transports", &|i| {
        i.peer_transports = MaybeUndefined::Value(PeerTransports {
            tcp: TransportDirections {
                incoming: true,
                outgoing: true,
            },
            utp: TransportDirections {
                incoming: true,
                outgoing: false,
            },
        });
    });
    push("outgoing_port_range", &|i| {
        i.outgoing_port_range = MaybeUndefined::Value(PortRange {
            first: 6881,
            last: 6889,
        });
    });
    push("disk_io_cache", &|i| {
        i.disk_io_cache = MaybeUndefined::Value(DiskIoCache {
            read: IoReadMode::EnableOsCache,
            write: IoWriteMode::WriteThrough,
        });
    });
    push("mmap_write_mode", &|i| {
        i.mmap_write_mode = MaybeUndefined::Value(MmapWriteMode::AlwaysPwrite);
    });
    push("suggest_mode", &|i| {
        i.suggest_mode = MaybeUndefined::Value(SuggestMode::ReadCache);
    });
    push("choking_algorithm", &|i| {
        i.choking_algorithm = MaybeUndefined::Value(ChokingAlgorithm::RateBased);
    });
    push("seed_choking_algorithm", &|i| {
        i.seed_choking_algorithm = MaybeUndefined::Value(SeedChokingAlgorithm::AntiLeech);
    });
    push("mixed_mode_algorithm", &|i| {
        i.mixed_mode_algorithm = MaybeUndefined::Value(MixedModeAlgorithm::PeerProportional);
    });
    push("outgoing_interfaces", &|i| {
        i.outgoing_interfaces = MaybeUndefined::Value(vec!["eth0".into()]);
    });
    push("listen_interfaces", &|i| {
        i.listen_interfaces = MaybeUndefined::Value(vec![ListenInterface {
            interface: "0.0.0.0".into(),
            port: 6881,
            ssl: false,
            local: false,
        }]);
    });
    push("dht_bootstrap_nodes", &|i| {
        i.dht_bootstrap_nodes = MaybeUndefined::Value(vec![HostPort {
            hostname: "router.example.com".into(),
            port: 6881,
        }]);
    });
    out
}

// ---- classification ------------------------------------------------------

/// The default-deny regression gate: BLACKLIST plus the field backing
/// tables must partition the generated settings table exactly. If a
/// libtorrent upgrade adds a setting, this fails until the name is
/// reviewed into the scalar catalog, a structured backing list, or
/// BLACKLIST.
#[test]
fn classification_partitions_all_settings() {
    let mut classified = HashSet::new();
    for &name in BLACKLIST {
        assert!(classified.insert(name), "{name} classified twice");
    }
    let mut fields = HashSet::new();
    for (field, backing) in backing() {
        assert!(fields.insert(field), "field {field} declared twice");
        for &name in backing {
            assert!(
                classified.insert(name),
                "{name} classified twice (via {field})"
            );
        }
    }

    let all: HashSet<&str> = all_settings().map(|(_, name, _)| name).collect();
    for &name in &classified {
        assert!(all.contains(name), "{name} is not a libtorrent setting");
    }
    for &name in &all {
        assert!(
            classified.contains(name),
            "{name} is unclassified: review it into the scalar catalog, a structured \
             backing list, or BLACKLIST"
        );
    }

    // The counts the catalog was reviewed at.
    assert_eq!(BLACKLIST.len(), 11);
    assert_eq!(catalog::SCALAR_BACKING.len(), 169);
    assert_eq!(structured::STRUCTURED_BACKING.len(), 15);
    assert_eq!(fields.len(), 184);
    let backed: usize = backing().map(|(_, b)| b.len()).sum();
    assert_eq!(backed, 210);
}

/// Scalar fields expose the libtorrent setting of the same name with the
/// matching kind, so the macro's field types cannot drift from the
/// generated table.
#[test]
fn scalar_fields_match_libtorrent_kinds() {
    use rbtorrent::SettingKind;
    let kinds: HashMap<&str, SettingKind> =
        all_settings().map(|(_, name, kind)| (name, kind)).collect();
    for (field, input) in catalog::scalar_single_field_inputs() {
        let delta = write_one({
            let mut i = undefined_input();
            i.scalars = input;
            i
        })
        .expect(field);
        let (key, _) = super::lt_setting(field);
        let staged_kind = match kinds[field] {
            SettingKind::Str => delta.get_str(key).is_some(),
            SettingKind::Int => delta.get_int(key).is_some(),
            SettingKind::Bool => delta.get_bool(key).is_some(),
        };
        assert!(staged_kind, "{field}: staged value has the wrong kind");
    }
}

// ---- backing agreement ------------------------------------------------------

/// Writing one field stages exactly that field's declared backing
/// settings — the partition test's tables describe real behavior.
#[test]
fn single_field_writes_stage_exactly_their_backing() {
    let table: HashMap<&str, &[&str]> = backing().collect();
    let mut cases: Vec<(&str, SettingsInput)> = Vec::new();
    for (field, scalars) in catalog::scalar_single_field_inputs() {
        let mut input = undefined_input();
        input.scalars = scalars;
        cases.push((field, input));
    }
    cases.extend(structured_single_field_inputs());
    assert_eq!(cases.len(), 184, "a field is missing a sample input");

    for (field, input) in cases {
        let delta = write_one(input).expect(field);
        let expected: HashSet<&str> = table[field].iter().copied().collect();
        for (key, name, _) in all_settings() {
            assert_eq!(
                delta.has(key),
                expected.contains(name),
                "{field}: unexpected staging state for {name}"
            );
        }
    }
}

/// A fully-populated input stages every non-blacklisted setting and no
/// blacklisted one, and the result roundtrips through read.
#[test]
fn full_write_stages_everything_and_roundtrips() {
    let before = read(&dense_defaults()).expect("read defaults");
    let delta = write_one(settings_to_input(before.clone())).expect("write");

    let blacklisted: HashSet<&str> = BLACKLIST.iter().copied().collect();
    for (key, name, _) in all_settings() {
        assert_eq!(
            delta.has(key),
            !blacklisted.contains(name),
            "unexpected staging state for {name}"
        );
    }

    let mut effective = dense_defaults();
    apply_delta(&mut effective, &delta);
    let after = read(&effective).expect("read after");
    assert_eq!(before, after);
}

/// A failing write may stage a prefix; the mutation discards the delta.
/// Within one group nothing is staged before validation passes.
#[test]
fn failed_group_write_stages_nothing() {
    let mut input = undefined_input();
    let mut bad = socks5_proxy();
    bad.port = 0;
    input.proxy = MaybeUndefined::Value(bad);
    let mut delta = SettingsPack::new();
    assert!(write(&mut delta, input).is_err());
    for (key, name, _) in all_settings() {
        assert!(!delta.has(key), "failed write staged {name}");
    }
}

// ---- schema agreement -------------------------------------------------------

/// `Settings` and `SettingsInput` expose exactly the camelCase forms of
/// the classified public fields — the GraphQL surface and the partition
/// tables cannot drift apart (this also proves the flattened scalar
/// fields made it into the schema).
#[tokio::test]
async fn schema_exposes_every_public_field() {
    let schema = async_graphql::Schema::build(
        crate::api::query::QueryRoot,
        crate::api::mutation::MutationRoot,
        crate::api::subscription::SubscriptionRoot,
    )
    .finish();
    let response = schema
        .execute(
            r#"{
                output: __type(name: "Settings") { fields { name } }
                input: __type(name: "SettingsInput") { inputFields { name } }
            }"#,
        )
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    let data = serde_json::to_value(&response.data).expect("json");

    let names = |value: &serde_json::Value, key: &str, list: &str| -> HashSet<String> {
        value[key][list]
            .as_array()
            .unwrap_or_else(|| panic!("{key}.{list} missing: {value}"))
            .iter()
            .map(|f| f["name"].as_str().expect("name").to_owned())
            .collect()
    };
    let expected: HashSet<String> = backing().map(|(field, _)| camel(field)).collect();
    assert_eq!(expected.len(), 184);
    assert_eq!(names(&data, "output", "fields"), expected);
    assert_eq!(names(&data, "input", "inputFields"), expected);
}

// ---- null and undefined semantics ---------------------------------------------

#[test]
fn explicit_null_is_rejected_outside_nullable_groups() {
    let mut input = undefined_input();
    input.scalars.upload_rate_limit = MaybeUndefined::Null;
    let msg = write_err(input);
    assert!(msg.contains("uploadRateLimit"), "{msg}");
    assert!(msg.contains("omit the field"), "{msg}");

    let mut input = undefined_input();
    input.encryption = MaybeUndefined::Null;
    assert!(write_err(input).contains("encryption"));
}

#[test]
fn undefined_input_stages_nothing() {
    let delta = write_one(undefined_input()).expect("empty write");
    for (key, name, _) in all_settings() {
        assert!(!delta.has(key), "empty input staged {name}");
    }
}

// ---- scalars ------------------------------------------------------------------

#[test]
fn scalar_roundtrip() {
    let mut input = undefined_input();
    input.scalars.upload_rate_limit = MaybeUndefined::Value(100_000);
    input.scalars.enable_dht = MaybeUndefined::Value(false);
    input.scalars.peer_fingerprint = MaybeUndefined::Value("-XX0001-".into());
    let effective = apply_input(input);
    let settings = read(&effective).expect("read");
    assert_eq!(settings.scalars.upload_rate_limit, 100_000);
    assert!(!settings.scalars.enable_dht);
    assert_eq!(settings.scalars.peer_fingerprint, "-XX0001-");
}

/// Value domains are enforced by rbtorrent's typed setters
/// (exhaustively tested there); here we only check the daemon-side
/// contract: a rejected value fails the whole delta with a camelCase
/// field message. (`torrentConnectBoost`, `announcePort` and
/// `numOptimisticUnchokeSlots` have no rejection cases left — their
/// field types make out-of-range values unrepresentable.)
#[test]
fn hazardous_scalar_fails_delta_with_field_message() {
    // filePoolSize: 0 violates a libtorrent assert; a negative size
    // hangs the file pool's eviction loop.
    for v in [-1, 0] {
        let mut input = undefined_input();
        input.scalars.file_pool_size = MaybeUndefined::Value(v);
        let msg = write_err(input);
        assert!(msg.contains("filePoolSize"), "{msg}");
        assert!(msg.contains("outside the valid range"), "{msg}");
    }

    // One bad value rejects the whole delta.
    let mut input = undefined_input();
    input.scalars.aio_threads = MaybeUndefined::Value(4);
    input.scalars.file_pool_size = MaybeUndefined::Value(-1);
    assert!(write_one(input).is_err());

    // Boundary values are accepted.
    let mut input = undefined_input();
    input.scalars.file_pool_size = MaybeUndefined::Value(1);
    input.scalars.aio_threads = MaybeUndefined::Value(1);
    input.scalars.hashing_threads = MaybeUndefined::Value(0);
    input.scalars.torrent_connect_boost = MaybeUndefined::Value(255);
    input.scalars.tick_interval = MaybeUndefined::Value(1000);
    input.scalars.dht_sample_infohashes_interval = MaybeUndefined::Value(21600);
    input.scalars.peer_dscp = MaybeUndefined::Value(63);
    input.scalars.utp_loss_multiplier = MaybeUndefined::Value(100);
    write_one(input).expect("boundary values are within the domains");
}

// ---- user_agent -----------------------------------------------------------------

#[test]
fn user_agent_identities() {
    // The libtorrent default reads back as LIBTORRENT.
    let defaults = read(&dense_defaults()).expect("read");
    assert_eq!(defaults.user_agent, UserAgent::Libtorrent);

    for (choice, expected) in [
        (UserAgent::None, String::new()),
        (
            UserAgent::Libtorrent,
            format!("libtorrent/{}", rbtorrent::libtorrent_version()),
        ),
        (
            UserAgent::Rsbtd,
            format!("rsbtd/{}", env!("CARGO_PKG_VERSION")),
        ),
        (
            UserAgent::QBittorrent,
            format!("qBittorrent/{QBITTORRENT_COMPAT_VERSION}"),
        ),
    ] {
        let mut input = undefined_input();
        input.user_agent = MaybeUndefined::Value(choice);
        let effective = apply_input(input);
        assert_eq!(effective.get_user_agent().unwrap(), expected, "{choice:?}");
        assert_eq!(read(&effective).expect("read").user_agent, choice);
    }

    // A value set outside this API reads back as UNRECOGNIZED (prefix
    // matching keeps version-stamped values recognizable)...
    let mut effective = dense_defaults();
    effective.user_agent("legacy/0.1");
    assert_eq!(
        read(&effective).expect("read").user_agent,
        UserAgent::Unrecognized
    );
    effective.user_agent("rsbtd/0.0.0-other-version");
    assert_eq!(read(&effective).expect("read").user_agent, UserAgent::Rsbtd);

    // ...and cannot be written back.
    let mut input = undefined_input();
    input.user_agent = MaybeUndefined::Value(UserAgent::Unrecognized);
    assert!(write_err(input).contains("UNRECOGNIZED"));
}

// ---- proxy -----------------------------------------------------------------------

#[test]
fn proxy_roundtrip_and_null() {
    let defaults = read(&dense_defaults()).expect("read");
    assert_eq!(defaults.proxy, None);

    let mut input = undefined_input();
    input.proxy = MaybeUndefined::Value(socks5_proxy());
    let effective = apply_input(input);
    let got = read(&effective).expect("read").proxy.expect("proxy set");
    assert_eq!(got.protocol, ProxyProtocol::Socks5);
    assert_eq!(got.hostname, "proxy.example");
    assert_eq!(got.port, 1080);
    assert_eq!(got.username, "");
    assert_eq!(got.password, "");
    assert!(got.resolve_hostnames);
    assert!(!got.socks5_udp_send_local_endpoint);
    assert!(!got.send_hostname_in_connect);

    // Readback includes the password.
    let mut with_auth = socks5_proxy();
    with_auth.protocol = ProxyProtocol::Socks5Password;
    with_auth.username = "u".into();
    with_auth.password = "secret".into();
    let mut input = undefined_input();
    input.proxy = MaybeUndefined::Value(with_auth);
    let got = read(&apply_input(input)).expect("read").proxy.expect("set");
    assert_eq!(got.password, "secret");

    // null disables the proxy and clears the backing fields to defaults.
    let mut input = undefined_input();
    input.proxy = MaybeUndefined::Value(socks5_proxy());
    let mut effective = dense_defaults();
    apply_delta(&mut effective, &write_one(input).expect("write"));
    let mut disable = undefined_input();
    disable.proxy = MaybeUndefined::Null;
    apply_delta(&mut effective, &write_one(disable).expect("write"));
    assert_eq!(read(&effective).expect("read").proxy, None);
    assert_eq!(effective.get_proxy_hostname().unwrap(), "");
    assert_eq!(effective.get_proxy_port().unwrap(), 0);
}

#[test]
fn proxy_validation() {
    let with_proxy = |f: &dyn Fn(&mut ProxySettings)| {
        let mut p = socks5_proxy();
        f(&mut p);
        let mut input = undefined_input();
        input.proxy = MaybeUndefined::Value(p);
        input
    };

    // Credentials required / forbidden per protocol.
    let msg = write_err(with_proxy(&|p| {
        p.protocol = ProxyProtocol::Socks4;
        p.resolve_hostnames = false;
    }));
    assert!(msg.contains("userid"), "{msg}");

    write_one(with_proxy(&|p| {
        p.protocol = ProxyProtocol::Socks4;
        p.resolve_hostnames = false;
        p.username = "u".into();
    }))
    .expect("socks4 with username");

    let msg = write_err(with_proxy(&|p| {
        p.protocol = ProxyProtocol::Socks4;
        p.resolve_hostnames = false;
        p.username = "u".into();
        p.password = "pw".into();
    }));
    assert!(msg.contains("does not use `password`"), "{msg}");

    let msg = write_err(with_proxy(&|p| p.username = "u".into()));
    assert!(msg.contains("does not use `username`"), "{msg}");

    let msg = write_err(with_proxy(&|p| {
        p.protocol = ProxyProtocol::HttpPassword;
        p.username = "u".into();
    }));
    assert!(msg.contains("password must not be empty"), "{msg}");

    // Protocol-specific booleans.
    let msg = write_err(with_proxy(&|p| p.send_hostname_in_connect = true));
    assert!(msg.contains("HTTP protocols"), "{msg}");

    let msg = write_err(with_proxy(&|p| {
        p.protocol = ProxyProtocol::Http;
        p.socks5_udp_send_local_endpoint = true;
    }));
    assert!(msg.contains("SOCKS5 protocols"), "{msg}");

    let msg = write_err(with_proxy(&|p| {
        p.protocol = ProxyProtocol::Socks4;
        p.username = "u".into();
    }));
    assert!(msg.contains("resolveHostnames"), "{msg}");

    // Value errors.
    let msg = write_err(with_proxy(&|p| p.port = 0));
    assert!(msg.contains("proxy.port"), "{msg}");
    let msg = write_err(with_proxy(&|p| p.hostname = String::new()));
    assert!(msg.contains("host must not be empty"), "{msg}");
}

// ---- i2p --------------------------------------------------------------------------

#[test]
fn i2p_roundtrip_and_null() {
    let defaults = read(&dense_defaults()).expect("read");
    assert_eq!(defaults.i2p, None);

    let mut input = undefined_input();
    input.i2p = MaybeUndefined::Value(i2p_config());
    let effective = apply_input(input);
    let got = read(&effective).expect("read").i2p.expect("i2p set");
    assert_eq!(got.hostname, "127.0.0.1");
    assert_eq!(got.port, 7656);
    assert!(!got.allow_mixed);
    assert_eq!(got.inbound.tunnels, 3);
    assert_eq!(got.inbound.hop_variance, 1);
    assert_eq!(got.outbound.hop_variance, 0);

    let mut effective = effective;
    let mut disable = undefined_input();
    disable.i2p = MaybeUndefined::Null;
    apply_delta(&mut effective, &write_one(disable).expect("write"));
    assert_eq!(read(&effective).expect("read").i2p, None);
    assert_eq!(effective.get_i2p_hostname().unwrap(), "");
}

#[test]
fn i2p_validation() {
    let with_i2p = |f: &dyn Fn(&mut I2pSettings)| {
        let mut c = i2p_config();
        f(&mut c);
        let mut input = undefined_input();
        input.i2p = MaybeUndefined::Value(c);
        input
    };
    assert!(write_err(with_i2p(&|c| c.inbound.tunnels = 0)).contains("1..=16"));
    assert!(write_err(with_i2p(&|c| c.outbound.hops = 8)).contains("0..=7"));
    assert!(write_err(with_i2p(&|c| c.inbound.hop_variance = -8)).contains("-7..=7"));
    assert!(write_err(with_i2p(&|c| c.hostname = String::new())).contains("SAM host"));
    assert!(write_err(with_i2p(&|c| c.port = 0)).contains("i2p.port"));
}

// ---- encryption ----------------------------------------------------------------------

#[test]
fn encryption_roundtrip_and_validation() {
    let mut input = undefined_input();
    input.encryption = MaybeUndefined::Value(EncryptionSettings {
        incoming: EncryptionPolicy::Forced,
        outgoing: EncryptionPolicy::Enabled,
        methods: EncryptionMethods {
            plaintext: false,
            rc4: true,
        },
        prefer_rc4: false,
        announce_support: true,
    });
    let got = read(&apply_input(input)).expect("read").encryption;
    assert_eq!(got.incoming, EncryptionPolicy::Forced);
    assert_eq!(got.outgoing, EncryptionPolicy::Enabled);
    assert!(!got.methods.plaintext);
    assert!(got.methods.rc4);

    // Defaults read back as a complete object.
    let _ = read(&dense_defaults()).expect("read").encryption;

    let with_enc = |plaintext: bool, rc4: bool, prefer_rc4: bool| {
        let mut input = undefined_input();
        input.encryption = MaybeUndefined::Value(EncryptionSettings {
            incoming: EncryptionPolicy::Enabled,
            outgoing: EncryptionPolicy::Enabled,
            methods: EncryptionMethods { plaintext, rc4 },
            prefer_rc4,
            announce_support: true,
        });
        input
    };
    assert!(write_err(with_enc(false, false, false)).contains("at least one"));
    assert!(write_err(with_enc(false, true, true)).contains("preferRc4"));
    write_one(with_enc(true, true, true)).expect("both methods, prefer rc4");
}

// ---- peer_transports --------------------------------------------------------------------

#[test]
fn peer_transports_roundtrip() {
    let mut input = undefined_input();
    input.peer_transports = MaybeUndefined::Value(PeerTransports {
        tcp: TransportDirections {
            incoming: true,
            outgoing: false,
        },
        utp: TransportDirections {
            incoming: false,
            outgoing: true,
        },
    });
    let got = read(&apply_input(input)).expect("read").peer_transports;
    assert!(got.tcp.incoming);
    assert!(!got.tcp.outgoing);
    assert!(!got.utp.incoming);
    assert!(got.utp.outgoing);
}

// ---- outgoing_port_range ------------------------------------------------------------------

#[test]
fn outgoing_port_range_mapping() {
    let defaults = read(&dense_defaults()).expect("read");
    assert_eq!(defaults.outgoing_port_range, None);

    let with_range = |first: u16, last: u16| {
        let mut input = undefined_input();
        input.outgoing_port_range = MaybeUndefined::Value(PortRange { first, last });
        input
    };

    // The backing mapping is first / last - first (inclusive iteration).
    let effective = apply_input(with_range(6881, 6889));
    assert_eq!(effective.get_outgoing_port(), Some(6881));
    assert_eq!(effective.get_num_outgoing_ports(), Some(8));
    let got = read(&effective)
        .expect("read")
        .outgoing_port_range
        .expect("set");
    assert_eq!((got.first, got.last), (6881, 6889));

    // Single-port range.
    let got = read(&apply_input(with_range(7000, 7000)))
        .expect("read")
        .outgoing_port_range
        .expect("set");
    assert_eq!((got.first, got.last), (7000, 7000));

    // null restores ephemeral ports.
    let mut effective = apply_input(with_range(6881, 6889));
    let mut disable = undefined_input();
    disable.outgoing_port_range = MaybeUndefined::Null;
    apply_delta(&mut effective, &write_one(disable).expect("write"));
    assert_eq!(effective.get_outgoing_port(), Some(0));
    assert_eq!(effective.get_num_outgoing_ports(), Some(0));
    assert_eq!(read(&effective).expect("read").outgoing_port_range, None);

    assert!(write_err(with_range(7000, 6999)).contains("empty"));
    assert!(write_err(with_range(0, 6999)).contains("must not start at port 0"));
}

// ---- disk I/O ------------------------------------------------------------------------------

#[test]
fn disk_io_cache_and_mmap_write_mode() {
    let mut input = undefined_input();
    input.disk_io_cache = MaybeUndefined::Value(DiskIoCache {
        read: IoReadMode::DisableOsCache,
        write: IoWriteMode::WriteThrough,
    });
    input.mmap_write_mode = MaybeUndefined::Value(MmapWriteMode::AlwaysPwrite);
    let got = read(&apply_input(input)).expect("read");
    assert_eq!(got.disk_io_cache.read, IoReadMode::DisableOsCache);
    assert_eq!(got.disk_io_cache.write, IoWriteMode::WriteThrough);
    assert_eq!(got.mmap_write_mode, MmapWriteMode::AlwaysPwrite);
}

#[test]
fn integer_enum_settings_roundtrip() {
    let mut input = undefined_input();
    input.suggest_mode = MaybeUndefined::Value(SuggestMode::ReadCache);
    input.choking_algorithm = MaybeUndefined::Value(ChokingAlgorithm::RateBased);
    input.seed_choking_algorithm = MaybeUndefined::Value(SeedChokingAlgorithm::AntiLeech);
    input.mixed_mode_algorithm = MaybeUndefined::Value(MixedModeAlgorithm::PeerProportional);
    let got = read(&apply_input(input)).expect("read");
    assert_eq!(got.suggest_mode, SuggestMode::ReadCache);
    assert_eq!(got.choking_algorithm, ChokingAlgorithm::RateBased);
    assert_eq!(got.seed_choking_algorithm, SeedChokingAlgorithm::AntiLeech);
    assert_eq!(
        got.mixed_mode_algorithm,
        MixedModeAlgorithm::PeerProportional
    );
}

// ---- structured strings -----------------------------------------------------------------------

#[test]
fn outgoing_interfaces_list() {
    let defaults = read(&dense_defaults()).expect("read");
    assert_eq!(defaults.outgoing_interfaces, Vec::<String>::new());

    let with_list = |list: Vec<String>| {
        let mut input = undefined_input();
        input.outgoing_interfaces = MaybeUndefined::Value(list);
        input
    };

    // IPv6 addresses are bare here (libtorrent brackets only
    // listen_interfaces entries, which carry a port).
    let effective = apply_input(with_list(vec![
        "eth0".into(),
        "10.0.0.4".into(),
        "2001:db8::1".into(),
    ]));
    assert_eq!(
        effective.get_outgoing_interfaces().unwrap(),
        "eth0,10.0.0.4,2001:db8::1"
    );
    assert_eq!(
        read(&effective).expect("read").outgoing_interfaces,
        vec!["eth0", "10.0.0.4", "2001:db8::1"]
    );

    // Empty list restores the default (OS-chosen route).
    let mut effective = apply_input(with_list(vec!["eth0".into()]));
    apply_delta(
        &mut effective,
        &write_one(with_list(Vec::new())).expect("write"),
    );
    assert_eq!(effective.get_outgoing_interfaces().unwrap(), "");

    assert!(write_err(with_list(vec![String::new()])).contains("empty"));
    assert!(write_err(with_list(vec!["a,b".into()])).contains("commas"));
    assert!(write_err(with_list(vec!["[2001:db8::1]".into()])).contains("IPv6"));
    assert!(write_err(with_list(vec!["not:an:address".into()])).contains("IPv6"));
}

#[test]
fn listen_interfaces_list() {
    let with_list = |list: Vec<ListenInterface>| {
        let mut input = undefined_input();
        input.listen_interfaces = MaybeUndefined::Value(list);
        input
    };
    let effective = apply_input(with_list(vec![
        ListenInterface {
            interface: "0.0.0.0".into(),
            port: 6881,
            ssl: false,
            local: false,
        },
        ListenInterface {
            interface: "[::]".into(),
            port: 6882,
            ssl: true,
            local: true,
        },
    ]));
    assert_eq!(
        effective.get_listen_interfaces().unwrap(),
        "0.0.0.0:6881,[::]:6882sl"
    );
    let got = read(&effective).expect("read").listen_interfaces;
    assert_eq!(got.len(), 2);
    assert_eq!(got[0].interface, "0.0.0.0");
    assert_eq!(got[0].port, 6881);
    assert!(!got[0].ssl);
    assert_eq!(got[1].interface, "[::]");
    assert!(got[1].ssl);
    assert!(got[1].local);

    // Port 0 = ephemeral is allowed.
    let effective = apply_input(with_list(vec![ListenInterface {
        interface: "127.0.0.1".into(),
        port: 0,
        ssl: false,
        local: false,
    }]));
    assert_eq!(effective.get_listen_interfaces().unwrap(), "127.0.0.1:0");

    let msg = write_err(with_list(vec![ListenInterface {
        interface: "::1".into(),
        port: 6881,
        ssl: false,
        local: false,
    }]));
    assert!(msg.contains("IPv6"), "{msg}");
}

#[test]
fn dht_bootstrap_nodes_list() {
    // The libtorrent default parses.
    let defaults = read(&dense_defaults()).expect("read");
    assert!(defaults.dht_bootstrap_nodes.iter().all(|n| n.port > 0));

    let with_nodes = |nodes: Vec<HostPort>| {
        let mut input = undefined_input();
        input.dht_bootstrap_nodes = MaybeUndefined::Value(nodes);
        input
    };
    let effective = apply_input(with_nodes(vec![
        HostPort {
            hostname: "router.example.com".into(),
            port: 6881,
        },
        HostPort {
            hostname: "10.1.2.3".into(),
            port: 25401,
        },
    ]));
    assert_eq!(
        effective.get_dht_bootstrap_nodes().unwrap(),
        "router.example.com:6881,10.1.2.3:25401"
    );
    let got = read(&effective).expect("read").dht_bootstrap_nodes;
    assert_eq!(got.len(), 2);
    assert_eq!(got[0].hostname, "router.example.com");
    assert_eq!(got[1].port, 25401);

    // Empty list = no bootstrap nodes.
    let effective = apply_input(with_nodes(Vec::new()));
    assert_eq!(effective.get_dht_bootstrap_nodes().unwrap(), "");
    assert_eq!(
        read(&effective).expect("read").dht_bootstrap_nodes,
        Vec::new()
    );

    assert!(
        write_err(with_nodes(vec![HostPort {
            hostname: "x".into(),
            port: 0,
        }]))
        .contains("1..=65535")
    );
}
