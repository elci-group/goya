<#
.SYNOPSIS
    Sash Windows smoke launch - first-launch evidence on any Windows machine.

.DESCRIPTION
    Minimal launch check for GitHub-hosted runners and other machines without
    the qualification tooling. It loads the compiler-generated expectations
    JSON, launches App.exe, verifies the window appears with the expected
    class and title, closes it, and checks the exit code.

    THIS IS A SMOKE CLAIM, NOT QUALIFICATION. A GitHub-hosted runner carries
    developer tools and is not a clean machine. It does not clear S1-EXE,
    S1-CLOSURE, or any Phase 1 gate. It only records that a Windows session
    could create the window, find the controls, and exit cleanly.

.PARAMETER ArtifactDir
    Directory holding App.exe and expectations.json.

.PARAMETER ResultsDir
    Directory for the smoke evidence (default: alongside the artifact).

.EXAMPLE
    .\smoke_launch.ps1 -ArtifactDir C:\sash-test\artifacts\x64\static-window
#>

param(
    [Parameter(Mandatory = $true)]
    [string]$ArtifactDir,

    [string]$ResultsDir = ''
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

if (-not $ResultsDir) { $ResultsDir = $ArtifactDir }

$BinaryPath = Join-Path $ArtifactDir 'App.exe'
$ExpectationsPath = Join-Path $ArtifactDir 'expectations.json'
$EvidencePath = Join-Path $ResultsDir 'smoke_evidence.json'

Add-Type @"
    using System;
    using System.Runtime.InteropServices;
    using System.Text;

    public class SmokeWin32 {
        [DllImport("user32.dll", SetLastError = true, CharSet = CharSet.Unicode)]
        public static extern IntPtr FindWindow(string lpClassName, string lpWindowName);

        [DllImport("user32.dll", SetLastError = true, CharSet = CharSet.Unicode)]
        public static extern int GetClassName(IntPtr hWnd, StringBuilder lpClassName, int nMaxCount);

        [DllImport("user32.dll", SetLastError = true, CharSet = CharSet.Unicode)]
        public static extern int GetWindowText(IntPtr hWnd, StringBuilder lpString, int nMaxCount);

        [DllImport("user32.dll", SetLastError = true)]
        public static extern IntPtr SendMessage(IntPtr hWnd, uint Msg, IntPtr wParam, IntPtr lParam);

        [DllImport("user32.dll", SetLastError = true)]
        public static extern IntPtr GetWindow(IntPtr hWnd, uint uCmd);

        public const uint WM_CLOSE = 0x0010;
        public const uint GW_CHILD = 5;
        public const uint GW_HWNDNEXT = 2;
    }
"@

$Results = [ordered]@{
    claim              = 'smoke-launch'
    qualified          = $false
    machine_note       = 'not a clean machine; does not clear S1-EXE or Phase 1 gates'
    artifact_dir       = $ArtifactDir
    started_at         = (Get-Date).ToString('o')
    checks             = [ordered]@{}
    overall            = 'PENDING'
}

function Check {
    param([string]$Name, [bool]$Condition, [string]$Detail = '')
    $Results.checks[$Name] = @{ pass = $Condition; detail = $Detail }
    $mark = if ($Condition) { 'PASS' } else { 'FAIL' }
    Write-Host "[$mark] $Name $Detail"
    if (-not $Condition) { $script:allPass = $false }
}

$allPass = $true

try {
    Write-Host "=== Sash smoke launch: $ArtifactDir ==="

    # Expectations are mandatory: the smoke check verifies against what the
    # compiler declared, never against a hand-copied table.
    if (-not (Test-Path $ExpectationsPath)) {
        throw "expectations.json not found at $ExpectationsPath - generate it with: sash expectations <scene> --format json"
    }
    $Expectations = Get-Content $ExpectationsPath -Raw | ConvertFrom-Json
    if ($Expectations.status -ne 'ADMITTED') {
        throw "expectations status is '$($Expectations.status)', not ADMITTED"
    }
    Check 'ExpectationsAdmitted' $true "profile: $($Expectations.presentation_profile), api floor: $($Expectations.api_floor)"

    if (-not (Test-Path $BinaryPath)) { throw "App.exe not found at $BinaryPath" }
    Check 'BinaryPresent' $true ((Get-Item $BinaryPath).Length.ToString() + ' bytes')

    $hash = (Get-FileHash -Algorithm SHA256 $BinaryPath).Hash.ToLower()
    $Results.exe_sha256 = $hash
    Check 'BinaryHashed' $true "sha256: $($hash.Substring(0, 16))..."

    # Launch
    $psi = New-Object System.Diagnostics.ProcessStartInfo
    $psi.FileName = $BinaryPath
    $psi.UseShellExecute = $false
    $proc = [System.Diagnostics.Process]::Start($psi)
    if (-not $proc) { throw 'the process could not be started' }
    Check 'ProcessStarted' $true "PID $($proc.Id)"

    try {
        # Wait for the expected window (up to 30s)
        $deadline = [DateTime]::UtcNow.AddSeconds(30)
        $hwnd = [IntPtr]::Zero
        while ([DateTime]::UtcNow -lt $deadline) {
            if ($proc.HasExited) { break }
            $hwnd = [SmokeWin32]::FindWindow($null, $Expectations.window.title)
            if ($hwnd -ne [IntPtr]::Zero) { break }
            Start-Sleep -Milliseconds 100
        }
        if ($hwnd -eq [IntPtr]::Zero) {
            throw "window '$($Expectations.window.title)' did not appear within 30s"
        }

        $sb = New-Object System.Text.StringBuilder(256)
        [SmokeWin32]::GetClassName($hwnd, $sb, 256) | Out-Null
        $class = $sb.ToString()
        Check 'WindowClassMatches' ($class -eq $Expectations.window.class) "expected $($Expectations.window.class), got $class"

        $sb2 = New-Object System.Text.StringBuilder(256)
        [SmokeWin32]::GetWindowText($hwnd, $sb2, 256) | Out-Null
        Check 'WindowTitleMatches' ($sb2.ToString() -eq $Expectations.window.title) "'$($sb2.ToString())'"

        # Count visible children and compare with the expectations table.
        $expectedCount = @($Expectations.controls).Count
        $children = @()
        $child = [SmokeWin32]::GetWindow($hwnd, [SmokeWin32]::GW_CHILD)
        while ($child -ne [IntPtr]::Zero) {
            $children += $child
            $child = [SmokeWin32]::GetWindow($child, [SmokeWin32]::GW_HWNDNEXT)
        }
        Check 'ControlCountMatches' ($children.Count -eq $expectedCount) "expected $expectedCount, saw $($children.Count)"

        # Every expected control text must appear among the children.
        foreach ($expected in $Expectations.controls) {
            $seen = $false
            foreach ($c in $children) {
                $sb3 = New-Object System.Text.StringBuilder(256)
                [SmokeWin32]::GetWindowText($c, $sb3, 256) | Out-Null
                if ($sb3.ToString() -eq $expected.text) { $seen = $true; break }
            }
            Check "ControlText_$($expected.semantic_id.Replace('.', '_'))" $seen "'$($expected.text)'"
        }
    }
    finally {
        if (-not $proc.HasExited) {
            if ($hwnd -ne [IntPtr]::Zero) {
                [SmokeWin32]::SendMessage($hwnd, [SmokeWin32]::WM_CLOSE, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null
            }
            if (-not $proc.WaitForExit(10000)) {
                $proc.Kill()
                Check 'CleanClose' $false 'process killed after WM_CLOSE timeout'
            }
        }
    }

    Check 'ExitCodeZero' ($proc.ExitCode -eq 0) "exit code $($proc.ExitCode)"

    $Results.overall = if ($allPass) { 'SMOKE-PASS' } else { 'FAIL' }
    $Results.ended_at = (Get-Date).ToString('o')
    $Results | ConvertTo-Json -Depth 5 | Out-File -FilePath $EvidencePath -Encoding utf8
    Write-Host "=== SMOKE RESULT: $($Results.overall) (evidence: $EvidencePath) ==="
    Write-Host "This is smoke evidence only. Runtime qualification stays BLOCKED until a clean Windows machine runs the qualification harness."
    exit 0
}
catch {
    $Results.overall = 'ERROR'
    $Results.error = $_.ToString()
    $Results.ended_at = (Get-Date).ToString('o')
    $Results | ConvertTo-Json -Depth 5 | Out-File -FilePath $EvidencePath -Encoding utf8
    Write-Host "SMOKE ERROR: $($_.Exception.Message)"
    if ($proc -and -not $proc.HasExited) { $proc.Kill() }
    exit 1
}
