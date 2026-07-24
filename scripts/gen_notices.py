#!/usr/bin/env python3
# Copyright (C) 2026  namazso <admin@namazso.eu>
# SPDX-License-Identifier: MPL-2.0
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.

"""Generate THIRD-PARTY-NOTICES.md, the consolidated third-party notice
bundle shipped with every binary artifact (RPMs, MSI, container images).

Covers:
  - the vendored/statically linked native components (libtorrent and the
    third-party code it bundles, Boost, OpenSSL, the Rust standard library)
  - the Rust crates linked into rsbtd/rsbtctl: the union of the normal
    dependency graphs of both binaries across every release target, resolved
    from Cargo.lock via `cargo tree`, with license metadata and license
    texts taken from the crate sources `cargo metadata` unpacks into the
    cargo registry
  - the web UI's production npm dependency closure, from
    webui/package-lock.json, with license texts from webui/node_modules

The output is checked in; CI regenerates it (--check) so it cannot go stale
against Cargo.lock, package-lock.json, the vendored submodule, or the
pinned Boost/OpenSSL versions. Regenerate on Linux with the webui installed
(cd webui && npm ci) and commit the diff:

  python3 scripts/gen_notices.py

Needs network access the first time (cargo downloads the crate sources);
identical inputs produce byte-identical output. scripts/notices-data/ holds
canonical SPDX license texts used for components that do not ship their
own copy of a license file.
"""

import argparse
import hashlib
import json
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DATA = Path(__file__).resolve().parent / "notices-data"
OUTPUT = ROOT / "THIRD-PARTY-NOTICES.md"

# One `cargo tree` per release target: the RPMs (linux-gnu), the MSIs
# (windows-msvc). The union is what can end up inside a shipped binary.
RUST_TARGETS = [
    "x86_64-unknown-linux-gnu",
    "aarch64-unknown-linux-gnu",
    "x86_64-pc-windows-msvc",
    "aarch64-pc-windows-msvc",
]

LICENSE_FILE_RE = re.compile(
    r"^(licen[cs]e|unlicen[cs]e|copying|copyright|notice)", re.IGNORECASE
)


def run(cmd, **kwargs):
    return subprocess.run(
        cmd, cwd=ROOT, check=True, capture_output=True, text=True, **kwargs
    ).stdout


def canonical_text(spdx_id):
    path = DATA / f"{spdx_id}.txt"
    if not path.is_file():
        sys.exit(
            f"error: no canonical license text for {spdx_id}; "
            f"add {path.relative_to(ROOT)}"
        )
    return path.read_text(encoding="utf-8")


def read_text(path):
    return path.read_text(encoding="utf-8", errors="replace").replace("\r\n", "\n")


def license_files(directory):
    """License/notice files shipped at a package root, sorted by name."""
    if not directory.is_dir():
        return []
    return sorted(
        p for p in directory.iterdir() if p.is_file() and LICENSE_FILE_RE.match(p.name)
    )


class TextPool:
    """Deduplicating pool of license texts, labeled L1, L2, ... in order of
    first use so the output is deterministic."""

    def __init__(self):
        self.labels = {}  # normalized-hash -> label
        self.texts = []  # (label, text)

    def add(self, text):
        key = hashlib.sha256(re.sub(r"\s+", " ", text).strip().encode()).hexdigest()
        if key not in self.labels:
            label = f"L{len(self.texts) + 1}"
            self.labels[key] = label
            self.texts.append((label, text))
        return self.labels[key]


def parse_pin(path, pattern, what):
    m = re.search(pattern, read_text(path), re.MULTILINE)
    if not m:
        sys.exit(f"error: cannot find the {what} pin in {path}")
    return m.group(1)


