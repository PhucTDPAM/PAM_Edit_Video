# Builds PAM_Edit Video on Windows x64 and gathers a runnable folder in dist\.
#
# Expects the prebuilt dependencies from scripts/ci/deps.lock unpacked in
# ..\pamedit-deps (FFmpeg 8.1 shared, ONNX Runtime 1.28, sherpa-onnx 1.13.7,
# LLVM 21), plus rustup, CMake and Visual Studio Build Tools with C++.
#
#   .\build-windows.ps1              release build (thin LTO)
#   .\build-windows.ps1 -Profile app shipping build (fat LTO, slower)

#   .\build-windows.ps1 -NoDeploy    build only; leave dist\ and a running copy alone

param([string]$Profile = "release", [switch]$NoDeploy)
$ErrorActionPreference = "Stop"

$root = $PSScriptRoot
$deps = Join-Path (Split-Path $root -Parent) "pamedit-deps"

$ffmpeg = (Get-ChildItem $deps -Directory -Filter "ffmpeg-*win64*" | Select-Object -First 1).FullName
$ort = (Get-ChildItem $deps -Directory -Filter "onnxruntime-win-x64-*" | Select-Object -First 1).FullName
$sherpa = (Get-ChildItem "$deps\sherpa" -Directory | Select-Object -First 1).FullName
foreach ($dir in @($ffmpeg, $ort, $sherpa, "$deps\llvm\bin")) {
    if (-not $dir -or -not (Test-Path $dir)) { throw "missing dependency folder in $deps" }
}

# sherpa-onnx's own import libraries and DLLs only: the one ONNX Runtime is
# Microsoft's, above (see "sherpa-onnx shared libraries" in build-app.yml).
$sherpaLibs = "$deps\sherpa-libs"
$sherpaDlls = "$deps\sherpa-dlls"
New-Item -ItemType Directory -Force $sherpaLibs, $sherpaDlls | Out-Null
Copy-Item "$sherpa\lib\sherpa-onnx-*.lib" $sherpaLibs -Force
Copy-Item "$sherpa\lib\sherpa-onnx-*.dll" $sherpaDlls -Force

$env:FFMPEG_DIR = $ffmpeg
$env:LIBCLANG_PATH = "$deps\llvm\bin"
$env:ORT_LIB_LOCATION = "$ort\lib"
$env:ORT_PREFER_DYNAMIC_LINK = "1"
$env:ORT_DIR = $ort
$env:SHERPA_ONNX_LIB_DIR = $sherpaLibs
$env:Path = "$env:USERPROFILE\.cargo\bin;$ffmpeg\bin;$ort\lib;$sherpaDlls;$env:Path"

# whisper.cpp builds with CMake. winget's user install puts no cmake on the
# path, so look in its package folder when the path has none.
if (-not (Get-Command cmake -ErrorAction SilentlyContinue)) {
    $cmake = Get-ChildItem "$env:LOCALAPPDATA\Microsoft\WinGet\Packages\Kitware.CMake_*" -Recurse -Filter cmake.exe -ErrorAction SilentlyContinue |
        Select-Object -First 1
    if (-not $cmake) { throw "CMake not found: winget install Kitware.CMake" }
    $env:Path = "$($cmake.DirectoryName);$env:Path"
}

# ── the build, with its progress ──
# Cargo runs on its own with its output in a log, and this loop reads the
# log: its own progress bar says how many crates of how many are done. The
# last crate, the window's (`pamedit`), is a single compiler run of ten
# minutes or more that cargo cannot measure, so its share is estimated from
# how long it took last time (kept in target\pamedit-build-stats.json).
$stats = "$root\src\target\pamedit-build-stats.json"
$finalLast = 1080
if (Test-Path $stats) {
    try { $finalLast = [double](Get-Content $stats -Raw | ConvertFrom-Json).final_seconds } catch {}
}
$log = Join-Path ([IO.Path]::GetTempPath()) "pamedit-build-$PID.log"
$out = Join-Path ([IO.Path]::GetTempPath()) "pamedit-build-$PID.out"
$env:CARGO_TERM_PROGRESS_WHEN = "always"
$env:CARGO_TERM_PROGRESS_WIDTH = "200"
$cargo = (Get-Command cargo).Source
$started = Get-Date
$finalStart = $null
$lastLine = Get-Date "2000-01-01"
$proc = Start-Process $cargo -ArgumentList "build", "--profile", $Profile, "-p", "pamedit" `
    -WorkingDirectory "$root\src" -NoNewWindow -PassThru `
    -RedirectStandardError $log -RedirectStandardOutput $out
# Held from the start: without it Windows PowerShell reports no exit code.
$null = $proc.Handle

