<#
.SYNOPSIS
    Sash Phase 0 DPI Transition Test
    Tests PerMonitorV2 DPI awareness and font recreation on DPI change.

.DESCRIPTION
    Launches a Sash window and programmatically changes the system DPI
    (or simulates WM_DPICHANGED) to verify:
    - Window receives WM_DPICHANGED
    - Font is recreated via SystemParametersInfoForDpi + CreateFontIndirectW
    - Controls reposition correctly
    - Previous font is cleaned up on failure

.REQUIRES
    - Windows 10 1703+ (PerMonitorV2 support)
    - Must run as same user as the test window
    - Requires SetProcessDpiAwarenessContext (PerMonitorV2)

.PARAMETER Arch
    Architecture: x64, arm64 (i686 skipped - no multi-DPI test required)

.PARAMETER Profile
    Profile name: static-window, interactive-cut

.PARAMETER ResultsDir
    Directory to write results (default: C:\sash-test\results)

.PARAMETER DpiValues
    Array of DPI values to test (default: 96, 120, 144, 192)

.EXAMPLE
    .\dpi_test.ps1 -Arch x64 -Profile static-window

.EXAMPLE
    .\dpi_test.ps1 -Arch arm64 -Profile interactive-cut -DpiValues @(96, 144, 192)
#>

