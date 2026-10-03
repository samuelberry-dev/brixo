# Copies Brixo's database backups from the server to this PC, so there's a
# copy that survives the server's disk. Run from anywhere in PowerShell:
#   powershell -ExecutionPolicy Bypass -File tools\pull-backups.ps1
# It copies the newest backup you don't have yet; -All copies every one you
# don't have. They go to Documents\Brixo Backups (or -To another folder).
# Asks for the server's password for each connection, unless you've set up
# an SSH key for root@playbrixo.com.
param(
    [string]$Server = "root@playbrixo.com",
    [string]$To = (Join-Path ([Environment]::GetFolderPath("MyDocuments")) "Brixo Backups"),
    [switch]$All
)
$ErrorActionPreference = "Stop"
New-Item -ItemType Directory -Force $To | Out-Null

Write-Host "== Asking the server what backups there are"
$list = ssh $Server "ls -1t /var/backups/brixo/*.sqlite.gz 2>/dev/null"
if ($LASTEXITCODE -ne 0 -or -not $list) { throw "No backups on the server yet (or couldn't connect). On the server: brixo-backup" }
$missing = @($list | Where-Object { $_ -and -not (Test-Path (Join-Path $To (Split-Path $_ -Leaf))) })
if (-not $All) { $missing = @($missing | Select-Object -First 1) }
if ($missing.Count -eq 0) {
    Write-Host "== You already have the newest backup. Nothing to copy."
    exit 0
}
foreach ($f in $missing) {
    $name = Split-Path $f -Leaf
    Write-Host "== Copying $name"
    scp "${Server}:$f" (Join-Path $To $name)
    if ($LASTEXITCODE -ne 0) { throw "Copying $name failed." }
}
Write-Host "== Done: $($missing.Count) copied to $To"
Write-Host "   (Keep a few recent ones; delete old ones yourself when you like.)"
