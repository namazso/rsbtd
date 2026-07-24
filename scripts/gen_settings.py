#!/usr/bin/env python3
# Copyright (C) 2026  namazso <admin@namazso.eu>
# SPDX-License-Identifier: MPL-2.0
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.

"""Generate the C settings constants from libtorrent's settings_pack.hpp.

Parses vendor/libtorrent (the pinned submodule) and writes:

  libctorrent/include/ctorrent/ct_settings_generated.h  - CT_SET_* keys and
      CT_<ENUM>_* value constants
  libctorrent/src/settings_asserts.inc                  - static_asserts
      verifying every generated constant against the libtorrent headers

The Rust settings surface (rbtorrent/src/settings/) is handwritten against
these constants; when the vendored libtorrent changes, re-run this script,
commit the diff, and extend the handwritten modules for any new settings.

Outputs are checked in ("generated once"); re-run this script only when the
vendored libtorrent changes, and commit the diff.
"""

from __future__ import annotations

import re
import sys
from dataclasses import dataclass, field
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
HPP = ROOT / "vendor/libtorrent/include/libtorrent/settings_pack.hpp"

SETTINGS_ENUMS = {
    "string_types": ("str", 0x0000),
    "int_types": ("int", 0x4000),
    "bool_types": ("bool", 0x8000),
}
BASE_NAMES = {"string_type_base", "int_type_base", "bool_type_base"}

# settings_pack value enums to export (enum name -> C/Rust base name)
VALUE_ENUMS = {
    "mmap_write_mode_t": "mmap_write_mode",
    "suggest_mode_t": "suggest_mode",
    "choking_algorithm_t": "choking_algorithm",
    "seed_choking_algorithm_t": "seed_choking_algorithm",
    "io_buffer_mode_t": "io_buffer_mode",
    "bandwidth_mixed_algo_t": "bandwidth_mixed_algo",
    "enc_policy": "enc_policy",
    "enc_level": "enc_level",
    "proxy_type_t": "proxy_type",
}

# int settings whose values come from a settings_pack value enum
# (curated; extend when libtorrent adds enum-valued settings)
ENUM_TYPED_SETTINGS = {
    "disk_write_mode": "mmap_write_mode_t",
    "suggest_mode": "suggest_mode_t",
    "choking_algorithm": "choking_algorithm_t",
    "seed_choking_algorithm": "seed_choking_algorithm_t",
    "disk_io_write_mode": "io_buffer_mode_t",
    "disk_io_read_mode": "io_buffer_mode_t",
    "mixed_mode_algorithm": "bandwidth_mixed_algo_t",
    "out_enc_policy": "enc_policy",
    "in_enc_policy": "enc_policy",
    "allowed_enc_level": "enc_level",
    "proxy_type": "proxy_type_t",
}


@dataclass
class Setting:
    name: str
    kind: str  # str | int | bool
    value: int
    doc: list[str] = field(default_factory=list)


@dataclass
class EnumValue:
    name: str
    value: int
    doc: list[str] = field(default_factory=list)
    emit: bool = True


def fail(msg: str) -> None:
    sys.exit(f"gen_settings.py: error: {msg}")


