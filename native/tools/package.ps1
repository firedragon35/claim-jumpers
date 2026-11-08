# Build the Windows release package: dist\ClaimJumpers-<version>-windows-x64.zip
#
#   powershell -ExecutionPolicy Bypass -File tools\package.ps1     (from the native folder)
#
# It builds the release exe, runs the unattended --smoke check from an
# unrelated folder (and stops if it fails), then stages the exe, the
# player README, the font licences and the third-party notices, zips
# them, and prints the zip's SHA-256.

$ErrorActionPreference = 'Stop'
$native = Split-Path -Parent $PSScriptRoot
Set-Location $native

$version = (Select-String -Path 'crates\cj-game\Cargo.toml' -Pattern '^version = "(.+)"').Matches[0].Groups[1].Value
$name = "ClaimJumpers-$version-windows-x64"
Write-Host "Packaging $name"

cargo build --release -p cj-game
if ($LASTEXITCODE -ne 0) { throw 'cargo build failed' }
$exe = Join-Path $native 'target\release\claim-jumpers.exe'

# The smoke check runs from TEMP so it can't lean on the working directory;
# it uses (and deletes) its own save folder.
$log = Join-Path $env:TEMP "cj-package-smoke.txt"
$p = Start-Process -FilePath $exe -ArgumentList '--smoke' -WorkingDirectory $env:TEMP `
    -RedirectStandardOutput $log -RedirectStandardError "$log.err" -PassThru -Wait
if ($p.ExitCode -ne 0) { throw "smoke check failed (exit $($p.ExitCode)); see $log" }
Write-Host "Smoke check passed ($((Select-String -Path $log -Pattern 'screenshot:').Count) screens)"

$dist = Join-Path $native 'dist'
$stage = Join-Path $dist $name
if (Test-Path $stage) { Remove-Item -Recurse -Force $stage }
New-Item -ItemType Directory -Force (Join-Path $stage 'licenses') | Out-Null

Copy-Item $exe $stage
(Get-Content -Raw 'packaging\README.txt').Replace('{VERSION}', $version) |
    Set-Content -Encoding utf8 -NoNewline (Join-Path $stage 'README.txt')
Copy-Item 'assets\fonts\licenses\*' (Join-Path $stage 'licenses')
Copy-Item 'assets\fonts\FONTS.md' (Join-Path $stage 'licenses\FONTS.md')
python tools\third_party.py (Join-Path $stage 'THIRD-PARTY-NOTICES.txt')
if ($LASTEXITCODE -ne 0) { throw 'third-party notices failed' }

$zip = Join-Path $dist "$name.zip"
if (Test-Path $zip) { Remove-Item -Force $zip }
Compress-Archive -Path $stage -DestinationPath $zip
$hash = (Get-FileHash -Algorithm SHA256 $zip).Hash
Write-Host ("{0} ({1:N1} MB)" -f $zip, ((Get-Item $zip).Length / 1MB))
Write-Host "SHA-256 $hash"
