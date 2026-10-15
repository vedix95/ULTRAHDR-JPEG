@echo off
setlocal enabledelayedexpansion

echo =====================================================================
echo   UltraHDR Studio - Generateur d'APK Android (Rust + Material 3)
echo =====================================================================
echo.

REM 1. Configuration automatique de la toolchain portable si presente
if exist "C:\APPLIS\Java\jdk-17" (
    set "JAVA_HOME=C:\APPLIS\Java\jdk-17"
    set "PATH=C:\APPLIS\Java\jdk-17\bin;!PATH!"
)
if exist "C:\APPLIS\Android\sdk" (
    set "ANDROID_HOME=C:\APPLIS\Android\sdk"
    set "ANDROID_NDK_HOME=C:\APPLIS\Android\sdk\ndk\26.3.11579264"
    set "NDK_HOME=C:\APPLIS\Android\sdk\ndk\26.3.11579264"
    set "PATH=!NDK_HOME!\toolchains\llvm\prebuilt\windows-x86_64\bin;!PATH!"
)

REM 2. Compilation de la librairie native libultrahdr.so (ARM64)
echo [*] Compilation de libultrahdr.so pour Android aarch64...
cd rust
cargo build --lib --target aarch64-linux-android --release
if %ERRORLEVEL% NEQ 0 (
    echo [ERREUR] Echec de la compilation Cargo pour aarch64-linux-android.
    cd ..
    exit /b 1
)
cd ..

REM 3. Copie de libultrahdr.so vers android/app/src/main/jniLibs/arm64-v8a
if not exist "android\app\src\main\jniLibs\arm64-v8a" (
    mkdir "android\app\src\main\jniLibs\arm64-v8a"
)
copy /Y "rust\target\aarch64-linux-android\release\libultrahdr.so" "android\app\src\main\jniLibs\arm64-v8a\libultrahdr.so" >nul

REM 4. Assemblage Gradle
echo [*] Assemblage de l'APK Release via Gradle...
cd android
if exist "C:\APPLIS\Android\gradle-8.5\bin\gradle.bat" (
    call "C:\APPLIS\Android\gradle-8.5\bin\gradle.bat" assembleRelease --no-daemon
) else (
    call gradle assembleRelease --no-daemon
)
if %ERRORLEVEL% NEQ 0 (
    echo [ERREUR] Echec de l'assemblage Gradle.
    cd ..
    exit /b 1
)
cd ..

REM 5. Copie finale de l'APK vers dist\android et la racine
if not exist "dist\android" mkdir "dist\android"
if exist "android\app\build\outputs\apk\release\app-release.apk" (
    copy /Y "android\app\build\outputs\apk\release\app-release.apk" "dist\android\ultrahdr-studio.apk" >nul
    copy /Y "android\app\build\outputs\apk\release\app-release.apk" "dist\android\ultrahdr.apk" >nul
    copy /Y "android\app\build\outputs\apk\release\app-release.apk" "ultrahdr-studio.apk" >nul
    echo.
    echo =====================================================================
    echo   [SUCCES] APK Android genere avec succes :
    echo   - dist\android\ultrahdr-studio.apk
    echo   - dist\android\ultrahdr.apk
    echo   - ultrahdr-studio.apk
    echo =====================================================================
)

endlocal