def comment_block(path):
    """First /* ... */ comment of a C/C++ file, unindented."""
    m = re.search(r"/\*(.*?)\*/", read_text(path), re.DOTALL)
    if not m:
        sys.exit(f"error: no leading comment block in {path}")
    lines = [line.rstrip() for line in m.group(1).strip("\n").splitlines()]
    indent = min(
        (len(line) - len(line.lstrip()) for line in lines if line.strip()), default=0
    )
    return "\n".join(line[indent:] for line in lines)


def native_components(pool):
    lt = ROOT / "vendor" / "libtorrent"
    if not (lt / "COPYING").is_file():
        sys.exit(
            "error: vendor/libtorrent is missing; "
            "run: git submodule update --init --recursive"
        )
    boost = parse_pin(
        ROOT / "windows" / "build.ps1", r"^\$BoostVersion = '([^']+)'", "Boost version"
    )
    openssl = parse_pin(
        ROOT / "windows" / "build-openssl.ps1",
        r"\[string\]\$Version = '([^']+)'",
        "OpenSSL version",
    )
    return [
        (
            "libtorrent-rasterbar",
            "BSD-3-Clause",
            "Vendored at vendor/libtorrent and statically linked into rsbtd "
            "and rsbtctl in all release builds.",
            pool.add(read_text(lt / "COPYING")),
        ),
        (
            "try_signal",
            "BSD-3-Clause",
            "Bundled with libtorrent (vendor/libtorrent/deps/try_signal).",
            pool.add(read_text(lt / "deps" / "try_signal" / "LICENSE")),
        ),
        (
            "ed25519",
            "Zlib",
            "Bundled with libtorrent (vendor/libtorrent/src/ed25519).",
            pool.add(read_text(lt / "src" / "ed25519" / "LICENSE")),
        ),
        (
            "puff",
            "Zlib",
            "Bundled with libtorrent (vendor/libtorrent/src/puff.cpp).",
            pool.add(comment_block(lt / "include" / "libtorrent" / "aux_" / "puff.hpp")),
        ),
        (
            f"Boost {boost}",
            "BSL-1.0",
            "Header-only libraries compiled into the statically linked "
            "libtorrent (the pinned version is the Windows build's; the RPM "
            "build uses the distribution's boost-devel).",
            pool.add(canonical_text("BSL-1.0")),
        ),
        (
            f"OpenSSL {openssl}",
            "Apache-2.0",
            "Statically linked into the Windows binaries (version pinned in "
            "windows/build-openssl.ps1); the Linux builds link the "
            "distribution's OpenSSL dynamically.",
            pool.add(canonical_text("Apache-2.0")),
        ),
        (
            "Rust standard library",
            "MIT OR Apache-2.0",
            "Statically linked into rsbtd and rsbtctl by rustc; copyright "
            "The Rust Project Developers.",
            pool.add(canonical_text("MIT")),
        ),
    ]


def rust_packages(pool):
    meta = json.loads(run(["cargo", "metadata", "--format-version", "1", "--locked"]))
    packages = {(p["name"], p["version"]): p for p in meta["packages"]}

    shipped = set()
    for target in RUST_TARGETS:
        out = run(
            [
                "cargo", "tree", "--locked", "-e", "normal", "--prefix", "none",
                "--features", "vendored", "-p", "rsbtd", "-p", "rsbtctl",
                "--target", target,
            ]
        )
        for line in out.splitlines():
            m = re.match(r"^(\S+) v(\S+?)(?: \((.*)\))?$", line.strip())
            if not m:
                continue
            annotation = m.group(3) or ""
            # A path annotation marks a workspace member (first-party).
            if "/" in annotation or "\\" in annotation:
                continue
            shipped.add((m.group(1), m.group(2)))

    rows = []
    for key in sorted(shipped):
        pkg = packages.get(key)
        if pkg is None:
            sys.exit(f"error: {key[0]} {key[1]} is in cargo tree but not metadata")
        crate_dir = Path(pkg["manifest_path"]).parent
        files = license_files(crate_dir)
        if files:
            labels = [pool.add(read_text(f)) for f in files]
        else:
            expr = pkg.get("license") or ""
            first = re.split(r"\s+OR\s+|/", expr)[0].strip()
            labels = [pool.add(canonical_text(first))]
        rows.append((key[0], key[1], pkg.get("license") or "(see text)", labels))
    return rows


