# Builds the portable Windows zip (markuli.exe).
#   scripts/package-windows.ps1 0.1.0
# Output: dist/Markuli-<version>-windows-x64.zip
param([Parameter(Mandatory = $true)][string]$Version)
$ErrorActionPreference = 'Stop'
$MaxBytes = 4MB # spec budget: artifact <= 4 MB

Set-Location (Join-Path $PSScriptRoot '..')
cargo build --release --locked --target x86_64-pc-windows-msvc
if ($LASTEXITCODE -ne 0) { throw 'cargo build failed' }

Remove-Item -Recurse -Force dist -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force dist/stage | Out-Null
Copy-Item target/x86_64-pc-windows-msvc/release/markuli.exe dist/stage/Markuli.exe
$zip = "dist/Markuli-$Version-windows-x64.zip"
Compress-Archive -Path dist/stage/Markuli.exe -DestinationPath $zip

$size = (Get-Item $zip).Length
Write-Host "exe: $((Get-Item dist/stage/Markuli.exe).Length) bytes"
Write-Host "zip: $size bytes (budget $MaxBytes)"
if ($size -gt $MaxBytes) { throw 'zip exceeds the 4 MB budget' }
