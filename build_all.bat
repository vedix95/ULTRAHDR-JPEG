@echo off
setlocal
echo ============================================================
echo   UltraHDR Studio — Compilation Multi-Plateforme Rust
echo   Interface Native en Dur (eframe / wgpu) + Mode CLI
echo   Cibles : Windows (x86_64) et Linux (x86_64-musl Wayland)
echo ============================================================

cd /d "%~dp0\rust"

echo [*] Compilation de la version Windows (.exe)...
cargo build --release
if %ERRORLEVEL% neq 0 (
    echo [!] Erreur lors de la compilation Windows.
    exit /b %ERRORLEVEL%
)

echo [*] Compilation de la version Linux (ELF statique Wayland)...
cargo +stable-x86_64-pc-windows-gnu build --release --target x86_64-unknown-linux-musl
if %ERRORLEVEL% neq 0 (
    echo [!] Erreur lors de la compilation Linux.
    exit /b %ERRORLEVEL%
)

echo [*] Copie des binaires vers ..\dist...
if not exist "..\dist\windows" mkdir "..\dist\windows"
if not exist "..\dist\linux" mkdir "..\dist\linux"

copy /y "target\release\ultrahdr.exe" "..\dist\windows\ultrahdr.exe" >nul
copy /y "target\x86_64-unknown-linux-musl\release\ultrahdr" "..\dist\linux\ultrahdr" >nul

cd /d "%~dp0"
if exist "build_android.bat" (
    echo.
    echo [*] Compilation de la version Android (APK)...
    call "build_android.bat"
)

echo ============================================================
echo [V] Compilation terminee avec succes !
echo   - Windows : dist\windows\ultrahdr.exe
echo   - Linux   : dist\linux\ultrahdr
echo   - Android : dist\android\ultrahdr.apk
echo ============================================================
if "%~1"=="" pause
