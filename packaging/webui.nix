# Copyright (C) 2026  namazso <admin@namazso.eu>
# SPDX-License-Identifier: MPL-2.0
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.

# The web UI, installed in the rsbtd-webui RPM's layout. The prebuild
# codegen runs offline from the checked-in webui/schema.graphql; node >= 24
# is an engines requirement (the codegen runs a .ts script directly).
{
  lib,
  buildNpmPackage,
  nodejs_24,
}:

(buildNpmPackage.override { nodejs = nodejs_24; }) {
  pname = "rsbtd-webui";
  version = (lib.importJSON ../webui/package.json).version;

  # The generated/installed dirs only exist when building from a dirty tree.
  src = lib.fileset.toSource {
    root = ../webui;
    fileset = lib.fileset.difference ../webui (
      lib.fileset.unions [
        (lib.fileset.maybeMissing ../webui/node_modules)
        (lib.fileset.maybeMissing ../webui/dist)
        (lib.fileset.maybeMissing ../webui/src/gen)
      ]
    );
  };

  npmDepsHash = "sha256-iUkjiUruYJ1/SvwWTJSX422lwoZs80LxiS4/l+LYNPs=";

  installPhase = ''
    runHook preInstall
    mkdir -p $out/share/rsbtd
    cp -r dist $out/share/rsbtd/webui
    runHook postInstall
  '';

  meta = {
    description = "Web UI for the rsbtd BitTorrent daemon";
    homepage = "https://github.com/namazso/rsbtd";
    license = lib.licenses.mpl20;
    platforms = lib.platforms.all;
  };
}
