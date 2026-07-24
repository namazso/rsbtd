// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Domain tests for the validating setters, transcribed from
//! `docs/libtorrent-settings-constraints.md` (the FORBIDDEN quick
//! reference and the per-area sections). Every checked setter appears in
//! [`checked_domains`] with its full accepted range; forbidden and
//! boundary values are derived from it.
//!
//! FORBIDDEN rows from the constraints doc with no test here are the
//! ones the type system makes unrepresentable:
//! - `torrent_connect_boost` (`u8`), `announce_port` (`u16`),
//!   `num_optimistic_unchoke_slots` (`u16`) — negative / wrapping values
//!   cannot be expressed.
//! - `seed_choking_algorithm = 3` (assert-fail enumerator),
//!   `choking_algorithm = 1` (undefined enumerator), `proxy_type > 6`
//!   (connect-time throw), `out_enc_policy`/`in_enc_policy > 2`,
//!   `allowed_enc_level` out of range, and the `suggest_mode`/disk-mode
//!   enums — the setters take closed Rust enums.

use super::*;

type Set = fn(&mut SettingsPack, i32) -> Result<(), SettingsError>;

/// `(setting, min, max, stored max, setter)` for every checked integer
/// setter. The stored value differs from the accepted maximum only for
/// setters that re-encode (`peer_dscp` stores the traffic-class byte).
fn checked_domains() -> Vec<(&'static str, i32, i32, i32, Set)> {
    macro_rules! row {
        ($name:ident, $min:expr, $max:expr) => {
            row!($name, $min, $max, stores $max)
        };
        ($name:ident, $min:expr, $max:expr, stores $stored:expr) => {
            (
                stringify!($name),
                $min,
                $max,
                $stored,
                (|p, v| p.$name(v).map(|_| ())) as Set,
            )
        };
    }
    vec![
        // Peer connections, timeouts, peer list (doc area 1).
        row!(peer_timeout, 1, 536_870_911),
        row!(handshake_timeout, 1, 536_870_911),
        row!(peer_connect_timeout, 1, 2_147_482_623),
        row!(min_reconnect_time, 0, 65_535),
        row!(max_failcount, 1, 16_383),
        row!(peer_turnover, 0, 100),
        row!(max_peerlist_size, 0, 22_605_091),
        row!(max_paused_peerlist_size, 0, 22_605_091),
        row!(max_peer_recv_buffer_size, 16_384, i32::MAX),
        row!(peer_dscp, 0, 63, stores 63 << 2),
        row!(alert_queue_size, 1, i32::MAX),
        // Bandwidth, choking, send buffers, uTP (doc area 2).
        row!(send_buffer_watermark, 1, i32::MAX),
        row!(utp_target_delay, 1, 2_147_483),
        row!(utp_connect_timeout, 1, i32::MAX),
        row!(utp_loss_multiplier, 0, 100),
        // Disk I/O, storage, hashing, metadata (doc area 3).
        row!(max_queued_disk_bytes, 0, i32::MAX),
        row!(max_suggest_pieces, 1, i32::MAX),
        row!(max_piece_count, 1, 1_073_741_823),
        row!(max_allowed_in_request_queue, 0, i32::MAX),
        row!(metadata_token_limit, 1, i32::MAX),
        row!(max_metadata_size, 1, i32::MAX),
        row!(hashing_threads, 0, 1_073_741_823),
        row!(checking_mem_usage, 1, 131_071),
        row!(file_pool_size, 1, i32::MAX),
        row!(aio_threads, 1, i32::MAX),
        // Trackers, announces (doc area 4).
        row!(max_concurrent_http_announces, 1, i32::MAX),
        row!(tracker_backoff, 0, 26_630),
        row!(tracker_completion_timeout, 1, i32::MAX),
        row!(tracker_receive_timeout, 1, i32::MAX),
        // DHT, LSD, UPnP (doc area 5).
        row!(dht_max_dht_items, 1, i32::MAX),
        row!(dht_announce_interval, 1, 2_147_483),
        row!(local_service_announce_interval, 1, 2_147_483),
        row!(upnp_lease_duration, 0, 715_827_882),
        row!(dht_max_peers_reply, 0, i32::MAX),
        row!(dht_search_branching, 1, 127),
        row!(dht_sample_infohashes_interval, 0, 21_600),
        // Alerts, tick, resolver (doc area 6).
        row!(tick_interval, 1, 1000),
    ]
}

