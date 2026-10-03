# Builds Brixo Player and Studio and puts them on playbrixo.com: Windows
# here, Mac on one of GitHub's Macs. Run from anywhere in PowerShell:
#   powershell -ExecutionPolicy Bypass -File tools\release.ps1
# It asks for the server's root password once, for the upload.
#
# The Mac part needs the GitHub CLI (winget install GitHub.cli, then
# "gh auth login" once) and your latest code pushed to GitHub, because
# that's what GitHub builds. Without those it releases Windows only and
# keeps the Mac downloads the site already has. -SkipMac skips it on purpose.
#
# Built the Mac version on Codemagic instead (codemagic.yaml)? Put the three
# files it gives you (BrixoPlayer.dmg, BrixoStudio.dmg, version.txt) in one
# folder and pass it: -MacFolder C:\Users\you\Downloads\brixo-mac
param(
    [string]$Server = "root@playbrixo.com",
    [string]$Site = "https://playbrixo.com",
    [switch]$SkipMac,
    [string]$MacFolder = ""
)
$ErrorActionPreference = "Stop"
Set-Location (Split-Path $PSScriptRoot)   # the repo folder

# Each release is stamped with the date and time; Player and Studio compare
# theirs with the website's to know when there's a newer one.
$version = Get-Date -Format "yyyy.MM.dd.HHmm"
$dist = Join-Path (Get-Location) "dist"
New-Item -ItemType Directory -Force $dist | Out-Null

# --- Mac: start GitHub's build first, so it runs while Windows builds ------
$macRun = $null
if ($MacFolder) {
    Write-Host "== Using the Mac build in $MacFolder"
    foreach ($f in "BrixoPlayer.dmg", "BrixoStudio.dmg", "version.txt") {
        if (-not (Test-Path (Join-Path $MacFolder $f))) { throw "$f isn't in $MacFolder. Download all three files from Codemagic into that folder." }
    }
} elseif ($SkipMac) {
    Write-Host "== Skipping the Mac version (-SkipMac)"
} elseif (-not (Get-Command gh -ErrorAction SilentlyContinue)) {
    Write-Host "== Skipping the Mac version: the GitHub CLI isn't installed (winget install GitHub.cli, then gh auth login)"
} else {
    git fetch --quiet 2>$null
    $ahead = git rev-list --count "@{u}..HEAD" 2>$null
    if ($LASTEXITCODE -ne 0) { $ahead = "?" }
    if ($ahead -ne "0") {
        Write-Host "== Skipping the Mac version: push your latest commits first (git push), since GitHub builds what's pushed"
    } else {
        if (git status --porcelain) {
            Write-Host "   (Heads up: you have changes that aren't committed. The Mac build won't have them.)"
        }
        Write-Host "== Starting the Mac build on GitHub"
        $asked = Get-Date
        gh workflow run mac.yml -f version=$version
        if ($LASTEXITCODE -eq 0) {
            # Find the run we just started.
            for ($i = 0; $i -lt 30 -and -not $macRun; $i++) {
                Start-Sleep -Seconds 2
                $runs = gh run list --workflow mac.yml --limit 5 --json databaseId,createdAt | ConvertFrom-Json
                $macRun = $runs | Where-Object { ([datetime]$_.createdAt) -ge $asked.AddSeconds(-60) } | Select-Object -First 1
            }
            if (-not $macRun) { Write-Host "   (Couldn't find the Mac build on GitHub; releasing Windows only.)" }
        } else {
            Write-Host "   (GitHub didn't start the Mac build; releasing Windows only. Try: gh auth login)"
        }
    }
}

