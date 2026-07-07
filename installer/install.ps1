# CopperMoon Installer — Windows
# Usage: irm https://raw.githubusercontent.com/coppermoondev/coppermoon/refs/heads/main/installer/install.ps1 | iex
#
# Environment variables:
#   COPPERMOON_INSTALL_DIR     — Custom install directory (default: ~\.coppermoon\bin)
#   COPPERMOON_VERSION         — Specific version to install (default: latest)
#   COPPERMOON_NO_MODIFY_PATH  — Set to 1 to skip PATH modification

param(
    [switch]$NoModifyPath
)

$ErrorActionPreference = "Stop"

# ─── Colors ─────────────────────────────────────────────────────────
function Write-Copper { param($Msg) Write-Host "> " -ForegroundColor DarkYellow -NoNewline; Write-Host $Msg }
function Write-Ok { param($Msg) Write-Host "✓ " -ForegroundColor Green -NoNewline; Write-Host $Msg }
function Write-Warn { param($Msg) Write-Host "! " -ForegroundColor Yellow -NoNewline; Write-Host $Msg }
function Write-Err { param($Msg) Write-Host "✗ " -ForegroundColor Red -NoNewline; Write-Host $Msg; exit 1 }

# ─── Banner ─────────────────────────────────────────────────────────
Write-Host ""
Write-Host "  ┌─────────────────────────────────────┐" -ForegroundColor DarkYellow
Write-Host "  │      🌙 CopperMoon Installer        │" -ForegroundColor DarkYellow
Write-Host "  │      Write Lua. Run at Rust speed.   │" -ForegroundColor DarkYellow
Write-Host "  └─────────────────────────────────────┘" -ForegroundColor DarkYellow
Write-Host ""

# ─── Detect Architecture ────────────────────────────────────────────
$Arch = if ($env:PROCESSOR_ARCHITECTURE -eq "AMD64" -or $env:PROCESSOR_ARCHITECTURE -eq "x86_64") {
    "x86_64"
} elseif ($env:PROCESSOR_ARCHITECTURE -eq "ARM64") {
    "aarch64"
} else {
    Write-Err "Unsupported architecture: $env:PROCESSOR_ARCHITECTURE"
}

$Target = "${Arch}-pc-windows-msvc"
Write-Copper "Detected platform: Windows $Arch ($Target)"

# ─── Configuration ──────────────────────────────────────────────────
$InstallDir = if ($env:COPPERMOON_INSTALL_DIR) { $env:COPPERMOON_INSTALL_DIR } else { "$env:USERPROFILE\.coppermoon\bin" }
$Version = if ($env:COPPERMOON_VERSION) { $env:COPPERMOON_VERSION } else { "latest" }
$GithubRepo = "coppermoondev/coppermoon"

if ($Version -eq "latest") {
    $DownloadUrl = "https://github.com/$GithubRepo/releases/latest/download/coppermoon-$Target.zip"
} else {
    $DownloadUrl = "https://github.com/$GithubRepo/releases/download/v$Version/coppermoon-$Target.zip"
}

Write-Copper "Install directory: $InstallDir"
Write-Copper "Version: $Version"

# ─── Create install directory ────────────────────────────────────────
if (-not (Test-Path $InstallDir)) {
    New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
}

# ─── Download ────────────────────────────────────────────────────────
$TmpDir = Join-Path ([System.IO.Path]::GetTempPath()) "coppermoon-install-$(Get-Random)"
New-Item -ItemType Directory -Path $TmpDir -Force | Out-Null
$Archive = Join-Path $TmpDir "coppermoon.zip"

Write-Copper "Downloading CopperMoon..."

try {
    $ProgressPreference = 'SilentlyContinue'
    Invoke-WebRequest -Uri $DownloadUrl -OutFile $Archive -UseBasicParsing
} catch {
    Remove-Item -Recurse -Force $TmpDir -ErrorAction SilentlyContinue
    Write-Err "Download failed. Check that version '$Version' exists for $Target.`n  URL: $DownloadUrl`n  Error: $_"
}

# ─── Extract ─────────────────────────────────────────────────────────
Write-Copper "Extracting binaries..."

try {
    Expand-Archive -Path $Archive -DestinationPath $TmpDir -Force
} catch {
    Remove-Item -Recurse -Force $TmpDir -ErrorAction SilentlyContinue
    Write-Err "Failed to extract archive. The download may be corrupted."
}

# Move binaries to install dir
$Binaries = @("coppermoon.exe", "harbor.exe", "shipyard.exe", "quarry.exe", "andesite.exe")
foreach ($bin in $Binaries) {
    # Check root and subdirectories
    $found = Get-ChildItem -Path $TmpDir -Filter $bin -Recurse -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($found) {
        Copy-Item -Path $found.FullName -Destination (Join-Path $InstallDir $bin) -Force
    }
}

# Cleanup
Remove-Item -Recurse -Force $TmpDir -ErrorAction SilentlyContinue

Write-Ok "Binaries installed to $InstallDir"

# ─── Update PATH ─────────────────────────────────────────────────────
$SkipPath = $NoModifyPath -or ($env:COPPERMOON_NO_MODIFY_PATH -eq "1")

if (-not $SkipPath) {
    $UserPath = [Environment]::GetEnvironmentVariable("Path", "User")

    if ($UserPath -and $UserPath.Split(";") -contains $InstallDir) {
        Write-Copper "Already in PATH"
    } else {
        if ($UserPath) {
            $NewPath = "$InstallDir;$UserPath"
        } else {
            $NewPath = $InstallDir
        }
        [Environment]::SetEnvironmentVariable("Path", $NewPath, "User")
        # Also update current session
        $env:Path = "$InstallDir;$env:Path"
        Write-Ok "Added to User PATH"
    }
} else {
    Write-Warn "Skipping PATH modification"
}

# ─── Verify ──────────────────────────────────────────────────────────
Write-Host ""

foreach ($bin in @("coppermoon", "harbor", "shipyard", "quarry")) {
    $binPath = Join-Path $InstallDir "$bin.exe"
    if (Test-Path $binPath) {
        try {
            $ver = & $binPath --version 2>&1
            Write-Ok "$($bin.PadRight(12)) $ver"
        } catch {
            Write-Ok "$($bin.PadRight(12)) installed"
        }
    }
}

# ─── Success ─────────────────────────────────────────────────────────
Write-Host ""
Write-Host "  Installation complete!" -ForegroundColor Green
Write-Host ""
Write-Host "  Restart your terminal, then:" -ForegroundColor DarkGray
Write-Host ""
Write-Host "    shipyard new my-app --template web" -ForegroundColor White
Write-Host "    cd my-app; shipyard dev" -ForegroundColor White
Write-Host ""
Write-Host "  Documentation:  " -ForegroundColor DarkGray -NoNewline
Write-Host "https://docs.coppermoon.dev" -ForegroundColor DarkYellow
Write-Host "  GitHub:         " -ForegroundColor DarkGray -NoNewline
Write-Host "https://github.com/$GithubRepo" -ForegroundColor DarkYellow
Write-Host ""