#[test]
fn every_forbidden_value_is_rejected_and_stages_nothing() {
    let domains = checked_domains();
    assert_eq!(domains.len(), 37, "checked-setter table drifted");
    for (name, min, max, _, set) in domains {
        let mut pack = SettingsPack::new();
        let mut forbidden = Vec::new();
        if min > i32::MIN {
            forbidden.push(min - 1);
            forbidden.push(i32::MIN);
        }
        if max < i32::MAX {
            forbidden.push(max + 1);
            forbidden.push(i32::MAX);
        }
        assert!(!forbidden.is_empty(), "{name}: whole-i32 domain in table");
        for v in forbidden {
            let err = set(&mut pack, v).expect_err(name);
            assert_eq!(err.setting(), name);
            assert!(
                err.message().contains("outside the valid range"),
                "{name}: {}",
                err.message()
            );
        }
        // A rejected value must not have been staged.
        let key = setting_by_name(name).expect(name);
        assert!(!pack.has(key), "{name}: rejected value was staged");
    }
}

#[test]
fn boundary_values_accepted() {
    for (name, min, max, stored_max, set) in checked_domains() {
        let mut pack = SettingsPack::new();
        set(&mut pack, min).expect(name);
        set(&mut pack, max).expect(name);
        let key = setting_by_name(name).expect(name);
        assert_eq!(pack.get_int(key), Some(stored_max), "{name}");
    }
}

#[test]
fn libtorrent_defaults_are_in_domain() {
    let defaults = SettingsPack::defaults();
    for (name, min, max, _, _) in checked_domains() {
        let key = setting_by_name(name).expect(name);
        let v = defaults.get_int(key).expect(name);
        assert!(
            (min..=max).contains(&v),
            "{name}: default {v} out of domain"
        );
    }
}

#[test]
fn settings_error_display_and_conversion() {
    let mut pack = SettingsPack::new();
    let err = pack.tick_interval(0).expect_err("out of domain");
    assert_eq!(err.setting(), "tick_interval");
    assert_eq!(err.message(), "0 is outside the valid range 1..=1000");
    assert_eq!(
        err.to_string(),
        "tick_interval: 0 is outside the valid range 1..=1000"
    );
    let as_crate: crate::Error = err.into();
    assert_eq!(as_crate.category(), crate::Category::Bindings);
}

#[test]
fn proxy_group_roundtrip_and_reset() {
    let mut pack = SettingsPack::new();
    let config = ProxyConfig {
        protocol: ProxyProtocol::Socks5 {
            auth: Some(Credentials {
                username: "user".into(),
                password: "pw".into(),
            }),
            resolve_hostnames: true,
            udp_send_local_endpoint: false,
        },
        host: "proxy.example".into(),
        port: std::num::NonZeroU16::new(1080).unwrap(),
        peer_connections: true,
        tracker_connections: false,
    };
    pack.proxy(Some(&config)).expect("valid proxy");
    assert_eq!(pack.get_proxy_type(), Some(ProxyType::Socks5Pw));
    assert_eq!(pack.get_proxy(), Some(Some(config)));

    pack.proxy(None).expect("reset");
    assert_eq!(pack.get_proxy(), Some(None));
    assert_eq!(pack.get_proxy_hostname().as_deref(), Some(""));
}

#[test]
fn proxy_ipv6_hosts_are_stored_bare() {
    let mut pack = SettingsPack::new();
    let mut config = ProxyConfig {
        protocol: ProxyProtocol::Socks5 {
            auth: None,
            resolve_hostnames: false,
            udp_send_local_endpoint: false,
        },
        host: "::1".into(),
        port: std::num::NonZeroU16::new(1080).unwrap(),
        peer_connections: true,
        tracker_connections: true,
    };
    pack.proxy(Some(&config)).expect("bare IPv6");
    assert_eq!(pack.get_proxy_hostname().as_deref(), Some("::1"));

    // Bracketed input is normalized: libtorrent's resolver only parses
    // bare literals.
    config.host = "[2001:db8::7]".into();
    pack.proxy(Some(&config)).expect("bracketed IPv6");
    assert_eq!(pack.get_proxy_hostname().as_deref(), Some("2001:db8::7"));

    config.host = "[::1".into();
    assert!(pack.proxy(Some(&config)).is_err());
    config.host = "]::1[".into();
    assert!(pack.proxy(Some(&config)).is_err());
    config.host = "fe80::1%eth0".into();
    pack.proxy(Some(&config)).expect("scoped bare IPv6");
}

