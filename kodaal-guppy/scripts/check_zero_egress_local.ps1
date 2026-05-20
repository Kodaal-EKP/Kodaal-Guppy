param(
  [int]$DurationSeconds = 30,
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

$homeDir = Join-Path ([System.IO.Path]::GetTempPath()) ("kodaal-egress-" + [System.Guid]::NewGuid())
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
  $start = Get-Date
  do {
    $connections = Get-NetTCPConnection -OwningProcess $process.Id -ErrorAction SilentlyContinue |
      Where-Object {
        $_.State -eq "Established" -and
        $_.RemoteAddress -notmatch "^(127\.|::1$|0\.0\.0\.0$|::$)"
      }
    if ($connections) {
      $summary = $connections | ForEach-Object { "$($_.RemoteAddress):$($_.RemotePort)" }
      throw "Outbound connection detected: $($summary -join ', ')"
    }
    Start-Sleep -Seconds 1
  } while (((Get-Date) - $start).TotalSeconds -lt $DurationSeconds)
  Write-Host "Zero-egress local socket gate passed."
} finally {
  if (!$process.HasExited) { Stop-Process -Id $process.Id -Force }
  Remove-Item -LiteralPath $homeDir -Recurse -Force
}
