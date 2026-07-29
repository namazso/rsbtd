# Copyright (C) 2026  namazso <admin@namazso.eu>
# SPDX-License-Identifier: MPL-2.0
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.

# rsbtd + rsbtctl against the flake's shared libtorrent-rasterbar 2.1
# (strict CTORRENT_LIBTORRENT_PREFIX override, no `vendored` feature).
# Reproduces the RPM spec's fat cross-language LTO (packaging/rsbtd.spec)
# over the shim and the Rust crate closure; the shared libtorrent stays
# outside it. Only works while the rust-toolchain.toml rustc and
# llvmPackages_21 share an LLVM major -- keep them in lockstep.
{
  lib,
  llvmPackages,
  rustPlatform,
  cmake,
  boost,
  openssl,
  libtorrent-rasterbar,
}:

let
  # cc-rs consults CC_<underscored-triple> before HOST_CC; see preBuild.
  rustTriple = lib.replaceStrings [ "-" ] [
    "_"
  ] llvmPackages.stdenv.hostPlatform.rust.rustcTarget;
in
# rustPlatform must be clang-based: gcc can neither emit nor link bitcode.
rustPlatform.buildRustPackage {
  pname = "rsbtd";
  version = (lib.importTOML ../Cargo.toml).workspace.package.version;

  src = lib.fileset.toSource {
    root = ../.;
    fileset = lib.fileset.unions [
      ../Cargo.toml
      ../Cargo.lock
      ../libctorrent
      ../libctorrent-sys
      ../rbtorrent
      ../rsbtd
      ../rsbtctl
      ../packaging/rsbtd.sysusers
      ../LICENSE
      ../THIRD-PARTY-NOTICES.md
      ../README.md
    ];
  };

  cargoLock.lockFile = ../Cargo.lock;

  nativeBuildInputs = [
    cmake
    rustPlatform.bindgenHook
    # llvm-ar/llvm-ranlib for the bitcode archives (see AR/RANLIB below)
    llvmPackages.llvm
  ];

  # cmake here is for the two projects build.rs drives, not the source root.
  dontUseCmakeConfigure = true;

  buildInputs = [
    libtorrent-rasterbar
    boost
    openssl
  ];

  env.CTORRENT_LIBTORRENT_PREFIX = "${libtorrent-rasterbar}";

  cargoBuildFlags = [
    "-p"
    "rsbtd"
    "-p"
    "rsbtctl"
  ];
  cargoTestFlags = [ "--workspace" ];

  # The LTO environment, mirroring the spec's %build. The cargo hooks
  # prefix cargo with compiler vars from nixpkgs' *default* (gcc) stdenv:
  # HOST_CC and CARGO_TARGET_*_LINKER. The per-target CC_/CXX_ vars outrank
  # HOST_CC in cc-rs and -Clinker outranks the injected linker, so crate C
  # code (e.g. ring's) and the final link stay on the clang/lld the bitcode
  # needs.
  preBuild = ''
    export CFLAGS="-flto=full" CXXFLAGS="-flto=full"
    export CMAKE_TOOLCHAIN_FILE="${../packaging/lto-toolchain.cmake}"
    export AR=llvm-ar RANLIB=llvm-ranlib
    export CC_${rustTriple}="$CC" CXX_${rustTriple}="$CXX"
    export RUSTFLAGS="-Clinker-plugin-lto -Clinker=clang -Clink-arg=-flto=full"
    export CARGO_PROFILE_RELEASE_LTO=fat
    export CARGO_PROFILE_RELEASE_CODEGEN_UNITS=1
  '';

  # Tests drop the LTO configuration like the spec's %check: each of the
  # ~20 test binary links would re-run whole-program codegen. Own target
  # dir, absolute (see the .gitignore note on trybuild) -- build.rs does
  # not watch these flags, so sharing target/ would link the LTO build's
  # bitcode archives. AR/RANLIB stay set; the fixup strip needs RANLIB.
  preCheck = ''
    export CARGO_TARGET_DIR="$PWD/target-check"
    unset RUSTFLAGS CFLAGS CXXFLAGS CMAKE_TOOLCHAIN_FILE
    unset CC_${rustTriple} CXX_${rustTriple}
    unset CARGO_PROFILE_RELEASE_LTO CARGO_PROFILE_RELEASE_CODEGEN_UNITS
  '';

  # cargoInstallHook must take the LTO binaries from target/.
  postCheck = ''
    unset CARGO_TARGET_DIR
  '';

  postInstall = ''
    install -Dm0644 rsbtd/etc/rsbtd.toml $out/share/doc/rsbtd/rsbtd.toml.example
    install -Dm0644 rsbtd/etc/rsbtd.service $out/lib/systemd/system/rsbtd.service
    substituteInPlace $out/lib/systemd/system/rsbtd.service \
      --replace-fail /usr/local/bin/rsbtd $out/bin/rsbtd
    install -Dm0644 packaging/rsbtd.sysusers $out/lib/sysusers.d/rsbtd.conf
    install -Dm0644 LICENSE THIRD-PARTY-NOTICES.md README.md -t $out/share/doc/rsbtd
  '';

  meta = {
    description = "BitTorrent client daemon with a GraphQL API";
    homepage = "https://github.com/namazso/rsbtd";
    license = lib.licenses.mpl20;
    platforms = lib.platforms.linux;
    mainProgram = "rsbtd";
  };
}
