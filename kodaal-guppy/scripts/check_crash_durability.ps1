param(
  [int]$PromptCount = 200,
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

function Wait-Health {
  param([int]$Seconds = 30)
  $start = Get-Date
  do {
    Start-Sleep -Milliseconds 200
    try {
      Invoke-RestMethod -Uri "$baseUrl/healthz" -TimeoutSec 1 | Out-Null
      return
    } catch {}
  } while (((Get-Date) - $start).TotalSeconds -lt $Seconds)
  throw "Kodaal did not become healthy within $Seconds seconds."
}

$homeDir = Join-Path ([System.IO.Path]::GetTempPath()) ("kodaal-crash-" + [System.Guid]::NewGuid())
New-Item -ItemType Directory -Path $homeDir | Out-Null
$port = New-FreePort
@"
[server]
port = $port
bind_address = "127.0.0.1"
request_timeout_seconds = 30
"@ | Set-Content -Path (Join-Path $homeDir "config.toml") -Encoding UTF8
$baseUrl = "http://127.0.0.1:$port"
$process = $null
try {
  $process = Start-Process -FilePath $binaryPath -ArgumentList @("--home", $homeDir, "start", "--no-watcher") -PassThru -WindowStyle Hidden
  Wait-Health
  $token = (Get-Content -Path (Join-Path $homeDir "token") -Raw).Trim()
  for ($i = 1; $i -le $PromptCount; $i++) {
    $body = @{ text = "crash durability prompt $i"; source = "cli"; source_app = "codex-cli" } | ConvertTo-Json
    Invoke-RestMethod -Method Post -Uri "$baseUrl/api/prompts" -Headers @{ "X-Kodaal-Token" = $token } -ContentType "application/json" -Body $body | Out-Null
  }
  Stop-Process -Id $process.Id -Force
  $process.WaitForExit()
  $process = Start-Process -FilePath $binaryPath -ArgumentList @("--home", $homeDir, "start", "--no-watcher") -PassThru -WindowStyle Hidden
  Wait-Health
  $result = Invoke-RestMethod -Uri "$baseUrl/api/prompts?limit=1" -Headers @{ "X-Kodaal-Token" = $token }
  if ([int]$result.total -lt $PromptCount) {
    throw "Expected at least $PromptCount prompts after forced stop; found $($result.total)."
  }
  Write-Host "Crash durability gate passed: $($result.total) prompts survived forced stop."
} finally {
  if ($process -and !$process.HasExited) { Stop-Process -Id $process.Id -Force }
  Remove-Item -LiteralPath $homeDir -Recurse -Force
}
