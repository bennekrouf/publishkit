#Requires -Version 5.1
<#
.SYNOPSIS
    Blog Manager - Windows dependency installer.

.DESCRIPTION
    Installs the tools that blog-manager needs on Windows:
        Node.js  v20 LTS   (required by generate-blog-data.js when publishing posts)
        WebView2 runtime   (required by the Dioxus desktop shell — usually pre-installed on Windows 11)

    Already-installed tools at the correct version are skipped.
    Requires an internet connection. No elevation needed: without it, Node.js is
    unpacked into %LOCALAPPDATA%\Programs\nodejs and added to the user PATH, and
    WebView2 is installed per-user. Run as Administrator for machine-wide installs.
#>
param(
    [switch]$NoPrompt   # Set by the graphical installer — skips all Read-Host pauses
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$NODE_VERSION = '20.18.1'
$NODE_URL     = "https://nodejs.org/dist/v${NODE_VERSION}/node-v${NODE_VERSION}-x64.msi"
$NODE_ZIP_URL = "https://nodejs.org/dist/v${NODE_VERSION}/node-v${NODE_VERSION}-win-x64.zip"
$NODE_USER_DIR = Join-Path $env:LOCALAPPDATA 'Programs\nodejs'
$TMP          = $env:TEMP
$IsAdmin      = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()
                ).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)

# ── Helpers ───────────────────────────────────────────────────────────────────
function Write-Step { param([string]$msg) Write-Host "`n>> $msg" -ForegroundColor Cyan }
function Write-Ok   { param([string]$msg) Write-Host "  OK  $msg" -ForegroundColor Green }
function Write-Skip { param([string]$msg) Write-Host "  --  $msg (already installed)" -ForegroundColor DarkGray }
function Write-Warn { param([string]$msg) Write-Host "  !!  $msg" -ForegroundColor Yellow }

function Refresh-Path {
    $machine = [System.Environment]::GetEnvironmentVariable('PATH', 'Machine')
    $user    = [System.Environment]::GetEnvironmentVariable('PATH', 'User')
    $env:PATH = "$machine;$user"
}

function Get-InstalledVersion {
    param([string]$Cmd, [string[]]$Arguments = @('--version'))
    try {
        $out = & $Cmd @Arguments 2>&1 | Out-String
        if ($out -match '(\d+\.\d+[\.\d]*)') { return $Matches[1] }
    } catch { }
    return $null
}

function Install-Msi {
    param([string]$Url, [string]$Label, [string]$OutFile)
    Write-Step "Installing $Label..."
    $msi = Join-Path $TMP $OutFile
    Write-Host "  Downloading $Url ..."
    (New-Object System.Net.WebClient).DownloadFile($Url, $msi)
    Write-Host "  Running installer (this may take a minute)..."
    $p = Start-Process msiexec.exe -ArgumentList "/i `"$msi`" /qn /norestart" -Wait -PassThru
    Remove-Item $msi -Force -ErrorAction SilentlyContinue
    if ($p.ExitCode -notin @(0, 3010)) {
        throw "$Label installer exited with code $($p.ExitCode)"
    }
    Refresh-Path
    Write-Ok "$Label installed"
}

# Per-user Node.js: unpack the official zip and put it on the user PATH.
function Install-NodeUser {
    param([string]$Label)
    Write-Step "Installing $Label (user scope)..."
    $zip = Join-Path $TMP 'node-setup.zip'
    $tmpDir = Join-Path $TMP 'node-setup'
    Write-Host "  Downloading $NODE_ZIP_URL ..."
    (New-Object System.Net.WebClient).DownloadFile($NODE_ZIP_URL, $zip)
    Remove-Item $tmpDir -Recurse -Force -ErrorAction SilentlyContinue
    Expand-Archive -Path $zip -DestinationPath $tmpDir -Force
    Remove-Item $NODE_USER_DIR -Recurse -Force -ErrorAction SilentlyContinue
    New-Item -ItemType Directory -Path (Split-Path $NODE_USER_DIR) -Force | Out-Null
    Move-Item (Join-Path $tmpDir "node-v${NODE_VERSION}-win-x64") $NODE_USER_DIR
    Remove-Item $zip, $tmpDir -Recurse -Force -ErrorAction SilentlyContinue

    $userPath = [System.Environment]::GetEnvironmentVariable('PATH', 'User')
    $parts = @($userPath -split ';' | Where-Object { $_ -and $_ -ne $NODE_USER_DIR })
    # Prepend so it wins over an older machine-wide Node.
    [System.Environment]::SetEnvironmentVariable('PATH', (@($NODE_USER_DIR) + $parts) -join ';', 'User')
    $env:PATH = "$NODE_USER_DIR;$env:PATH"
    Write-Ok "$Label installed"
}

function Install-Node {
    param([string]$Label)
    if ($IsAdmin) { Install-Msi $NODE_URL $Label "node-setup.msi" }
    else          { Install-NodeUser $Label }
}

Write-Host ""
Write-Host "  Blog Manager — Windows Setup" -ForegroundColor White
Write-Host "  ============================" -ForegroundColor White
Write-Host ""

# ── Node.js ───────────────────────────────────────────────────────────────────
Write-Step "Checking Node.js..."
$nodeVer = Get-InstalledVersion 'node'
if ($null -eq $nodeVer) {
    Install-Node "Node.js $NODE_VERSION"
    Write-Ok "Node.js installed ($(Get-InstalledVersion 'node'))"
} else {
    $major = [int]($nodeVer -split '\.')[0]
    if ($major -lt 18) {
        Write-Warn "Node.js $nodeVer is too old — upgrading to $NODE_VERSION..."
        Install-Node "Node.js $NODE_VERSION"
        Write-Ok "Node.js upgraded ($(Get-InstalledVersion 'node'))"
    } else {
        Write-Skip "Node.js $nodeVer"
    }
}

# ── WebView2 runtime ──────────────────────────────────────────────────────────
Write-Step "Checking WebView2 runtime..."

$wv2Key = 'HKLM:\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}'
$wv2User = "HKCU:\Software\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}"

if ((Test-Path $wv2Key) -or (Test-Path $wv2User)) {
    Write-Skip "WebView2 runtime already installed"
} else {
    Write-Host "  Downloading WebView2 bootstrapper..."
    $wv2 = Join-Path $TMP "MicrosoftEdgeWebview2Setup.exe"
    (New-Object System.Net.WebClient).DownloadFile(
        "https://go.microsoft.com/fwlink/p/?LinkId=2124703", $wv2)
    Write-Host "  Installing WebView2 (user scope)..."
    Start-Process $wv2 -ArgumentList "/silent /install" -Wait
    Remove-Item $wv2 -Force -ErrorAction SilentlyContinue
    Write-Ok "WebView2 runtime installed"
}

# ── Done ──────────────────────────────────────────────────────────────────────
Write-Host ""
Write-Host "  Setup complete." -ForegroundColor Green
Write-Host "  Launch blog-manager.exe to start the app."
Write-Host ""

if (-not $NoPrompt) {
    Read-Host "Press Enter to close"
}
