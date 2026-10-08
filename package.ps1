# Builds a release and packages it for distribution:
#   dist\SetMTU-v<version>-win-x64.zip  (SetMTU.exe, README.txt, LICENSE.txt, Inter-OFL.txt)
#   dist\SHA256SUMS.txt
# Then prints a ready-to-paste Discord post.
#
# Usage:  powershell -ExecutionPolicy Bypass -File .\package.ps1

$ErrorActionPreference = 'Stop'
$root = $PSScriptRoot

$version = (Select-String -Path "$root\Cargo.toml" -Pattern '^version\s*=\s*"([^"]+)"' |
    Select-Object -First 1).Matches[0].Groups[1].Value
$name = "SetMTU-v$version-win-x64"
Write-Host "Packaging $name"

# Build. cargo writes progress to stderr, so don't let PowerShell treat it as an error.
Push-Location $root
try {
    $ErrorActionPreference = 'Continue'
    cargo build --release
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }
} finally {
    $ErrorActionPreference = 'Stop'
    Pop-Location
}
$exe = "$root\target\release\setmtu.exe"

# The exe must not need the Visual C++ Redistributable (see .cargo\config.toml).
$imports = [Text.Encoding]::ASCII.GetString([IO.File]::ReadAllBytes($exe))
if ($imports -match '(?i)vcruntime\d+\.dll') {
    throw "setmtu.exe still depends on $($Matches[0]); check .cargo\config.toml (+crt-static)"
}

# Stage the files.
$dist = "$root\dist"
$stage = "$dist\$name"
if (Test-Path $stage) { Remove-Item -Recurse -Force $stage }
New-Item -ItemType Directory -Force $stage | Out-Null
Copy-Item $exe "$stage\SetMTU.exe"
(Get-Content "$root\packaging\README.txt" -Raw).Replace('{VERSION}', "v$version") |
    Set-Content "$stage\README.txt" -Encoding ASCII -NoNewline
Copy-Item "$root\LICENSE" "$stage\LICENSE.txt"
Copy-Item "$root\assets\Inter-OFL.txt" "$stage\Inter-OFL.txt"

# Zip it.
$zip = "$dist\$name.zip"
if (Test-Path $zip) { Remove-Item -Force $zip }
Compress-Archive -Path "$stage\*" -DestinationPath $zip

# Checksums.
$zipHash = (Get-FileHash $zip -Algorithm SHA256).Hash.ToLower()
$exeHash = (Get-FileHash "$stage\SetMTU.exe" -Algorithm SHA256).Hash.ToLower()
@(
    "$zipHash  $name.zip"
    "$exeHash  SetMTU.exe"
) | Set-Content "$dist\SHA256SUMS.txt" -Encoding ASCII

$exeMb = '{0:N1}' -f ((Get-Item "$stage\SetMTU.exe").Length / 1MB)
$zipMb = '{0:N1}' -f ((Get-Item $zip).Length / 1MB)
Write-Host ""
Write-Host "Done: $zip ($zipMb MB, exe $exeMb MB)"
Write-Host ""
Write-Host "---- Discord post ----"
Write-Host @"
**SetMTU v$version** - sets your network adapter's MTU (1428 by default) and restores the original with one click.
Download: https://github.com/riaanjutte/SetMTU/releases/tag/v$version
Unzip and run SetMTU.exe. If Windows says "Windows protected your PC", click **More info** then **Run anyway** (not code-signed yet).
SHA-256 (exe): ``$exeHash``
"@
