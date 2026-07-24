# Windows build & packaging

The canonical Windows build of rsbtd: a per-user tray application
(`rsbtd.exe`), the `rsbtctl` CLI, and a WiX MSI installer, built with the
same Rust/LLVM versions and full cross-language LTO as the Linux RPMs
(see `packaging/`). The build runs once per target architecture
(`-Arch`), cross-compiling when the target differs from the host. This
directory holds only the build path — the tray app's source lives in
`rsbtd/src/windows/`.

## Entry points

| File | Role |
|---|---|
| `build.ps1` | The canonical build: LTO toolchain env, deps, cargo, the full workspace test suite (`-SkipTests` to skip locally), MSI. Only builds; installs no tooling. |
| `build-openssl.ps1` | Pinned static OpenSSL (hybrid CRT, clang-cl), cached per arch under `dist\windows\`. Called by `build.ps1`. |
| `smoke-test.ps1` | Silent install → first-start token generation → API checks → uninstall → reinstall-keeps-token. Headless-safe; used by CI. |
| `lto-toolchain.cmake` | llvm-lib archiver + static-CRT pin for the CMake-built C/C++ parts. |
| `lto-toolchain-arm64.cmake` | The same, plus the arm64 cross target system declaration. |
| `boost-config-shim/` | Config-mode `Boost::headers` package for CMake ≥ 4 (FindBoost was removed). |
| `installer/Package.wxs` | WiX authoring (per-user, WixUI_Minimal, close-on-upgrade, relaunch-after-install). Files only — no registry, no custom action DLLs: plain file-keypath components with WiX auto GUIDs, and the web UI tree harvested by a `<Files>` wildcard. |

## Usage

```powershell
# web UI first (or use the CI artifact)
cd webui; npm ci; npm run build; cd ..

