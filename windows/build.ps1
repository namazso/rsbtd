# Copyright (C) 2026  namazso <admin@namazso.eu>
# SPDX-License-Identifier: MPL-2.0
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.

# Canonical Windows build: rsbtd.exe (tray app) + rsbtctl.exe with full
# cross-language LTO, the full workspace test suite, and the per-user
# MSI installer.
#
#   windows\build.ps1 -WebUiDist <path\to\webui\dist> [-Arch x64|arm64]
#                     [-Smoke] [-SkipTests]
#
# The web UI is built separately (`npm ci && npm run build` in webui/, or
# a CI artifact) and passed in; this script never runs npm.
#
# -Arch arm64 cross-compiles on an x64 host: the same pinned toolchain
# retargeted (clang-cl --target, cargo --target), the target's VS dev
# shell environment, and a per-arch OpenSSL/stage/MSI. It additionally
# needs the VS ARM64 toolset component and the matching rust-std
# (rustup target add). The host cannot run what it cross-compiles, so
# the test suite is skipped and -Smoke refused; the natively built CI
# legs keep that coverage.
#
# The script only builds -- it installs no tooling. Requirements (see
# windows\README.md): VS 2022 Build Tools (VC x64 + Windows SDK), LLVM
# $LlvmVersion (clang-cl/lld-link/llvm-lib/libclang), the Rust toolchain
# from rust-toolchain.toml, CMake >= 3.25, Ninja, NASM, Perl, and the
# WiX toolset (>= 6) as a dotnet tool. Boost headers and a static
# OpenSSL are downloaded/built into dist\windows\ and cached there.
#
# Toolchain shape (mirrors packaging/rsbtd.spec on Linux): C/C++ compiled
# by clang-cl with -flto=full into bitcode archives (llvm-lib via
# lto-toolchain.cmake), Rust with -Clinker-plugin-lto, one fat-LTO merge
# in lld-link. CRT is "hybrid": everything compiles /MT (static
# vcruntime/STL), but the final link swaps in the universal CRT DLLs the
# OS ships, so no vcredist is needed. Developers who just want a working
# build can ignore all of this: a plain `cargo build --features vendored`
# in a VS dev shell works with BOOST_ROOT/CMAKE_PREFIX_PATH/
# OPENSSL_ROOT_DIR/LIBCLANG_PATH set (see README).

[CmdletBinding()]
param(
    # Directory containing the built web UI (index.html at its root).
    [Parameter(Mandatory)] [string]$WebUiDist,
    # Target architecture. arm64 cross-compiles on an x64 host (see the
    # header comment).
    [ValidateSet('x64', 'arm64')] [string]$Arch = 'x64',
    # Run windows\smoke-test.ps1 against the produced MSI.
    [switch]$Smoke,
    # Skip the workspace test suite (local iteration; CI always runs it).
    [switch]$SkipTests,
    # Escape hatch for the OpenSSL build (see build-openssl.ps1).
    [string]$OpensslCompiler = 'clang-cl'
)

$ErrorActionPreference = 'Stop'

# ---- pins -----------------------------------------------------------------
$LlvmVersion = '21.1.8'
$BoostVersion = '1.88.0'
$BoostSha256 = '3621533e820dcab1e8012afd583c0c73cf0f77694952b81352bf38c1488f9cb4'
# OpenSSL is pinned inside build-openssl.ps1.

# ---- per-arch shape --------------------------------------------------------
# Rust and clang spell the arm64 triple differently.
$target = @{ x64 = 'x86_64-pc-windows-msvc'; arm64 = 'aarch64-pc-windows-msvc' }[$Arch]
$clangTarget = @{ x64 = 'x86_64-pc-windows-msvc'; arm64 = 'arm64-pc-windows-msvc' }[$Arch]

