# Build the release exe and assemble the publish/ folder:
#   publish\superpp-win.exe   (copied, git-ignored)
#   publish\config.json       (relative symlink -> ..\config.json, git-tracked)
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

cargo build --release
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

New-Item -ItemType Directory -Force -Path publish | Out-Null
# If the published exe is running it is locked, but Windows still allows
# renaming it. Move it aside, then clean up the leftover on a later run.
Remove-Item publish\superpp-win.exe.old -Force -ErrorAction SilentlyContinue
Remove-Item publish\spacepp-win.exe.old -Force -ErrorAction SilentlyContinue
try {
    Copy-Item target\release\superpp-win.exe publish\superpp-win.exe -Force
} catch [System.IO.IOException] {
    Move-Item publish\superpp-win.exe publish\superpp-win.exe.old -Force -ErrorAction SilentlyContinue
    Move-Item publish\spacepp-win.exe publish\spacepp-win.exe.old -Force -ErrorAction SilentlyContinue
    Copy-Item target\release\superpp-win.exe publish\superpp-win.exe -Force
    Write-Host "Note: old exe was running; renamed aside (restart the app to use the new build)."
}

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
