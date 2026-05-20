param(
  [string]$Profile = "release",
  [string]$BuildRoot = ""
)

$ErrorActionPreference = "Stop"
$root = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$base = if ($BuildRoot) {
  $BuildRoot
} elseif ($IsWindows -or $env:OS -eq "Windows_NT") {
  Join-Path ([System.IO.Path]::GetPathRoot($root)) "kg-repro"
} else {
  [System.IO.Path]::GetTempPath()
}
New-Item -ItemType Directory -Path $base -Force | Out-Null
$one = Join-Path $base ("kodaal-repro-a-" + [System.Guid]::NewGuid())
$two = Join-Path $base ("kodaal-repro-b-" + [System.Guid]::NewGuid())
function Get-Sha256Hex([string]$Path) {
  $lastError = $null
  for ($attempt = 1; $attempt -le 30; $attempt++) {
    try {
      $stream = [System.IO.File]::Open($Path, [System.IO.FileMode]::Open, [System.IO.FileAccess]::Read, [System.IO.FileShare]::ReadWrite -bor [System.IO.FileShare]::Delete)
      try {
        $sha = [System.Security.Cryptography.SHA256]::Create()
        try {
          $bytes = $sha.ComputeHash($stream)
          return ([System.BitConverter]::ToString($bytes)).Replace("-", "")
        } finally {
          $sha.Dispose()
        }
      } finally {
        $stream.Dispose()
      }
    } catch {
      $lastError = $_
      Start-Sleep -Seconds 2
    }
  }
  if ($lastError) {
    throw "Unable to hash $Path after retries: $($lastError.Exception.Message)"
  }
  throw "Unable to hash $Path after retries."
}
try {
  Push-Location $root
  powershell -ExecutionPolicy Bypass -File scripts/build_release.ps1 -Profile $Profile -TargetDir $one
  $name = if ($IsWindows -or $env:OS -eq "Windows_NT") { "kodaal.exe" } else { "kodaal" }
  $first = Join-Path (Join-Path $one $Profile) $name
  $firstHash = Get-Sha256Hex $first
  powershell -ExecutionPolicy Bypass -File scripts/build_release.ps1 -Profile $Profile -TargetDir $two
  $second = Join-Path (Join-Path $two $Profile) $name
  $secondHash = Get-Sha256Hex $second
  if ($firstHash -ne $secondHash) {
    throw "Reproducible build gate failed: $firstHash != $secondHash"
  }
  Write-Host "Reproducible build gate passed: $firstHash"
} finally {
  Pop-Location
  Remove-Item -LiteralPath $one -Recurse -Force -ErrorAction SilentlyContinue
  Remove-Item -LiteralPath $two -Recurse -Force -ErrorAction SilentlyContinue
  if (!$BuildRoot -and (Test-Path $base) -and -not (Get-ChildItem -LiteralPath $base -Force -ErrorAction SilentlyContinue)) {
    Remove-Item -LiteralPath $base -Force -ErrorAction SilentlyContinue
  }
}