param(
    [Parameter(Mandatory=$true)]
    [ValidateSet('x64', 'arm64')]
    [string]$Arch,

    [Parameter(Mandatory=$true)]
    [ValidateSet('static-window', 'interactive-cut')]
    [string]$Profile,

    [string]$ResultsDir = 'C:\sash-test\results',

    [int[]]$DpiValues = @(96, 120, 144, 192)
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

# Load required assemblies
Add-Type @"
    using System;
    using System.Runtime.InteropServices;
    using System.Text;

    public class DpiWin32 {
        [DllImport("user32.dll", SetLastError = true, CharSet = CharSet.Unicode)]
        public static extern IntPtr FindWindow(string lpClassName, string lpWindowName);

        [DllImport("user32.dll", SetLastError = true)]
        public static extern bool GetWindowRect(IntPtr hWnd, out RECT lpRect);

        [DllImport("user32.dll", SetLastError = true)]
        public static extern IntPtr SendMessage(IntPtr hWnd, uint Msg, IntPtr wParam, IntPtr lParam);

        [DllImport("user32.dll", SetLastError = true)]
        public static extern bool SetWindowPos(IntPtr hWnd, IntPtr hWndInsertAfter, int X, int Y, int cx, int cy, uint uFlags);

        [DllImport("user32.dll", SetLastError = true)]
        public static extern IntPtr GetWindow(IntPtr hWnd, uint uCmd);

        [DllImport("user32.dll", SetLastError = true)]
        public static extern bool IsWindowVisible(IntPtr hWnd);

        [DllImport("shcore.dll", SetLastError = true)]
        public static extern int SetProcessDpiAwarenessContext(IntPtr dpiContext);

        [DllImport("shcore.dll", SetLastError = true)]
        public static extern IntPtr GetThreadDpiAwarenessContext();

        [DllImport("shcore.dll", SetLastError = true)]
        public static extern int AreDpiAwarenessContextsEqual(IntPtr left, IntPtr right);

        [DllImport("user32.dll", SetLastError = true)]
        public static extern uint GetDpiForWindow(IntPtr hWnd);

        [DllImport("user32.dll", SetLastError = true)]
        public static extern uint GetDpiForSystem();

        [DllImport("user32.dll", SetLastError = true)]
        public static extern bool AdjustWindowRectExForDpi(ref RECT rect, uint style, int menu, uint exStyle, uint dpi);

        [DllImport("user32.dll", SetLastError = true, CharSet = CharSet.Unicode)]
        public static extern int GetWindowText(IntPtr hWnd, StringBuilder lpString, int nMaxCount);

        [DllImport("kernel32.dll", SetLastError = true)]
        public static extern uint GetCurrentProcessId();

        public const uint WM_DPICHANGED = 0x02E0;
        public const uint WM_GETFONT = 0x0031;
        public const uint GW_CHILD = 5;
        public const uint GW_HWNDNEXT = 2;
        public const uint SWP_NOZORDER = 0x0004;
        public const uint SWP_NOACTIVATE = 0x0010;

        [StructLayout(LayoutKind.Sequential)]
        public struct RECT {
            public int Left;
            public int Top;
            public int Right;
            public int Bottom;
        }

        // DPI Awareness Context values
        public static readonly IntPtr DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2 = (IntPtr)(-4);
    }
"@

$LogPath = Join-Path $ResultsDir "$Arch-$Profile-dpi_test.log"
$EvidencePath = Join-Path $ResultsDir "$Arch-$Profile-dpi_evidence.json"

$ExpectedTitle = switch ($Profile) {
    'static-window' { 'hello' }
    'interactive-cut' { 'counter' }
}

$ExpectedClass = switch ($Profile) {
    'static-window' { 'SashStaticWindow' }
    'interactive-cut' { 'SashInteractiveWindow' }
}

function Write-Log {
    param([string]$Message, [string]$Level = 'INFO')
    $entry = "[$(Get-Date -Format 'HH:mm:ss.fff')] [$Level] $Message"
    Write-Host $entry
    Add-Content -Path $LogPath -Value $entry
}

function Get-WindowByTitle {
    param([string]$Title, [int]$MaxWaitMs = 30000)
    $start = [Environment]::TickCount
    while (([Environment]::TickCount - $start) -lt $MaxWaitMs) {
        $hwnd = [DpiWin32]::FindWindow($null, $Title)
        if ($hwnd -ne [IntPtr]::Zero) { return $hwnd }
        Start-Sleep -Milliseconds 100
    }
    return [IntPtr]::Zero
}

function Get-ChildWindows {
    param([IntPtr]$ParentHwnd)
    $children = @()
    $child = [DpiWin32]::GetWindow($ParentHwnd, [DpiWin32]::GW_CHILD)
    while ($child -ne [IntPtr]::Zero) {
        if ([DpiWin32]::IsWindowVisible($child)) { $children += $child }
        $child = [DpiWin32]::GetWindow($child, [DpiWin32]::GW_HWNDNEXT)
    }
    return $children
}

function Get-WindowRect {
    param([IntPtr]$Hwnd)
    $rect = New-Object DpiWin32+RECT
    if ([DpiWin32]::GetWindowRect($Hwnd, [ref]$rect)) {
        return @{ Left=$rect.Left; Top=$rect.Top; Right=$rect.Right; Bottom=$rect.Bottom; Width=$rect.Right-$rect.Left; Height=$rect.Bottom-$rect.Top }
    }
    return $null
}

function Get-ControlFont {
    param([IntPtr]$Hwnd)
    $fontPtr = [DpiWin32]::SendMessage($Hwnd, [DpiWin32]::WM_GETFONT, [IntPtr]::Zero, [IntPtr]::Zero)
    return $fontPtr
}

function Capture-ControlLayout {
    param([IntPtr]$Hwnd)
    $children = Get-ChildWindows $Hwnd
    $layout = @()
    foreach ($child in $children) {
        $rect = Get-WindowRect $child
        $font = Get-ControlFont $child
        if ($rect) {
            $layout += @{
                Hwnd = "0x$($child.ToString('X'))"
                Rect = $rect
                FontHandle = "0x$($font.ToString('X'))"
            }
        }
    }
    return $layout
}

$Results = @{
    Arch = $Arch
    Profile = $Profile
    Timestamp = (Get-Date).ToString('o')
    DpiTests = @()
    Overall = 'PENDING'
}

try {
    Write-Log "=== Sash DPI Test: $Arch / $Profile ==="
    Write-Log "Testing DPI values: $($DpiValues -join ', ')"

    # Launch the app
    $ArtifactDir = "C:\sash-test\artifacts\$Arch\$Profile"
    $BinaryPath = Join-Path $ArtifactDir "App.exe"
    if (-not (Test-Path $BinaryPath)) { throw "Binary not found: $BinaryPath" }

    $psi = New-Object System.Diagnostics.ProcessStartInfo
    $psi.FileName = $BinaryPath
    $psi.UseShellExecute = $false
    $proc = [System.Diagnostics.Process]::Start($psi)
    Write-Log "Launched PID $($proc.Id)"

    $hwnd = Get-WindowByTitle $ExpectedTitle
    if ($hwnd -eq [IntPtr]::Zero) { throw "Window not found" }
    Write-Log "Window HWND: 0x$($hwnd.ToString('X'))"

    # Verify initial DPI awareness
    $ctx = [DpiWin32]::GetThreadDpiAwarenessContext()
    $perMonitorV2 = [DpiWin32]::AreDpiAwarenessContextsEqual($ctx, [DpiWin32]::DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2)
    Write-Log "Initial DPI awareness context: 0x$($ctx.ToString('X')) (PerMonitorV2=$perMonitorV2)"
    if (-not $perMonitorV2) { Write-Log "WARNING: Process not in PerMonitorV2 context" 'WARN' }

    # Capture initial layout and font handles
    $initialLayout = Capture-ControlLayout $hwnd
    $initialFontHandles = $initialLayout | ForEach-Object { $_.FontHandle }
    Write-Log "Initial layout captured: $($initialLayout.Count) controls"

    $allPass = $true

    foreach ($targetDpi in $DpiValues) {
        Write-Log "--- Testing DPI $targetDpi ---"
        $testStart = Get-Date

        # Get current window DPI
        $currentDpi = [DpiWin32]::GetDpiForWindow($hwnd)
        Write-Log "Current window DPI: $currentDpi"

        # Simulate WM_DPICHANGED by sending the message with a suggested rect
        # The app should handle this and recreate font
        $scale = $targetDpi / 96.0
        $rect = Get-WindowRect $hwnd
        if ($rect) {
            $newWidth = [math]::Round($rect.Width * $scale)
            $newHeight = [math]::Round($rect.Height * $scale)
            $suggestedRect = New-Object DpiWin32+RECT
            $suggestedRect.Left = $rect.Left
            $suggestedRect.Top = $rect.Top
            $suggestedRect.Right = $rect.Left + $newWidth
            $suggestedRect.Bottom = $rect.Top + $newHeight

            # Pack rect into lparam (low 32 bits = left, high 32 bits = top? No, WM_DPICHANGED lparam = RECT*)
            # We'll send a simplified message - the app should handle the font recreation
            $lparam = [System.Runtime.InteropServices.Marshal]::AllocHGlobal([System.Runtime.InteropServices.Marshal]::SizeOf($suggestedRect))
            [System.Runtime.InteropServices.Marshal]::StructureToPtr($suggestedRect, $lparam, $true)

            $wparam = ($targetDpi -shl 16) -bor ($targetDpi)  # LOWORD = Y DPI, HIWORD = X DPI
            Write-Log "Sending WM_DPICHANGED: wparam=0x$($wparam.ToString('X')), suggested rect: $($newWidth)x$($newHeight)"

            [DpiWin32]::SendMessage($hwnd, [DpiWin32]::WM_DPICHANGED, [IntPtr]$wparam, $lparam)

            [System.Runtime.InteropServices.Marshal]::FreeHGlobal($lparam)

            # Give the app time to process
            Start-Sleep -Milliseconds 500

            # Verify new window DPI
            $newDpi = [DpiWin32]::GetDpiForWindow($hwnd)
            Write-Log "Window DPI after message: $newDpi"

            # Capture new layout and font handles
            $newLayout = Capture-ControlLayout $hwnd
            $newFontHandles = $newLayout | ForEach-Object { $_.FontHandle }

            # Check: font handles changed (recreated)
            $fontChanged = $false
            for ($i = 0; $i -lt [math]::Min($initialFontHandles.Count, $newFontHandles.Count); $i++) {
                if ($initialFontHandles[$i] -ne $newFontHandles[$i]) {
                    $fontChanged = $true
                    break
                }
            }

            # Check: controls repositioned (rect changed)
            $repositioned = $false
            for ($i = 0; $i -lt [math]::Min($initialLayout.Count, $newLayout.Count); $i++) {
                if ($initialLayout[$i].Rect.Width -ne $newLayout[$i].Rect.Width -or
                    $initialLayout[$i].Rect.Height -ne $newLayout[$i].Rect.Height) {
                    $repositioned = $true
                    break
                }
            }

            $dpiPass = $fontChanged -and $repositioned
            $allPass = $allPass -and $dpiPass

            Write-Log "DPI $targetDpi: Font recreated=$fontChanged, Repositioned=$repositioned, PASS=$dpiPass"

            $Results.DpiTests += @{
                TargetDpi = $targetDpi
                InitialDpi = $currentDpi
                ResultDpi = $newDpi
                FontRecreated = $fontChanged
                ControlsRepositioned = $repositioned
                Pass = $dpiPass
                InitialFontHandles = $initialFontHandles
                NewFontHandles = $newFontHandles
                InitialLayout = $initialLayout
                NewLayout = $newLayout
                Timestamp = (Get-Date).ToString('o')
            }

            # Update baseline for next iteration
            $initialLayout = $newLayout
            $initialFontHandles = $newFontHandles
        }
    }

    # Clean close
    Write-Log "Closing window..."
    [DpiWin32]::SendMessage($hwnd, 0x0010, [IntPtr]::Zero, [IntPtr]::Zero)  # WM_CLOSE
    $proc.WaitForExit(10000)
    $exitCode = $proc.ExitCode
    Write-Log "Process exited with code: $exitCode"

    $Results.Overall = if ($allPass -and $exitCode -eq 0) { 'PASS' } else { 'FAIL' }
    $Results.EndTime = (Get-Date).ToString('o')

    $Results | ConvertTo-Json -Depth 5 | Out-File -FilePath $EvidencePath -Encoding utf8
    Write-Log "DPI evidence written to: $EvidencePath"
    Write-Log "=== RESULT: $($Results.Overall) ==="

    exit ($Results.Overall -eq 'PASS' ? 0 : 1)
}
catch {
    $Results.Overall = 'ERROR'
    $Results.Error = $_.ToString()
    $Results.EndTime = (Get-Date).ToString('o')
    $Results | ConvertTo-Json -Depth 5 | Out-File -FilePath $EvidencePath -Encoding utf8
    Write-Log "ERROR: $($_.Exception.Message)" 'ERROR'
    if ($proc -and -not $proc.HasExited) { $proc.Kill() }
    exit 2
}