#[test]
fn proxy_group_rejections() {
    let mut pack = SettingsPack::new();
    let base = ProxyConfig {
        protocol: ProxyProtocol::Socks4 {
            username: String::new(),
        },
        host: "proxy.example".into(),
        port: std::num::NonZeroU16::new(1080).unwrap(),
        peer_connections: true,
        tracker_connections: true,
    };
    let err = pack.proxy(Some(&base)).expect_err("empty SOCKS4 userid");
    assert_eq!(err.setting(), "proxy");

    let mut bad_host = base.clone();
    bad_host.protocol = ProxyProtocol::Socks4 {
        username: "u".into(),
    };
    bad_host.host = "bad host".into();
    assert!(pack.proxy(Some(&bad_host)).is_err());

    let mut empty_auth = base.clone();
    empty_auth.protocol = ProxyProtocol::Http {
        auth: Some(Credentials {
            username: "u".into(),
            password: String::new(),
        }),
        resolve_hostnames: false,
        send_hostname_in_connect: false,
    };
    assert!(pack.proxy(Some(&empty_auth)).is_err());

    let long = "x".repeat(256);
    let mut long_creds = base;
    long_creds.protocol = ProxyProtocol::Socks5 {
        auth: Some(Credentials {
            username: long.clone(),
            password: "pw".into(),
        }),
        resolve_hostnames: false,
        udp_send_local_endpoint: false,
    };
    assert!(pack.proxy(Some(&long_creds)).is_err());
    // HTTP basic auth has no such limit.
    let mut http_long = ProxyConfig {
        protocol: ProxyProtocol::Http {
            auth: Some(Credentials {
                username: long,
                password: "pw".into(),
            }),
            resolve_hostnames: false,
            send_hostname_in_connect: false,
        },
        host: "proxy.example".into(),
        port: std::num::NonZeroU16::new(8080).unwrap(),
        peer_connections: true,
        tracker_connections: true,
    };
    pack.proxy(Some(&http_long)).expect("long HTTP creds");
    http_long.protocol = ProxyProtocol::Http {
        auth: None,
        resolve_hostnames: false,
        send_hostname_in_connect: false,
    };
    pack.proxy(Some(&http_long)).expect("no auth");
    // A rejected group stages nothing on a fresh pack.
    let mut fresh = SettingsPack::new();
    let _ = fresh.proxy(Some(&bad_host));
    assert_eq!(fresh.get_proxy_type(), None);
}

#[test]
fn proxy_getter_is_lenient_on_foreign_values() {
    let mut pack = SettingsPack::new();
    // proxy_type = 6 (i2p_proxy) is outside the modeled enum.
    pack.set_int(setting_by_name("proxy_type").unwrap(), 6);
    assert_eq!(pack.get_proxy(), None);
    // A zero port with a real proxy type does not fit the model either.
    let mut zero_port = SettingsPack::new();
    zero_port.set_int(setting_by_name("proxy_type").unwrap(), 2);
    zero_port.set_int(setting_by_name("proxy_port").unwrap(), 0);
    zero_port.set_str(setting_by_name("proxy_hostname").unwrap(), "h");
    zero_port.set_str(setting_by_name("proxy_username").unwrap(), "");
    zero_port.set_str(setting_by_name("proxy_password").unwrap(), "");
    zero_port.set_bool(setting_by_name("proxy_hostnames").unwrap(), true);
    assert_eq!(zero_port.get_proxy(), None);
}

#[test]
fn i2p_group_roundtrip_and_rejections() {
    let mut pack = SettingsPack::new();
    let config = I2pConfig {
        sam_host: "127.0.0.1".into(),
        sam_port: std::num::NonZeroU16::new(7656).unwrap(),
        inbound: I2pTunnels {
            quantity: 3,
            length: 3,
            variance: 1,
        },
        outbound: I2pTunnels {
            quantity: 3,
            length: 3,
            variance: -1,
        },
        allow_mixed: false,
    };
    pack.i2p(Some(&config)).expect("valid i2p");
    assert_eq!(pack.get_i2p(), Some(Some(config.clone())));

    pack.i2p(None).expect("reset");
    assert_eq!(pack.get_i2p(), Some(None));

    for bad in [
        I2pConfig {
            inbound: I2pTunnels {
                quantity: 0,
                ..config.inbound
            },
            ..config.clone()
        },
        I2pConfig {
            outbound: I2pTunnels {
                length: 8,
                ..config.outbound
            },
            ..config.clone()
        },
        I2pConfig {
            inbound: I2pTunnels {
                variance: -8,
                ..config.inbound
            },
            ..config.clone()
        },
        I2pConfig {
            sam_host: "not a host".into(),
            ..config.clone()
        },
    ] {
        let err = pack.i2p(Some(&bad)).expect_err("out of SAM domain");
        assert_eq!(err.setting(), "i2p");
    }
}

