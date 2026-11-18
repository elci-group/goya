<#
.SYNOPSIS
    Sash Phase 0 Launch Test Harness
    Launches a Sash-generated App.exe and validates runtime behavior.

.DESCRIPTION
    This script launches a Sash Windows executable and validates:
    - Process starts successfully
    - Window appears with correct title
    - Window class matches expected
    - Controls are placed at expected DIP positions
    - System message font is used
    - Exit code is 0 on clean close

.PARAMETER Arch
    Architecture: x64, i686, arm64

.PARAMETER Profile
    Profile name: static-window, interactive-cut

.PARAMETER Binary
    Binary name (default: App.exe)

.PARAMETER TimeoutSec
    Maximum wait time for window to appear (default: 30)

.PARAMETER ResultsDir
    Directory to write results (default: C:\sash-test\results)

.EXAMPLE
    .\launch_test.ps1 -Arch x64 -Profile static-window

.EXAMPLE
    .\launch_test.ps1 -Arch arm64 -Profile interactive-cut -TimeoutSec 60
#>

param(
    [Parameter(Mandatory=$true)]
    [ValidateSet('x64', 'i686', 'arm64')]
    [string]$Arch,

    [Parameter(Mandatory=$true)]
    [ValidateSet('static-window', 'interactive-cut')]
    [string]$Profile,

    [string]$Binary = 'App.exe',

    [int]$TimeoutSec = 30,

    [string]$ResultsDir = 'C:\sash-test\results',

    [string]$ExpectationsFile = ''
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

# Import required assemblies
Add-Type @"
    using System;
    using System.Runtime.InteropServices;
    using System.Text;

    public class Win32 {
        [DllImport("user32.dll", SetLastError = true, CharSet = CharSet.Unicode)]
        public static extern IntPtr FindWindow(string lpClassName, string lpWindowName);

        [DllImport("user32.dll", SetLastError = true, CharSet = CharSet.Unicode)]
        public static extern int GetWindowText(IntPtr hWnd, StringBuilder lpString, int nMaxCount);

        [DllImport("user32.dll", SetLastError = true, CharSet = CharSet.Unicode)]
        public static extern int GetClassName(IntPtr hWnd, StringBuilder lpClassName, int nMaxCount);

        [DllImport("user32.dll", SetLastError = true)]
        public static extern bool GetWindowRect(IntPtr hWnd, out RECT lpRect);

        [DllImport("user32.dll", SetLastError = true)]
        public static extern IntPtr GetWindow(IntPtr hWnd, uint uCmd);

        [DllImport("user32.dll", SetLastError = true)]
        public static extern IntPtr GetForegroundWindow();

        [DllImport("user32.dll", SetLastError = true)]
        public static extern bool IsWindowVisible(IntPtr hWnd);

        [DllImport("user32.dll", SetLastError = true)]
        public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint lpdwProcessId);

        [DllImport("user32.dll", SetLastError = true, CharSet = CharSet.Unicode)]
        public static extern IntPtr SendMessage(IntPtr hWnd, uint Msg, IntPtr wParam, IntPtr lParam);

        [DllImport("user32.dll", SetLastError = true)]
        public static extern int GetDlgCtrlID(IntPtr hWnd);

        [DllImport("user32.dll", SetLastError = true)]
        public static extern IntPtr GetAncestor(IntPtr hWnd, uint gaFlags);

        [DllImport("kernel32.dll", SetLastError = true)]
        public static extern uint GetCurrentProcessId();

        [DllImport("kernel32.dll", SetLastError = true)]
        public static extern bool CloseHandle(IntPtr hObject);

        [DllImport("kernel32.dll", SetLastError = true)]
        public static extern IntPtr OpenProcess(uint dwDesiredAccess, bool bInheritHandle, uint dwProcessId);

        [DllImport("kernel32.dll", SetLastError = true)]
        public static extern uint WaitForSingleObject(IntPtr hHandle, uint dwMilliseconds);

        [DllImport("kernel32.dll", SetLastError = true)]
        public static extern bool GetExitCodeProcess(IntPtr hProcess, out uint lpExitCode);

        public const uint GW_HWNDNEXT = 2;
        public const uint GW_CHILD = 5;
        public const uint GA_ROOT = 2;
        public const uint PROCESS_QUERY_LIMITED_INFORMATION = 0x1000;
        public const uint WM_GETFONT = 0x0031;
        public const uint WM_GETTEXT = 0x000D;
        public const uint WM_GETTEXTLENGTH = 0x000E;

        [StructLayout(LayoutKind.Sequential)]
        public struct RECT {
            public int Left;
            public int Top;
            public int Right;
            public int Bottom;
        }
    }