# Can this host execute binaries of the target arch? (arm64 hosts run
# x64 under emulation; x64 hosts cannot run arm64 at all.)
$hostArch = if ($env:PROCESSOR_ARCHITECTURE -eq 'ARM64') { 'arm64' } else { 'x64' }
$canRunTarget = ($Arch -eq $hostArch) -or ($hostArch -eq 'arm64' -and $Arch -eq 'x64')
if (-not $canRunTarget) {
    if ($Smoke) { throw "-Smoke: a $Arch MSI cannot be installed on this $hostArch host" }
    if (-not $SkipTests) {
        Write-Host "note: $Arch binaries cannot run on this $hostArch host; skipping the test suite"
        $SkipTests = $true
    }
}

$RepoRoot = Split-Path -Parent $PSScriptRoot
$Dist = Join-Path $RepoRoot 'dist\windows'
$Stage = Join-Path $Dist "stage-$Arch"
New-Item -ItemType Directory -Force $Dist, $Stage | Out-Null

$WebUiDist = (Resolve-Path $WebUiDist).Path
if (-not (Test-Path (Join-Path $WebUiDist 'index.html'))) {
    throw "-WebUiDist $WebUiDist does not look like a built web UI (no index.html)"
}

# ---- VS developer environment ----------------------------------------------
# The dev shell targets $Arch (x64-hosted tools either way): cl/link/lib
# for the target arch on PATH, INCLUDE/LIB pointing at its CRT and SDK.
$vswhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
if (-not (Test-Path $vswhere)) { throw 'Visual Studio Build Tools not found (no vswhere.exe)' }
$vsRequires = @('Microsoft.VisualStudio.Component.VC.Tools.x86.x64')
if ($Arch -eq 'arm64') { $vsRequires += 'Microsoft.VisualStudio.Component.VC.Tools.ARM64' }
$vsPath = & $vswhere -latest -products * -requires @vsRequires -property installationPath
if (-not $vsPath) { throw "no VS installation with the VC toolset(s) for $Arch found ($($vsRequires -join ' + '))" }
Import-Module (Join-Path $vsPath 'Common7\Tools\Microsoft.VisualStudio.DevShell.dll')
Enter-VsDevShell -VsInstallPath $vsPath -SkipAutomaticLocation -DevCmdArguments "-arch=$Arch -host_arch=x64" | Out-Null
Set-Location $RepoRoot

# ---- preflight: right tools, right versions ---------------------------------
# NASM is only OpenSSL's x64 assembler; on arm64 clang-cl assembles.
$tools = @('cargo', 'rustc', 'cmake', 'ninja', 'perl', 'clang-cl', 'lld-link', 'llvm-lib', 'wix', 'dumpbin')
if ($Arch -eq 'x64') { $tools += 'nasm' }
foreach ($tool in $tools) {
    if (-not (Get-Command $tool -ErrorAction SilentlyContinue)) {
        throw "required tool '$tool' not found on PATH"
    }
}

$clangVersion = ((& clang-cl --version) | Select-Object -First 1) -replace '.*clang version ([0-9.]+).*', '$1'
if ($clangVersion -ne $LlvmVersion) {
    throw "clang-cl is $clangVersion; this build pins LLVM $LlvmVersion"
}

$pinnedRust = @((Get-Content (Join-Path $RepoRoot 'rust-toolchain.toml')) -match '^channel')[0] -replace '.*"(.*)".*', '$1'
$rustcVersion = (& rustc --version) -replace 'rustc ([0-9.]+).*', '$1'
if ($rustcVersion -ne $pinnedRust) {
    throw "rustc is $rustcVersion; rust-toolchain.toml pins $pinnedRust (is rustup's cargo first on PATH?)"
}

# Cross-language LTO is only sound when rustc's bundled LLVM and clang-cl
# agree on the bitcode major version (same guard as the RPM builder image).
$rustLlvm = ((& rustc -vV) | Select-String 'LLVM version: (.*)').Matches.Groups[1].Value
if ($rustLlvm.Split('.')[0] -ne $LlvmVersion.Split('.')[0]) {
    throw "rustc bundles LLVM $rustLlvm but clang-cl is $LlvmVersion; LTO needs matching majors"
}

