#!/usr/bin/env pwsh
#Requires -Version 5.1

<#
.SYNOPSIS
    Braid-Iroh Unified Run Script with Real-Time Debug Output

.DESCRIPTION
    Runs Braid-Iroh with colored debug output showing P2P connections,
    gossip events, and synchronization in real-time.

.PARAMETER Mode
    Running mode: dev (default), test, example, or leptos

.EXAMPLE
    .\run.ps1              # Run application
    .\run.ps1 test         # Run tests
    .\run.ps1 example      # Run two-peers example
    .\run.ps1 leptos       # Run with cargo-leptos
#>

[CmdletBinding()]
param(
    [ValidateSet("dev", "test", "example", "leptos")]
    [string]$Mode = "dev"
)

# Environment setup
$env:RUST_LOG = "braid_iroh=debug,iroh_gossip=debug,iroh=info"
$env:RUST_BACKTRACE = "1"

# Color definitions
$esc = [char]27
$colors = @{
    Reset = "$esc[0m"
    Bold = "$esc[1m"
    Cyan = "$esc[36m"
    Green = "$esc[32m"
    Yellow = "$esc[33m"
    Magenta = "$esc[35m"
    Red = "$esc[31m"
    Blue = "$esc[34m"
    Gray = "$esc[90m"
    White = "$esc[97m"
}

# Clear screen and show banner
Clear-Host
Write-Host "╔══════════════════════════════════════════════════════════════════════════════╗" -ForegroundColor Cyan
Write-Host "║                                                                              ║" -ForegroundColor Cyan
Write-Host "║   BRAID-IROH: P2P Synchronization Demo with Real-Time Debug                  ║" -ForegroundColor Cyan
Write-Host "║                                                                              ║" -ForegroundColor Cyan
Write-Host "╚══════════════════════════════════════════════════════════════════════════════╝" -ForegroundColor Cyan
Write-Host ""

# Legend
Write-Host "Debug Output Legend:" -ForegroundColor White
Write-Host "  [🔵] Connection  " -ForegroundColor Cyan -NoNewline; Write-Host "- QUIC handshakes, peer connections, endpoint binding"
Write-Host "  [🟣] Gossip      " -ForegroundColor Magenta -NoNewline; Write-Host "- Topic subscriptions, broadcasts, membership events"
Write-Host "  [🟢] PUT/Update  " -ForegroundColor Green -NoNewline; Write-Host "- Saving data, versioning, broadcasting to peers"
Write-Host "  [🟡] GET/Retrieve" -ForegroundColor Yellow -NoNewline; Write-Host "- HTTP/3 requests, fetching resource state"
Write-Host "  [🔴] Error       " -ForegroundColor Red -NoNewline; Write-Host "- Connection failures, protocol errors"
Write-Host "  [⚪] Info        " -ForegroundColor Gray -NoNewline; Write-Host "- General information"
Write-Host ""
Write-Host "══════════════════════════════════════════════════════════════════════════════" -ForegroundColor Gray
Write-Host ""

# Determine command
switch ($Mode) {
    "test" { 
        $command = "cargo test -- --nocapture"
        Write-Host "[Mode] Running tests with debug output..." -ForegroundColor Yellow
    }
    "example" { 
        $command = "cargo run --example two_peers_debug"
        Write-Host "[Mode] Running two-peers debug example..." -ForegroundColor Yellow
    }
    "leptos" { 
        $command = "cargo leptos watch"
        Write-Host "[Mode] Running with cargo-leptos (SSR demo)..." -ForegroundColor Yellow
    }
    default { 
        $command = "cargo run"
        Write-Host "[Mode] Running application (cargo run)..." -ForegroundColor Yellow
    }
}

Write-Host "[Env]  RUST_LOG=$($env:RUST_LOG)" -ForegroundColor DarkGray
Write-Host "[Env]  RUST_BACKTRACE=$($env:RUST_BACKTRACE)" -ForegroundColor DarkGray
Write-Host ""
Write-Host "Starting... Press Ctrl+C to stop" -ForegroundColor Green
Write-Host "──────────────────────────────────────────────────────────────────────────────" -ForegroundColor Gray
Write-Host ""

# Colorize function
function Colorize-Output($line) {
    # Connection events
    if ($line -match 'endpoint.*bound|Endpoint|node_id|NodeId|peer.*connected|Connecting|Connection|QUIC|handshake|bound') {
        return $colors.Cyan + "🔵 " + $line + $colors.Reset
    }
    # Gossip events
    elseif ($line -match 'gossip|Gossip|topic|subscribe|broadcast|join|membership|overlay|TopicId') {
        return $colors.Magenta + "🟣 " + $line + $colors.Reset
    }
    # PUT/Update events
    elseif ($line -match 'PUT|put|update|Update|version|Version|snapshot|store|saving|broadcast') {
        return $colors.Green + "🟢 " + $line + $colors.Reset
    }
    # GET/Retrieve events
    elseif ($line -match 'GET|get|retrieve|fetch|request|response|http|h3') {
        return $colors.Yellow + "🟡 " + $line + $colors.Reset
    }
    # Error events
    elseif ($line -match 'ERROR|Error|error|FAIL|fail|panic|FATAL|critical') {
        return $colors.Red + "🔴 " + $line + $colors.Reset
    }
    # Warning events
    elseif ($line -match 'WARN|Warn|warn|warning|Warning') {
        return $colors.Yellow + "⚠️  " + $line + $colors.Reset
    }
    # Info with keywords
    elseif ($line -match 'protocol|router|spawn|shutdown|ready|listening|accepting') {
        return $colors.Blue + "⚪ " + $line + $colors.Reset
    }
    # Default
    else {
        return $colors.Gray + "   " + $line + $colors.Reset
    }
}

# Run the command with colorized output
try {
    $psi = New-Object System.Diagnostics.ProcessStartInfo
    $psi.FileName = "cargo"
    $psi.Arguments = $command -replace "^cargo\s*", ""
    $psi.EnvironmentVariables["RUST_LOG"] = $env:RUST_LOG
    $psi.EnvironmentVariables["RUST_BACKTRACE"] = $env:RUST_BACKTRACE
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    $psi.UseShellExecute = $false
    $psi.CreateNoWindow = $true
    
    $process = [System.Diagnostics.Process]::Start($psi)
    
    # Read and colorize output
    $process.StandardOutput.ReadLineAsync() | ForEach-Object {
        while ($null -ne ($line = $process.StandardOutput.ReadLine())) {
            Write-Output (Colorize-Output $line)
        }
    }
    
    # Also read stderr
    while ($null -ne ($line = $process.StandardError.ReadLine())) {
        Write-Output (Colorize-Output $line)
    }
    
    $process.WaitForExit()
}
finally {
    Write-Host ""
    Write-Host "──────────────────────────────────────────────────────────────────────────────" -ForegroundColor Gray
    Write-Host "Application stopped" -ForegroundColor Yellow
    $host.UI.RawUI.WindowTitle = "Braid-Iroh [Stopped]"
}
