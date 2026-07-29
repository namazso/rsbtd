# Copyright (C) 2026  namazso <admin@namazso.eu>
# SPDX-License-Identifier: MPL-2.0
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.

{
  description = "BitTorrent client daemon with a GraphQL API";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    {
      self,
      nixpkgs,
      rust-overlay,
    }:
    let
      inherit (nixpkgs) lib;
      systems = [
        "x86_64-linux"
        "aarch64-linux"
      ];
      perSystem = lib.genAttrs systems (
        system:
        let
          pkgs = import nixpkgs {
            inherit system;
            overlays = [ rust-overlay.overlays.default ];
          };
          llvmPackages = pkgs.llvmPackages_21;
          rustToolchain = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
          # lld does rsbtd.nix's cross-language LTO merge. Going through the
          # *wrapped* linker keeps nix's -L/-rpath handling; a bare
          # -fuse-ld=lld would ship binaries without their store rpaths.
          clangLldStdenv = pkgs.overrideCC llvmPackages.stdenv (
            llvmPackages.clang.override { bintools = llvmPackages.bintools; }
          );
          rustPlatform = pkgs.makeRustPlatform {
            cargo = rustToolchain;
            rustc = rustToolchain;
            stdenv = clangLldStdenv;
          };
          libtorrent-rasterbar = pkgs.callPackage ./packaging/libtorrent-rasterbar-2.1.nix { };
        in
        {
          inherit
            pkgs
            llvmPackages
            rustToolchain
            rustPlatform
            libtorrent-rasterbar
            ;
          rsbtd = pkgs.callPackage ./packaging/rsbtd.nix {
            inherit rustPlatform llvmPackages libtorrent-rasterbar;
          };
          rsbtd-webui = pkgs.callPackage ./packaging/webui.nix { };
        }
      );
      eachSystem = f: lib.genAttrs systems (system: f perSystem.${system});
    in
    {
      packages = eachSystem (s: {
        inherit (s) libtorrent-rasterbar rsbtd rsbtd-webui;
        default = s.rsbtd;
      });

      devShells = eachSystem (s: {
        # No CTORRENT_LIBTORRENT_PREFIX needed here: the shim's find_package
        # picks the flake's libtorrent up from the cmake hook's search paths.
        default = (s.pkgs.mkShell.override { stdenv = s.llvmPackages.stdenv; }) {
          packages = [
            s.rustToolchain
            s.rustPlatform.bindgenHook
            s.pkgs.cmake
            s.pkgs.ninja
            s.pkgs.pkg-config
            s.pkgs.nodejs_24
            # for reproducing the package's LTO configuration by hand
            s.llvmPackages.llvm
            s.llvmPackages.lld
          ];
          buildInputs = [
            s.libtorrent-rasterbar
            s.pkgs.boost
            s.pkgs.openssl
          ];
        };
      });

      checks = eachSystem (s: {
        inherit (s) libtorrent-rasterbar rsbtd rsbtd-webui;
      });
    };
}
