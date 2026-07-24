# Copyright (C) 2026  namazso <admin@namazso.eu>
# SPDX-License-Identifier: MPL-2.0
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.

# rsbtd RPM spec. Built by packaging/build-rpm.sh inside the Oracle
# Linux 10 builder container (packaging/Containerfile.builder): distro
# clang/lld/llvm 21.1.8, Rust 1.94.0 via rustup. Rust comes from rustup
# rather than an RPM, hence no BuildRequires on rust/cargo.
#
# The whole native stack -- vendored libtorrent, the libctorrent shim,
# and every Rust crate -- is compiled to LLVM bitcode and merged by lld
# in one full ("fat") cross-language LTO link per binary. All of that
# configuration is local to this spec (plus packaging/lto-toolchain.cmake);
# a plain `cargo build --features vendored` elsewhere is unaffected.

# Clang-safe hardened build flags (no gcc-only -specs/annobin).
%global toolchain clang

# rpmbuild injects %%_lto_cflags into the CFLAGS/CXXFLAGS it exports in
# every section (%%set_build_flags or not), and the plain %%check test
# links cannot digest LLVM bitcode archives. Keep the macro nil and opt
# into full LTO explicitly in %%build, the one section that wants it.
%global _lto_cflags %{nil}

# dwz gains little on these two large LTO'd binaries and costs minutes.
%global _find_debuginfo_dwz_opts %{nil}

# One combined debuginfo package instead of per-binary ones: rpm's
# per-subpackage debuginfo filter keeps only build-id-matched
# /usr/lib/debug paths, which would strip the license files the debug
# packages must carry like every other published package.
%undefine _debuginfo_subpackages

# The auto-generated debuginfo/debugsource manifests are the lists
# find-debuginfo writes into the build directory. Stage one shared
# license dir in the buildroot and append %%license entries to both
# lists (identical paths may be packaged by several subpackages of one
# spec). %%define, not %%global: the guard must expand at use, after
# %%debug_package defines __debug_package.
%define __spec_install_post \
    %{?__debug_package:%{__debug_install_post}} \
    %{__arch_install_post} \
    %{__os_install_post} \
    %{?__debug_package:%{__debug_license_post}} \
%{nil}

%define __debug_license_post \
    install -Dpm0644 LICENSE THIRD-PARTY-NOTICES.md -t %{buildroot}%{_defaultlicensedir}/%{name}-debug; \
    for l in debugfiles.list debugsourcefiles.list; do \
        [ -f "$l" ] || continue; \
        echo "%%dir %{_defaultlicensedir}/%{name}-debug" >> "$l"; \
        echo "%%license %{_defaultlicensedir}/%{name}-debug/LICENSE" >> "$l"; \
        echo "%%license %{_defaultlicensedir}/%{name}-debug/THIRD-PARTY-NOTICES.md" >> "$l"; \
    done \
%{nil}

# Toolchain selection shared by %%build and %%check (each rpm section
# runs in its own shell): both cargo invocations drive CMake through
# the cmake crate, and the rpm-injected CFLAGS are clang-only
# (--config=redhat-hardened-clang.cfg) -- the gcc that dnf drags into
# the image anyway could not consume them.
%global build_env \
export CC=clang \
export CXX=clang++ \
export CMAKE_GENERATOR=Ninja

# `rpmbuild --without check` (build-rpm.sh: RSBTD_RPM_SKIP_CHECK=1)
# skips the test suite for quick local packaging iteration.
%bcond_without check

# Version/release are normally injected by packaging/build-rpm.sh from
# the workspace Cargo.toml (and CI may add a snapshot release suffix).
%{!?rsbtd_version:%define rsbtd_version 1.0.0}
%{!?rsbtd_release:%define rsbtd_release 1}

Name:           rsbtd
Version:        %{rsbtd_version}
Release:        %{rsbtd_release}%{?dist}
Summary:        BitTorrent client daemon with a GraphQL API
# License of rsbtd itself; the statically linked libtorrent is BSD-3-Clause.
License:        MPL-2.0 AND BSD-3-Clause
URL:            https://github.com/namazso/rsbtd
Source0:        rsbtd-%{version}.tar.gz
Source1:        rsbtd-webui-%{version}.tar.gz

