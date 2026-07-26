# Copyright (C) 2026  namazso <admin@namazso.eu>
# SPDX-License-Identifier: MPL-2.0
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.

# Local nixpkgs-style package of the 2.1.0 release, kept only until
# nixpkgs itself carries >= 2.1; written so it could be upstreamed
# near-verbatim. deprecated-functions=ON gives TORRENT_ABI_VERSION=2, the
# distro norm rsbtd's ABI probe expects as its baseline.
{
  lib,
  stdenv,
  fetchurl,
  cmake,
  ninja,
  boost,
  openssl,
}:

stdenv.mkDerivation (finalAttrs: {
  pname = "libtorrent-rasterbar";
  version = "2.1.0";

  src = fetchurl {
    url = "https://github.com/arvidn/libtorrent/releases/download/v${finalAttrs.version}/libtorrent-rasterbar-${finalAttrs.version}.tar.gz";
    hash = "sha256-zu1ldga430U+xed1Mm48dZoneeEgL6BKvkLtJi578LY=";
  };

  nativeBuildInputs = [
    cmake
    ninja
  ];

  buildInputs = [
    boost
    openssl
  ];

  cmakeFlags = [
    # The generated .pc file joins ${prefix} with these, so the absolute
    # paths the nixpkgs cmake hook presets would break it (nixpkgs#144170).
    "-DCMAKE_INSTALL_LIBDIR=lib"
    "-DCMAKE_INSTALL_INCLUDEDIR=include"
    (lib.cmakeBool "deprecated-functions" true)
    # Needs the libdatachannel submodule tree, absent from the tarball.
    (lib.cmakeBool "webtorrent" false)
    (lib.cmakeBool "python-bindings" false)
    (lib.cmakeBool "build_tests" false)
    (lib.cmakeBool "build_examples" false)
  ];

  meta = {
    description = "Feature-complete C++ BitTorrent implementation";
    homepage = "https://libtorrent.org/";
    changelog = "https://github.com/arvidn/libtorrent/releases/tag/v${finalAttrs.version}";
    license = lib.licenses.bsd3;
    platforms = lib.platforms.unix;
  };
})
