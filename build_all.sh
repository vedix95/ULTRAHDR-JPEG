#!/usr/bin/env bash
set -e

echo "============================================================"
echo "  UltraHDR Studio — Compilation Multi-Plateforme Rust"
echo "  Cibles : Windows (x86_64) et Linux (x86_64-musl)"
echo "============================================================"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR/rust"

echo "[*] Compilation de la version Linux..."
cargo build --release --target x86_64-unknown-linux-musl

echo "[*] Compilation de la version Windows..."
cargo build --release

mkdir -p "$SCRIPT_DIR/dist/windows" "$SCRIPT_DIR/dist/linux"
cp target/release/ultrahdr.exe "$SCRIPT_DIR/dist/windows/ultrahdr.exe"
cp target/x86_64-unknown-linux-musl/release/ultrahdr "$SCRIPT_DIR/dist/linux/ultrahdr"
chmod +x "$SCRIPT_DIR/dist/linux/ultrahdr"

echo "============================================================"
echo "[✓] Terminé avec succès :"
echo "  - Linux   : dist/linux/ultrahdr"
echo "  - Windows : dist/windows/ultrahdr.exe"
echo "============================================================"