BuildRequires:  clang
BuildRequires:  clang-devel
BuildRequires:  llvm
BuildRequires:  lld
BuildRequires:  cmake >= 3.25
BuildRequires:  ninja-build
BuildRequires:  boost-devel
BuildRequires:  openssl-devel
BuildRequires:  systemd-rpm-macros

# User creation: sysusers.d file (rpm >= 4.19 creates the user natively)
# with a scriptlet fallback for environments where that is inert.
Requires(pre):  shadow-utils

%description
A BitTorrent client daemon built on libtorrent-rasterbar 2.1 (statically
linked, built from the pinned vendored sources), controlled entirely
over a GraphQL HTTP API, with support for serving a web UI.

%package -n rsbtctl
Summary:        Command-line client for the rsbtd daemon

%description -n rsbtctl
A oneshot command-line client for the rsbtd BitTorrent daemon, covering
day-to-day operations for scripts and quick checks.

%package webui
Summary:        Web UI for the rsbtd daemon
BuildArch:      noarch
# No hard Requires: the dist is plain static files, equally servable by
# rsbtd (api.serve_root) or any web server / reverse proxy.

%description webui
The prebuilt rsbtd web UI, installed to %{_datadir}/rsbtd/webui. Point
api.serve_root in /etc/rsbtd/rsbtd.toml at it to have the daemon serve
its own UI on GET /.

%prep
# Source0 is the git tree (incl. submodules) with no top-level prefix;
# -c creates the build directory. Source1 is the built web UI, extracted
# alongside it under webui-dist/.
%autosetup -c -a 1

%build
%set_build_flags
%{build_env}

# --- Fat cross-language LTO ---------------------------------------------
# C/C++ (vendored libtorrent + libctorrent, both driven as CMake projects
# by libctorrent-sys's build.rs): CC/CXX above and the -flto=full
# appended here (%%_lto_cflags is nil, see top) reach both projects via
# the cmake crate. The toolchain file swaps in llvm-ar/llvm-ranlib so
# the static archives of bitcode get a usable symbol index. (cc-rs
# notices linker-plugin-lto in RUSTFLAGS and injects its own -flto=thin,
# but our -flto=full comes later on the command line and the last -flto
# wins in clang.)
export CFLAGS="$CFLAGS -flto=full"
export CXXFLAGS="$CXXFLAGS -flto=full"
export CMAKE_TOOLCHAIN_FILE="$PWD/packaging/lto-toolchain.cmake"
export AR=llvm-ar
export RANLIB=llvm-ranlib

# Rust: -Clinker-plugin-lto makes rustc emit LLVM bitcode instead of
# machine code; the final link runs through clang -fuse-ld=lld, where
# lld's LTO merges the Rust bitcode with the C++ bitcode archives and
# optimizes the whole program at once (cross-language inlining included).
# rustc 1.94 and clang here are both LLVM 21, which is what makes the
# bitcode interchangeable (guarded in Containerfile.builder).
export RUSTFLAGS="-Clinker-plugin-lto -Clinker=clang -Clink-arg=-fuse-ld=lld -Clink-arg=-flto=full -Clink-arg=-Wl,--build-id=sha1"

# Fat LTO on the Rust side too (rustc merges the crate graph before the
# linker sees it) and one codegen unit per crate; both keep the LTO
# configuration out of the workspace's Cargo.toml.
export CARGO_PROFILE_RELEASE_LTO=fat
export CARGO_PROFILE_RELEASE_CODEGEN_UNITS=1
# -------------------------------------------------------------------------

cargo build --release --locked --features vendored -p rsbtd -p rsbtctl

%install
install -Dpm0755 target/release/rsbtd %{buildroot}%{_bindir}/rsbtd
install -Dpm0755 target/release/rsbtctl %{buildroot}%{_bindir}/rsbtctl

# Config is root:rsbtd 0640: it can carry the API bearer token.
install -Dpm0640 rsbtd/etc/rsbtd.toml %{buildroot}%{_sysconfdir}/rsbtd/rsbtd.toml

# The unit in the tree points at a `cargo build` install location.
install -Dpm0644 rsbtd/etc/rsbtd.service %{buildroot}%{_unitdir}/rsbtd.service
sed -i 's|/usr/local/bin/rsbtd|%{_bindir}/rsbtd|' %{buildroot}%{_unitdir}/rsbtd.service

install -Dpm0644 packaging/rsbtd.sysusers %{buildroot}%{_sysusersdir}/rsbtd.conf