#[test]
fn outgoing_ports_group() {
    let mut pack = SettingsPack::new();
    pack.outgoing_ports(Some(6881..=6889)).expect("valid");
    assert_eq!(pack.get_outgoing_port(), Some(6881));
    assert_eq!(pack.get_num_outgoing_ports(), Some(8));
    assert_eq!(pack.get_outgoing_ports(), Some(Some(6881..=6889)));

    pack.outgoing_ports(None).expect("reset");
    assert_eq!(pack.get_outgoing_ports(), Some(None));

    assert!(pack.outgoing_ports(Some(0..=100)).is_err());
    #[allow(clippy::reversed_empty_ranges)]
    let err = pack.outgoing_ports(Some(7000..=6999)).expect_err("empty");
    assert_eq!(err.setting(), "outgoing_ports");
}

#[test]
fn listen_and_outgoing_interfaces_groups() {
    let mut pack = SettingsPack::new();
    let endpoints = [
        ListenEndpoint::new("0.0.0.0", 6881),
        ListenEndpoint {
            addr: "[::]".into(),
            port: 0,
            ssl: true,
            local: true,
        },
    ];
    pack.listen_interfaces(&endpoints).expect("valid");
    assert_eq!(
        pack.get_listen_interfaces().as_deref(),
        Some("0.0.0.0:6881,[::]:0sl")
    );
    assert_eq!(
        pack.get_listen_interfaces_parsed(),
        Some(endpoints.to_vec())
    );
    assert!(
        pack.listen_interfaces(&[ListenEndpoint::new("a,b", 1)])
            .is_err()
    );
    assert!(
        pack.listen_interfaces(&[ListenEndpoint::new("::1", 1)])
            .is_err()
    );
    assert!(
        pack.listen_interfaces(&[ListenEndpoint::new("[bad:literal]", 1)])
            .is_err()
    );

    pack.outgoing_interfaces(&["eth0", "10.0.0.1", "fe80::1%eth0"])
        .expect("valid");
    assert_eq!(
        pack.get_outgoing_interfaces().as_deref(),
        Some("eth0,10.0.0.1,fe80::1%eth0")
    );
    // Bare-IPv6 rule: bracketed forms and non-address colons rejected.
    assert!(pack.outgoing_interfaces(&["[::1]"]).is_err());
    let empty: [&str; 0] = [];
    pack.outgoing_interfaces(&empty).expect("reset");
    assert_eq!(pack.get_outgoing_interfaces().as_deref(), Some(""));
}

#[test]
fn dht_bootstrap_nodes_group() {
    let mut pack = SettingsPack::new();
    let nodes = [
        HostPort {
            host: "router.bittorrent.com".into(),
            port: std::num::NonZeroU16::new(6881).unwrap(),
        },
        HostPort {
            host: "[2001:db8::1]".into(),
            port: std::num::NonZeroU16::new(6881).unwrap(),
        },
    ];
    pack.dht_bootstrap_nodes(&nodes).expect("valid");
    assert_eq!(
        pack.get_dht_bootstrap_nodes().as_deref(),
        Some("router.bittorrent.com:6881,[2001:db8::1]:6881")
    );
    assert_eq!(pack.get_dht_bootstrap_nodes_parsed(), Some(nodes.to_vec()));
    assert!(
        pack.dht_bootstrap_nodes(&[HostPort {
            host: "spaced host".into(),
            port: std::num::NonZeroU16::new(1).unwrap(),
        }])
        .is_err()
    );
    // Bracketed tokens must hold a real IPv6 literal (scopes allowed).
    let node = |host: &str| HostPort {
        host: host.into(),
        port: std::num::NonZeroU16::new(1).unwrap(),
    };
    assert!(pack.dht_bootstrap_nodes(&[node("[not-an-ip]")]).is_err());
    assert!(pack.dht_bootstrap_nodes(&[node("[]")]).is_err());
    assert!(pack.dht_bootstrap_nodes(&[node("[::1")]).is_err());
    pack.dht_bootstrap_nodes(&[node("[fe80::1%eth0]")])
        .expect("scoped IPv6 literal");
    pack.dht_bootstrap_nodes(&[]).expect("reset");
    assert_eq!(pack.get_dht_bootstrap_nodes().as_deref(), Some(""));
}

#[test]
fn peer_fingerprint_length_rule() {
    let mut pack = SettingsPack::new();
    pack.peer_fingerprint("-XX0001-").expect("short is fine");
    pack.peer_fingerprint(&"x".repeat(19)).expect("19 is fine");
    let err = pack
        .peer_fingerprint(&"x".repeat(20))
        .expect_err("20 bytes is the deterministic peer id");
    assert_eq!(err.setting(), "peer_fingerprint");
}
