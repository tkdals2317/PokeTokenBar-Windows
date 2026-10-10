# Installs (or updates) the latest PokeTokenBar for Windows and starts it.
# Per-user install: no admin rights needed.
#   irm https://raw.githubusercontent.com/tkdals2317/PokeTokenBar-Windows/main/install.ps1 | iex

$ErrorActionPreference = 'Stop'
$repo = 'tkdals2317/PokeTokenBar-Windows'

Write-Host 'Finding the latest release...'
# The updater feed sits behind a plain download URL, so this avoids the GitHub API and its
# 60-requests-per-hour limit for anonymous callers (shared office/CI IPs hit it).
$feed = Invoke-RestMethod "https://github.com/$repo/releases/latest/download/latest.json"
$platform = $feed.platforms.'windows-x86_64-nsis'
if (-not $platform) { $platform = $feed.platforms.'windows-x86_64' }
if (-not $platform -or $platform.url -notlike '*-setup.exe') { throw "No installer found for v$($feed.version)." }

$name = Split-Path $platform.url -Leaf
$installer = Join-Path $env:TEMP $name
Write-Host "Downloading $name..."
Invoke-WebRequest $platform.url -OutFile $installer -UseBasicParsing
Unblock-File $installer

# Close a running copy so the installer can replace it.
Get-Process -Name 'poketokenbar' -ErrorAction SilentlyContinue | Stop-Process -Force

Write-Host 'Installing...'
Start-Process $installer -ArgumentList '/S' -Wait
Remove-Item $installer -ErrorAction SilentlyContinue

$exe = Get-ChildItem (Join-Path $env:LOCALAPPDATA 'PokeTokenBar') -Filter '*.exe' -ErrorAction SilentlyContinue |
    Where-Object { $_.Name -notlike 'uninstall*' } | Select-Object -First 1
if (-not $exe) { throw 'Installed, but the app was not found under %LOCALAPPDATA%\PokeTokenBar.' }

Start-Process $exe.FullName
Write-Host "PokeTokenBar v$($feed.version) installed and started."
Write-Host 'Look for the egg on your desktop and the tray icon (taskbar ^ area).'
