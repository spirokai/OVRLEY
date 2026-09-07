# Dot-source to configure this PowerShell session. Downloads/extracts only; never builds.
param([string]$SevenZip = 'C:/Program Files/7-Zip/7z.exe')

$ErrorActionPreference = 'Stop'
$motionRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$motionNative = Join-Path $motionRoot 'tmp/motion-native'
if (-not (Test-Path -LiteralPath $SevenZip -PathType Leaf)) {
    throw 'Install 7-Zip or pass -SevenZip with its executable path.'
}
New-Item -ItemType Directory -Force -Path $motionNative | Out-Null

function Get-MotionArchive($Name, $Url, $Sha256) {
    $archivePath = Join-Path $motionNative $Name
    if (-not (Test-Path -LiteralPath $archivePath -PathType Leaf)) {
        & curl.exe -fL --retry 2 -o $archivePath $Url
        if ($LASTEXITCODE -ne 0) { throw "Download failed: $Name" }
    }
    if ((Get-FileHash -LiteralPath $archivePath -Algorithm SHA256).Hash -ne $Sha256) {
        throw "Checksum mismatch: $archivePath. Remove the incorrect archive and retry."
    }
    return $archivePath
}

$motionOpenCvArchive = Get-MotionArchive 'opencv-4.11.0-windows.exe' `
    'https://github.com/opencv/opencv/releases/download/4.11.0/opencv-4.11.0-windows.exe' `
    '7C9D1C0B70DB1B1952CC815252FCED9A07F51267563CF3EAA1674D734C49B8E4'
$motionLlvmArchive = Get-MotionArchive 'LLVM-20.1.8-win64.exe' `
    'https://github.com/llvm/llvm-project/releases/download/llvmorg-20.1.8/LLVM-20.1.8-win64.exe' `
    '3197846A2B19063687DD56E93E34CD941E3548D907F23A6131571321BDF9FE7B'

& $SevenZip x $motionOpenCvArchive "-o$motionNative" -y `
    'opencv/build/include/*' 'opencv/build/x64/vc16/lib/opencv_world4110.lib' `
    'opencv/build/x64/vc16/bin/opencv_world4110.dll' 'opencv/LICENSE*' 'opencv/build/etc/licenses/*'
if ($LASTEXITCODE -ne 0) { throw 'OpenCV extraction failed.' }
& $SevenZip x $motionLlvmArchive "-o$motionNative/llvm20" -y `
    'bin/libclang.dll' 'bin/clang.exe' 'lib/clang/*' 'LICENSE*'
if ($LASTEXITCODE -ne 0) { throw 'LLVM extraction failed.' }

$env:OPENCV_INCLUDE_PATHS = Join-Path $motionNative 'opencv/build/include'
$env:OPENCV_LINK_PATHS = Join-Path $motionNative 'opencv/build/x64/vc16/lib'
$env:OPENCV_LINK_LIBS = 'opencv_world4110'
$env:LIBCLANG_PATH = Join-Path $motionNative 'llvm20/bin'
$env:PATH = "$env:LIBCLANG_PATH;$(Join-Path $motionNative 'opencv/build/x64/vc16/bin');$env:PATH"
Write-Host 'OpenCV 4.11.0 and LLVM 20.1.8 are configured for this session. No build was run.'
