$ErrorActionPreference = 'Stop'
Set-Location (Join-Path $PSScriptRoot '../..')
# Explicit target keeps CRT flags away from host proc-macros and build scripts.
$previousRustFlags = $env:RUSTFLAGS
try {
    $env:RUSTFLAGS = "$previousRustFlags -C target-feature=+crt-static"
    cargo build --release --locked --target x86_64-pc-windows-msvc
    if ($LASTEXITCODE -ne 0) { throw 'Rust build failed' }
} finally { $env:RUSTFLAGS = $previousRustFlags }
$version = (cargo metadata --no-deps --format-version 1 | ConvertFrom-Json).packages[0].version
$out = Join-Path (Get-Location) 'release/windows/ViGo'
New-Item -ItemType Directory -Force $out | Out-Null
Copy-Item 'target/x86_64-pc-windows-msvc/release/ViGo.exe' $out
Copy-Item 'LICENSE','THIRD_PARTY_LICENSES.md' $out
python tools/native/licenses.py (Join-Path $out 'licenses')
if ($LASTEXITCODE -ne 0) { throw 'Dependency license collection failed' }
@"
ViGo $version - Windows 11 x64 beta
Extract the whole folder, then run ViGo.exe (no installer, WebView or administrator access required).
Hold V, move to an application, release V. Tap V types normally. Esc / right-click cancels.
The actual Explorer taskbar is read through UI Automation and refreshed automatically.
Use the cat tray icon to pause, inspect taskbar status, or quit.

Unsigned testing build. SmartScreen may require additional confirmation.
Hosted CI verifies compilation, resources and a controlled GUI smoke fixture on Windows Server.
Real Windows 11 taskbar integration, focus rules, IMEs and multi-monitor behavior still need hardware testing.
Elevated applications bypass the V gesture because Windows blocks replay into higher integrity processes.
"@ | Set-Content -Encoding utf8 (Join-Path $out 'README.txt')
$zip = Join-Path (Get-Location) 'release/ViGo-Windows-x64.zip'
Compress-Archive -Path $out -DestinationPath $zip -Force
$hash = (Get-FileHash -Algorithm SHA256 $zip).Hash.ToLowerInvariant()
"$hash  ViGo-Windows-x64.zip" | Set-Content -Encoding ascii 'release/WINDOWS-SHA256SUMS.txt'
Write-Output $zip
