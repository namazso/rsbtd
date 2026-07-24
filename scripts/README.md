# Code generators

These scripts parse the pinned `vendor/libtorrent` submodule and write
generated-once files at their final source locations. The outputs are checked
in; the scripts only need to be re-run when the vendored libtorrent is
updated. Commit the resulting diff together with the submodule bump.

| script | outputs |
|---|---|
| `gen_settings.py` | `libctorrent/include/ctorrent/ct_settings_generated.h`, `libctorrent/src/settings_asserts.inc` (the typed Rust surface in `rbtorrent/src/settings/` is handwritten against these constants; extend it by hand for new settings) |
| `gen_alerts.py` | `libctorrent/include/ctorrent/ct_alerts_generated.h`, `libctorrent/src/alert_asserts.inc`, `rbtorrent/src/alerts/generated.rs` (alert IDs, `AlertType`, name table; the per-alert view structs are handwritten) |
| `gen_fixtures.cpp` | `rbtorrent/tests/fixtures/{v1,v2,hybrid,transfer}.torrent` (deterministic test torrents; `transfer` is trackerless/web-seedless for network-hermetic session tests; build instructions in the file header — links against the vendored libtorrent build) |
| `gen_notices.py` | `THIRD-PARTY-NOTICES.md` (the consolidated license/notice bundle shipped in the RPMs, the MSI, and the container images; inputs are Cargo.lock/`cargo metadata`, `webui/package-lock.json` + `node_modules`, the vendored submodule, and the Boost/OpenSSL pins — so re-run it when any of those change, on Linux, after `npm ci` in `webui/`; canonical fallback license texts live in `notices-data/`) |

Every generated constant is verified against the libtorrent headers by
`static_assert`s (the `*_asserts.inc` files, compiled into libctorrent), so a
stale or hand-edited generated file fails the build rather than misbehaving.

CI should run each script and `git diff --exit-code` to prove the checked-in
outputs match the generators.

Known upstream quirk (libtorrent 2.1.0): `announce_to_all_tiers` and
`announce_to_all_trackers` are swapped between the settings enum and the
runtime name table. Generated constants follow the enum (which libtorrent's
own behavior uses); name-based lookups follow the linked library. See the
`name_lookup_roundtrip` test in `rbtorrent/src/settings/mod.rs`.
