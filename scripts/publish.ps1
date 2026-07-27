# Build the release exe and assemble the publish/ folder:
#   publish\spacepp-win.exe   (copied, git-ignored)
#   publish\config.json       (relative symlink -> ..\config.json, git-tracked)
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

cargo build --release
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

New-Item -ItemType Directory -Force -Path publish | Out-Null
Copy-Item target\release\spacepp-win.exe publish\spacepp-win.exe -Force

$link = Join-Path $root "publish\config.json"
$item = Get-Item $link -ErrorAction SilentlyContinue
if ($item -and $item.LinkType -ne "SymbolicLink") {
    Remove-Item $link -Force
    $item = $null
}
if (-not $item) {
    # PowerShell 5.1's New-Item cannot create unprivileged symlinks, so use
    # mklink, which works without elevation when Developer Mode is enabled.
    cmd /c mklink "publish\config.json" "..\config.json" | Out-Null
    if ($LASTEXITCODE -ne 0) {
        Write-Error "Failed to create symlink. Enable Windows Developer Mode (Settings > System > For developers) or run as administrator."
        exit 1
    }
}

Write-Host "publish/ ready:"
Get-ChildItem publish | Format-Table Mode, Name, Length, LinkType -AutoSize