if (@(& rustup target list --installed) -notcontains $target) {
    throw "the $target rust-std is not installed; run: rustup target add $target"
}

$wixVersion = (& wix --version) -replace '\+.*', ''
if ([int]($wixVersion.Split('.')[0]) -lt 6) {
    throw "wix is $wixVersion; version 6 or newer is required"
}

if (-not (Test-Path (Join-Path $RepoRoot 'vendor\libtorrent\CMakeLists.txt'))) {
    throw 'vendor/libtorrent is missing; run: git submodule update --init --recursive'
}

# ---- dependencies: Boost headers + static OpenSSL (cached in dist) ----------
$boostRoot = Join-Path $Dist 'boost'
$boostMarker = Join-Path $boostRoot '.rsbtd-boost-version'
if (-not ((Test-Path $boostMarker) -and ((Get-Content $boostMarker) -eq $BoostVersion))) {
    Write-Host "boost: downloading $BoostVersion"
    if (Test-Path $boostRoot) { Remove-Item -Recurse -Force $boostRoot }
    $underscored = $BoostVersion -replace '\.', '_'
    $tarball = Join-Path $Dist "boost_$underscored.tar.gz"
    [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
    (New-Object System.Net.WebClient).DownloadFile(
        "https://archives.boost.io/release/$BoostVersion/source/boost_$underscored.tar.gz", $tarball)
    $hash = (Get-FileHash -Algorithm SHA256 $tarball).Hash.ToLowerInvariant()
    if ($hash -ne $BoostSha256) { throw "boost: checksum mismatch ($hash)" }
    Write-Host 'boost: extracting (headers only, but the archive is large)'
    tar -xzf $tarball -C $Dist
    if ($LASTEXITCODE -ne 0) { throw 'boost: extraction failed' }
    Move-Item (Join-Path $Dist "boost_$underscored") $boostRoot
    Remove-Item $tarball
    Set-Content $boostMarker -Value $BoostVersion -Encoding ascii
}

# Per-arch prefix; 'openssl' (no suffix) for x64 so existing caches
# stay valid.
$opensslPrefix = Join-Path $Dist 'openssl'
if ($Arch -ne 'x64') { $opensslPrefix = Join-Path $Dist "openssl-$Arch" }
& (Join-Path $PSScriptRoot 'build-openssl.ps1') -Prefix $opensslPrefix -Compiler $OpensslCompiler -Arch $Arch
if ($LASTEXITCODE -ne 0) { throw 'openssl build failed' }

# ---- the LTO build environment ----------------------------------------------
$env:CC = 'clang-cl'
$env:CXX = 'clang-cl'
# Cargo build scripts that compile C through cc-rs (ring) archive their
# objects themselves, and with -flto=full those are LLVM bitcode, which
# MSVC lib.exe cannot read (LNK1107). Pin the LLVM archiver for them --
# the CMake-built parts already get it via lto-toolchain.cmake. Without
# the pin, cc-rs only prefers llvm-lib when its compiler is clang-cl,
# and ring swaps the compiler to plain clang on arm64, landing on the
# dev shell's lib.exe.
$env:AR = 'llvm-lib'
if ($Arch -eq 'x64') {
    # -msse4.2: libtorrent's popcnt/crc32c fast paths are runtime-cpuid-guarded
    # but reach the MSVC intrinsics on clang-cl (no __GNUC__ inline-asm branch),
    # and clang refuses those without the target feature. This sets the CPU
    # floor to SSE4.2 (Nehalem, 2008) -- comfortably below any Windows 10/11
    # machine.
    $archCFlags = '-msse4.2'
} else {
    # clang-cl targets its host by default; everything else about the
    # cross build (LIB/INCLUDE, the archiver, the linker) is arch-neutral
    # or supplied by the arm64 dev shell. The CPU floor stays baseline
    # armv8: libtorrent's crc32 fast path needs +crc and quietly falls
    # back to the table implementation without it.
    $archCFlags = "--target=$clangTarget"
}
$env:CFLAGS = "-flto=full $archCFlags"
$env:CXXFLAGS = "-flto=full $archCFlags"
$env:CMAKE_GENERATOR = 'Ninja'
# The arm64 variant additionally declares the cross target system.
$toolchainFile = 'lto-toolchain.cmake'
if ($Arch -eq 'arm64') { $toolchainFile = 'lto-toolchain-arm64.cmake' }
$env:CMAKE_TOOLCHAIN_FILE = Join-Path $PSScriptRoot $toolchainFile
$env:BOOST_ROOT = $boostRoot
# CMake 4 has no FindBoost module; config mode finds our shim through the
# prefix path (harmless on older CMake, where the module wins).
$env:CMAKE_PREFIX_PATH = Join-Path $PSScriptRoot 'boost-config-shim'
$env:OPENSSL_ROOT_DIR = $opensslPrefix
$env:LIBCLANG_PATH = Split-Path -Parent (Get-Command clang-cl).Source
# Hybrid CRT: compile /MT everywhere (crt-static; the toolchain file pins
# the CMake side), then link the OS-provided universal CRT instead of the
# static one. Only rustc links final binaries, so this lives in RUSTFLAGS.
$env:RUSTFLAGS = '-Ctarget-feature=+crt-static -Clinker-plugin-lto -Clinker=lld-link ' +
    '-Clink-arg=/NODEFAULTLIB:libucrt.lib -Clink-arg=/DEFAULTLIB:ucrt.lib'
$env:CARGO_PROFILE_RELEASE_LTO = 'fat'
$env:CARGO_PROFILE_RELEASE_CODEGEN_UNITS = '1'

# The explicit --target keeps RUSTFLAGS away from host artifacts (build
# scripts, proc-macro dylibs): -Clinker-plugin-lto cannot be combined
# with -Cprefer-dynamic on Windows targets. ($target is per-arch, see
# the top of the script.)
$targetDir = Join-Path $RepoRoot "target\$target\release"

Write-Host 'cargo: building rsbtd + rsbtctl (release, fat LTO)'
cargo build --release --locked --features vendored -p rsbtd -p rsbtctl --target $target
if ($LASTEXITCODE -ne 0) { throw 'cargo build failed' }

# ---- verify machine type and CRT shape --------------------------------------
# Hybrid CRT means: universal CRT via api-ms-win-crt-* DLLs, but no
# vcruntime/msvcp DLL dependencies (those are linked statically). The
# machine check guards the cross build against any component silently
# falling back to the host arch.
$machine = @{ x64 = 'x64'; arm64 = 'ARM64' }[$Arch]
foreach ($exe in 'rsbtd.exe', 'rsbtctl.exe') {
    $headers = & dumpbin /nologo /headers (Join-Path $targetDir $exe) | Out-String
    if ($headers -notmatch "machine \($machine\)") {
        throw "$exe is not a $machine binary; the $Arch build produced the wrong architecture"
    }
    $deps = & dumpbin /nologo /dependents (Join-Path $targetDir $exe) | Out-String
    if ($deps -match '(?i)vcruntime|msvcp') {
        throw "$exe depends on the VC runtime DLLs; the hybrid-CRT link is broken"
    }
    if ($deps -notmatch '(?i)api-ms-win-crt-') {
        throw "$exe does not import the universal CRT; the hybrid-CRT link is broken"
    }
}

# ---- workspace test suite ---------------------------------------------------
# Runs after the release build (artifacts amortized either way) and
# before the MSI: a red suite must never produce an installer. Dev
# profile with the LTO pieces stripped, in its own target dir: fat-LTO-
# linking every test binary would take longer than the build itself, and
# build.rs does not watch CFLAGS/RUSTFLAGS, so sharing target\ with the
# LTO build would link its stale bitcode archives. The CRT shape stays
# identical (crt-static + ucrt swap): the cached OpenSSL is built
# VC-WIN64A-HYBRIDCRT and must not meet /MD.
if (-not $SkipTests) {
    Write-Host 'cargo: running the workspace test suite (dev profile, no LTO)'
    $saved = @{}
    foreach ($name in 'CFLAGS', 'CXXFLAGS', 'RUSTFLAGS',
             'CARGO_PROFILE_RELEASE_LTO', 'CARGO_PROFILE_RELEASE_CODEGEN_UNITS',
             'CARGO_TARGET_DIR') {
        $saved[$name] = [Environment]::GetEnvironmentVariable($name)
    }
    try {
        $env:CFLAGS = $archCFlags
        $env:CXXFLAGS = $archCFlags
        $env:RUSTFLAGS = '-Ctarget-feature=+crt-static -Clinker=lld-link ' +
            '-Clink-arg=/NODEFAULTLIB:libucrt.lib -Clink-arg=/DEFAULTLIB:ucrt.lib'
        Remove-Item Env:CARGO_PROFILE_RELEASE_LTO -ErrorAction SilentlyContinue
        Remove-Item Env:CARGO_PROFILE_RELEASE_CODEGEN_UNITS -ErrorAction SilentlyContinue
        $env:CARGO_TARGET_DIR = Join-Path $RepoRoot 'target\check'
        cargo test --workspace --locked --features vendored --target $target
        if ($LASTEXITCODE -ne 0) { throw 'cargo test failed' }
    }
    finally {
        foreach ($name in $saved.Keys) {
            [Environment]::SetEnvironmentVariable($name, $saved[$name])
        }
    }
}

# ---- MSI --------------------------------------------------------------------
$version = (Select-String -Path (Join-Path $RepoRoot 'Cargo.toml') -Pattern '^version\s*=\s*"([^"]+)"').Matches[0].Groups[1].Value
if (-not $version) { throw 'cannot read the workspace version from Cargo.toml' }

Copy-Item (Join-Path $targetDir 'rsbtd.exe') $Stage -Force
Copy-Item (Join-Path $targetDir 'rsbtctl.exe') $Stage -Force

$notices = Join-Path $RepoRoot 'THIRD-PARTY-NOTICES.md'
if (-not (Test-Path $notices)) {
    throw 'THIRD-PARTY-NOTICES.md is missing; regenerate it with scripts/gen_notices.py'
}

# WiX >= 7 requires accepting the Open Source Maintenance Fee EULA on
# every invocation (https://wixtoolset.org/osmf/); WiX 6 has no such
# switch, so only pass it where needed.
$acceptEula = @()
if ([int]($wixVersion.Split('.')[0]) -ge 7) { $acceptEula = @('-acceptEula', 'wix7') }

# WiX extensions are nuget-style packages, fetched like any other build
# dependency (version-matched to the installed wix tool).
foreach ($ext in 'WixToolset.UI.wixext', 'WixToolset.Util.wixext') {
    wix extension add @acceptEula -g "$ext/$wixVersion" | Out-Null
}

$msi = Join-Path $Dist "rsbtd-$version-$Arch.msi"
Write-Host "wix: building $msi"
wix build (Join-Path $PSScriptRoot 'installer\Package.wxs') `
    @acceptEula `
    -arch $Arch `
    -ext WixToolset.UI.wixext -ext WixToolset.Util.wixext `
    -d "ProductVersion=$version" `
    -d "StageDir=$Stage" `
    -d "WebUiDist=$WebUiDist" `
    -d "IconPath=$(Join-Path $RepoRoot 'rsbtd\assets\rsbtd.ico')" `
    -d "LicenseRtf=$(Join-Path $PSScriptRoot 'installer\License.rtf')" `
    -d "NoticesPath=$notices" `
    -d "LicensePath=$(Join-Path $RepoRoot 'LICENSE')" `
    -o $msi
if ($LASTEXITCODE -ne 0) { throw 'wix build failed' }

Write-Host "built $msi"

if ($Smoke) {
    & (Join-Path $PSScriptRoot 'smoke-test.ps1') -Msi $msi
    if ($LASTEXITCODE -ne 0) { throw 'smoke test failed' }
}
