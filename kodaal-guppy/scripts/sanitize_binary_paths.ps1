param(
  [Parameter(Mandatory = $true)]
  [string]$Binary,
  [Parameter(Mandatory = $true)]
  [string[]]$Prefixes
)

$ErrorActionPreference = "Stop"
if (!(Test-Path $Binary)) {
  throw "Binary not found: $Binary"
}

$encoding = [System.Text.Encoding]::GetEncoding("iso-8859-1")
$bytes = [System.IO.File]::ReadAllBytes($Binary)
$originalBytes = [byte[]]$bytes.Clone()
$text = $encoding.GetString($bytes)
$replacements = 0

function Read-UInt16Le([byte[]]$Data, [int]$Offset) {
  if ($Offset -lt 0 -or ($Offset + 1) -ge $Data.Length) { return 0 }
  return [int]$Data[$Offset] -bor ([int]$Data[$Offset + 1] -shl 8)
}

function Read-UInt32Le([byte[]]$Data, [int]$Offset) {
  if ($Offset -lt 0 -or ($Offset + 3) -ge $Data.Length) { return 0 }
  return [uint32](
    [uint32]$Data[$Offset] -bor
    ([uint32]$Data[$Offset + 1] -shl 8) -bor
    ([uint32]$Data[$Offset + 2] -shl 16) -bor
    ([uint32]$Data[$Offset + 3] -shl 24)
  )
}

function Write-Zeroes([byte[]]$Data, [int]$Offset, [int]$Count) {
  if ($Offset -lt 0 -or $Count -le 0 -or $Offset -ge $Data.Length) { return $false }
  $end = [Math]::Min($Data.Length, $Offset + $Count)
  for ($i = $Offset; $i -lt $end; $i++) {
    if ($Data[$i] -ne 0) {
      $script:metadataNormalizations += 1
      $Data[$i] = 0
    }
  }
  return $true
}

function Convert-RvaToOffset([byte[]]$Data, [uint32]$Rva, [int]$SectionTable, [int]$SectionCount) {
  for ($i = 0; $i -lt $SectionCount; $i++) {
    $section = $SectionTable + ($i * 40)
    if (($section + 39) -ge $Data.Length) { break }
    $virtualSize = Read-UInt32Le $Data ($section + 8)
    $virtualAddress = Read-UInt32Le $Data ($section + 12)
    $rawSize = Read-UInt32Le $Data ($section + 16)
    $rawPointer = Read-UInt32Le $Data ($section + 20)
    $span = [Math]::Max($virtualSize, $rawSize)
    if ($Rva -ge $virtualAddress -and $Rva -lt ($virtualAddress + $span)) {
      return [int]($rawPointer + ($Rva - $virtualAddress))
    }
  }
  return -1
}

$metadataNormalizations = 0

if ($bytes.Length -gt 0x40) {
  $peOffset = [int](Read-UInt32Le $bytes 0x3c)
  if ($peOffset -gt 0 -and ($peOffset + 24) -lt $bytes.Length -and
      $bytes[$peOffset] -eq 0x50 -and $bytes[$peOffset + 1] -eq 0x45 -and
      $bytes[$peOffset + 2] -eq 0 -and $bytes[$peOffset + 3] -eq 0) {
    Write-Zeroes $bytes ($peOffset + 8) 4 | Out-Null

    $sectionCount = Read-UInt16Le $bytes ($peOffset + 6)
    $optionalSize = Read-UInt16Le $bytes ($peOffset + 20)
    $optionalOffset = $peOffset + 24
    $sectionTable = $optionalOffset + $optionalSize
    $magic = Read-UInt16Le $bytes $optionalOffset
    $dataDirectoryOffset = if ($magic -eq 0x20b) { $optionalOffset + 112 } elseif ($magic -eq 0x10b) { $optionalOffset + 96 } else { -1 }
    if ($dataDirectoryOffset -gt 0) {
      $debugDirectoryEntry = $dataDirectoryOffset + (6 * 8)
      $debugRva = Read-UInt32Le $bytes $debugDirectoryEntry
      $debugSize = Read-UInt32Le $bytes ($debugDirectoryEntry + 4)
      if ($debugRva -ne 0 -and $debugSize -ge 28) {
        $debugOffset = Convert-RvaToOffset $bytes $debugRva $sectionTable $sectionCount
        if ($debugOffset -ge 0) {
          for ($entry = $debugOffset; $entry -lt ($debugOffset + [int]$debugSize); $entry += 28) {
            Write-Zeroes $bytes ($entry + 4) 4 | Out-Null
          }
        }
      }
    }
  }
}

for ($i = 0; $i -le ($bytes.Length - 20); $i++) {
  if ($bytes[$i] -eq 0x52 -and $bytes[$i + 1] -eq 0x53 -and $bytes[$i + 2] -eq 0x44 -and $bytes[$i + 3] -eq 0x53) {
    Write-Zeroes $bytes ($i + 4) 16 | Out-Null
  }
}

foreach ($prefix in $Prefixes) {
  if (!$prefix) { continue }
  $variants = @($prefix, ($prefix -replace "\\", "/")) | Sort-Object -Unique
  foreach ($variant in $variants) {
    if (!$variant) { continue }
    $pattern = [regex]::Escape($variant)
    $matches = [regex]::Matches($text, $pattern).Count
    if ($matches -gt 0) {
      $text = [regex]::Replace($text, $pattern, "#" * $variant.Length)
      $replacements += $matches
    }
  }
}

$pathBytes = $encoding.GetBytes($text)
if ($pathBytes.Length -eq $bytes.Length) {
  for ($i = 0; $i -lt $bytes.Length; $i++) {
    if ($pathBytes[$i] -ne $originalBytes[$i]) {
      $bytes[$i] = $pathBytes[$i]
    }
  }
} else {
  throw "Binary path sanitizer changed byte length for $Binary"
}

if ($replacements -gt 0 -or $metadataNormalizations -gt 0) {
  [System.IO.File]::WriteAllBytes($Binary, $bytes)
}
Write-Host "Binary path sanitizer replaced $replacements path occurrence(s) and normalized $metadataNormalizations metadata byte(s)."
