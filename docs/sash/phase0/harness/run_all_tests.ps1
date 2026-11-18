<#
.SYNOPSIS
    Sash Phase 0 Master Test Runner
    Orchestrates all Phase 0 tests for a given architecture and profile.

.DESCRIPTION
    Runs the complete test suite for a Sash binary:
    1. Import closure capture (static + dynamic)
    2. Launch test (window, controls, font, exit code)
    3. Accessibility capture (UIA tree)
    4. DPI transition test (x64, ARM64 only)

.PARAMETER Arch
    Architecture: x64, i686, arm64

.PARAMETER Profile
    Profile name: static-window, interactive-cut

.PARAMETER ResultsDir
    Base results directory (default: C:\sash-test\results)

.PARAMETER SkipDpi
    Skip DPI test (for i686 or if not applicable)

.EXAMPLE
    .\run_all_tests.ps1 -Arch x64 -Profile static-window

.EXAMPLE
    .\run_all_tests.ps1 -Arch arm64 -Profile interactive-cut

.EXAMPLE
    .\run_all_tests.ps1 -Arch i686 -Profile static-window -SkipDpi
#>

param(
    [Parameter(Mandatory=$true)]
    [ValidateSet('x64', 'i686', 'arm64')]
    [string]$Arch,

    [Parameter(Mandatory=$true)]
    [ValidateSet('static-window', 'interactive-cut')]
    [string]$Profile,

    [string]$ResultsDir = 'C:\sash-test\results',

    [switch]$SkipDpi
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$HarnessDir = Split-Path $MyInvocation.MyCommand.Path
$LogPath = Join-Path $ResultsDir "$Arch-$Profile-master.log"

function Write-Log {
    param([string]$Message, [string]$Level = 'INFO')
    $entry = "[$(Get-Date -Format 'HH:mm:ss.fff')] [$Level] $Message"
    Write-Host $entry
    Add-Content -Path $LogPath -Value $entry
}

function Run-Test {
    param([string]$Name, [string]$Script, [hashtable]$Params = @{})
    Write-Log "=== Starting $Name ==="
    $start = Get-Date
    $cmd = "& '$Script' " + ($Params.GetEnumerator() | ForEach-Object { "-$($_.Key) $($_.Value)" }) -join ' '
    Write-Log "Command: $cmd"
    try {
        $exitCode = & powershell.exe -NoProfile -ExecutionPolicy Bypass -Command $cmd
        $duration = (Get-Date) - $start
        if ($exitCode -eq 0) {
            Write-Log "$Name PASS (${duration.TotalSeconds:N1}s)"
            return @{ Name = $Name; Pass = $true; Duration = $duration.TotalSeconds }
        } else {
            Write-Log "$Name FAIL (exit $exitCode, ${duration.TotalSeconds:N1}s)" 'FAIL'
            return @{ Name = $Name; Pass = $false; ExitCode = $exitCode; Duration = $duration.TotalSeconds }
        }
    }
    catch {
        $duration = (Get-Date) - $start
        Write-Log "$Name ERROR: $($_.Exception.Message) (${duration.TotalSeconds:N1}s)" 'ERROR'
        return @{ Name = $Name; Pass = $false; Error = $_.ToString(); Duration = $duration.TotalSeconds }
    }
}

$MasterResults = @{
    Arch = $Arch
    Profile = $Profile
    Timestamp = (Get-Date).ToString('o')
    Tests = @()
    Overall = 'PENDING'
}

try {
    Write-Log "=== Sash Phase 0 Master Test: $Arch / $Profile ==="
    Write-Log "Results directory: $ResultsDir"

    # Ensure results directory exists
    New-Item -ItemType Directory -Force -Path $ResultsDir | Out-Null

    # Test 1: Import Closure (always runs)
    $MasterResults.Tests += Run-Test 'ImportClosure' "$HarnessDir\capture_imports.ps1" @{
        Arch = $Arch
        Profile = $Profile
        ResultsDir = $ResultsDir
    }

    # Test 2: Launch Test (always runs)
    $MasterResults.Tests += Run-Test 'LaunchTest' "$HarnessDir\launch_test.ps1" @{
        Arch = $Arch
        Profile = $Profile
        ResultsDir = $ResultsDir
    }

    # Test 3: Accessibility Capture (always runs)
    $MasterResults.Tests += Run-Test 'Accessibility' "$HarnessDir\capture_a11y.ps1" @{
        Arch = $Arch
        Profile = $Profile
        ResultsDir = $ResultsDir
    }

    # Test 4: DPI Test (x64 and ARM64 only, unless skipped)
    if (-not $SkipDpi -and ($Arch -eq 'x64' -or $Arch -eq 'arm64')) {
        $MasterResults.Tests += Run-Test 'DpiTransition' "$HarnessDir\dpi_test.ps1" @{
            Arch = $Arch
            Profile = $Profile
            ResultsDir = $ResultsDir
        }
    } else {
        Write-Log "Skipping DPI test for $Arch (SkipDpi=$SkipDpi)"
        $MasterResults.Tests += @{ Name = 'DpiTransition'; Pass = $true; Skipped = $true; Reason = "Not applicable for $Arch" }
    }

    # Aggregate results
    $failed = $MasterResults.Tests | Where-Object { -not $_.Pass -and -not $_.Skipped }
    $MasterResults.Overall = if ($failed.Count -eq 0) { 'PASS' } else { 'FAIL' }
    $MasterResults.EndTime = (Get-Date).ToString('o')
    $MasterResults.Summary = @{
        Total = $MasterResults.Tests.Count
        Passed = ($MasterResults.Tests | Where-Object { $_.Pass }).Count
        Failed = $failed.Count
        Skipped = ($MasterResults.Tests | Where-Object { $_.Skipped }).Count
    }

    # Write master evidence
    $MasterPath = Join-Path $ResultsDir "$Arch-$Profile-master_evidence.json"
    $MasterResults | ConvertTo-Json -Depth 10 | Out-File -FilePath $MasterPath -Encoding utf8

    Write-Log "=== MASTER RESULT: $($MasterResults.Overall) ==="
    Write-Log "Summary: $($MasterResults.Summary.Passed)/$($MasterResults.Summary.Total) tests passed"
    foreach ($test in $MasterResults.Tests) {
        $status = if ($test.Skipped) { 'SKIP' } elseif ($test.Pass) { 'PASS' } else { 'FAIL' }
        Write-Log "  [$status] $($test.Name) ($([math]::Round($test.Duration,1))s)"
    }
    Write-Log "Master evidence: $MasterPath"

    exit ($MasterResults.Overall -eq 'PASS' ? 0 : 1)
}
catch {
    $MasterResults.Overall = 'ERROR'
    $MasterResults.Error = $_.ToString()
    $MasterResults.EndTime = (Get-Date).ToString('o')
    $MasterPath = Join-Path $ResultsDir "$Arch-$Profile-master_evidence.json"
    $MasterResults | ConvertTo-Json -Depth 10 | Out-File -FilePath $MasterPath -Encoding utf8
    Write-Log "MASTER ERROR: $($_.Exception.Message)" 'ERROR'
    exit 2
}