# Copyright (C) 2026  namazso <admin@namazso.eu>
# SPDX-License-Identifier: MPL-2.0
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.

# Builder image for rsbtd RPMs (used by packaging/build-rpm.sh).
#
# Oracle Linux 10 with the distro clang/LLVM toolchain and a
# rustup-pinned Rust. The pins are deliberate: cross-language LTO feeds
# rustc-emitted LLVM bitcode to the system lld, so rustc's bundled LLVM
# and the distro clang/lld must be the same LLVM major (both 21 here).
# The RUN below fails the image build if they ever drift apart.
FROM container-registry.oracle.com/os/oraclelinux:10

ARG LLVM_VERSION=21.1.8
ARG RUST_VERSION=1.94.0

# ninja-build lives in the CodeReady Builder repo.
RUN dnf -y --setopt=install_weak_deps=0 --enablerepo=ol10_codeready_builder install \
        clang-${LLVM_VERSION} \
        clang-devel-${LLVM_VERSION} \
        llvm-${LLVM_VERSION} \
        lld-${LLVM_VERSION} \
        cmake \
        ninja-build \
        boost-devel \
        openssl-devel \
        rpm-build \
        redhat-rpm-config \
        systemd-rpm-macros \
    && dnf clean all

# Rust via rustup: OL10's rust package lags the workspace's pinned
# toolchain (rust-toolchain.toml). clippy/rustfmt are preinstalled so
# rustup does not have to fetch them when it honors the toolchain file.
ENV RUSTUP_HOME=/opt/rust/rustup \
    CARGO_HOME=/opt/rust/cargo \
    PATH=/opt/rust/cargo/bin:${PATH}

RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
        | sh -s -- -y --no-modify-path --profile minimal \
              --default-toolchain ${RUST_VERSION} \
              --component clippy --component rustfmt \
    && rustc -vV \
    && rust_llvm="$(rustc -vV | sed -n 's/^LLVM version: //p')" \
    && test "${rust_llvm%%.*}" = "${LLVM_VERSION%%.*}" || { \
           echo "LLVM major mismatch: rustc ${rust_llvm} vs clang ${LLVM_VERSION};" \
                "cross-language LTO needs matching majors" >&2; exit 1; }