def parse() -> tuple[list[Setting], dict[str, list[EnumValue]]]:
    settings: list[Setting] = []
    value_enums: dict[str, list[EnumValue]] = {}

    lines = HPP.read_text().splitlines()
    i = 0
    n = len(lines)

    def parse_settings_enum(start: int, kind: str, base: int) -> int:
        idx = 0
        doc: list[str] = []
        seen: set[str] = set()
        j = start
        # pp state: None (outside), "skip" (inactive branch), "keep"
        pp: list[str] = []
        while j < n:
            line = lines[j].strip()
            j += 1
            if line.startswith("#if"):
                m = re.match(r"#if TORRENT_ABI_VERSION (==|<=|<) *\d+", line)
                if not m:
                    fail(f"unhandled directive in settings enum: {line!r}")
                pp.append("skip")
                doc = []
                continue
            if line.startswith("#else"):
                if not pp:
                    fail("unmatched #else in settings enum")
                pp[-1] = "keep"
                continue
            if line.startswith("#endif"):
                if not pp:
                    fail("unmatched #endif in settings enum")
                pp.pop()
                continue
            if pp and pp[-1] == "skip":
                continue
            if line == "{" or line == "":
                doc = []
                continue
            if line.startswith("//"):
                doc.append(line[2:].removeprefix(" "))
                continue
            if line == "};":
                fail(f"settings enum ended without sentinel (kind={kind})")
            name = line.split("=")[0].rstrip(",").strip()
            if name.endswith("_internal"):
                return j  # sentinel: end of group
            if "=" in line:
                rhs = line.split("=", 1)[1].rstrip(",").strip()
                if rhs in seen:
                    # backwards-compat alias (e.g. peer_tos = peer_dscp):
                    # no slot, not exposed
                    doc = []
                    continue
                if rhs not in BASE_NAMES:
                    fail(f"unexpected explicit value in settings enum: {line!r}")
                if idx != 0:
                    fail(f"type base not at position 0: {line!r}")
            seen.add(name)
            if not name.startswith("deprecated_"):
                settings.append(Setting(name, kind, base + idx, doc))
            idx += 1
            doc = []
        fail(f"unterminated settings enum (kind={kind})")
        return j

    def parse_value_enum(start: int, enum_name: str) -> int:
        values: list[EnumValue] = []
        next_val = 0
        doc: list[str] = []
        j = start
        pp: list[str] = []  # "keep-noemit" | "keep" | "skip"
        while j < n:
            line = lines[j].strip()
            j += 1
            if line.startswith("#if"):
                if re.match(r"#if TORRENT_ABI_VERSION (==|<=|<) *\d+", line):
                    pp.append("skip")
                elif re.match(r"#if TORRENT_USE_\w+", line):
                    # feature-gated values exist in the default build but are
                    # internal; keep counting, don't emit
                    pp.append("keep-noemit")
                else:
                    fail(f"unhandled directive in enum {enum_name}: {line!r}")
                doc = []
                continue
            if line.startswith("#else"):
                pp[-1] = "keep" if pp[-1] == "skip" else "skip"
                continue
            if line.startswith("#endif"):
                pp.pop()
                continue
            if pp and pp[-1] == "skip":
                continue
            if line == "{" or line == "":
                doc = []
                continue
            if line.startswith("//"):
                doc.append(line[2:].removeprefix(" "))
                continue
            if line == "};":
                value_enums[enum_name] = values
                return j
            name = line.split("=")[0].rstrip(",").strip()
            name = name.replace("TORRENT_DEPRECATED_ENUM", "").strip()
            if "=" in line:
                rhs = line.split("=", 1)[1].rstrip(",").strip()
                next_val = int(rhs, 0)
            val = next_val
            next_val += 1
            emit = (
                not name.startswith("deprecated_")
                and not (pp and pp[-1] == "keep-noemit")
            )
            values.append(EnumValue(name, val, doc, emit))
            doc = []
        fail(f"unterminated enum {enum_name}")
        return j

    while i < n:
        stripped = lines[i].strip()
        m = re.match(r"enum (\w+)( : std::uint8_t)?$", stripped)
        i += 1
        if not m:
            continue
        enum_name = m.group(1)
        if enum_name in SETTINGS_ENUMS:
            kind, base = SETTINGS_ENUMS[enum_name]
            i = parse_settings_enum(i, kind, base)
        elif enum_name in VALUE_ENUMS:
            i = parse_value_enum(i, enum_name)

    missing = set(VALUE_ENUMS) - set(value_enums)
    if missing:
        fail(f"value enums not found: {sorted(missing)}")
    for setting, enum_name in ENUM_TYPED_SETTINGS.items():
        if not any(s.name == setting for s in settings):
            fail(f"enum-typed setting {setting!r} not found")
        if enum_name not in value_enums:
            fail(f"enum {enum_name!r} for setting {setting!r} not found")
    return settings, value_enums


