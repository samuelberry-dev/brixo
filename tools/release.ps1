# Builds Brixo Player and Studio for Windows and puts them on playbrixo.com.
# Run from anywhere in PowerShell:
#   powershell -ExecutionPolicy Bypass -File tools\release.ps1
# It asks for the server's root password once, for the upload.
param([string]$Server = "root@playbrixo.com")
$ErrorActionPreference = "Stop"
Set-Location (Split-Path $PSScriptRoot)   # the repo folder

# Each release is stamped with the date and time; Player and Studio compare
# theirs with the website's to know when there's a newer one.
$version = Get-Date -Format "yyyy.MM.dd.HHmm"
Write-Host "== Building Brixo $version (a few minutes)"
$env:BRIXO_BUILD = $version
cargo build --release -p brixo-player -p brixo-studio
$built = $LASTEXITCODE
Remove-Item Env:BRIXO_BUILD
if ($built -ne 0) { throw "The build failed; nothing was uploaded." }

$dist = Join-Path (Get-Location) "dist"
New-Item -ItemType Directory -Force $dist | Out-Null
Copy-Item "target\release\brixo-player.exe" (Join-Path $dist "BrixoPlayer.exe") -Force
Copy-Item "target\release\brixo-studio.exe" (Join-Path $dist "BrixoStudio.exe") -Force
$versions = [ordered]@{
    player = [ordered]@{ version = $version; file = "BrixoPlayer.exe"; bytes = (Get-Item (Join-Path $dist "BrixoPlayer.exe")).Length }
    studio = [ordered]@{ version = $version; file = "BrixoStudio.exe"; bytes = (Get-Item (Join-Path $dist "BrixoStudio.exe")).Length }
}
Set-Content -Path (Join-Path $dist "versions.json") -Value ($versions | ConvertTo-Json) -Encoding ascii

Write-Host "== Uploading to $Server (it asks for the server password)"
# versions.json goes last, so the site never offers a file before it's there.
scp (Join-Path $dist "BrixoPlayer.exe") (Join-Path $dist "BrixoStudio.exe") (Join-Path $dist "versions.json") "${Server}:/var/www/brixo-downloads/"
if ($LASTEXITCODE -ne 0) { throw "The upload failed. The files are in dist\ if you want to try again." }

Write-Host "== Done. https://playbrixo.com/download now offers Brixo $version"
