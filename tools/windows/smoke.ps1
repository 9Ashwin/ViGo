$ErrorActionPreference = 'Stop'
Set-Location (Join-Path $PSScriptRoot '../..')
$unpack = Join-Path (Get-Location) 'release/windows-smoke'
Expand-Archive 'release/ViGo-Windows-x64.zip' -DestinationPath $unpack -Force
$exe = Join-Path $unpack 'ViGo/ViGo.exe'
# Inspect the actual archived PE, not the working target binary.
$bytes = [IO.File]::ReadAllBytes($exe)
$pe = [BitConverter]::ToInt32($bytes, 0x3c)
if ([BitConverter]::ToUInt32($bytes, $pe) -ne 0x4550) { throw 'PE signature missing' }
if ([BitConverter]::ToUInt16($bytes, $pe + 4) -ne 0x8664) { throw 'Expected x64 machine' }
if ([BitConverter]::ToUInt16($bytes, $pe + 24 + 68) -ne 2) { throw 'Expected GUI subsystem' }
if ([BitConverter]::ToUInt32($bytes, $pe + 24 + 112 + 16) -eq 0) { throw 'Icon/manifest resource directory missing' }
$report = Join-Path (Get-Location) 'release/windows-smoke.json'
$proc = Start-Process -FilePath $exe -ArgumentList @('--smoke-test', '--report', ('"' + $report + '"')) -PassThru
if (-not $proc.WaitForExit(30000)) { $proc.Kill(); throw 'GUI smoke timed out' }
if ($proc.ExitCode -ne 0) { throw "GUI smoke exited $($proc.ExitCode)" }
$result = Get-Content -Raw $report | ConvertFrom-Json
if (-not $result.windowCreated -or -not $result.rendered -or -not $result.spriteDecoded -or -not $result.surfaceCreated -or -not $result.presentSucceeded -or $result.nonzeroAlphaPixels -lt 1000) { throw 'GUI smoke failed' }
if ($result.taskbarInteractionTested) { throw 'Fixture smoke must not claim real taskbar integration' }
Get-Content $report
