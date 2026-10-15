# Makes the Windows installer for PAM_Edit Video from what build-windows.ps1
# put in dist\, and a portable zip beside it, both in installer\.
#
#   .\build-windows.ps1        first, so dist\ holds the build to ship
#   .\make-installer.ps1       then this
#
# The Visual C++ runtime (VCRUNTIME140, MSVCP140) goes in beside the exe, so
# the app opens on a freshly installed Windows that has never had it. Needs
# Inno Setup 6 (winget install JRSoftware.InnoSetup) and Visual Studio Build
# Tools with C++, whose Redist folder the runtime is taken from.

$ErrorActionPreference = "Stop"
$root = $PSScriptRoot
$dist = "$root\dist\PAM_Edit Video"
if (-not (Test-Path "$dist\pamedit.exe")) { throw "dist\ has no pamedit.exe: run build-windows.ps1 first" }

$version = (Select-String -Path "$root\src\Cargo.toml" -Pattern '^version = "(.+)"').Matches[0].Groups[1].Value

$iscc = @(
    "$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe",
    "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe",
    "$env:ProgramFiles\Inno Setup 6\ISCC.exe"
) | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $iscc) { throw "Inno Setup not found: winget install JRSoftware.InnoSetup" }

$crt = Get-ChildItem "${env:ProgramFiles(x86)}\Microsoft Visual Studio\*\*\VC\Redist\MSVC\*\x64\Microsoft.VC14*.CRT" -Directory -ErrorAction SilentlyContinue |
    Sort-Object FullName -Descending | Select-Object -First 1
if (-not $crt) { throw "Visual C++ runtime not found under Visual Studio's VC\Redist" }

# The staged folder: the build, plus the runtime.
$out = "$root\installer"
$stage = "$out\stage"
if (Test-Path $stage) { Remove-Item $stage -Recurse -Force }
New-Item -ItemType Directory -Force $stage | Out-Null
Copy-Item "$dist\*" $stage -Recurse -Force
Copy-Item "$($crt.FullName)\vcruntime140.dll", "$($crt.FullName)\vcruntime140_1.dll",
    "$($crt.FullName)\msvcp140.dll", "$($crt.FullName)\msvcp140_1.dll" $stage -Force

& $iscc /Q "/DVersion=$version" "/DArch=x64compatible" "/DSuffix=x86_64" "/DStage=$stage" "/DOut=$out" "$root\assets\windows\pamedit.iss"
if ($LASTEXITCODE -ne 0) { throw "Inno Setup failed" }

# The same folder as a zip, for a machine where installing is not allowed.
$zip = "$out\PAM_Edit_Video-$version-windows-x86_64-portable.zip"
if (Test-Path $zip) { Remove-Item $zip -Force }
$portable = "$out\PAM_Edit Video"
if (Test-Path $portable) { Remove-Item $portable -Recurse -Force }
Copy-Item $stage $portable -Recurse
Compress-Archive -Path $portable -DestinationPath $zip -CompressionLevel Optimal
Remove-Item $portable -Recurse -Force
Remove-Item $stage -Recurse -Force

Get-ChildItem $out -File | Select-Object Name, @{ n = "MB"; e = { [math]::Round($_.Length / 1MB, 1) } }
