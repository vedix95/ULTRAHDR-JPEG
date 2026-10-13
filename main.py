"""main.py — Lanceur Desktop pour UltraHDR Studio.

Démarre le serveur FastAPI et ouvre automatiquement l'interface
dans une fenêtre d'application native Edge ou Chrome.
Le serveur reste actif tant que la console est ouverte (ou arrêt par Ctrl+C).
"""
from __future__ import annotations

import os
import sys
import time
import shutil
import socket
import tempfile
import threading
import subprocess
import webbrowser
from pathlib import Path

# Encodage UTF-8 sécurisé pour console Windows
if hasattr(sys.stdout, "reconfigure"):
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
if hasattr(sys.stderr, "reconfigure"):
    sys.stderr.reconfigure(encoding="utf-8", errors="replace")

import uvicorn


def find_free_port(default_port: int = 8765) -> int:
    """Trouve un port réseau disponible."""
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
        try:
            s.bind(("127.0.0.1", default_port))
            return default_port
        except OSError:
            s.bind(("127.0.0.1", 0))
            return s.getsockname()[1]


def find_browser_executable() -> tuple[str | None, str]:
    """Recherche l'exécutable Microsoft Edge ou Google Chrome."""
    # 1. Microsoft Edge
    edge_paths = [
        os.path.expandvars(r"%ProgramFiles(x86)%\Microsoft\Edge\Application\msedge.exe"),
        os.path.expandvars(r"%ProgramFiles%\Microsoft\Edge\Application\msedge.exe"),
        os.path.expandvars(r"%LocalAppData%\Microsoft\Edge\Application\msedge.exe"),
        shutil.which("msedge"),
    ]
    for p in edge_paths:
        if p and os.path.isfile(p):
            return p, "msedge"

    # 2. Google Chrome
    chrome_paths = [
        os.path.expandvars(r"%ProgramFiles%\Google\Chrome\Application\chrome.exe"),
        os.path.expandvars(r"%ProgramFiles(x86)%\Google\Chrome\Application\chrome.exe"),
        os.path.expandvars(r"%LocalAppData%\Google\Chrome\Application\chrome.exe"),
        shutil.which("chrome"),
    ]
    for p in chrome_paths:
        if p and os.path.isfile(p):
            return p, "chrome"

    return None, "default"


def open_browser_window(app_url: str):
    """Ouvre l'application dans une fenêtre dédiée après démarrage du serveur."""
    # Pause pour s'assurer que le serveur écoute
    time.sleep(0.7)

    browser_bin, browser_name = find_browser_executable()
    if browser_bin:
        # Profil isolé dans le dossier temporaire pour éviter la fermeture prématurée
        user_data = Path(tempfile.gettempdir()) / "ultrahdr_browser_profile"
        user_data.mkdir(exist_ok=True)

        cmd = [
            browser_bin,
            f"--app={app_url}",
            f"--user-data-dir={user_data}",
            "--no-first-run",
            "--no-default-browser-check",
            "--enable-features=HDR,HighDynamicRangeHeadroom",
            "--force-color-profile=scRGB-linear",
            "--window-size=1440,920"
        ]
        try:
            print(f"[*] Ouverture de la fenêtre d'application native ({browser_name})...")
            subprocess.Popen(cmd)
            return
        except Exception as e:
            print(f"[!] Échec du lancement direct ({e}), bascule sur navigateur standard.")

    print("[*] Ouverture dans votre navigateur par défaut...")
    webbrowser.open(app_url)


def main():
    print("=" * 62)
    print("        UltraHDR Studio — Éditeur Photo vers JPEG Ultra HDR")
    print("=" * 62)

    port = find_free_port(8765)
    app_url = f"http://127.0.0.1:{port}"

    print(f"[*] Serveur actif à l'adresse : {app_url}")
    print("[*] Gardez cette fenêtre ouverte pendant que vous utilisez l'application.")
    print("[*] Appuyez sur Ctrl+C dans cette fenêtre pour quitter.\n")

    # Lancer l'ouverture de la fenêtre dans un thread détaché
    browser_thread = threading.Thread(target=open_browser_window, args=(app_url,), daemon=True)
    browser_thread.start()

    # Exécution du serveur sur le thread principal pour qu'il ne se ferme pas
    from backend.server import app
    try:
        uvicorn.run(app, host="127.0.0.1", port=port, log_level="warning")
    except KeyboardInterrupt:
        print("\n[*] Arrêt d'UltraHDR Studio. À bientôt !")
        sys.exit(0)


if __name__ == "__main__":
    main()