# --- Windows ------------------------------------------------------------------
Write-Host "== Building Brixo $version for Windows (a few minutes)"
$env:BRIXO_BUILD = $version
cargo build --release -p brixo-player -p brixo-studio
$built = $LASTEXITCODE
Remove-Item Env:BRIXO_BUILD
if ($built -ne 0) { throw "The build failed; nothing was uploaded." }
Copy-Item "target\release\brixo-player.exe" (Join-Path $dist "BrixoPlayer.exe") -Force
Copy-Item "target\release\brixo-studio.exe" (Join-Path $dist "BrixoStudio.exe") -Force
$files = @((Join-Path $dist "BrixoPlayer.exe"), (Join-Path $dist "BrixoStudio.exe"))
$size = { param($name) (Get-Item (Join-Path $dist $name)).Length }
# Player and Studio check what they download against this before using it.
$sha = { param($name) (Get-FileHash -Algorithm SHA256 (Join-Path $dist $name)).Hash.ToLower() }
$versions = [ordered]@{
    player = [ordered]@{ version = $version; file = "BrixoPlayer.exe"; bytes = (& $size "BrixoPlayer.exe"); sha256 = (& $sha "BrixoPlayer.exe") }
    studio = [ordered]@{ version = $version; file = "BrixoStudio.exe"; bytes = (& $size "BrixoStudio.exe"); sha256 = (& $sha "BrixoStudio.exe") }
}

# --- Mac: from Codemagic, or wait for GitHub and fetch the .dmg files ----
$macDone = $false
if ($MacFolder) {
    # The version baked into the Mac apps, so their update check matches.
    $macVersion = (Get-Content (Join-Path $MacFolder "version.txt") -Raw).Trim()
    foreach ($dmg in "BrixoPlayer.dmg", "BrixoStudio.dmg") {
        Copy-Item (Join-Path $MacFolder $dmg) (Join-Path $dist $dmg) -Force
        $files += (Join-Path $dist $dmg)
    }
    $versions["player_mac"] = [ordered]@{ version = $macVersion; file = "BrixoPlayer.dmg"; bytes = (& $size "BrixoPlayer.dmg"); sha256 = (& $sha "BrixoPlayer.dmg") }
    $versions["studio_mac"] = [ordered]@{ version = $macVersion; file = "BrixoStudio.dmg"; bytes = (& $size "BrixoStudio.dmg"); sha256 = (& $sha "BrixoStudio.dmg") }
    $macDone = $true
} elseif ($macRun) {
    Write-Host "== Waiting for the Mac build on GitHub (10-20 minutes the first time, less after)"
    gh run watch $macRun.databaseId --exit-status --interval 30
    if ($LASTEXITCODE -eq 0) {
        $macDir = Join-Path $dist "mac"
        Remove-Item -Recurse -Force $macDir -ErrorAction SilentlyContinue
        gh run download $macRun.databaseId -n brixo-mac -D $macDir
        if ($LASTEXITCODE -eq 0) {
            foreach ($dmg in "BrixoPlayer.dmg", "BrixoStudio.dmg") {
                Copy-Item (Join-Path $macDir $dmg) (Join-Path $dist $dmg) -Force
                $files += (Join-Path $dist $dmg)
            }
            $versions["player_mac"] = [ordered]@{ version = $version; file = "BrixoPlayer.dmg"; bytes = (& $size "BrixoPlayer.dmg"); sha256 = (& $sha "BrixoPlayer.dmg") }
            $versions["studio_mac"] = [ordered]@{ version = $version; file = "BrixoStudio.dmg"; bytes = (& $size "BrixoStudio.dmg"); sha256 = (& $sha "BrixoStudio.dmg") }
            $macDone = $true
        }
    }
    if (-not $macDone) {
        Write-Host "   (The Mac build didn't work; see it on GitHub under Actions. Releasing Windows only.)"
    }
}
if (-not $macDone) {
    # Keep offering the Mac downloads the site already has.
    try {
        $current = Invoke-RestMethod "$Site/api/version" -TimeoutSec 15
        foreach ($k in "player_mac", "studio_mac") {
            if ($current.$k) { $versions[$k] = $current.$k }
        }
    } catch {}
}

Set-Content -Path (Join-Path $dist "versions.json") -Value ($versions | ConvertTo-Json) -Encoding ascii
$files += (Join-Path $dist "versions.json")

Write-Host "== Uploading to $Server (it asks for the server password)"
# versions.json goes last, so the site never offers a file before it's there.
scp @files "${Server}:/var/www/brixo-downloads/"
if ($LASTEXITCODE -ne 0) { throw "The upload failed. The files are in dist\ if you want to try again." }

$what = if ($macDone) { "Windows and Mac" } else { "Windows" }
Write-Host "== Done. $Site/download now offers Brixo $version ($what)"
