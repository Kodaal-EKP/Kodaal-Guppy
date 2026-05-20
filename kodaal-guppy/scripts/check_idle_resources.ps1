param(
  [int]$DurationSeconds = 300,
  [int]$MaxMemoryMb = 80,
  [double]$MaxCpuPercent = 1.0,
  [string]$Binary = "target\debug\kodaal.exe"
)

$ErrorActionPreference = "Stop"
$root = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$binaryPath = if ([System.IO.Path]::IsPathRooted($Binary)) { $Binary } else { Join-Path $root $Binary }
if (!(Test-Path $binaryPath)) {
  throw "Binary not found: $binaryPath"
}

function New-FreePort {
  $listener = [System.Net.Sockets.TcpListener]::new([System.Net.IPAddress]::Loopback, 0)
  $listener.Start()
  try {
    return [int]$listener.LocalEndpoint.Port
  } finally {
    $listener.Stop()
  }
}

$homeDir = Join-Path ([System.IO.Path]::GetTempPath()) ("kodaal-idle-" + [System.Guid]::NewGuid())
New-Item -ItemType Directory -Path $homeDir | Out-Null
$port = New-FreePort
@"
[server]
port = $port
bind_address = "127.0.0.1"
request_timeout_seconds = 30
"@ | Set-Content -Path (Join-Path $homeDir "config.toml") -Encoding UTF8
$process = Start-Process -FilePath $binaryPath -ArgumentList @("--home", $homeDir, "start", "--no-watcher") -PassThru -WindowStyle Hidden
try {
  Start-Sleep -Seconds 3
  $startCpu = (Get-Process -Id $process.Id).CPU
  $maxMemory = 0L
  $start = Get-Date
  do {
    $sample = Get-Process -Id $process.Id
    $maxMemory = [Math]::Max($maxMemory, $sample.WorkingSet64)
    Start-Sleep -Seconds 1
  } while (((Get-Date) - $start).TotalSeconds -lt $DurationSeconds)
  $endCpu = (Get-Process -Id $process.Id).CPU
  $cpuPercent = (($endCpu - $startCpu) / [Math]::Max(1, $DurationSeconds)) * 100
  $memoryMb = $maxMemory / 1MB
  if ($memoryMb -gt $MaxMemoryMb) { throw ("Idle memory exceeded {0}MB: {1:N1}MB" -f $MaxMemoryMb, $memoryMb) }
  if ($cpuPercent -gt $MaxCpuPercent) { throw ("Idle CPU exceeded {0}%: {1:N2}%" -f $MaxCpuPercent, $cpuPercent) }
  Write-Host ("Idle resource gate passed: {0:N1}MB max, {1:N2}% CPU" -f $memoryMb, $cpuPercent)
} finally {
  if (!$process.HasExited) { Stop-Process -Id $process.Id -Force }
  Remove-Item -LiteralPath $homeDir -Recurse -Force
}