windows\build.ps1 -WebUiDist webui\dist [-Arch x64|arm64] [-Smoke] [-SkipTests]
```

Output: `dist\windows\rsbtd-<version>-<arch>.msi`. Boost headers and the
static OpenSSL build are cached under `dist\windows\` and reused (the
OpenSSL prefix and stage directory are per-arch).

Between the release build and the MSI, the script runs the full
workspace test suite (dev profile, no LTO, own `target\check` dir — see
the comment in `build.ps1` for why it must not share the LTO target
dir). `-SkipTests` skips it for quick local iteration; CI never does
where the suite can run. A cross build skips it automatically (and
refuses `-Smoke`): the host cannot execute the binaries it just built.
CI leans on the natively built legs for that coverage.

## Requirements (the script checks, but does not install)

* Visual Studio 2022 Build Tools: VC x64 toolset + Windows 11 SDK
  (plus the VC ARM64 toolset for `-Arch arm64`)
* LLVM (clang-cl, lld-link, llvm-lib, libclang) — the exact pinned version
* Rust via rustup — the version pinned by `rust-toolchain.toml`, with
  the rust-std for every `-Arch` target you build (`rustup target add`)
* CMake ≥ 3.25, Ninja, NASM (x64 only), Perl (for OpenSSL), WiX ≥ 6
  (`dotnet tool install --global wix`)

GitHub's `windows-latest` runners provide everything except LLVM, NASM,
the exact Rust toolchain, and WiX — the `msi` job in
`.github/workflows/ci.yml` installs exactly those.

## Version pins

| What | Where |
|---|---|
| Rust | `rust-toolchain.toml` (must share rustc's LLVM major with clang-cl) |
| LLVM | `$LlvmVersion` in `build.ps1` and `LLVM_VERSION` in the workflow |
| Boost | `$BoostVersion`/`$BoostSha256` in `build.ps1` |
| OpenSSL | `$Version`/`$Sha256` defaults in `build-openssl.ps1` |
| WiX | workflow (`dotnet tool install --global wix --version …`) |

`build.ps1` refuses to run with a mismatched clang-cl or rustc and
verifies rustc's bundled LLVM major equals clang-cl's (the same lockstep
guard as `packaging/Containerfile.builder`) — cross-language LTO merges
bitcode from both compilers in one lld-link pass, which is only sound on
a shared LLVM major.

## Build shape

* C/C++ (vendored libtorrent + libctorrent): clang-cl with `-flto=full`,
  archived by llvm-lib (bitcode archives). On x64 also `-msse4.2`
  (libtorrent's popcnt/crc32c intrinsics need the feature enabled under
  clang-cl; the resulting CPU floor is Nehalem-era, 2008); on arm64
  `--target=arm64-pc-windows-msvc` instead, with a baseline-armv8 floor
  (libtorrent falls back to table crc32 without `+crc`). Configuration
  comes from the
  `CC/CXX/CFLAGS/CXXFLAGS/CMAKE_GENERATOR/CMAKE_TOOLCHAIN_FILE`
  environment exported by `build.ps1`, flowing through libctorrent-sys's
  build script — the workspace `Cargo.toml` carries no LTO settings. The
  arm64 toolchain file additionally declares the cross target system so
  CMake never tries to run target binaries.
* Rust: `-Clinker-plugin-lto`, `CARGO_PROFILE_RELEASE_LTO=fat`, one
  codegen unit; lld-link performs the single cross-language LTO merge.
  The build passes an explicit `--target` triple so cargo keeps
  `RUSTFLAGS` away from host artifacts — proc-macro dylibs cannot take
  `-Clinker-plugin-lto` on Windows (binaries land under
  `target\<triple>\release`).
* CRT: "hybrid" — compiled `/MT` (`-Ctarget-feature=+crt-static`,
  `CMAKE_MSVC_RUNTIME_LIBRARY=MultiThreaded`) so vcruntime/STL are
  static, then `/NODEFAULTLIB:libucrt.lib /DEFAULTLIB:ucrt.lib` swaps in
  the OS-shipped universal CRT. No vcredist required; `build.ps1` fails
  the build if the result imports `vcruntime*.dll`/`msvcp*.dll` or drops
  the `api-ms-win-crt-*` imports.
* OpenSSL: static libs via `VC-WIN64A-HYBRIDCRT` (x64) or
  `VC-CLANG-WIN64-CLANGASM-ARM` (arm64 — equivalent CRT shape: no-shared
  static libs compile `/MT /Zl`, and the HYBRIDCRT lflags only affect
  apps/DLLs, which are disabled), same compiler, no LTO (machine-code
  archives link fine into the LTO'd binary).

Developers do not need any of this to hack on rsbtd on Windows: in a VS
x64 dev shell with `BOOST_ROOT` (+`CMAKE_PREFIX_PATH` to
`boost-config-shim` on CMake ≥ 4), `OPENSSL_ROOT_DIR` (optional; omit to
build without TLS support) and `LIBCLANG_PATH` set, a plain
`cargo build --features vendored -p rsbtd -p rsbtctl` works.

## The installer

Per-user MSI (no elevation), installs to
`%LOCALAPPDATA%\Programs\rsbtd`. The installer only lays down files;
per-user state belongs to the app: on its first start rsbtd generates a
random 64-hex-char API token into `HKCU\Software\rsbtd\Token` and asks
whether to enable autostart (the per-user Run key). The tray app itself
documents the full registry schema (`rsbtd/src/windows/mod.rs`).
Third-party license notices install alongside the binaries as
`THIRD-PARTY-NOTICES.md` (generated by `scripts/gen_notices.py`).

Behaviors worth knowing:

* **Upgrades** replace the files and restart the app; the installer
  never touches the registry, so Token/Listen/StateDir and the
  autostart choice all survive.
* **Uninstall** removes the program but deliberately leaves
  `HKCU\Software\rsbtd` (so a reinstall keeps the token), the autostart
  Run value (inert — a broken startup entry — while uninstalled, and
  revived by a reinstall to the same path), and the state directory
  (resume data; torrents come back on reinstall).
* The running tray app is closed gracefully during upgrades/uninstall
  (WM_CLOSE → engine flush, 45s cap) and started again afterwards.
* A non-loopback `Listen` value makes Windows show a firewall prompt on
  next start; per-user installs cannot add firewall rules silently.
