#!/usr/bin/env bash
set -e

echo "====================================================================="
echo "  UltraHDR Studio - Générateur d'APK Android (Rust + Material 3)"
echo "====================================================================="
echo ""

rustup target add aarch64-linux-android || true

if [ -z "$ANDROID_HOME" ]; then
    if [ -d "$HOME/Android/Sdk" ]; then
        export ANDROID_HOME="$HOME/Android/Sdk"
        echo "[*] Android SDK détecté dans : $ANDROID_HOME"
    fi
fi

if command -v cargo-apk &> /dev/null; then
    echo "[*] Compilation avec cargo-apk..."
    cd rust
    cargo apk build --release
    echo ""
    echo "[OK] APK généré : rust/target/release/apk/ultrahdr-studio.apk"
    exit 0
fi

echo "[INFO] Installez cargo-apk : cargo install cargo-apk"
echo "NOTE : GitHub Actions compile et publie l'APK automatiquement à chaque push/tag."
