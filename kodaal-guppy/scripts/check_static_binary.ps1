param([string]$Binary = "")

$ErrorActionPreference = "Stop"
$root = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
if (!$Binary) {
  $Binary = if ($IsWindows -or $env:OS -eq "Windows_NT") { "target\release\kodaal.exe" } else { "target/release/kodaal" }
}
$binaryPath = if ([System.IO.Path]::IsPathRooted($Binary)) { $Binary } else { Join-Path $root $Binary }
if (!(Test-Path $binaryPath)) {
  throw "Binary not found: $binaryPath"
}

function Assert-NoBuildHostStrings {
  param([string]$Path)

  $strings = & powershell -NoProfile -Command "(Get-Command strings.exe -ErrorAction SilentlyContinue).Path"
  if (!$strings) {
    Write-Host "Build-host path string scan skipped because strings.exe is unavailable."
    return
  }
  $leaks = & strings $Path | Select-String -Pattern "C:\\Users\\|/home/|/Users/|kodaal-repro-|KodaalGuppyTarget" | Select-Object -First 20
  if ($leaks) {
    throw "Build-host path leak detected in binary strings: $($leaks -join '; ')"
  }
}

function Assert-WindowsSystemDlls {
  param([string[]]$Lines)

  $dlls = @()
  foreach ($line in $Lines) {
    if ($line -match "DLL Name:\s*([A-Za-z0-9_.-]+\.dll)") {
      $dlls += $Matches[1].ToLowerInvariant()
    } elseif ($line -match "^\s*([A-Za-z0-9_.-]+\.dll)\s*$") {
      $dlls += $Matches[1].ToLowerInvariant()
    }
  }
  $dlls = $dlls | Sort-Object -Unique
  if (!$dlls) {
    throw "No imported DLLs were parsed from the PE inspection output."
  }
  $allowed = "^(api-ms-win-|kernel32\.dll$|ntdll\.dll$|bcrypt\.dll$|bcryptprimitives\.dll$|user32\.dll$|crypt32\.dll$|advapi32\.dll$|ws2_32\.dll$|vcruntime140(_1)?\.dll$|ucrtbase\.dll$)"
  $blocked = $dlls | Where-Object { $_ -notmatch $allowed }
  if ($blocked) {
    throw "Third-party dynamic dependency detected: $($blocked -join ', ')"
  }
  Write-Host "Static binary gate passed: imports only Windows system/runtime DLLs: $($dlls -join ', ')"
}

function Invoke-NativeInspector {
  param(
    [string]$Exe,
    [string[]]$Arguments
  )
  $lastLines = @()
  $lastCode = 0
  for ($attempt = 1; $attempt -le 30; $attempt++) {
    $lastLines = & $Exe @Arguments 2>&1
    $lastCode = $LASTEXITCODE
    if ($lastCode -eq 0) {
      return $lastLines
    }
    $text = $lastLines -join "`n"
    if ($text -match "Permission denied|Access is denied|being used by another process") {
      Start-Sleep -Seconds 2
      continue
    }
    Write-Error $text
    exit $lastCode
  }
  Write-Error ($lastLines -join "`n")
  exit $lastCode
}

if ($IsWindows -or $env:OS -eq "Windows_NT") {
  $dumpbin = & powershell -NoProfile -Command "(Get-Command dumpbin.exe -ErrorAction SilentlyContinue).Path"
  if ($dumpbin) {
    $lines = Invoke-NativeInspector $dumpbin @("/dependents", $binaryPath)
    Assert-WindowsSystemDlls $lines
    Assert-NoBuildHostStrings $binaryPath
    exit 0
  }
  $objdump = & powershell -NoProfile -Command "(Get-Command objdump.exe -ErrorAction SilentlyContinue).Path"
  if ($objdump) {
    $lines = Invoke-NativeInspector $objdump @("-p", $binaryPath)
    Assert-WindowsSystemDlls $lines
    Assert-NoBuildHostStrings $binaryPath
    exit 0
  }
  throw "No PE import inspector found. Install dumpbin.exe or objdump.exe to prove NFR-015 on Windows."
}

if ($IsMacOS) {
  & otool -L $binaryPath
  exit $LASTEXITCODE
}

& ldd $binaryPath
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
Assert-NoBuildHostStrings $binaryPath
