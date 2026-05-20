param(
  [string]$Profile = "release",
  [string]$TargetDir = ""
)

$ErrorActionPreference = "Stop"
$root = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$targetPath = if ($TargetDir) { $TargetDir } elseif ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $root "target" }
$cargoHome = if ($env:CARGO_HOME) { $env:CARGO_HOME } else { Join-Path $env:USERPROFILE ".cargo" }

function Join-RustFlags {
  param([string[]]$Flags)
  return ($Flags | Where-Object { $_ }) -join ([char]0x1f)
}

$savedRustflags = $env:RUSTFLAGS
$savedEncodedRustflags = $env:CARGO_ENCODED_RUSTFLAGS
$savedCargoTargetDir = $env:CARGO_TARGET_DIR
$savedCargoIncremental = $env:CARGO_INCREMENTAL
$savedSourceDateEpoch = $env:SOURCE_DATE_EPOCH

try {
  $flags = @(
    "--remap-path-prefix=$root=.",
    "--remap-path-prefix=$cargoHome=~/.cargo",
    "--remap-path-prefix=$targetPath=target",
    "-C",
    "debuginfo=0"
  )
  if ($IsWindows -or $env:OS -eq "Windows_NT") {
    $flags += @("-C", "link-arg=/Brepro")
  }

  Remove-Item Env:\RUSTFLAGS -ErrorAction SilentlyContinue
  $env:CARGO_ENCODED_RUSTFLAGS = Join-RustFlags $flags
  $env:CARGO_INCREMENTAL = "0"
  $env:SOURCE_DATE_EPOCH = "0"
  if ($TargetDir) {
    $env:CARGO_TARGET_DIR = $TargetDir
  }

  Push-Location $root
  cargo build -p kodaal-core --profile $Profile --locked
  $name = if ($IsWindows -or $env:OS -eq "Windows_NT") { "kodaal.exe" } else { "kodaal" }
  $binary = Join-Path (Join-Path $targetPath $Profile) $name
  $prefixes = @($targetPath, $root, $cargoHome, $env:USERPROFILE)
  & (Join-Path $root "scripts/sanitize_binary_paths.ps1") -Binary $binary -Prefixes $prefixes
} finally {
  Pop-Location -ErrorAction SilentlyContinue
  if ($null -ne $savedRustflags) { $env:RUSTFLAGS = $savedRustflags } else { Remove-Item Env:\RUSTFLAGS -ErrorAction SilentlyContinue }
  if ($null -ne $savedEncodedRustflags) { $env:CARGO_ENCODED_RUSTFLAGS = $savedEncodedRustflags } else { Remove-Item Env:\CARGO_ENCODED_RUSTFLAGS -ErrorAction SilentlyContinue }
  if ($null -ne $savedCargoTargetDir) { $env:CARGO_TARGET_DIR = $savedCargoTargetDir } else { Remove-Item Env:\CARGO_TARGET_DIR -ErrorAction SilentlyContinue }
  if ($null -ne $savedCargoIncremental) { $env:CARGO_INCREMENTAL = $savedCargoIncremental } else { Remove-Item Env:\CARGO_INCREMENTAL -ErrorAction SilentlyContinue }
  if ($null -ne $savedSourceDateEpoch) { $env:SOURCE_DATE_EPOCH = $savedSourceDateEpoch } else { Remove-Item Env:\SOURCE_DATE_EPOCH -ErrorAction SilentlyContinue }
}