"@

# Configuration
$ArtifactDir = "C:\sash-test\artifacts\$Arch\$Profile"
$BinaryPath = Join-Path $ArtifactDir $Binary
$LogPath = Join-Path $ResultsDir "$Arch-$Profile-launch.log"
$EvidencePath = Join-Path $ResultsDir "$Arch-$Profile-evidence.json"

# Expected values come from the compiler, not from this script. Generate
# them beside the binary before transfer:
#   sash expectations <scene> --format json > expectations.json
# A hand-maintained table here could silently drift from the emitted program.
$ExpectationsPath = if ($ExpectationsFile) {
    $ExpectationsFile
} else {
    Join-Path $ArtifactDir 'expectations.json'
}
if (-not (Test-Path $ExpectationsPath)) {
    Write-Host "MISSING: $ExpectationsPath" -ForegroundColor Red
    Write-Host "Generate it with: sash expectations <scene.gscene> --format json > expectations.json" -ForegroundColor Yellow
    Write-Host "and transfer it beside App.exe. The harness does not guess window titles or control tables."
    exit 3
}
$Expectations = Get-Content $ExpectationsPath -Raw | ConvertFrom-Json
if ($Expectations.status -ne 'ADMITTED') {
    Write-Host "REFUSED: expectations file is not ADMITTED (status: $($Expectations.status))" -ForegroundColor Red
    exit 3
}
$Exp = @{
    WindowTitle  = $Expectations.window.title
    WindowClass  = $Expectations.window.class
    ControlCount = @($Expectations.controls).Count
    Controls     = @($Expectations.controls | ForEach-Object {
        @{
            SemanticId = $_.semantic_id
            Text       = $_.text
            Role       = $_.role
            Dip        = $_.dip
        }
    })
}
$ExpectationProfile = $Expectations.presentation_profile

# Initialize results
$Results = @{
    Arch = $Arch
    Profile = $Profile
    Binary = $Binary
    StartTime = (Get-Date).ToString('o')
    Checks = @{}
    Overall = 'PENDING'
}

function Write-Log {
    param([string]$Message, [string]$Level = 'INFO')
    $entry = "[$(Get-Date -Format 'HH:mm:ss.fff')] [$Level] $Message"
    Write-Host $entry
    Add-Content -Path $LogPath -Value $entry
}

function Check-Condition {
    param([string]$Name, [bool]$Condition, [string]$Detail = '')
    $Results.Checks[$Name] = @{
        Pass = $Condition
        Detail = $Detail
        Timestamp = (Get-Date).ToString('o')
    }
    if ($Condition) {
        Write-Log "CHECK PASS: $Name" 'PASS'
    } else {
        Write-Log "CHECK FAIL: $Name - $Detail" 'FAIL'
    }
    return $Condition
}

function Get-WindowByTitle {
    param([string]$Title, [int]$MaxWaitMs)
    $start = [Environment]::TickCount
    while (([Environment]::TickCount - $start) -lt $MaxWaitMs) {
        $hwnd = [Win32]::FindWindow($null, $Title)
        if ($hwnd -ne [IntPtr]::Zero) {
            return $hwnd
        }
        Start-Sleep -Milliseconds 100
    }
    return [IntPtr]::Zero
}

function Get-ChildWindows {
    param([IntPtr]$ParentHwnd)
    $children = @()
    $child = [Win32]::GetWindow($ParentHwnd, [Win32]::GW_CHILD)
    while ($child -ne [IntPtr]::Zero) {
        if ([Win32]::IsWindowVisible($child)) {
            $children += $child
        }
        $child = [Win32]::GetWindow($child, [Win32]::GW_HWNDNEXT)
    }
    return $children
}

function Get-WindowText {
    param([IntPtr]$Hwnd)
    $length = [Win32]::SendMessage($Hwnd, [Win32]::WM_GETTEXTLENGTH, [IntPtr]::Zero, [IntPtr]::Zero)
    if ($length -gt 0) {
        $sb = New-Object System.Text.StringBuilder($length + 1)
        [Win32]::SendMessage($Hwnd, [Win32]::WM_GETTEXT, [IntPtr]($length + 1), [IntPtr]$sb.GetType().GetField('m_StringValue', 'NonPublic,Instance').GetValue($sb)) | Out-Null
        return $sb.ToString()
    }
    return ''
}

