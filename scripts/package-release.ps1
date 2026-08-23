param(
    [Parameter(Mandatory = $true)]
    [string]$Version
)

$ErrorActionPreference = 'Stop'

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot '..')
$dist = Join-Path $repoRoot 'dist'
$packageRoot = Join-Path $dist "which-key-windows-$Version-windows-x64"
$zipPath = Join-Path $dist "which-key-windows-$Version-windows-x64.zip"

New-Item -ItemType Directory -Force -Path $packageRoot | Out-Null

Copy-Item -LiteralPath (Join-Path $repoRoot 'target\release\which-key-windows.exe') -Destination $packageRoot -Force
Copy-Item -LiteralPath (Join-Path $repoRoot 'README.md') -Destination $packageRoot -Force
Copy-Item -LiteralPath (Join-Path $repoRoot 'README.zh-CN.md') -Destination $packageRoot -Force

if (Test-Path -LiteralPath $zipPath) {
    Remove-Item -LiteralPath $zipPath -Force
}

Compress-Archive -Path (Join-Path $packageRoot '*') -DestinationPath $zipPath

Get-Item -LiteralPath $zipPath | Select-Object FullName,Length
