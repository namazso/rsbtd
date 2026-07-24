// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! IP and port filtering: wrappers for libtorrent's `ip_filter` and
//! `port_filter`, controlling which addresses and ports the session will
//! connect to or accept connections from.

use crate::error::{Error, Result};
use libctorrent_sys as sys;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

/// IP address filter: a set of rules categorizing addresses as allowed or
/// blocked. A fresh filter allows all addresses.
pub struct IpFilter {
    pub(crate) ptr: *mut sys::ct_ip_filter,
}

impl IpFilter {
    /// Create a new IP filter that allows all addresses by default.
    pub fn new() -> Result<Self> {
        crate::error::with_error(|err| unsafe { sys::ct_ip_filter_new(err) })
            .map(|ptr| IpFilter { ptr })
    }

    /// Returns true if the filter does not contain any rules.
    pub fn is_empty(&self) -> Result<bool> {
        crate::error::with_error(|err| unsafe { sys::ct_ip_filter_empty(self.ptr, err) })
    }

    /// Add a rule blocking or allowing the inclusive range
    /// `first..=last`. Errors unless both endpoints are the same IP
    /// version and `first <= last`. On overlap, the last rule applied
    /// takes precedence.
    pub fn add_rule(&mut self, first: IpAddr, last: IpAddr, blocked: bool) -> Result<()> {
        match (first, last) {
            (IpAddr::V4(f), IpAddr::V4(l)) => {
                if f > l {
                    return Err(Error::binding("IP filter range is descending"));
                }
            }
            (IpAddr::V6(f), IpAddr::V6(l)) => {
                if f > l {
                    return Err(Error::binding("IP filter range is descending"));
                }
            }
            _ => {
                return Err(Error::binding(
                    "IP filter range endpoints must be the same IP version",
                ));
            }
        }

        let first_ct = ip_to_ct_address(first);
        let last_ct = ip_to_ct_address(last);
        let flags = if blocked {
            sys::CT_IP_FILTER_BLOCKED
        } else {
            0
        };

        crate::error::with_error(|err| unsafe {
            sys::ct_ip_filter_add_rule(self.ptr, &first_ct, &last_ct, flags, err)
        })
    }

    /// Returns true if the address is blocked, false if allowed.
    pub fn access(&self, addr: IpAddr) -> Result<bool> {
        let ct_addr = ip_to_ct_address(addr);
        crate::error::with_error(|err| unsafe { sys::ct_ip_filter_access(self.ptr, &ct_addr, err) })
            .map(|flags| flags & sys::CT_IP_FILTER_BLOCKED != 0)
    }

    /// Export the filter as a minimal set of non-overlapping
    /// `(first, last, blocked)` ranges.
    pub fn export(&self) -> Result<Vec<(IpAddr, IpAddr, bool)>> {
        let mut len: usize = 0;
        let ptr = crate::error::with_error(|err| unsafe {
            sys::ct_ip_filter_export(self.ptr, &mut len, err)
        })?;

        if ptr.is_null() {
            return Ok(Vec::new());
        }

        let ranges = unsafe { std::slice::from_raw_parts(ptr, len) };
        let result: Vec<_> = ranges
            .iter()
            .map(|r| {
                let first = ct_address_to_ip(&r.first);
                let last = ct_address_to_ip(&r.last);
                let blocked = r.flags & sys::CT_IP_FILTER_BLOCKED != 0;
                (first, last, blocked)
            })
            .collect();

        unsafe { sys::ct_ip_filter_export_free(ptr) };
        Ok(result)
    }

    pub(crate) fn as_ptr(&self) -> *const sys::ct_ip_filter {
        self.ptr
    }
}

impl Drop for IpFilter {
    fn drop(&mut self) {
        unsafe { sys::ct_ip_filter_free(self.ptr) };
    }
}

unsafe impl Send for IpFilter {}
unsafe impl Sync for IpFilter {}

impl Default for IpFilter {
    fn default() -> Self {
        Self::new().expect("Failed to create default IpFilter")
    }
}

/// Port filter: marks destination port ranges that should not be
/// connected to. A fresh filter allows all ports.
pub struct PortFilter {
    ptr: *mut sys::ct_port_filter,
}

impl PortFilter {
    /// Create a new port filter that allows all ports by default.
    pub fn new() -> Result<Self> {
        crate::error::with_error(|err| unsafe { sys::ct_port_filter_new(err) })
            .map(|ptr| PortFilter { ptr })
    }

    /// Add a rule blocking or allowing the inclusive port range
    /// `first..=last`. Errors if `first > last`.
    pub fn add_rule(&mut self, first: u16, last: u16, blocked: bool) -> Result<()> {
        if first > last {
            return Err(Error::binding("port filter range is descending"));
        }
        let flags = if blocked {
            sys::CT_PORT_FILTER_BLOCKED
        } else {
            0
        };

        crate::error::with_error(|err| unsafe {
            sys::ct_port_filter_add_rule(self.ptr, first, last, flags, err)
        })
    }

    /// Returns true if the port is blocked, false if allowed.
    pub fn access(&self, port: u16) -> Result<bool> {
        crate::error::with_error(|err| unsafe { sys::ct_port_filter_access(self.ptr, port, err) })
            .map(|flags| flags & sys::CT_PORT_FILTER_BLOCKED != 0)
    }

    pub(crate) fn as_ptr(&self) -> *const sys::ct_port_filter {
        self.ptr
    }
}

impl Drop for PortFilter {
    fn drop(&mut self) {
        unsafe { sys::ct_port_filter_free(self.ptr) };
    }
}

unsafe impl Send for PortFilter {}
unsafe impl Sync for PortFilter {}

impl Default for PortFilter {
    fn default() -> Self {
        Self::new().expect("Failed to create default PortFilter")
    }
}

fn ip_to_ct_address(addr: IpAddr) -> sys::ct_address {
    let mut ct_addr = sys::ct_address {
        bytes: [0u8; 16],
        port: 0,
        is_v6: 0,
        _pad: [0u8; 5],
    };

    match addr {
        IpAddr::V4(v4) => {
            ct_addr.is_v6 = 0;
            ct_addr.bytes[..4].copy_from_slice(&v4.octets());
        }
        IpAddr::V6(v6) => {
            ct_addr.is_v6 = 1;
            ct_addr.bytes.copy_from_slice(&v6.octets());
        }
    }

    ct_addr
}

fn ct_address_to_ip(ct_addr: &sys::ct_address) -> IpAddr {
    if ct_addr.is_v6 != 0 {
        let mut bytes = [0u8; 16];
        bytes.copy_from_slice(&ct_addr.bytes);
        IpAddr::V6(Ipv6Addr::from(bytes))
    } else {
        let mut bytes = [0u8; 4];
        bytes.copy_from_slice(&ct_addr.bytes[..4]);
        IpAddr::V4(Ipv4Addr::from(bytes))
    }
}
