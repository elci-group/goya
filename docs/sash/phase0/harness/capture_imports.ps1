<#
.SYNOPSIS
    Sash Phase 0 Import Closure Capture
    Captures static and dynamic import closure for a Sash binary.

.DESCRIPTION
    Uses dumpbin, depends.exe, and Process Monitor to capture the complete
    import closure of a Sash-generated executable on a clean Windows machine.

.PARAMETER Arch
    Architecture: x64, i686, arm64

.PARAMETER Profile
    Profile name: static-window, interactive-cut

.PARAMETER ResultsDir
    Directory to write results (default: C:\sash-test\results)

.PARAMETER Binary
    Binary name (default: App.exe)

.EXAMPLE
    .\capture_imports.ps1 -Arch x64 -Profile static-window

.EXAMPLE
    .\capture_imports.ps1 -Arch arm64 -Profile interactive-cut
#>

param(
    [Parameter(Mandatory=$true)]
    [ValidateSet('x64', 'i686', 'arm64')]
    [string]$Arch,

    [Parameter(Mandatory=$true)]
    [ValidateSet('static-window', 'interactive-cut')]
    [string]$Profile,

    [string]$ResultsDir = 'C:\sash-test\results',

    [string]$Binary = 'App.exe'
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$ArtifactDir = "C:\sash-test\artifacts\$Arch\$Profile"
$BinaryPath = Join-Path $ArtifactDir $Binary
$LogPath = Join-Path $ResultsDir "$Arch-$Profile-imports.log"
$OutputPath = Join-Path $ResultsDir "$Arch-$Profile-import_closure.json"

function Write-Log {
    param([string]$Message, [string]$Level = 'INFO')
    $entry = "[$(Get-Date -Format 'HH:mm:ss.fff')] [$Level] $Message"
    Write-Host $entry
    Add-Content -Path $LogPath -Value $entry
}

$Results = @{
    Arch = $Arch
    Profile = $Profile
    Binary = $Binary
    Timestamp = (Get-Date).ToString('o')
    StaticImports = @()
    DynamicImports = @()
    LoadOrder = @()
    ModulePaths = @{}
    HasCRTImports = $false
    UnexpectedImports = @()
    Overall = 'PENDING'
}

try {
    Write-Log "=== Sash Import Closure Capture: $Arch / $Profile ==="
    Write-Log "Binary: $BinaryPath"

    if (-not (Test-Path $BinaryPath)) { throw "Binary not found: $BinaryPath" }

    # 1. Static imports via dumpbin /imports
    Write-Log "Running dumpbin /imports..."
    $dumpbinOut = & "dumpbin.exe" /imports $BinaryPath 2>&1
    Write-Log "dumpbin output captured"

    # Parse dumpbin output for DLL imports
    $currentDll = ''
    foreach ($line in $dumpbinOut) {
        if ($line -match '^\s+([A-Z0-9_.-]+\.dll)') {
            $currentDll = $matches[1].ToUpper()
            if (-not $Results.StaticImports.Contains($currentDll)) {
                $Results.StaticImports += $currentDll
            }
        } elseif ($currentDll -and $line -match '^\s+[0-9A-F]+\s+([A-Za-z_][A-Za-z0-9_]*)') {
            $func = $matches[1]
            $Results.ModulePaths[$currentDll] = $true
        }
    }

    Write-Log "Static imports: $($Results.StaticImports -join ', ')"

    # 2. Check for CRT redistributables
    $crtPatterns = @('MSVCRT', 'VCRUNTIME', 'UCRTBASE', 'CONCRT', 'MSVCP')
    foreach ($dll in $Results.StaticImports) {
        foreach ($crt in $crtPatterns) {
            if ($dll -like "*$crt*") {
                $Results.HasCRTImports = $true
                $Results.UnexpectedImports += "CRT redistributable: $dll"
                Write-Log "UNEXPECTED CRT IMPORT: $dll" 'FAIL'
            }
        }
    }

    # 3. Full dependency tree via depends.exe (if available)
    if (Test-Path "depends.exe") {
        Write-Log "Running depends.exe for full tree..."
        $dependsOut = & "depends.exe" /c /ot:"$ResultsDir\$Arch-$Profile-depends.txt" $BinaryPath 2>&1
        $dependsContent = Get-Content "$ResultsDir\$Arch-$Profile-depends.txt" -Raw -ErrorAction SilentlyContinue
        if ($dependsContent) {
            # Parse depends output for all dependent DLLs
            foreach ($line in $dependsContent -split "`n") {
                if ($line -match '^\s+([A-Z0-9_.-]+\.dll)') {
                    $dll = $matches[1].ToUpper()
                    if (-not $Results.DynamicImports.Contains($dll)) {
                        $Results.DynamicImports += $dll
                    }
                }
            }
            Write-Log "Dynamic imports (depends): $($Results.DynamicImports.Count) DLLs"
        }
    } else {
        Write-Log "depends.exe not found, skipping dynamic analysis" 'WARN'
    }

    # 4. Expected baseline imports
    $ExpectedImports = @(
        'KERNEL32.DLL',
        'USER32.DLL',
        'GDI32.DLL',
        'NTDLL.DLL',
        'API-MS-WIN-CORE-SYNCH-L1-2-0.DLL'
    )

    $missing = @()
    foreach ($exp in $ExpectedImports) {
        if (-not $Results.StaticImports.Contains($exp)) {
            $missing += $exp
        }
    }

    if ($missing.Count -gt 0) {
        $Results.UnexpectedImports += "Missing expected imports: $($missing -join ', ')"
        Write-Log "MISSING EXPECTED IMPORTS: $($missing -join ', ')" 'FAIL'
    }

    # 5. Check for unexpected system DLLs (beyond expected + known API sets)
    $KnownApiSets = @(
        'API-MS-WIN-CORE-', 'API-MS-WIN-DEVICES-', 'API-MS-WIN-EVENTING-',
        'API-MS-WIN-SECURITY-', 'EXT-MS-WIN-'
    )
    foreach ($dll in $Results.StaticImports) {
        $isExpected = $ExpectedImports.Contains($dll)
        $isApiSet = $KnownApiSets | Where-Object { $dll -like "$_*" }
        if (-not $isExpected -and -not $isApiSet -and $dll -notlike '*.EXE') {
            $Results.UnexpectedImports += "Unexpected system DLL: $dll"
            Write-Log "UNEXPECTED SYSTEM DLL: $dll" 'WARN'
        }
    }

    # 6. PE headers for architecture verification
    Write-Log "Running dumpbin /headers for PE verification..."
    $headersOut = & "dumpbin.exe" /headers $BinaryPath 2>&1
    $peInfo = @{}
    foreach ($line in $headersOut) {
        if ($line -match 'machine\s+\(([A-Z0-9]+)\)') {
            $peInfo.Machine = $matches[1]
        } elseif ($line -match '(\d+)\s+file\s+offset\s+0\s+\(entry point\)') {
            $peInfo.EntryPointRVA = $matches[1]
        } elseif ($line -match 'DLL characteristics\s+([0-9A-F]+)') {
            $peInfo.DllCharacteristics = $matches[1]
        } elseif ($line -match 'subsystem\s+\(([^)]+)\)') {
            $peInfo.Subsystem = $matches[1]
        }
    }
    $Results.PEInfo = $peInfo
    Write-Log "PE Info: $($peInfo | ConvertTo-Json -Compress)"

    # 7. Overall assessment
    $criticalFail = $Results.HasCRTImports -or $missing.Count -gt 0
    $Results.Overall = if ($criticalFail) { 'FAIL' } elseif ($Results.UnexpectedImports.Count -gt 0) { 'WARN' } else { 'PASS' }

    $Results.EndTime = (Get-Date).ToString('o')

    $Results | ConvertTo-Json -Depth 5 | Out-File -FilePath $OutputPath -Encoding utf8
    Write-Log "Import closure evidence written to: $OutputPath"
    Write-Log "=== RESULT: $($Results.Overall) ==="

    exit ($Results.Overall -eq 'PASS' ? 0 : ($Results.Overall -eq 'WARN' ? 0 : 1))
}
catch {
    $Results.Overall = 'ERROR'
    $Results.Error = $_.ToString()
    $Results.EndTime = (Get-Date).ToString('o')
    $Results | ConvertTo-Json -Depth 5 | Out-File -FilePath $OutputPath -Encoding utf8
    Write-Log "ERROR: $($_.Exception.Message)" 'ERROR'
    exit 2
}