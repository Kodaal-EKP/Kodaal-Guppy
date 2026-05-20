$ErrorActionPreference = "Stop"

function Invoke-Gate {
    param(
        [Parameter(Mandatory = $true)]
        [string] $FilePath,

        [Parameter()]
        [string[]] $Arguments = @()
    )

    & $FilePath @Arguments
    if ($LASTEXITCODE -ne 0) {
        throw "Gate failed with exit code $LASTEXITCODE`: $FilePath $($Arguments -join ' ')"
    }
}

Invoke-Gate cargo @("fmt", "--all", "--", "--check")
Invoke-Gate cargo @("clippy", "--workspace", "--all-targets", "--", "-D", "warnings")
Invoke-Gate cargo @("test", "--workspace")
Invoke-Gate cargo @("check", "-p", "kodaal-tray", "--features", "desktop", "--bin", "kodaal-tray")
Invoke-Gate python @("scripts/check_traceability.py", "--check")
Invoke-Gate python @("scripts/check_markdown_links.py")
Invoke-Gate python @("scripts/check_brand_jsx_copies.py")
Invoke-Gate python @("scripts/check_file_size.py")
Invoke-Gate python @("scripts/check_api_parity.py")
Invoke-Gate python @("scripts/check_no_banned_patterns.py")
Invoke-Gate python @("scripts/check_placeholders.py")
Invoke-Gate python @("scripts/check_no_outbound_or_telemetry.py")
Invoke-Gate python @("scripts/check_dependency_pins.py")
Invoke-Gate python @("scripts/check_cargo_licenses.py")
Invoke-Gate python @("scripts/check_cargo_deny.py")
Invoke-Gate python @("scripts/check_ui_accessibility_static.py")
Invoke-Gate powershell @("-ExecutionPolicy", "Bypass", "-File", "scripts/check_coverage.ps1")
Invoke-Gate node @("browser-ext/scripts/validate-extension.mjs")
Invoke-Gate node @("--test", "browser-ext/tests/*.test.mjs")
Invoke-Gate node @("browser-ext/scripts/build.mjs", "all")
Invoke-Gate node @("ide-ext/vscode/scripts/validate-extension.mjs")
Invoke-Gate node @("--test", "ide-ext/vscode/tests/*.test.mjs")
Invoke-Gate node @("ide-ext/vscode/scripts/package.mjs")
Invoke-Gate python @("scripts/check_package_artifacts.py")
Invoke-Gate powershell @("-ExecutionPolicy", "Bypass", "-File", "scripts/check_installer_feasibility.ps1")
Invoke-Gate powershell @("-ExecutionPolicy", "Bypass", "-File", "scripts/check_time_to_first_capture.ps1")
Invoke-Gate powershell @("-ExecutionPolicy", "Bypass", "-File", "scripts/check_crash_durability.ps1")
Invoke-Gate powershell @("-ExecutionPolicy", "Bypass", "-File", "scripts/check_idle_resources.ps1", "-DurationSeconds", "300")
Invoke-Gate powershell @("-ExecutionPolicy", "Bypass", "-File", "scripts/check_zero_egress_local.ps1")
Invoke-Gate powershell @("-ExecutionPolicy", "Bypass", "-File", "scripts/build_release.ps1")
Invoke-Gate powershell @("-ExecutionPolicy", "Bypass", "-File", "scripts/check_static_binary.ps1")
Invoke-Gate cargo @("build", "--workspace")

Write-Host "All local gates passed."
