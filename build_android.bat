@echo off
setlocal enabledelayedexpansion

echo =====================================================================
echo   UltraHDR Studio - Generateur d'APK Android (Rust + Material 3)
echo =====================================================================
echo.

REM 1. Verification de la toolchain Rust
rustup target add aarch64-linux-android >nul 2>&1
if %ERRORLEVEL% NEQ 0 (
    echo [!] Impossible d'ajouter la cible aarch64-linux-android via rustup.
)

REM 2. Verification d'Android SDK / NDK
if "%ANDROID_HOME%"=="" (
    if exist "%LOCALAPPDATA%\Android\Sdk" (
        set "ANDROID_HOME=%LOCALAPPDATA%\Android\Sdk"
        echo [*] Android SDK detecte dans : !ANDROID_HOME!
    ) else (
        echo [!] AVERTISSEMENT : Variable ANDROID_HOME non definie.
        echo     Veuillez definir ANDROID_HOME vers votre SDK Android.
    )
)

REM 3. Construction via cargo-apk (Recommande)
where cargo-apk >nul 2>&1
if %ERRORLEVEL% EQU 0 (
    echo [*] Lancement du build via cargo-apk...
    cd rust
    cargo apk build --release
    if %ERRORLEVEL% EQU 0 (
        echo.
        echo [OK] APK genere avec succes dans :
        echo      rust\target\release\apk\ultrahdr-studio.apk
        goto :success
    )
    cd ..
)

REM 4. Construction via Gradle + NDK si installe
if exist "android\gradlew.bat" (
    echo [*] Lancement de l'assemblage Gradle...
    cd android
    call gradlew.bat assembleRelease
    cd ..
    goto :success
)

echo.
echo =====================================================================
echo [INFO] Pour compiler localement l'APK :
echo   1. Installez cargo-apk : cargo install cargo-apk
echo   2. Installez Android NDK et SDK
echo   3. Relancez ce script !
echo.
echo NOTE : L'APK est egalement compile et publie automatiquement
echo        par GitHub Actions sur chaque commit et release !
echo =====================================================================

:success
endlocal
