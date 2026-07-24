# Packaging

Containerized, reproducible-environment RPM and container-image builds
for rsbtd, plus a CI-only Alpine image. Everything here also runs in CI
(`.github/workflows/ci.yml`) through the same scripts.

```sh
packaging/build-rpm.sh      # → dist/rpms/*.rpm  (rsbtd, rsbtctl, rsbtd-webui, debuginfo, SRPM)
packaging/build-image.sh    # → localhost/rsbtd:<version>  (input: the RPMs)
packaging/build-alpine.sh   # → localhost/rsbtd:alpine  (system libtorrent, CI canary)
```

Requirements on the host: `podman` (or `docker` with
`CONTAINER_ENGINE=docker`), `git`, initialized submodules, and a built
web UI (`cd webui && npm ci && npm run build`, or `RSBTD_WEBUI_DIST`
pointing at one) — it ships in the `rsbtd-webui` noarch subpackage and
is baked into both images.

All three scripts build for the host architecture by default; CI runs
them natively on x86_64 and aarch64 runners. `RSBTD_ARCH=aarch64` (or
`x86_64`) forces a foreign target: podman then pulls the foreign base
images and runs the whole build through qemu binfmt emulation
(`qemu-user-static` with `F`-flagged binfmt entries required). That is
slow — an hour-plus for the RPM build — but it is exactly the CI leg,
which is the point: it lets the foreign pipeline be tested locally.
`RSBTD_RPM_SKIP_CHECK=1` / `RSBTD_ALPINE_SKIP_TESTS=1` skip the test
suites, which CI runs natively anyway.

## How it fits together

| file | role |
|---|---|
| `Containerfile.builder` | Build environment: Oracle Linux 10, distro clang/lld/llvm 21.1.8, Rust 1.94.0 via rustup. Fails the image build if rustc's LLVM major ever diverges from clang's. |
| `rsbtd.spec` | Builds the workspace with the vendored libtorrent and installs daemon + client, systemd unit, config, sysusers, and the web UI (noarch `rsbtd-webui` subpackage from the prebuilt dist). Every package ships `LICENSE` and the generated `THIRD-PARTY-NOTICES.md` (see `scripts/gen_notices.py`) as `%license` — the statically linked libtorrent and crate closure make that a redistribution requirement, and the production image inherits them at `/usr/share/licenses/`. Runs the full workspace test suite in `%check` (dev profile, own target dir — fat-LTO test links would dwarf the build; `RSBTD_RPM_SKIP_CHECK=1` skips it). **All LTO configuration lives here**, not in the workspace. |
| `lto-toolchain.cmake` | Injected via the `CMAKE_TOOLCHAIN_FILE` env var into the two CMake projects that `libctorrent-sys`'s build.rs drives; swaps in `llvm-ar`/`llvm-ranlib` for the bitcode archives. |
| `build-rpm.sh` | Tars the git tree (incl. submodules) and the web UI dist, runs `rpmbuild` in the builder container, collects RPMs into `dist/rpms/`. |
| `Containerfile` + `build-image.sh` + `container-entrypoint.sh` | Production image from `oraclelinux:10-slim`; its only build input is the RPMs. Ships no config: the entrypoint generates one from `RSBTD_TOKEN` (required) and `RSBTD_LISTEN` unless one is mounted at `/etc/rsbtd/rsbtd.toml`. Serves the web UI on GET /. |
| `Containerfile.alpine` + `build-alpine.sh` | CI-only canary image on `alpine:edge`: system libtorrent-rasterbar ≥ 2.1 (no vendored sources), Alpine's current rust/cargo (no toolchain pin), default gcc, no LTO — the deliberate opposite corner from the OL10 build. The builder stage runs the full test suite; not published anywhere. |

## The LTO scheme

The goal is one whole-program ("fat") LTO across the vendored
libtorrent, the libctorrent C shim and the Rust code:

- C/C++ is compiled by clang with `-flto=full`, so the static archives
  (`libtorrent-rasterbar.a`, `libctorrent.a`) contain LLVM bitcode, not
  machine code.
- Rust is compiled with `-Clinker-plugin-lto` (bitcode instead of
  machine code in the rlibs) plus fat LTO and one codegen unit per crate
  on the Rust side.
- The final link runs through `clang -fuse-ld=lld`, where lld's LTO
  merges and optimizes all of that bitcode at once — cross-language
  inlining between the Rust bindings and the C shim/libtorrent included.

This only works because rustc 1.94.0 and OL10's clang are built on the
same LLVM (21.1.8): lld must be able to read rustc's bitcode. When
bumping either toolchain, keep the LLVM majors in lockstep — the builder
image enforces this.

Nothing in the workspace (`Cargo.toml`, build.rs, CI test builds) knows
about any of this; a plain `cargo build --features vendored` keeps
building exactly as before.

## Version pins

| what | where |
|---|---|
| Rust 1.94.0 | `rust-toolchain.toml` (authoritative), `Containerfile.builder` ARG `RUST_VERSION` |
| clang/lld/llvm 21.1.8 | `Containerfile.builder` ARG `LLVM_VERSION` (OL10 distro packages) |
| libtorrent | `vendor/libtorrent` submodule pin |
| crates | `Cargo.lock` (build uses `--locked`) |

## Notes

- The source tarball contains the **git-tracked** working tree
  (`git ls-files --recurse-submodules`): new files must be at least
  staged to be picked up.
- Crates are fetched from crates.io during the build; the registry cache
  persists in `dist/cargo-cache` (`RSBTD_CARGO_CACHE` to relocate, CI
  caches it on `Cargo.lock`).
- `RSBTD_RPM_RELEASE` overrides the RPM `Release`; CI stamps non-tag
  builds as `0.<commits>.g<sha>` so snapshots upgrade cleanly to tagged
  releases.
- The production container runs as the `rsbtd` user, listens on `:3928`
  (API + web UI) and `:6881` TCP/UDP (peers — libtorrent's default
  `listen_interfaces`; uTP and DHT ride the UDP side), and stores state
  in the `/var/lib/rsbtd` volume. Publish `6881:6881` and
  `6881:6881/udp` for incoming peer connections; a different peer port
  set through the settings API needs matching `-p` mappings. It has no
  default credentials: pass `-e RSBTD_TOKEN=<secret>` (or mount a config
  at `/etc/rsbtd/rsbtd.toml`) or it refuses to start. Stop it with a
  grace period (`podman stop -t 45`) so resume data gets flushed.
- On `v*` tags CI pushes the smoke-tested production image to
  `ghcr.io/namazso/rsbtd` as `:<version>` and `:latest` (master pushes
  `:nightly`); `localhost/rsbtd:<version>` is only the local default
  tag. The published tags are multi-arch manifest lists (x86_64 +
  aarch64), assembled by the `publish-image` job from the per-arch
  images that passed their smoke tests.
- The builder image tag is arch-suffixed
  (`localhost/rsbtd-builder:ol10-<arch>`) so native and emulated
  builders coexist; `dist/rpms` likewise accumulates all arches, and
  `build-image.sh` stages only the `RSBTD_ARCH` one.