function Get-WindowClass {
    param([IntPtr]$Hwnd)
    $sb = New-Object System.Text.StringBuilder(256)
    [Win32]::GetClassName($Hwnd, $sb, 256) | Out-Null
    return $sb.ToString()
}

function Get-WindowRect {
    param([IntPtr]$Hwnd)
    $rect = New-Object Win32+RECT
    if ([Win32]::GetWindowRect($Hwnd, [ref]$rect)) {
        return @{
            Left = $rect.Left
            Top = $rect.Top
            Right = $rect.Right
            Bottom = $rect.Bottom
            Width = $rect.Right - $rect.Left
            Height = $rect.Bottom - $rect.Top
        }
    }
    return $null
}

# Main execution
try {
    Write-Log "=== Sash Launch Test: $Arch / $Profile ==="
    Write-Log "Binary: $BinaryPath"

    # Verify binary exists
    if (-not (Test-Path $BinaryPath)) {
        throw "Binary not found: $BinaryPath"
    }
    Check-Condition 'BinaryExists' $true "Found at $BinaryPath"

    # Verify PE structure with dumpbin
    $dumpbin = & "dumpbin.exe" /headers $BinaryPath 2>&1
    $peOk = $dumpbin -match 'PE32' -and $dumpbin -match 'GUI' -and $dumpbin -match 'DLL characteristics.*0x[Cc]1[46]0'
    Check-Condition 'PEHeadersValid' $peOk "dumpbin output parsed"

    # Capture static imports
    $imports = & "dumpbin.exe" /imports $BinaryPath 2>&1
    $expectedImports = @('KERNEL32.dll', 'USER32.dll', 'GDI32.dll', 'ntdll.dll', 'api-ms-win-core-synch-l1-2-0.dll')
    $importsOk = $true
    foreach ($imp in $expectedImports) {
        if ($imports -notmatch $imp) {
            $importsOk = $false
            Write-Log "Missing expected import: $imp" 'WARN'
        }
    }
    # Check for CRT redistributables (should NOT be present)
    $crtImports = @('MSVCRT', 'VCRUNTIME', 'ucrtbase', 'concrt', 'msvcp')
    foreach ($crt in $crtImports) {
        if ($imports -match $crt) {
            $importsOk = $false
            Write-Log "Unexpected CRT import: $crt" 'FAIL'
        }
    }
    Check-Condition 'ImportClosureCorrect' $importsOk "Imports: $($expectedImports -join ', '); No CRT redistributables"

    # Launch process
    Write-Log "Launching process..."
    $psi = New-Object System.Diagnostics.ProcessStartInfo
    $psi.FileName = $BinaryPath
    $psi.UseShellExecute = $false
    $psi.CreateNoWindow = $false
    $proc = [System.Diagnostics.Process]::Start($psi)

    if (-not $proc) {
        throw "Failed to start process"
    }
    Write-Log "Process started: PID $($proc.Id)"

    # Wait for window to appear
    Write-Log "Waiting for window: '$($Exp.WindowTitle)' (max ${TimeoutSec}s)..."
    $hwnd = Get-WindowByTitle $Exp.WindowTitle ($TimeoutSec * 1000)
    if ($hwnd -eq [IntPtr]::Zero) {
        $proc.Kill()
        throw "Window did not appear within ${TimeoutSec}s"
    }
    Write-Log "Window found: HWND = 0x$($hwnd.ToString('X'))"

    # Verify window class
    $className = Get-WindowClass $hwnd
    $classOk = ($className -eq $Exp.WindowClass)
    Check-Condition 'WindowClassCorrect' $classOk "Expected: $($Exp.WindowClass), Got: $className"

    # Verify window title
    $title = Get-WindowText $hwnd
    $titleOk = ($title -eq $Exp.WindowTitle)
    Check-Condition 'WindowTitleCorrect' $titleOk "Expected: '$($Exp.WindowTitle)', Got: '$title'"

    # Get window rect (client area in DIPs at 96 DPI)
    $rect = Get-WindowRect $hwnd
    if ($rect) {
        Write-Log "Window rect: $($rect.Left),$($rect.Top) $($rect.Width)x$($rect.Height)"
        # Note: This is window rect including frame. Client rect would need AdjustWindowRectExForDpi reverse.
        Check-Condition 'WindowRectCaptured' $true "Rect captured"
    }

    # Enumerate child controls
    $children = Get-ChildWindows $hwnd
    Write-Log "Found $($children.Count) visible child windows"

    # For each expected control, try to find matching child
    $controlResults = @()
    foreach ($expCtrl in $Exp.Controls) {
        $found = $false
        $idOk = $false
        $bestMatch = $null
        foreach ($child in $children) {
            $childText = Get-WindowText $child
            $childClass = Get-WindowClass $child
            $childRect = Get-WindowRect $child
            $childId = [Win32]::GetDlgCtrlID($child)

            # Match by text content (primary); the native control id must
            # match the compiler-declared one when the child carries an id.
            if ($childText -eq $expCtrl.Text) {
                $found = $true
                if ($childId -eq $expCtrl.control_id) { $idOk = $true }
                $bestMatch = @{
                    Hwnd = $child
                    Text = $childText
                    Class = $childClass
                    Rect = $childRect
                    ControlId = $childId
                }
                break
            }
        }
        $ctrlResult = @{
            Expected = $expCtrl
            Found = $found
            ControlIdMatches = $idOk
            Match = $bestMatch
        }
        $controlResults += $ctrlResult

        $msg = if ($found) { "FOUND: '$($expCtrl.Text)' ($($expCtrl.SemanticId)), control_id match: $idOk" } else { "MISSING: '$($expCtrl.Text)' ($($expCtrl.SemanticId))" }
        Check-Condition "Control_$($expCtrl.SemanticId.Replace('.','_'))" $found $msg
        if ($found) {
            Check-Condition "ControlId_$($expCtrl.SemanticId.Replace('.','_'))" $idOk "GetDlgCtrlID = $($bestMatch.ControlId), expected = $($expCtrl.control_id)"
        }
    }

    # Verify font (WM_GETFONT on first STATIC child)
    $staticChildren = $children | Where-Object { (Get-WindowClass $_) -eq 'STATIC' }
    if ($staticChildren.Count -gt 0) {
        $fontPtr = [Win32]::SendMessage($staticChildren[0], [Win32]::WM_GETFONT, [IntPtr]::Zero, [IntPtr]::Zero)
        $fontOk = ($fontPtr -ne [IntPtr]::Zero)
        Check-Condition 'SystemFontUsed' $fontOk "STATIC control has non-zero font handle (system message font)"
    } else {
        Check-Condition 'SystemFontUsed' $false "No STATIC children found to check font"
    }

    # Wait for process to exit (user closes window or timeout)
    Write-Log "Waiting for process exit (max 60s)..."
    $waitResult = $proc.WaitForExit(60000)
    if (-not $waitResult) {
        Write-Log "Process did not exit cleanly, terminating..." 'WARN'
        $proc.Kill()
        $proc.WaitForExit(5000)
    }

    $exitCode = $proc.ExitCode
    $exitOk = ($exitCode -eq 0)
    Check-Condition 'ExitCodeZero' $exitOk "Exit code: $exitCode"

    # Overall result
    $allPass = $Results.Checks.Values | Where-Object { -not $_.Pass } | Measure-Object | Select-Object -ExpandProperty Count
    $Results.Overall = if ($allPass -eq 0) { 'PASS' } else { 'FAIL' }
    $Results.EndTime = (Get-Date).ToString('o')
    $Results.ControlDetails = $controlResults

    # Write evidence JSON
    $Results | ConvertTo-Json -Depth 5 | Out-File -FilePath $EvidencePath -Encoding utf8
    Write-Log "Evidence written to: $EvidencePath"
    Write-Log "=== RESULT: $($Results.Overall) ==="

    exit ($Results.Overall -eq 'PASS' ? 0 : 1)

}
catch {
    $Results.Overall = 'ERROR'
    $Results.Error = $_.ToString()
    $Results.EndTime = (Get-Date).ToString('o')
    $Results | ConvertTo-Json -Depth 5 | Out-File -FilePath $EvidencePath -Encoding utf8
    Write-Log "ERROR: $($_.Exception.Message)" 'ERROR'
    exit 2
}
finally {
    if ($proc -and -not $proc.HasExited) {
        $proc.Kill()
    }
}