$ErrorActionPreference = "Stop"

$Root = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
$TempRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("kodaal-install-smoke-" + [System.Guid]::NewGuid().ToString("N"))
$InstallDir = Join-Path $TempRoot "bin"
$HomeDir = Join-Path $TempRoot "home"
$Port = Get-Random -Minimum 30000 -Maximum 45000
New-Item -ItemType Directory -Force -Path $InstallDir, $HomeDir | Out-Null

$LocationPushed = $false

function Stop-SmokeProcesses {
  param(
    [string] $Exe,
    [string] $HomeDir,
    [string] $TempRoot
  )

  $PidPath = Join-Path $HomeDir "run\kodaal.pid"
  if ((Test-Path -LiteralPath $Exe) -and (Test-Path -LiteralPath $PidPath)) {
    & $Exe --home $HomeDir --port $Port stop --force 2>$null | Out-Null
  }

  $processes = Get-CimInstance Win32_Process |
    Where-Object {
      $_.ExecutablePath -and $_.ExecutablePath.StartsWith($TempRoot, [System.StringComparison]::OrdinalIgnoreCase)
    }
  foreach ($process in $processes) {
    Stop-Process -Id $process.ProcessId -Force -ErrorAction SilentlyContinue
    Wait-Process -Id $process.ProcessId -Timeout 5 -ErrorAction SilentlyContinue
  }
}

function Remove-SmokeRoot {
  param([string] $TempRoot)

  for ($attempt = 1; $attempt -le 10; $attempt++) {
    try {
      Remove-Item -LiteralPath $TempRoot -Recurse -Force -ErrorAction Stop
      return
    } catch {
      if ($attempt -eq 10) {
        throw
      }
      Start-Sleep -Milliseconds 500
    }
  }
}

try {
  Push-Location $Root
  $LocationPushed = $true
  cargo build --workspace
  $TargetDir = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $Root "target" }
  $SourceExe = Join-Path $TargetDir "debug\kodaal.exe"
  & $SourceExe --home $HomeDir --port $Port install --bin-dir $InstallDir | Out-Null
  $Exe = Join-Path $InstallDir "kodaal.exe"
  $NativeExe = Join-Path $InstallDir "kodaal-native-token-host.exe"
  if (!(Test-Path -LiteralPath $Exe)) { throw "installed binary was not created" }
  if (!(Test-Path -LiteralPath $NativeExe)) { throw "native token host binary was not created" }
  $start = Start-Process -FilePath $Exe -ArgumentList @("--home", $HomeDir, "--port", "$Port", "start", "--detach", "--no-watcher") -PassThru -WindowStyle Hidden
  Start-Sleep -Seconds 2
  & $Exe --home $HomeDir --port $Port status | Out-Null
  if (!(Test-Path (Join-Path $HomeDir "token"))) { throw "token was not created under smoke home" }
  if (!(Test-Path (Join-Path $HomeDir "config.toml"))) { throw "config was not created under smoke home" }
  if (!(Test-Path (Join-Path $HomeDir "guppy.db"))) { throw "database was not created under smoke home" }
  & $Exe --home $HomeDir --port $Port stop --force | Out-Null
  Wait-Process -Id $start.Id -Timeout 10 -ErrorAction SilentlyContinue
  & $SourceExe --home $HomeDir --port $Port uninstall --bin-dir $InstallDir | Out-Null
  if ($LASTEXITCODE -ne 0) { throw "installed binary uninstall failed with exit code $LASTEXITCODE" }
  Write-Host "Installer feasibility check passed."
}
finally {
  if ($LocationPushed) {
    Pop-Location
  }
  if (Test-Path $TempRoot) {
    Stop-SmokeProcesses -Exe (Join-Path $InstallDir "kodaal.exe") -HomeDir $HomeDir -TempRoot $TempRoot
    $TargetDir = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $Root "target" }
    $SourceExe = Join-Path $TargetDir "debug\kodaal.exe"
    $InstalledExe = Join-Path $InstallDir "kodaal.exe"
    $NativeExe = Join-Path $InstallDir "kodaal-native-token-host.exe"
    if ((Test-Path -LiteralPath $SourceExe) -and ((Test-Path -LiteralPath $InstalledExe) -or (Test-Path -LiteralPath $NativeExe))) {
      & $SourceExe --home $HomeDir --port $Port uninstall --bin-dir $InstallDir 2>$null | Out-Null
    }
    Remove-SmokeRoot -TempRoot $TempRoot
  }
}
