@echo off
setlocal

:: ========================================
:: link_iroh - Build Script
:: ========================================

set "APP_DIR=%~dp0"
set "RUST_LOG=info"

echo ========================================
echo   link_iroh - Build
echo ========================================
echo.

cd /d "%APP_DIR%"

echo Building release binary...
echo.

cargo build --release

if errorlevel 1 (
    echo.
    echo ERROR: Build failed.
    pause
    exit /b 1
)

echo.
echo ========================================
echo   Build Complete
echo ========================================
echo.
echo Binary location:
echo   target\release\braid_iroh.exe
echo.

endlocal
pause
