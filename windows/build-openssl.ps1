# Copyright (C) 2026  namazso <admin@namazso.eu>
# SPDX-License-Identifier: MPL-2.0
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.

# Builds a static OpenSSL for the Windows rsbtd build.
#
# Compiled with clang-cl (same LLVM toolchain as the rest of the build,
# though without LTO -- plain machine-code archives mix fine under
# lld-link) against the hybrid CRT (static vcruntime, dynamic ucrt),
# matching the product binaries. On x64 that is OpenSSL's
# VC-WIN64A-HYBRIDCRT target; on arm64 the plain
# VC-CLANG-WIN64-CLANGASM-ARM target is equivalent: no-shared static
# libs compile /MT /Zl (CRT-neutral objects, no default-lib directives)
# and the HYBRIDCRT configs only add lflags for apps/DLLs, which
# no-shared/no-apps/no-tests never builds. clang-cl doubles as the
# assembler on arm64, so NASM is x64-only.
#
# Requires a VS developer environment targeting $Arch (nmake, lib, link
# on PATH and INCLUDE/LIB set) plus perl (and nasm on x64); build.ps1
# arranges all of that. The result is cached: an existing install with
# a matching version marker is reused.

[CmdletBinding()]
param(
    # Pinned OpenSSL release (LTS line). Bump deliberately, with the hash.
    [string]$Version = '3.5.7',
    [string]$Sha256 = 'a8c0d28a529ca480f9f36cf5792e2cd21984552a3c8e4aa11a24aa31aeac98e8',
    # Install prefix; libs land in $Prefix\lib, headers in $Prefix\include.
    [Parameter(Mandatory)] [string]$Prefix,
    # Compiler driving the OpenSSL build. clang-cl keeps the toolchain
    # uniform; pass 'cl' as an escape hatch if a new OpenSSL release
    # misbehaves under clang-cl.
    [string]$Compiler = 'clang-cl',
    # Target architecture (must match the VS dev shell's target arch).
    [ValidateSet('x64', 'arm64')] [string]$Arch = 'x64'
)

$ErrorActionPreference = 'Stop'

# On arm64 clang-cl assembles the aarch64 perlasm output even when cl
# compiles the C (the CLANGASM configs), so it is required either way.
$configs = @{
    'x64-clang-cl'   = 'VC-WIN64A-HYBRIDCRT'
    'x64-cl'         = 'VC-WIN64A-HYBRIDCRT'
    'arm64-clang-cl' = 'VC-CLANG-WIN64-CLANGASM-ARM'
    'arm64-cl'       = 'VC-WIN64-CLANGASM-ARM'
}
$configName = $configs["$Arch-$Compiler"]
if (-not $configName) { throw "openssl: no Configure target for arch=$Arch compiler=$Compiler" }

# Marker format: the historical "$Version-$Compiler" for x64 (existing
# caches stay valid), arch-suffixed otherwise.
$markerValue = "$Version-$Compiler"
if ($Arch -ne 'x64') { $markerValue += "-$Arch" }

$marker = Join-Path $Prefix '.rsbtd-openssl-version'
if ((Test-Path (Join-Path $Prefix 'lib\libcrypto.lib')) -and
    (Test-Path $marker) -and
    ((Get-Content $marker -ErrorAction SilentlyContinue) -eq $markerValue)) {
    Write-Host "openssl: reusing cached $Version at $Prefix"
    exit 0
}

$tools = @('perl', 'nmake', $Compiler)
if ($Arch -eq 'x64') { $tools += 'nasm' } else { $tools += 'clang-cl' }
foreach ($tool in ($tools | Select-Object -Unique)) {
    if (-not (Get-Command $tool -ErrorAction SilentlyContinue)) {
        throw "openssl: required tool '$tool' not found on PATH (run from build.ps1 or a VS dev shell)"
    }
}

$work = Join-Path ([System.IO.Path]::GetTempPath()) "rsbtd-openssl-$Version"
if (Test-Path $work) { Remove-Item -Recurse -Force $work }
New-Item -ItemType Directory -Force $work | Out-Null

$tarball = Join-Path $work "openssl-$Version.tar.gz"
$url = "https://github.com/openssl/openssl/releases/download/openssl-$Version/openssl-$Version.tar.gz"
Write-Host "openssl: downloading $url"
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
(New-Object System.Net.WebClient).DownloadFile($url, $tarball)

$actual = (Get-FileHash -Algorithm SHA256 $tarball).Hash.ToLowerInvariant()
if ($actual -ne $Sha256.ToLowerInvariant()) {
    throw "openssl: checksum mismatch for $url`n  expected $Sha256`n  actual   $actual"
}

Write-Host 'openssl: extracting'
tar -xzf $tarball -C $work
if ($LASTEXITCODE -ne 0) { throw 'openssl: extraction failed' }
$src = Join-Path $work "openssl-$Version"

Push-Location $src
try {
    $env:CC = $Compiler
    Write-Host "openssl: configuring ($configName, CC=$Compiler)"
    perl Configure $configName no-shared no-tests no-apps `
        --prefix="$Prefix" --openssldir="$Prefix\ssl"
    if ($LASTEXITCODE -ne 0) { throw 'openssl: Configure failed' }

    Write-Host 'openssl: building (nmake)'
    nmake
    if ($LASTEXITCODE -ne 0) { throw 'openssl: nmake failed' }

    # clang-cl embeds debug info in the objects (/Z7 style) instead of
    # writing the compiler PDB that install_sw expects to copy.
    if (-not (Test-Path ossl_static.pdb)) {
        New-Item -ItemType File ossl_static.pdb | Out-Null
    }

    Write-Host 'openssl: installing'
    nmake install_sw
    if ($LASTEXITCODE -ne 0) { throw 'openssl: nmake install_sw failed' }
}
finally {
    Pop-Location
}

Set-Content -Path $marker -Value $markerValue -Encoding ascii
Remove-Item -Recurse -Force $work
Write-Host "openssl: installed $Version to $Prefix"
