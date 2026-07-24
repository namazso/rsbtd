#!/usr/bin/env bash
# Copyright (C) 2026  namazso <admin@namazso.eu>
# SPDX-License-Identifier: MPL-2.0
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.

# Build the rsbtd production container image from the RPMs.
#
# The image's build context is staged from dist/rpms (built by
# build-rpm.sh first if missing): only the rsbtd/rsbtctl binary RPMs and
# the entrypoint script -- the RPMs are the sole build input.
#
# Environment:
#   CONTAINER_ENGINE   podman (default) or docker
#   RSBTD_ARCH         target architecture, x86_64 or aarch64 (default:
#                      host); selects which RPMs are staged and, for a
#                      cross build, runs the image build through qemu
#                      binfmt emulation
#   RSBTD_IMAGE        image tag (default localhost/rsbtd:<version>)
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="$(dirname "$here")"
engine="${CONTAINER_ENGINE:-podman}"
out="$root/dist"

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
platform="linux/$oci_arch"

version="$(sed -n '/^\[workspace\.package\]/,/^\[/s/^version *= *"\([^"]*\)".*/\1/p' \
               "$root/Cargo.toml" | head -n1)"
image="${RSBTD_IMAGE:-localhost/rsbtd:$version}"

# The image is tagged with the current workspace version, so only RPMs
# of exactly that version may go in — an RPM of some older build must
# not satisfy the check or be staged.
if ! compgen -G "$out/rpms/rsbtd-$version-*.$arch.rpm" >/dev/null; then
    "$here/build-rpm.sh"
fi

# Exactly one RPM may match: several releases of the same version in
# dist/rpms would otherwise be staged together silently.
pick_rpm() {
    local matches
    matches=$(compgen -G "$1") || {
        echo "error: no RPM matches $1 (run packaging/build-rpm.sh)" >&2
        exit 1
    }
    if [ "$(wc -l <<<"$matches")" -ne 1 ]; then
        {
            echo "error: multiple RPMs match $1:"
            sed 's/^/  /' <<<"$matches"
            echo "remove the stale releases from dist/rpms"
        } >&2
        exit 1
    fi
    printf '%s\n' "$matches"
}

daemon_rpm="$(pick_rpm "$out/rpms/rsbtd-$version-*.$arch.rpm")"
client_rpm="$(pick_rpm "$out/rpms/rsbtctl-$version-*.$arch.rpm")"
webui_rpm="$(pick_rpm "$out/rpms/rsbtd-webui-$version-*.noarch.rpm")"

context="$out/image-context"
rm -rf "$context"
mkdir -p "$context/rpms"
# exact binary RPMs only: no debuginfo/debugsource, no SRPM
cp "$daemon_rpm" "$client_rpm" "$webui_rpm" "$context/rpms/"
cp "$here/container-entrypoint.sh" "$context/"

echo "==> Building $image ($arch)"
"$engine" build --platform "$platform" \
    -t "$image" -f "$here/Containerfile" "$context"
echo "==> Built $image"
