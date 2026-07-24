#!/usr/bin/env bash
# Copyright (C) 2026  namazso <admin@namazso.eu>
# SPDX-License-Identifier: MPL-2.0
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.

# Containerized RPM build for rsbtd.
#
# Builds everything -- the vendored libtorrent, the libctorrent shim and
# the Rust workspace -- inside an Oracle Linux 10 builder image
# (packaging/Containerfile.builder: clang/lld 21.1.8, Rust 1.94.0) with
# full cross-language LTO, and produces RPMs (running the workspace test
# suite in %check). Used both locally and by .github/workflows/ci.yml.
#
# Output:
#   dist/rpms/          rsbtd, rsbtctl, rsbtd-webui (+ debuginfo/
#                       debugsource, SRPM)
#   dist/rpmbuild/      rpm _topdir (build tree; kept for debugging)
#
# Environment:
#   CONTAINER_ENGINE      podman (default) or docker
#   RSBTD_ARCH            target architecture, x86_64 or aarch64
#                         (default: host). A non-host value runs the
#                         whole build through qemu binfmt emulation --
#                         slow, but exactly the CI aarch64 leg; used
#                         for local testing.
#   RSBTD_BUILDER_IMAGE   use an existing builder image instead of
#                         building localhost/rsbtd-builder:ol10-<arch>
#   RSBTD_CARGO_CACHE     host dir for the cargo registry cache
#                         (default dist/cargo-cache; cached in CI)
#   RSBTD_RPM_RELEASE     RPM Release (default 1; CI sets a snapshot
#                         suffix for non-tag builds)
#   RSBTD_WEBUI_DIST      built web UI to package (default webui/dist)
#   RSBTD_RPM_SKIP_CHECK  1 skips the test suite (%check) for quick
#                         local packaging iteration
#   CARGO_BUILD_JOBS      forwarded into the container; bound it on
#                         memory-constrained hosts (rustc + clang at
#                         full parallelism can OOM)
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="$(dirname "$here")"
engine="${CONTAINER_ENGINE:-podman}"

# Target architecture (rpm naming); mapped to the container engine's
# GOARCH-style name. Every build/run gets an explicit --platform (the
# flag both podman and docker support): without it, a native run could
# silently reuse a cached foreign-arch base image left behind by an
# earlier cross build. A cross platform runs via qemu binfmt.
host_arch="$(uname -m)"
arch="${RSBTD_ARCH:-$host_arch}"
case "$arch" in
    x86_64)  oci_arch=amd64 ;;
    aarch64) oci_arch=arm64 ;;
    *) echo "error: unsupported RSBTD_ARCH '$arch' (x86_64 or aarch64)" >&2
       exit 1 ;;
esac
platform="linux/$oci_arch"

image="${RSBTD_BUILDER_IMAGE:-localhost/rsbtd-builder:ol10-$arch}"
out="$root/dist"
topdir="$out/rpmbuild"
cache="${RSBTD_CARGO_CACHE:-$out/cargo-cache}"

if [ ! -f "$root/vendor/libtorrent/CMakeLists.txt" ] ||
   [ ! -f "$root/vendor/libtorrent/deps/try_signal/CMakeLists.txt" ]; then
    echo "error: vendor/libtorrent is missing or incomplete;" \
         "run: git submodule update --init --recursive" >&2
    exit 1
fi

webui_dist="${RSBTD_WEBUI_DIST:-$root/webui/dist}"
if [ ! -f "$webui_dist/index.html" ]; then
    echo "error: $webui_dist does not look like a built web UI (no index.html);" \
         "run: (cd webui && npm ci && npm run build), or set RSBTD_WEBUI_DIST" >&2
    exit 1
fi

