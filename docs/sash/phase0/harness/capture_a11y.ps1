<#
.SYNOPSIS
    Sash Phase 0 Accessibility Capture
    Captures UI Automation tree for a running Sash window.

.DESCRIPTION
    Uses UI Automation (UIA) via .NET to capture the full accessibility tree
    of a Sash window, including AutomationId, Name, ControlType, BoundingRectangle,
    and IsKeyboardFocusable. Outputs structured JSON for evidence.

.PARAMETER Arch
    Architecture: x64, i686, arm64

.PARAMETER Profile
    Profile name: static-window, interactive-cut

.PARAMETER ResultsDir
    Directory to write results (default: C:\sash-test\results)

.PARAMETER WindowTitle
    Window title to find (default: derived from profile)

.EXAMPLE
    .\capture_a11y.ps1 -Arch x64 -Profile static-window

.EXAMPLE
    .\capture_a11y.ps1 -Arch arm64 -Profile interactive-cut
#>

param(
    [Parameter(Mandatory=$true)]
    [ValidateSet('x64', 'i686', 'arm64')]
    [string]$Arch,

    [Parameter(Mandatory=$true)]
    [ValidateSet('static-window', 'interactive-cut')]
    [string]$Profile,

    [string]$ResultsDir = 'C:\sash-test\results',

    [string]$WindowTitle = ''
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

# Determine window title from profile if not provided
if (-not $WindowTitle) {
    $WindowTitle = switch ($Profile) {
        'static-window' { 'hello' }
        'interactive-cut' { 'counter' }
        default { throw "Unknown profile: $Profile" }
    }
}

$LogPath = Join-Path $ResultsDir "$Arch-$Profile-a11y.log"
$OutputPath = Join-Path $ResultsDir "$Arch-$Profile-a11y_snapshot.json"

# Load UI Automation assemblies
Add-Type -AssemblyName 'UIAutomationClient', 'UIAutomationTypes', 'System.Windows.Forms'

function Write-Log {
    param([string]$Message, [string]$Level = 'INFO')
    $entry = "[$(Get-Date -Format 'HH:mm:ss.fff')] [$Level] $Message"
    Write-Host $entry
    Add-Content -Path $LogPath -Value $entry
}

function Find-WindowByTitle {
    param([string]$Title, [int]$MaxWaitMs = 30000)
    $start = [Environment]::TickCount
    while (([Environment]::TickCount - $start) -lt $MaxWaitMs) {
        $windows = [System.Windows.Automation.AutomationElement]::RootElement.FindAll(
            [System.Windows.Automation.TreeScope]::Children,
            [System.Windows.Automation.PropertyCondition]::new(
                [System.Windows.Automation.AutomationElement]::NameProperty, $Title
            )
        )
        if ($windows.Count -gt 0) {
            return $windows[0]
        }
        Start-Sleep -Milliseconds 200
    }
    return $null
}

function Capture-Element {
    param([System.Windows.Automation.AutomationElement]$Element, [int]$Depth = 0)

    $props = @{}

    # Core properties
    try { $props.AutomationId = $Element.Current.AutomationId } catch { $props.AutomationId = '' }
    try { $props.Name = $Element.Current.Name } catch { $props.Name = '' }
    try { $props.ControlType = $Element.Current.ControlType.ProgrammaticName } catch { $props.ControlType = '' }
    try { $props.LocalizedControlType = $Element.Current.LocalizedControlType } catch { $props.LocalizedControlType = '' }
    try { $props.ClassName = $Element.Current.ClassName } catch { $props.ClassName = '' }
    try { $props.FrameworkId = $Element.Current.FrameworkId } catch { $props.FrameworkId = '' }
    try { $props.IsKeyboardFocusable = $Element.Current.IsKeyboardFocusable } catch { $props.IsKeyboardFocusable = $false }
    try { $props.IsEnabled = $Element.Current.IsEnabled } catch { $props.IsEnabled = $false }
    try { $props.IsOffscreen = $Element.Current.IsOffscreen } catch { $props.IsOffscreen = $false }

    # Bounding rectangle
    try {
        $rect = $Element.Current.BoundingRectangle
        $props.BoundingRectangle = @{
            Left = [math]::Round($rect.Left)
            Top = [math]::Round($rect.Top)
            Width = [math]::Round($rect.Width)
            Height = [math]::Round($rect.Height)
            Right = [math]::Round($rect.Right)
            Bottom = [math]::Round($rect.Bottom)
        }
    } catch {
        $props.BoundingRectangle = $null
    }

    # Patterns supported
    try {
        $patterns = @()
        foreach ($pattern in $Element.GetSupportedPatterns()) {
            $patterns += $pattern.ProgrammaticName
        }
        $props.SupportedPatterns = $patterns
    } catch {
        $props.SupportedPatterns = @()
    }

    # Children
    $children = @()
    $childElements = $Element.FindAll(
        [System.Windows.Automation.TreeScope]::Children,
        [System.Windows.Automation.Condition]::TrueCondition
    )
    foreach ($child in $childElements) {
        $children += Capture-Element $child ($Depth + 1)
    }
    $props.Children = $children

    return $props
}

try {
    Write-Log "=== Sash Accessibility Capture: $Arch / $Profile ==="
    Write-Log "Target window: '$WindowTitle'"

    # Find the window
    Write-Log "Searching for window..."
    $window = Find-WindowByTitle $WindowTitle
    if (-not $window) {
        throw "Window '$WindowTitle' not found within timeout"
    }
    Write-Log "Found window: $($window.Current.Name) (AutomationId: $($window.Current.AutomationId))"

    # Capture full tree
    Write-Log "Capturing UIA tree..."
    $tree = Capture-Element $window

    # Add metadata
    $result = @{
        Arch = $Arch
        Profile = $Profile
        WindowTitle = $WindowTitle
        Timestamp = (Get-Date).ToString('o')
        Tree = $tree
    }

    # Write JSON
    $result | ConvertTo-Json -Depth 20 | Out-File -FilePath $OutputPath -Encoding utf8
    Write-Log "Accessibility snapshot written to: $OutputPath"

    # Quick validation checks
    $staticCount = 0
    $editCount = 0
    $buttonCount = 0
    $hasAutomationIds = $true

    function Count-Controls {
        param($node)
        if ($node.ControlType -like '*Text*' -or $node.ControlType -eq 'Text') { $script:staticCount++ }
        if ($node.ControlType -like '*Edit*') { $script:editCount++ }
        if ($node.ControlType -like '*Button*') { $script:buttonCount++ }
        if (-not $node.AutomationId) { $script:hasAutomationIds = $false }
        foreach ($child in $node.Children) { Count-Controls $child }
    }

    Count-Controls $tree

    Write-Log "Control counts: STATIC=$staticCount, EDIT=$editCount, BUTTON=$buttonCount"
    Write-Log "All elements have AutomationId: $hasAutomationIds"

    # Expected counts per profile
    $expected = switch ($Profile) {
        'static-window' { @{ Static = 1; Edit = 0; Button = 0 } }
        'interactive-cut' { @{ Static = 2; Edit = 1; Button = 1 } }
    }

    $countsOk = ($staticCount -eq $expected.Static) -and ($editCount -eq $expected.Edit) -and ($buttonCount -eq $expected.Button)
    Write-Log "Control counts match expected: $countsOk"

    exit 0
}
catch {
    Write-Log "ERROR: $($_.Exception.Message)" 'ERROR'
    @{
        Arch = $Arch
        Profile = $Profile
        Timestamp = (Get-Date).ToString('o')
        Error = $_.ToString()
    } | ConvertTo-Json -Depth 5 | Out-File -FilePath $OutputPath -Encoding utf8
    exit 1
}