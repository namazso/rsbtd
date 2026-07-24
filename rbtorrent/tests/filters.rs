// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use rbtorrent::{IpFilter, PortFilter, Session, SessionParams, SettingsPack};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

#[test]
fn ip_filter_basic() {
    let mut filter = IpFilter::new().unwrap();

    assert!(filter.is_empty().unwrap());

    let first = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 0));
    let last = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 255));
    filter.add_rule(first, last, true).unwrap();

    assert!(!filter.is_empty().unwrap());

    let blocked_addr = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 100));
    assert!(filter.access(blocked_addr).unwrap());

    let allowed_addr = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1));
    assert!(!filter.access(allowed_addr).unwrap());

    let ranges = filter.export().unwrap();
    assert!(!ranges.is_empty());
}

#[test]
fn ip_filter_ipv6() {
    let mut filter = IpFilter::new().unwrap();

    let first = IpAddr::V6(Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 0));
    let last = IpAddr::V6(Ipv6Addr::new(
        0x2001, 0xdb8, 0, 0, 0xffff, 0xffff, 0xffff, 0xffff,
    ));
    filter.add_rule(first, last, true).unwrap();

    let blocked = IpAddr::V6(Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 1));
    assert!(filter.access(blocked).unwrap());

    let allowed = IpAddr::V6(Ipv6Addr::new(0x2001, 0xdb9, 0, 0, 0, 0, 0, 1));
    assert!(!filter.access(allowed).unwrap());
}

#[test]
fn ip_filter_rejects_invalid_ranges() {
    let mut filter = IpFilter::new().unwrap();

    // Ascending range is accepted
    filter
        .add_rule(
            IpAddr::V4(Ipv4Addr::new(10, 0, 0, 0)),
            IpAddr::V4(Ipv4Addr::new(10, 0, 0, 255)),
            true,
        )
        .unwrap();

    // Single-address range (first == last) is accepted
    filter
        .add_rule(
            IpAddr::V4(Ipv4Addr::new(10, 1, 0, 1)),
            IpAddr::V4(Ipv4Addr::new(10, 1, 0, 1)),
            true,
        )
        .unwrap();

    // Reversed IPv4 range is rejected
    assert!(
        filter
            .add_rule(
                IpAddr::V4(Ipv4Addr::new(192, 168, 1, 255)),
                IpAddr::V4(Ipv4Addr::new(192, 168, 1, 0)),
                true,
            )
            .is_err()
    );

    // Reversed IPv6 range is rejected
    assert!(
        filter
            .add_rule(
                IpAddr::V6(Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 0xffff)),
                IpAddr::V6(Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 0)),
                true,
            )
            .is_err()
    );

    // Mixed address families are rejected (both orders)
    assert!(
        filter
            .add_rule(
                IpAddr::V4(Ipv4Addr::new(10, 0, 0, 0)),
                IpAddr::V6(Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 1)),
                true,
            )
            .is_err()
    );
    assert!(
        filter
            .add_rule(
                IpAddr::V6(Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 1)),
                IpAddr::V4(Ipv4Addr::new(10, 0, 0, 0)),
                true,
            )
            .is_err()
    );

    // The filter still only contains the valid rules
    let blocked = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 42));
    assert!(filter.access(blocked).unwrap());
    let allowed = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 100));
    assert!(!filter.access(allowed).unwrap());
}

#[test]
fn port_filter_rejects_reversed_range() {
    let mut filter = PortFilter::new().unwrap();

    filter.add_rule(1000, 2000, true).unwrap();
    filter.add_rule(3000, 3000, true).unwrap();

    assert!(filter.add_rule(2000, 1000, true).is_err());

    assert!(filter.access(1500).unwrap());
    assert!(filter.access(3000).unwrap());
    assert!(!filter.access(2500).unwrap());
}

#[test]
fn port_filter_basic() {
    let mut filter = PortFilter::new().unwrap();

    filter.add_rule(0, 1023, true).unwrap();

    assert!(filter.access(80).unwrap());
    assert!(filter.access(443).unwrap());

    assert!(!filter.access(8080).unwrap());
    assert!(!filter.access(50000).unwrap());
}

#[tokio::test]
async fn session_ip_filter_integration() {
    let mut settings = SettingsPack::new();
    settings
        .enable_dht(false)
        .enable_lsd(false)
        .enable_upnp(false)
        .enable_natpmp(false)
        .listen_interfaces(&[rbtorrent::ListenEndpoint::new("127.0.0.1", 0)])
        .unwrap();

    let params = SessionParams::new().settings(&settings);
    let session = Session::new(params).unwrap();

    let mut filter = IpFilter::new().unwrap();
    let first = IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0));
    let last = IpAddr::V4(Ipv4Addr::new(255, 255, 255, 255));
    filter.add_rule(first, last, true).unwrap();

    session.set_ip_filter(&filter).unwrap();

    let retrieved = session.get_ip_filter().unwrap();
    let ranges = retrieved.export().unwrap();

    // Should have at least the IPv4 block rule
    assert!(ranges.iter().any(|(f, l, blocked)| {
        matches!(f, IpAddr::V4(_)) && matches!(l, IpAddr::V4(_)) && *blocked
    }));

    session.close().await;
}

#[tokio::test]
async fn session_port_filter_integration() {
    let mut settings = SettingsPack::new();
    settings
        .enable_dht(false)
        .enable_lsd(false)
        .enable_upnp(false)
        .enable_natpmp(false)
        .listen_interfaces(&[rbtorrent::ListenEndpoint::new("127.0.0.1", 0)])
        .unwrap();

    let params = SessionParams::new().settings(&settings);
    let session = Session::new(params).unwrap();

    let mut filter = PortFilter::new().unwrap();
    filter.add_rule(0, 1023, true).unwrap();

    session.set_port_filter(&filter).unwrap();

    // Verify filter was applied (no errors)
    assert!(filter.access(80).unwrap());
    assert!(!filter.access(8080).unwrap());

    session.close().await;
}
