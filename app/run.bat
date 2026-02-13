@echo off
echo ========================================
echo   link_iroh Launcher
echo   Braid-HTTP over Iroh P2P
echo ========================================
echo.

echo [*] Starting Tauri development engine...
echo.

set RUST_LOG=info,braid_iroh=info,iroh_gossip=info
cargo tauri dev
