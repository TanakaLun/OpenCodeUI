#Requires -Version 5.1
<#
.SYNOPSIS
    Builds the portable Windows package for OpenCodeUI.

.DESCRIPTION
    Produces a self-contained folder (and a .zip next to it):

        OpenCode/
          OpenCode.exe        Tauri shell (portable feature compiled in)
          opencode.exe        backend shipped from GitHub releases
          portable.txt        marker: portable mode is on
          README.txt
          data/               created on first run, holds every writable file

    Requires: Node 18+, Rust stable (MSVC toolchain), Visual Studio Build Tools
    with "Desktop development with C++", and WebView2 runtime (preinstalled on
    Windows 10/11).

.EXAMPLE
    pwsh scripts/build-portable.ps1
    pwsh scripts/build-portable.ps1 -SkipFrontend -OutDir dist-portable
#>
param(
    [string]$OutDir = "dist-portable",
    [string]$OpencodeRepo = "anomalyco/opencode",
    [string]$OpencodeAsset = "",
    [switch]$SkipFrontend,
    [switch]$SkipBackend
)

$ErrorActionPreference = "Stop"
$repoRoot = Split-Path -Parent $PSScriptRoot
Push-Location $repoRoot
try {
    $appName = "OpenCode"
    $packageDir = Join-Path $repoRoot (Join-Path $OutDir $appName)

    # ---------------------------------------------------------------- frontend
    if (-not $SkipFrontend -and -not (Test-Path "node_modules")) {
        Write-Host "==> npm ci"
        npm ci
        if ($LASTEXITCODE -ne 0) { throw "npm ci failed" }
    }

    # ------------------------------------------------------------- tauri build
    # tauri runs `beforeBuildCommand` (npm run build) by itself, so the
    # frontend is only built once.
    Write-Host "==> tauri build (features=portable, no bundle)"
    npm run tauri build -- --features portable --no-bundle
    if ($LASTEXITCODE -ne 0) { throw "tauri build failed" }

    $exeCandidates = @(
        "src-tauri/target/release/opencodeui.exe",
        "src-tauri/target/release/OpenCode.exe"
    )
    $builtExe = $exeCandidates | Where-Object { Test-Path $_ } | Select-Object -First 1
    if (-not $builtExe) { throw "built executable not found under src-tauri/target/release" }

    if (Test-Path $packageDir) { Remove-Item $packageDir -Recurse -Force }
    New-Item -ItemType Directory -Path $packageDir -Force | Out-Null
    Copy-Item $builtExe (Join-Path $packageDir "$appName.exe")

    # ----------------------------------------------------------------- backend
    if (-not $SkipBackend) {
        if ($OpencodeAsset) {
            $downloadUrl = $OpencodeAsset
            $assetName = Split-Path $downloadUrl -Leaf
        } else {
            Write-Host "==> resolving latest opencode release from $OpencodeRepo"
            $headers = @{ "User-Agent" = "opencodeui-portable-build" }
            if ($env:GITHUB_TOKEN) {
                $headers["Authorization"] = "Bearer $($env:GITHUB_TOKEN)"
            }
            $release = Invoke-RestMethod -Uri "https://api.github.com/repos/$OpencodeRepo/releases/latest" `
                -Headers $headers
            $asset = $release.assets |
                Where-Object { $_.name -match "windows" -and $_.name -match "x64" -and $_.name -notmatch "baseline" } |
                Select-Object -First 1
            if (-not $asset) { throw "no windows-x64 asset found in $OpencodeRepo release $($release.tag_name)" }
            $downloadUrl = $asset.browser_download_url
            $assetName = $asset.name
        }

        Write-Host "==> downloading $assetName"
        $tmp = Join-Path ([System.IO.Path]::GetTempPath()) ("opencode-" + [guid]::NewGuid())
        New-Item -ItemType Directory -Path $tmp -Force | Out-Null
        $archive = Join-Path $tmp $assetName
        Invoke-WebRequest -Uri $downloadUrl -OutFile $archive

        if ($archive -match "\.zip$") {
            Expand-Archive -Path $archive -DestinationPath $tmp -Force
            $backend = Get-ChildItem -Path $tmp -Recurse -Filter "opencode.exe" | Select-Object -First 1
            if (-not $backend) { throw "opencode.exe not found inside $assetName" }
            Copy-Item $backend.FullName (Join-Path $packageDir "opencode.exe")
        } elseif ($archive -match "\.exe$") {
            Copy-Item $archive (Join-Path $packageDir "opencode.exe")
        } else {
            throw "unsupported backend asset: $assetName"
        }
        Remove-Item $tmp -Recurse -Force -ErrorAction SilentlyContinue
    }

    # --------------------------------------------------------------- portable
    @"
Portable mode is enabled while this file exists next to OpenCode.exe.
All data (settings, sessions, logs, WebView profile, opencode auth/config)
is stored in the ./data folder.
"@ | Set-Content -Path (Join-Path $packageDir "portable.txt") -Encoding UTF8

    @"
OpenCodeUI portable
===================

Run OpenCode.exe. Everything writable stays inside this folder:

  data\config\     window state, WSL server list
  data\logs\       application logs
  data\webview\    WebView2 profile (UI settings, localStorage)
  data\opencode\   opencode backend data/config/state/cache

Requirements
------------
* Windows 10/11 with the WebView2 runtime (preinstalled on up-to-date systems).
* Keep the whole folder together; do not move OpenCode.exe out on its own.

Updating
--------
Replace OpenCode.exe and opencode.exe with newer builds. The data folder and
portable.txt can stay as they are. Deleting data\ resets the app.
"@ | Set-Content -Path (Join-Path $packageDir "README.txt") -Encoding UTF8

    # -------------------------------------------------------------------- zip
    $zipPath = Join-Path $repoRoot (Join-Path $OutDir "OpenCodeUI-portable-windows-x64.zip")
    if (Test-Path $zipPath) { Remove-Item $zipPath -Force }
    Compress-Archive -Path $packageDir -DestinationPath $zipPath

    Write-Host ""
    Write-Host "Portable package: $packageDir"
    Write-Host "Archive:          $zipPath"
} finally {
    Pop-Location
}