function Read-Shared($path) {
    try {
        $stream = [IO.File]::Open($path, "Open", "Read", "ReadWrite")
        $reader = New-Object IO.StreamReader($stream)
        $text = $reader.ReadToEnd(); $reader.Close(); return $text
    } catch { return "" }
}
function Clock($seconds) {
    $span = [TimeSpan]::FromSeconds([math]::Max(0, $seconds))
    return "{0}:{1:00}" -f [int][math]::Floor($span.TotalMinutes), $span.Seconds
}

while (-not $proc.HasExited) {
    Start-Sleep -Seconds 1
    $text = Read-Shared $log
    $done = 0; $total = 0; $now = ""
    $bars = [regex]::Matches($text, "Building \[[^\]]*\]\s+(\d+)/(\d+):\s*([^\r\n]*)")
    if ($bars.Count -gt 0) {
        $bar = $bars[$bars.Count - 1]
        $done = [int]$bar.Groups[1].Value; $total = [int]$bar.Groups[2].Value
        $now = $bar.Groups[3].Value.Trim()
    }
    $elapsed = ((Get-Date) - $started).TotalSeconds
    # The window's crate alone is left: the long step.
    $final = $now -match '^pamedit(\(bin\))?(, pamedit(\(bin\))?)*$' -or
        ($total -gt 0 -and $done -ge $total - 2 -and $now -like "pamedit*" -and $now -notmatch ',\s*pamedit-')
    if ($final -and -not $finalStart) { $finalStart = Get-Date }
    if ($finalStart) {
        $spent = ((Get-Date) - $finalStart).TotalSeconds
        $share = [math]::Min(99, [int](100 * $spent / [math]::Max(60, $finalLast)))
        $status = "Biên dịch giao diện: $share% · $(Clock $spent) / ~$(Clock $finalLast)"
        Write-Progress -Id 1 -Activity "PAM_Edit Video" -Status "Crate $done/$total" -PercentComplete 99
        Write-Progress -Id 2 -ParentId 1 -Activity "Giao diện (pamedit)" -Status $status -PercentComplete $share
        $line = "[build] $done/$total crate · giao diện $share% · đã chạy $(Clock $elapsed) · còn ~$(Clock ([math]::Max(0, $finalLast - $spent)))"
    } elseif ($total -gt 0) {
        $share = [int](100 * $done / $total)
        Write-Progress -Id 1 -Activity "PAM_Edit Video" -Status "Crate $done/$total ($share%) · $now" -PercentComplete $share
        $line = "[build] crate $done/$total ($share%) · đã chạy $(Clock $elapsed) · đang: $now"
    } else {
        $line = "[build] đang chuẩn bị · $(Clock $elapsed)"
    }
    # A plain line now and then too, for a log that shows no progress bar.
    if (((Get-Date) - $lastLine).TotalSeconds -ge 15) {
        Write-Host $line
        $lastLine = Get-Date
    }
}
$proc.WaitForExit()
Write-Progress -Id 2 -Activity "Giao diện (pamedit)" -Completed
Write-Progress -Id 1 -Activity "PAM_Edit Video" -Completed

# The log without the progress bar's redraws: what was compiled, and any
# warning or error, as cargo would have printed them.
$text = Read-Shared $log
($text -split "`r?`n") | ForEach-Object { ($_ -split "`r")[-1] } |
    Where-Object { $_ -and $_ -notmatch '^\s*Building \[' } | ForEach-Object { Write-Host $_ }
Remove-Item $log, $out -ErrorAction SilentlyContinue

if ($proc.ExitCode -ne 0) { throw "cargo build failed" }
if ($finalStart) {
    $seconds = [int]((Get-Date) - $finalStart).TotalSeconds
    New-Item -ItemType Directory -Force (Split-Path $stats) | Out-Null
    @{ final_seconds = $seconds } | ConvertTo-Json | Set-Content $stats -Encoding utf8
}
Write-Host "[build] xong sau $(Clock ((Get-Date) - $started).TotalSeconds)"

$target = if ($Profile -eq "dev") { "debug" } else { $Profile }
if ($NoDeploy) {
    Write-Host "Built $root\src\target\$target\pamedit.exe (dist\ left as it was)"
    return
}
$dist = "$root\dist\PAM_Edit Video"
New-Item -ItemType Directory -Force $dist | Out-Null
# A copy running from dist\ holds its files: close it so the new one lands.
Get-Process pamedit -ErrorAction SilentlyContinue |
    Where-Object { $_.Path -like "$dist\*" } |
    Stop-Process -Force -ErrorAction SilentlyContinue
Start-Sleep -Milliseconds 500
Copy-Item "$root\src\target\$target\pamedit.exe" $dist -Force
Copy-Item "$ffmpeg\bin\*.dll" $dist -Force
Copy-Item "$sherpaDlls\*.dll" $dist -Force
Copy-Item "$ort\lib\*.dll" $dist -Force
Copy-Item "$root\LICENSE", "$root\NOTICE.md", "$root\THIRD_PARTY_NOTICES.md" $dist -Force
Write-Host "PAM_Edit Video is in $dist"
