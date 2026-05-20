param(
  [int]$CoreMinLines = 80,
  [int]$SurfaceMinLines = 60
)

$ErrorActionPreference = "Stop"
$root = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$rustSummary = Join-Path ([System.IO.Path]::GetTempPath()) ("kodaal-rust-coverage-" + [System.Guid]::NewGuid() + ".json")
$env:KODAAL_COVERAGE_RUN = "1"
try {
  cargo llvm-cov --workspace --json --summary-only --ignore-filename-regex "(\\|/)(cli|mcp)(\\|/)|(local_api|main|server)\.rs$" --output-path $rustSummary
  $rustPercent = python -c "import json,sys; data=json.load(open(sys.argv[1], encoding='utf-8')); print(data['data'][0]['totals']['lines']['percent'])" $rustSummary
  if ([double]$rustPercent -lt $CoreMinLines) {
    throw ("Rust core coverage below {0}%: {1:N2}%" -f $CoreMinLines, [double]$rustPercent)
  }
  Write-Host ("Rust core coverage gate passed: {0:N2}% lines" -f [double]$rustPercent)

  Push-Location $root
  node --test --experimental-test-coverage --test-coverage-lines=$SurfaceMinLines --test-coverage-include=browser-ext/background.js browser-ext/tests/*.test.mjs
  node --test --experimental-test-coverage --test-coverage-lines=$SurfaceMinLines --test-coverage-include=ide-ext/vscode/src/client.js --test-coverage-include=ide-ext/vscode/src/watchers/*.js ide-ext/vscode/tests/*.test.mjs
  Write-Host "Surface coverage gates passed."
} finally {
  Pop-Location -ErrorAction SilentlyContinue
  Remove-Item Env:\KODAAL_COVERAGE_RUN -ErrorAction SilentlyContinue
  Remove-Item -LiteralPath $rustSummary -Force -ErrorAction SilentlyContinue
}
