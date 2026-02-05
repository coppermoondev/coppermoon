# CopperMoon — Build & Release Script
# Usage:
#   .\build.ps1 build                    Debug build
#   .\build.ps1 release                  Release build
#   .\build.ps1 archive                  Release build + create distributable archive
#   .\build.ps1 publish -Version 0.1.0   Tag + push → triggers GitHub Actions release
#   .\build.ps1 clean                    Remove build artifacts
#   .\build.ps1 help                     Show usage

param(
    [Parameter(Position = 0)]
    [ValidateSet("build", "release", "archive", "publish", "clean", "help")]
    [string]$Command = "help",

    [string]$Version
)

$ErrorActionPreference = "Stop"

$Repo = "coppermoondev/coppermoon"
$Binaries = @("coppermoon", "harbor", "shipyard")
$DistDir = "dist"

# Detect platform
if ($IsLinux) {
    $Arch = if ((uname -m) -eq "aarch64") { "aarch64" } else { "x86_64" }
    $Target = "$Arch-unknown-linux-gnu"
    $Ext = ""
    $ArchiveExt = "tar.gz"
} elseif ($IsMacOS) {
    $Arch = if ((uname -m) -eq "arm64") { "aarch64" } else { "x86_64" }
    $Target = "$Arch-apple-darwin"
    $Ext = ""
    $ArchiveExt = "tar.gz"
} else {
    $Target = "x86_64-pc-windows-msvc"
    $Ext = ".exe"
    $ArchiveExt = "zip"
}

$ArchiveName = "coppermoon-$Target"

function Write-Step { param($Msg) Write-Host "  > " -ForegroundColor DarkYellow -NoNewline; Write-Host $Msg }
function Write-Ok { param($Msg) Write-Host "  + " -ForegroundColor Green -NoNewline; Write-Host $Msg }

# ─── Commands ─────────────────────────────────────────────────────────

function Invoke-Build {
    Write-Step "Debug build..."
    cargo build
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    Write-Ok "Done"
}

function Invoke-Release {
    Write-Step "Release build..."
    cargo build --release
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    Write-Ok "Done"
}

function Invoke-Archive {
    Invoke-Release

    Write-Step "Creating archive for $Target..."

    $archiveDir = Join-Path $DistDir $ArchiveName
    New-Item -ItemType Directory -Path $archiveDir -Force | Out-Null

    foreach ($bin in $Binaries) {
        $src = "target/release/$bin$Ext"
        if (Test-Path $src) {
            Copy-Item $src (Join-Path $archiveDir "$bin$Ext")
        }
    }

    if ($ArchiveExt -eq "zip") {
        $zipPath = Join-Path $DistDir "$ArchiveName.zip"
        if (Test-Path $zipPath) { Remove-Item $zipPath }
        Compress-Archive -Path "$archiveDir/*" -DestinationPath $zipPath -Force
    } else {
        Push-Location $DistDir
        tar czf "$ArchiveName.tar.gz" $ArchiveName
        Pop-Location
    }

    Remove-Item -Recurse -Force $archiveDir
    Write-Ok "Done: $DistDir/$ArchiveName.$ArchiveExt"
}

function Invoke-Publish {
    if (-not $Version) {
        Write-Host "ERROR: Version required. Usage: .\build.ps1 publish -Version 0.1.0" -ForegroundColor Red
        exit 1
    }

    Write-Host ""
    Write-Host "  Publishing CopperMoon v$Version..." -ForegroundColor White
    Write-Host ""

    Write-Step "Cleaning up existing tag v$Version (if any)"
    $ErrorActionPreference = "Continue"
    git tag -d "v$Version" 2>&1 | Out-Null
    git push origin --delete "v$Version" 2>&1 | Out-Null
    $ErrorActionPreference = "Stop"

    Write-Step "Creating tag v$Version"
    git tag -a "v$Version" -m "Release v$Version"
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

    Write-Step "Pushing tag to origin"
    git push origin "v$Version"
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

    Write-Host ""
    Write-Ok "Done! GitHub Actions will now build and create the release."
    Write-Host "     https://github.com/$Repo/actions" -ForegroundColor DarkGray
    Write-Host "     https://github.com/$Repo/releases/tag/v$Version" -ForegroundColor DarkGray
    Write-Host ""
}

function Invoke-Clean {
    Write-Step "Cleaning..."
    cargo clean
    if (Test-Path $DistDir) { Remove-Item -Recurse -Force $DistDir }
    Write-Ok "Done"
}

function Invoke-Help {
    Write-Host ""
    Write-Host "  CopperMoon Build System" -ForegroundColor White
    Write-Host ""
    Write-Host "  .\build.ps1 build                    " -NoNewline; Write-Host "Debug build" -ForegroundColor DarkGray
    Write-Host "  .\build.ps1 release                  " -NoNewline; Write-Host "Release build" -ForegroundColor DarkGray
    Write-Host "  .\build.ps1 archive                  " -NoNewline; Write-Host "Release build + archive for current platform" -ForegroundColor DarkGray
    Write-Host "  .\build.ps1 publish -Version x.y.z   " -NoNewline; Write-Host "Tag + push (triggers GitHub Actions release)" -ForegroundColor DarkGray
    Write-Host "  .\build.ps1 clean                    " -NoNewline; Write-Host "Remove build artifacts" -ForegroundColor DarkGray
    Write-Host ""
    Write-Host "  The 'publish' command creates a git tag and pushes it." -ForegroundColor DarkGray
    Write-Host "  GitHub Actions then builds for all platforms automatically:" -ForegroundColor DarkGray
    Write-Host "    - x86_64-unknown-linux-gnu" -ForegroundColor DarkGray
    Write-Host "    - aarch64-unknown-linux-gnu" -ForegroundColor DarkGray
    Write-Host "    - x86_64-apple-darwin" -ForegroundColor DarkGray
    Write-Host "    - aarch64-apple-darwin" -ForegroundColor DarkGray
    Write-Host "    - x86_64-pc-windows-msvc" -ForegroundColor DarkGray
    Write-Host ""
}

# ─── Dispatch ─────────────────────────────────────────────────────────

switch ($Command) {
    "build"   { Invoke-Build }
    "release" { Invoke-Release }
    "archive" { Invoke-Archive }
    "publish" { Invoke-Publish }
    "clean"   { Invoke-Clean }
    "help"    { Invoke-Help }
}
