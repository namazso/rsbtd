# Copyright (C) 2026  namazso <admin@namazso.eu>
# SPDX-License-Identifier: MPL-2.0
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.

# Smoke test for the Windows MSI: silent per-user install, first-start
# token generation by the app, daemon + API checks, token survival
# across uninstall/reinstall, silent uninstall. Registry/HTTP level only
# -- no desktop interaction -- so it runs headless in CI. (The app's
# first-start autostart prompt stays unanswered; the daemon deliberately
# does not wait for it.)
#
#   windows\smoke-test.ps1 -Msi dist\windows\rsbtd-<ver>-x64.msi

[CmdletBinding()]
param(
    [Parameter(Mandatory)] [string]$Msi
)

$ErrorActionPreference = 'Stop'
$Msi = (Resolve-Path $Msi).Path
$logDir = Join-Path (Split-Path -Parent $Msi) 'smoke-logs'
New-Item -ItemType Directory -Force $logDir | Out-Null

function Invoke-Msiexec([string[]]$Arguments, [string]$What) {
    $p = Start-Process msiexec.exe -ArgumentList $Arguments -Wait -PassThru
    if ($p.ExitCode -ne 0) {
        Get-Content (Join-Path $logDir '*.log') -Tail 50 -ErrorAction SilentlyContinue
        throw "$What failed with msiexec exit code $($p.ExitCode)"
    }
}

function Get-ConfigValue([string]$Name) {
    (Get-ItemProperty -Path 'HKCU:\Software\rsbtd' -Name $Name -ErrorAction Stop).$Name
}

function Wait-For([scriptblock]$Check, [string]$What, [int]$Attempts = 30) {
    for ($i = 0; $i -lt $Attempts; $i++) {
        try { if (& $Check) { return } } catch {}
        Start-Sleep -Seconds 1
    }
    throw "timed out waiting for: $What"
}

Write-Host 'smoke: installing'
Invoke-Msiexec @('/i', "`"$Msi`"", '/qn', '/l*v', "`"$logDir\install.log`"") 'install'

$installDir = Join-Path $env:LOCALAPPDATA 'Programs\rsbtd'
foreach ($f in 'rsbtd.exe', 'rsbtctl.exe', 'webui\index.html') {
    if (-not (Test-Path (Join-Path $installDir $f))) { throw "missing installed file: $f" }
}

# The installer writes no configuration; rsbtd.exe (launched by the
# install) generates the API token on its first start, and the listen
# address is the built-in default.
Write-Host 'smoke: waiting for the daemon'
$listen = '127.0.0.1:3928'
Wait-For { Get-Process rsbtd -ErrorAction SilentlyContinue } 'rsbtd.exe to start'
Wait-For { Get-ConfigValue 'Token' } 'the first-start token'
$token = Get-ConfigValue 'Token'
if ($token -notmatch '^[0-9a-f]{64}$') { throw "generated token looks wrong: '$token'" }
Wait-For { (Invoke-WebRequest "http://$listen/healthz" -UseBasicParsing -TimeoutSec 2).StatusCode -eq 200 } 'the API to come up'

# Web UI is served on /, and the API rejects a missing/wrong token.
$root = Invoke-WebRequest "http://$listen/" -UseBasicParsing
if ($root.Content -notmatch '<div id="root">|<!doctype html>') { throw 'GET / does not serve the web UI' }
$unauthorized = $null
try {
    Invoke-WebRequest "http://$listen/graphql" -UseBasicParsing -Method Post `
        -ContentType 'application/json' -Body '{"query":"{ __typename }"}' | Out-Null
} catch { $unauthorized = $_.Exception.Response.StatusCode.value__ }
if ($unauthorized -ne 401) { throw "expected 401 without a token, got '$unauthorized'" }
$authorized = Invoke-WebRequest "http://$listen/graphql" -UseBasicParsing -Method Post `
    -ContentType 'application/json' -Headers @{ Authorization = "Bearer $token" } `
    -Body '{"query":"{ __typename }"}'
if ($authorized.StatusCode -ne 200) { throw 'authenticated GraphQL request failed' }

& (Join-Path $installDir 'rsbtctl.exe') --url "http://$listen" --token $token version
if ($LASTEXITCODE -ne 0) { throw 'rsbtctl version failed' }

Write-Host 'smoke: uninstalling'
Invoke-Msiexec @('/x', "`"$Msi`"", '/qn', '/l*v', "`"$logDir\uninstall.log`"") 'uninstall'
Wait-For { -not (Get-Process rsbtd -ErrorAction SilentlyContinue) } 'rsbtd.exe to stop'
if (Test-Path $installDir) { throw 'install directory still present after uninstall' }
# The autostart prompt was never answered; closing the app mid-prompt
# (as this uninstall just did) must not count as a Yes.
if (Get-ItemProperty -Path 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run' -Name rsbtd -ErrorAction SilentlyContinue) {
    throw 'autostart was enabled without an answer to the prompt'
}
# Config (incl. the token) deliberately survives uninstall...
if ((Get-ConfigValue 'Token') -ne $token) { throw 'token did not survive uninstall' }

Write-Host 'smoke: reinstalling (must keep the token)'
Invoke-Msiexec @('/i', "`"$Msi`"", '/qn', '/l*v', "`"$logDir\reinstall.log`"") 'reinstall'
Wait-For { Get-Process rsbtd -ErrorAction SilentlyContinue } 'rsbtd.exe to start again'
if ((Get-ConfigValue 'Token') -ne $token) { throw 'reinstall regenerated the token' }

Write-Host 'smoke: final uninstall'
Invoke-Msiexec @('/x', "`"$Msi`"", '/qn', '/l*v', "`"$logDir\uninstall2.log`"") 'final uninstall'
Wait-For { -not (Get-Process rsbtd -ErrorAction SilentlyContinue) } 'rsbtd.exe to stop'

Write-Host 'smoke: OK'