install -dm0750 %{buildroot}%{_sharedstatedir}/rsbtd

# Web UI (Source1). Artifact-derived files can carry arbitrary modes;
# normalize.
install -dm0755 %{buildroot}%{_datadir}/rsbtd/webui
cp -a webui-dist/. %{buildroot}%{_datadir}/rsbtd/webui/
find %{buildroot}%{_datadir}/rsbtd/webui -type d -exec chmod 0755 {} +
find %{buildroot}%{_datadir}/rsbtd/webui -type f -exec chmod 0644 {} +

%check
%if %{with check}
%{build_env}
# Full workspace test suite. Deliberately NOT the fat-LTO configuration
# from %%build: LTO'ing the tests makes every one of the ~20 test
# binary links re-run LTO codegen over all of libtorrent, which dwarfs
# the build itself. With %%_lto_cflags nil (see top) the flags rpmbuild
# injects here carry no -flto, so this is the same plain dev-profile
# build the CI test matrix runs -- in its own target dir, because
# build.rs does not watch CFLAGS/RUSTFLAGS and sharing target/ with
# %%build would link its stale bitcode archives.
export CARGO_TARGET_DIR="$PWD/target-check"
cargo test --workspace --locked --features vendored
%endif

%pre
# Fallback for installs where rpm's native sysusers.d handling is not in
# effect; harmless duplicate of it otherwise.
getent group rsbtd >/dev/null || groupadd -r rsbtd
getent passwd rsbtd >/dev/null || \
    useradd -r -g rsbtd -d %{_sharedstatedir}/rsbtd -s /usr/sbin/nologin \
        -c "rsbtd torrent daemon" rsbtd
exit 0

%post
%systemd_post rsbtd.service

%preun
%systemd_preun rsbtd.service

%postun
%systemd_postun_with_restart rsbtd.service

%files
%license LICENSE THIRD-PARTY-NOTICES.md
%doc README.md
%{_bindir}/rsbtd
%dir %{_sysconfdir}/rsbtd
%config(noreplace) %attr(0640,root,rsbtd) %{_sysconfdir}/rsbtd/rsbtd.toml
%{_unitdir}/rsbtd.service
%{_sysusersdir}/rsbtd.conf
%dir %attr(0750,rsbtd,rsbtd) %{_sharedstatedir}/rsbtd

%files -n rsbtctl
%license LICENSE THIRD-PARTY-NOTICES.md
%{_bindir}/rsbtctl

%files webui
%license LICENSE THIRD-PARTY-NOTICES.md
%dir %{_datadir}/rsbtd
%{_datadir}/rsbtd/webui/

%changelog
* Thu Jul 23 2026 rsbtd maintainers <noreply@github.com> - 1.0.0-4
- Ship LICENSE and THIRD-PARTY-NOTICES.md in the debuginfo/debugsource
  packages too, merging the per-binary debuginfo packages into one
  (rpm's per-subpackage filter would strip the license files).

* Sun Jul 19 2026 rsbtd maintainers <noreply@github.com> - 1.0.0-3
- Ship LICENSE and THIRD-PARTY-NOTICES.md (scripts/gen_notices.py) as
  %%license in every package.

* Sun Jul 19 2026 rsbtd maintainers <noreply@github.com> - 1.0.0-2
- Keep LTO out of %%check: %%_lto_cflags is now nil and %%build opts in
  explicitly. rpmbuild injects the macro into every section's flags, so
  the %%check C/C++ archives were LLVM bitcode that the default
  cc/bfd-ld test links cannot read (broke the aarch64 leg; x86_64 only
  worked via rustc's rust-lld default).

* Sat Jul 18 2026 rsbtd maintainers <noreply@github.com> - 1.0.0-1
- Version 1.0.0.

* Sat Jul 18 2026 rsbtd maintainers <noreply@github.com> - 0.1.0-2
- Run the full workspace test suite in %%check (dev profile, own
  target dir; skippable with --without check).
- New noarch rsbtd-webui subpackage: the prebuilt web UI at
  /usr/share/rsbtd/webui (Source1, staged by build-rpm.sh).

* Fri Jul 17 2026 rsbtd maintainers <noreply@github.com> - 0.1.0-1
- Initial packaging: rsbtd + rsbtctl, vendored libtorrent, fat
  cross-language LTO (clang/lld + rustc linker-plugin LTO).