def npm_packages(pool):
    lock = json.loads(read_text(ROOT / "webui" / "package-lock.json"))
    prod = {}
    for path, entry in lock["packages"].items():
        if not path or entry.get("dev") or entry.get("link"):
            continue
        name = path.split("node_modules/")[-1]
        prod[(name, entry.get("version") or "")] = (path, entry)

    if not (ROOT / "webui" / "node_modules").is_dir():
        sys.exit("error: webui/node_modules is missing; run: (cd webui && npm ci)")

    rows = []
    for (name, version), (path, entry) in sorted(prod.items()):
        files = license_files(ROOT / "webui" / path)
        if files:
            labels = [pool.add(read_text(f)) for f in files]
        else:
            expr = entry.get("license") or ""
            first = re.split(r"\s+OR\s+|/", expr.strip("()"))[0].strip()
            labels = [pool.add(canonical_text(first))]
        rows.append((name, version, entry.get("license") or "(see text)", labels))
    return rows


def generate():
    pool = TextPool()
    natives = native_components(pool)
    crates = rust_packages(pool)
    npm = npm_packages(pool)

    out = []
    w = out.append
    w("# Third-party notices")
    w("")
    w("rsbtd is Copyright (C) 2026 namazso, licensed under the Mozilla Public")
    w("License 2.0 (see LICENSE; source at https://github.com/namazso/rsbtd).")
    w("The binary distributions -- the RPMs, the Windows MSI installer, and")
    w("the container images -- additionally contain the third-party components")
    w("listed here, reproduced together with their license texts and copyright")
    w("notices. Each component is used under the license named next to it;")
    w("`Lnn` references point into the License texts section at the end. The")
    w("inventory is the union over all release platforms, so any single")
    w("artifact contains a subset. Operating-system packages in the container")
    w("images carry their own license files under /usr/share/licenses.")
    w("")
    w("Generated by scripts/gen_notices.py; do not edit by hand (CI checks")
    w("that this file matches the generator's inputs).")
    w("")
    w("## Native components")
    w("")
    for name, license_, comment, label in natives:
        w(f"- **{name}** -- {license_} [{label}]. {comment}")
    w("")
    w("## Rust crates")
    w("")
    w("| Crate | Version | License | Text |")
    w("|---|---|---|---|")
    for name, version, license_, labels in crates:
        w(f"| {name} | {version} | {license_} | {', '.join(labels)} |")
    w("")
    w("## Web UI npm packages")
    w("")
    w("| Package | Version | License | Text |")
    w("|---|---|---|---|")
    for name, version, license_, labels in npm:
        w(f"| {name} | {version} | {license_} | {', '.join(labels)} |")
    w("")
    w("## License texts")
    for label, text in pool.texts:
        w("")
        w(f"### {label}")
        w("")
        w("````text")
        w(text.strip("\n"))
        w("````")
    w("")
    return "\n".join(out)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--check",
        action="store_true",
        help="verify that the checked-in file matches instead of writing it",
    )
    args = parser.parse_args()

    content = generate()
    if args.check:
        on_disk = OUTPUT.read_text(encoding="utf-8") if OUTPUT.is_file() else ""
        if on_disk != content:
            sys.exit(
                f"error: {OUTPUT.name} is stale; regenerate it with\n"
                "  python3 scripts/gen_notices.py\n"
                "(on Linux, with webui/node_modules installed) and commit the diff"
            )
        print(f"{OUTPUT.name} is up to date")
    else:
        OUTPUT.write_text(content, encoding="utf-8")
        print(f"wrote {OUTPUT.name}")


if __name__ == "__main__":
    main()
