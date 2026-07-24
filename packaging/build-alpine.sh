#!/usr/bin/env bash
# Copyright (C) 2026  namazso <admin@namazso.eu>
# SPDX-License-Identifier: MPL-2.0
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.

# CI-only Alpine image for rsbtd: system libtorrent (no vendored
# feature), Alpine's current rust/cargo, default gcc, no LTO -- the
# deliberate opposite corner from the OL10 RPM build. The builder stage
# also runs the full workspace test suite. Not published anywhere.
#
# Environment:
#   CONTAINER_ENGINE          podman (default) or docker
#   RSBTD_ARCH                target architecture, x86_64 or aarch64
#                             (default: host); a non-host value builds
#                             through qemu binfmt emulation
#   RSBTD_WEBUI_DIST          built web UI to bake in (default webui/dist)
#   RSBTD_ALPINE_IMAGE        image tag (default localhost/rsbtd:alpine)
#   RSBTD_ALPINE_SKIP_TESTS   1 skips the test suite in the builder
#                             stage -- for slow emulated local runs
#                             only; CI always runs it
#   CARGO_BUILD_JOBS          forwarded into the build; bound it on
#                             memory-constrained hosts
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="$(dirname "$here")"
engine="${CONTAINER_ENGINE:-podman}"
image="${RSBTD_ALPINE_IMAGE:-localhost/rsbtd:alpine}"
webui_dist="${RSBTD_WEBUI_DIST:-$root/webui/dist}"

host_arch="$(uname -m)"
arch="${RSBTD_ARCH:-$host_arch}"
case "$arch" in
    x86_64)  oci_arch=amd64 ;;
    aarch64) oci_arch=arm64 ;;
    *) echo "error: unsupported RSBTD_ARCH '$arch' (x86_64 or aarch64)" >&2
       exit 1 ;;
esac
# Always explicit (--platform works on both podman and docker): without
# it, a native run could silently reuse a cached foreign-arch base image
# left behind by an earlier cross build.
build_flags=(--platform "linux/$oci_arch")
[ "${RSBTD_ALPINE_SKIP_TESTS:-0}" = 1 ] && build_flags+=(--build-arg SKIP_TESTS=1)
[ -n "${CARGO_BUILD_JOBS:-}" ] && build_flags+=(--build-arg "JOBS=$CARGO_BUILD_JOBS")

if [ ! -f "$webui_dist/index.html" ]; then
    echo "error: $webui_dist does not look like a built web UI (no index.html);" \
         "run: (cd webui && npm ci && npm run build), or set RSBTD_WEBUI_DIST" >&2
    exit 1
fi

context="$root/dist/alpine-context"
rm -rf "$context"
mkdir -p "$context/source"

# Git-tracked sources only. vendor/ is not needed (system libtorrent);
# rust-toolchain.toml must not leak a pin into the apk-rust build.
git -C "$root" ls-files -z |
    grep -zv -e '^vendor/' -e '^rust-toolchain\.toml$' |
    tar --null -C "$root" --files-from=- -cf - |
    tar -C "$context/source" -xf -
cp -a "$webui_dist" "$context/webui-dist"

echo "==> Building $image ($arch; compiles rsbtd and runs the full test suite)"
"$engine" build ${build_flags[@]+"${build_flags[@]}"} \
    -t "$image" -f "$here/Containerfile.alpine" "$context"
echo "==> Built $image"