version="$(sed -n '/^\[workspace\.package\]/,/^\[/s/^version *= *"\([^"]*\)".*/\1/p' \
               "$root/Cargo.toml" | head -n1)"
if [ -z "$version" ]; then
    echo "error: cannot read workspace.package.version from Cargo.toml" >&2
    exit 1
fi

if [ -z "${RSBTD_BUILDER_IMAGE:-}" ]; then
    echo "==> Building builder image $image ($arch)"
    "$engine" build --platform "$platform" \
        -t "$image" -f "$here/Containerfile.builder" "$here"
fi

rm -rf "$topdir"
mkdir -p "$topdir"/{SOURCES,SPECS,RPMS,SRPMS,BUILD} "$cache" "$out/rpms"

# Reproducible source tarballs: force every mtime to the HEAD commit
# date (this also normalizes the web UI dist, whose files carry
# checkout/artifact-download times), sort entries, use numeric root
# ownership, strip the non-deterministic pax headers, and keep gzip
# from recording a timestamp (-n).
SOURCE_DATE_EPOCH="$(git -C "$root" log -1 --format=%ct)"
tar_repro=(--sort=name --mtime="@$SOURCE_DATE_EPOCH" --owner=0 --group=0
    --numeric-owner
    --pax-option=exthdr.name=%d/PaxHeaders/%f,delete=atime,delete=ctime)

echo "==> Creating source tarball (git-tracked files incl. submodules)"
git -C "$root" ls-files -z --recurse-submodules |
    tar --null -C "$root" --files-from=- "${tar_repro[@]}" -cf - |
    gzip -n > "$topdir/SOURCES/rsbtd-$version.tar.gz"
cp "$here/rsbtd.spec" "$topdir/SPECS/"

echo "==> Creating web UI tarball (Source1) from $webui_dist"
webui_stage="$topdir/webui-stage"
mkdir -p "$webui_stage"
cp -a "$webui_dist" "$webui_stage/webui-dist"
tar -C "$webui_stage" "${tar_repro[@]}" -cf - webui-dist |
    gzip -n > "$topdir/SOURCES/rsbtd-webui-$version.tar.gz"
rm -rf "$webui_stage"

check_flag=()
[ "${RSBTD_RPM_SKIP_CHECK:-0}" = 1 ] && check_flag=(--without check)

jobs_flag=()
[ -n "${CARGO_BUILD_JOBS:-}" ] && jobs_flag=(-e "CARGO_BUILD_JOBS=$CARGO_BUILD_JOBS")

# A rootful engine (docker's default, or sudo podman) runs container
# root as real root, leaving root-owned trees in the bind-mounted
# dist/rpmbuild and cargo cache that later host-side runs cannot clean;
# build as the invoking user there. This must stay detection, not the
# default: under a rootless engine container root already maps to the
# invoking user, and --user would map to a subordinate uid instead.
# rpmbuild itself is uid-agnostic but needs a writable HOME.
run_as=()
rootless=$("$engine" info --format '{{.Host.Security.Rootless}}' 2>/dev/null || true)
if [ "$rootless" != true ]; then
    security=$("$engine" info --format '{{.SecurityOptions}}' 2>/dev/null || true)
    case "$security" in
        *rootless*) ;;
        *) run_as=(--user "$(id -u):$(id -g)" -e HOME=/tmp) ;;
    esac
fi

echo "==> Building RPMs (rsbtd $version-${RSBTD_RPM_RELEASE:-1}, $arch)"
"$engine" run --rm \
    --platform "$platform" \
    -v "$topdir:/work/rpmbuild:Z" \
    -v "$cache:/cache/cargo:Z" \
    -e CARGO_HOME=/cache/cargo \
    ${run_as[@]+"${run_as[@]}"} \
    ${jobs_flag[@]+"${jobs_flag[@]}"} \
    "$image" \
    rpmbuild -ba ${check_flag[@]+"${check_flag[@]}"} \
        --define "_topdir /work/rpmbuild" \
        --define "rsbtd_version $version" \
        --define "rsbtd_release ${RSBTD_RPM_RELEASE:-1}" \
        /work/rpmbuild/SPECS/rsbtd.spec

# Replace only this arch's RPMs (plus the arch-independent noarch/SRPM,
# rebuilt every run) so a local multi-arch sequence accumulates in
# dist/rpms instead of clobbering the other architecture.
rm -f "$out/rpms"/*."$arch".rpm "$out/rpms"/*.noarch.rpm "$out/rpms"/*.src.rpm
cp "$topdir"/RPMS/*/*.rpm "$topdir"/SRPMS/*.rpm "$out/rpms/"

echo "==> RPMs in $out/rpms:"
ls -lh "$out/rpms"
