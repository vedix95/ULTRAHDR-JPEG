#!/usr/bin/env bash
set -e

echo "============================================================"
echo "  UltraHDR Studio — Compilation Multi-Plateforme Rust"
echo "  Cibles : Linux (x86_64) et Windows (via CI GitHub)"
echo "============================================================"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR/rust"

echo "[*] Exécution des tests Rust..."
cargo test

echo "[*] Compilation de la version Linux Release..."
cargo build --release

mkdir -p "$SCRIPT_DIR/dist/linux"
cp target/release/ultrahdr "$SCRIPT_DIR/dist/linux/ultrahdr"
chmod +x "$SCRIPT_DIR/dist/linux/ultrahdr"

echo "============================================================"
echo "[✓] Terminé avec succès :"
echo "  - Linux   : dist/linux/ultrahdr"
echo "============================================================"

