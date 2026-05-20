param(
  [int]$LimitSeconds = 300,
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

$homeDir = Join-Path ([System.IO.Path]::GetTempPath()) ("kodaal-ttfc-" + [System.Guid]::NewGuid())
New-Item -ItemType Directory -Path $homeDir | Out-Null
$port = New-FreePort
@"
[server]
port = $port
bind_address = "127.0.0.1"
request_timeout_seconds = 30
"@ | Set-Content -Path (Join-Path $homeDir "config.toml") -Encoding UTF8
$baseUrl = "http://127.0.0.1:$port"
$process = Start-Process -FilePath $binaryPath -ArgumentList @("--home", $homeDir, "start", "--no-watcher") -PassThru -WindowStyle Hidden
try {
  $start = Get-Date
  do {
    Start-Sleep -Milliseconds 200
    try {
      Invoke-RestMethod -Uri "$baseUrl/healthz" -TimeoutSec 1 | Out-Null
      break
    } catch {}
  } while (((Get-Date) - $start).TotalSeconds -lt $LimitSeconds)

  $token = (Get-Content -Path (Join-Path $homeDir "token") -Raw).Trim()
  $body = @{ text = "time to first capture proof"; source = "cli"; source_app = "codex-cli" } | ConvertTo-Json
  Invoke-RestMethod -Method Post -Uri "$baseUrl/api/prompts" -Headers @{ "X-Kodaal-Token" = $token } -ContentType "application/json" -Body $body | Out-Null
  $elapsed = ((Get-Date) - $start).TotalSeconds
  if ($elapsed -gt $LimitSeconds) {
    throw "Time to first capture exceeded $LimitSeconds seconds: $elapsed"
  }
  Write-Host ("Time-to-first-capture gate passed: {0:N2}s" -f $elapsed)
} finally {
  if (!$process.HasExited) { Stop-Process -Id $process.Id -Force }
  Remove-Item -LiteralPath $homeDir -Recurse -Force
}