def gen_c_header(settings: list[Setting], enums: dict[str, list[EnumValue]]) -> str:
    o = [
        "/* generated by scripts/gen_settings.py - do not edit */",
        "#ifndef CT_SETTINGS_GENERATED_H_INCLUDED",
        "#define CT_SETTINGS_GENERATED_H_INCLUDED",
        "",
        "/* Setting keys: (type base | index), matching lt::settings_pack. */",
    ]
    for s in settings:
        o.append(f"#define CT_SET_{s.name.upper()} 0x{s.value:04x}")
    o.append("")
    o.append("/* Setting counts known at generation time. */")
    for kind, count in count_by_kind(settings).items():
        o.append(f"#define CT_NUM_{kind.upper()}_SETTINGS {count}")
    o.append("")
    o.append("#define CT_SETTINGS_TYPE_MASK 0xc000")
    o.append("#define CT_SETTINGS_INDEX_MASK 0x3fff")
    o.append("#define CT_SETTINGS_STR_BASE 0x0000")
    o.append("#define CT_SETTINGS_INT_BASE 0x4000")
    o.append("#define CT_SETTINGS_BOOL_BASE 0x8000")
    for enum_name, values in enums.items():
        base = VALUE_ENUMS[enum_name].upper()
        o.append("")
        o.append(f"/* values of lt::settings_pack::{enum_name} */")
        for v in values:
            if v.emit:
                o.append(f"#define CT_{base}_{v.name.upper()} {v.value}")
    o += ["", "#endif", ""]
    return "\n".join(o)


def count_by_kind(settings: list[Setting]) -> dict[str, int]:
    counts = {"string": 0, "int": 0, "bool": 0}
    for s in settings:
        counts[{"str": "string", "int": "int", "bool": "bool"}[s.kind]] = max(
            counts[{"str": "string", "int": "int", "bool": "bool"}[s.kind]],
            (s.value & 0x3FFF) + 1,
        )
    return counts


def gen_asserts(settings: list[Setting], enums: dict[str, list[EnumValue]]) -> str:
    o = [
        "// generated by scripts/gen_settings.py - do not edit",
        "// included by settings.cpp inside an anonymous namespace",
    ]
    for s in settings:
        o.append(
            f"static_assert(CT_SET_{s.name.upper()} == lt::settings_pack::{s.name});"
        )
    o.append("")
    for kind in ("string", "int", "bool"):
        o.append(
            f"static_assert(lt::settings_pack::num_{kind}_settings >= "
            f"CT_NUM_{kind.upper()}_SETTINGS);"
        )
    o.append("")
    o.append("static_assert(CT_SETTINGS_TYPE_MASK == lt::settings_pack::type_mask);")
    o.append("static_assert(CT_SETTINGS_INDEX_MASK == lt::settings_pack::index_mask);")
    o.append(
        "static_assert(CT_SETTINGS_STR_BASE == lt::settings_pack::string_type_base);"
    )
    o.append(
        "static_assert(CT_SETTINGS_INT_BASE == lt::settings_pack::int_type_base);"
    )
    o.append(
        "static_assert(CT_SETTINGS_BOOL_BASE == lt::settings_pack::bool_type_base);"
    )
    o.append("")
    for enum_name, values in enums.items():
        base = VALUE_ENUMS[enum_name].upper()
        for v in values:
            if v.emit:
                o.append(
                    f"static_assert(CT_{base}_{v.name.upper()} == "
                    f"lt::settings_pack::{v.name});"
                )
    o.append("")
    return "\n".join(o)


def main() -> None:
    settings, enums = parse()
    counts = count_by_kind(settings)
    by_kind = {"str": 0, "int": 0, "bool": 0}
    for s in settings:
        by_kind[s.kind] += 1
    print(
        f"parsed {len(settings)} settings "
        f"(str={by_kind['str']} int={by_kind['int']} bool={by_kind['bool']}; "
        f"slots: {counts})"
    )

    (ROOT / "libctorrent/include/ctorrent/ct_settings_generated.h").write_text(
        gen_c_header(settings, enums)
    )
    (ROOT / "libctorrent/src/settings_asserts.inc").write_text(
        gen_asserts(settings, enums)
    )
    print("wrote ct_settings_generated.h, settings_asserts.inc")


if __name__ == "__main__":
    main()
