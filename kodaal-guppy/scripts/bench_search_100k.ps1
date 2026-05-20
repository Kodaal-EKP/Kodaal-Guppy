param(
  [int]$PromptCount = 100000,
  [int]$MaxP95Ms = 200,
  [string]$Binary = "target\release\kodaal.exe"
)

$ErrorActionPreference = "Stop"
$root = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$binaryPath = if ([System.IO.Path]::IsPathRooted($Binary)) { $Binary } else { Join-Path $root $Binary }
if (!(Test-Path $binaryPath)) {
  throw "Binary not found: $binaryPath"
}

$homeDir = Join-Path ([System.IO.Path]::GetTempPath()) ("kodaal-bench-" + [System.Guid]::NewGuid())
New-Item -ItemType Directory -Path $homeDir | Out-Null
$process = Start-Process -FilePath $binaryPath -ArgumentList @("--home", $homeDir, "start", "--no-watcher") -PassThru -WindowStyle Hidden
try {
  Start-Sleep -Seconds 3
  $token = (Get-Content -Path (Join-Path $homeDir "token") -Raw).Trim()
  for ($i = 1; $i -le $PromptCount; $i++) {
    $body = @{ text = "benchmark prompt $i sqlite refactor platform analytics"; source = "cli"; source_app = "codex-cli" } | ConvertTo-Json
    Invoke-RestMethod -Method Post -Uri "http://127.0.0.1:7878/api/prompts" -Headers @{ "X-Kodaal-Token" = $token } -ContentType "application/json" -Body $body | Out-Null
  }
  $samples = @()
  for ($i = 1; $i -le 30; $i++) {
    $timer = [System.Diagnostics.Stopwatch]::StartNew()
    Invoke-RestMethod -Uri "http://127.0.0.1:7878/api/prompts?q=sqlite&limit=50" -Headers @{ "X-Kodaal-Token" = $token } | Out-Null
    $timer.Stop()
    $samples += $timer.Elapsed.TotalMilliseconds
  }
  $sorted = $samples | Sort-Object
  $index = [Math]::Min($sorted.Count - 1, [Math]::Ceiling($sorted.Count * 0.95) - 1)
  $p95 = $sorted[$index]
  if ($p95 -gt $MaxP95Ms) { throw ("Search p95 exceeded {0}ms: {1:N1}ms" -f $MaxP95Ms, $p95) }
  Write-Host ("100k search benchmark passed: p95 {0:N1}ms" -f $p95)
} finally {
  if (!$process.HasExited) { Stop-Process -Id $process.Id -Force }
  Remove-Item -LiteralPath $homeDir -Recurse -Force
}
