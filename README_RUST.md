# UltraHDR Studio en Rust (Interface Native Desktop & Multi-Plateforme)

Version native en **Rust pur** d'UltraHDR Studio avec **interface graphique de bureau 100% native (sans HTML, sans navigateur)** et support **HDR matériel natif** sous **Windows** et **Linux (Wayland)**.

Conforme aux spécifications **Google Ultra HDR** et **ISO 21496-1**.

---

## ⚡ Caractéristiques et Avantages de la Version Rust

- **Interface Graphique "En Dur" (Zéro HTML / Zéro Navigateur)** :
  - Développée avec `eframe` et `egui`.
  - Rendu matériel direct GPU via `wgpu`.
  - Pas de serveur web local, pas de Chromium/Edge, pas de HTML/CSS/JS : une véritable application de bureau native ultra-réactive.
- **Support HDR Matériel Natif (Windows & Linux Wayland)** :
  - **Windows** : Pipeline de rendu linéaire `scRGB` via Direct3D 12 / DXGI (`Rgba16Float`). Les hautes lumières dépassent 1.0 et exploitent directement la réserve de luminance physique (headroom) de votre écran HDR (jusqu'à 1000–2000 nits).
  - **Linux (Wayland)** : Détection et présentation native via le protocole Color Management / Vulkan (`VK_COLOR_SPACE_EXTENDED_SRGB_LINEAR_EXT`).
  - **Écrans SDR** : Simulation tone-mapping automatique (ACES Filmic / Reinhard / EV Shift) pour visualiser la dynamique sans aucun cramage.
- **Visionneuse Interactive Complète** :
  - **Mode Split (Avant / Après)** : Ligne de séparation interactive à glisser en temps réel.
  - **Mode Côte à côte** : Comparaison simultanée SDR original vs Ultra HDR boosté.
  - **Mode Image HDR** : Vue pleine résolution en dynamique étendue.
  - **Mode Gain Map** : Visualisation de la carte de gain en niveaux de gris.
  - **Mode Heatmap** : Carte thermique Turbo en fausses couleurs des nits.
  - **Zoom & Pan** : Zoom fluide à la molette (20% à 800%) et déplacement au clic droit.
- **Traitement par Glisser-Déposer (Drag & Drop)** :
  - Glissez n'importe quelle photo (JPEG, PNG, WebP, Radiance HDR, TIFF 16-bit, BMP) directement dans la fenêtre.
- **Double Mode : GUI Desktop et CLI Haute Performance** :
  - Double-cliquez pour ouvrir l'interface de bureau.
  - Ou utilisez la ligne de commande pour la conversion unitaire ou par lot de dossiers complets multithreadée avec **Rayon**.

---

## 📁 Emplacement des Binaires Précompilés

Les deux compilations prêtes à l'emploi sont situées dans le dossier `dist/` :

| Plateforme | Fichier | Type de Binaire | Taille |
| :--- | :--- | :--- | :--- |
| **Windows** | `dist/windows/ultrahdr.exe` | Exécutable Windows x86_64 PE autonome (Direct3D 12 / scRGB) | ~8 Mo |
| **Linux** | `dist/linux/ultrahdr` | Exécutable Linux x86_64 ELF statique `musl` (Wayland / Vulkan) | ~10 Mo |

> Le binaire Linux est lié statiquement avec `x86_64-unknown-linux-musl` : il s'exécute sur n'importe quelle distribution Linux moderne (Ubuntu, Debian, Fedora, Arch, Alpine, etc.) sous session Wayland ou X11.

---

## 🚀 Utilisation

### 1. Lancement de l'Application Desktop Native
- **Sous Windows** : Double-cliquez sur `dist\windows\ultrahdr.exe` (ou exécutez `run_rust.bat`).
- **Sous Linux** : Lancez simplement dans un terminal ou depuis votre gestionnaire de fichiers :
  ```bash
  ./dist/linux/ultrahdr
  ```

---

### 2. Mode Ligne de Commande (CLI)

#### Conversion d'une image :
```bash
# Windows
dist\windows\ultrahdr.exe photo.jpg -o photo_UltraHDR.jpg

# Linux
./dist/linux/ultrahdr photo.jpg -o photo_UltraHDR.jpg
```

#### Traitement par lot d'un dossier complet :
```bash
dist\windows\ultrahdr.exe C:\DossierPhotos\
```

#### Avec un profil prédéfini :
```bash
dist\windows\ultrahdr.exe coucher_soleil.jpg --preset sunset
```
*Profils disponibles : `sunset`, `night_neon`, `landscape`, `studio`, `native_hdr`, `moderate`.*

#### Réglages personnalisés :
```bash
dist\windows\ultrahdr.exe image.png --peak-nits 1400 --shadow-boost 1.20 --gamma 1.05 --save-gainmap
```

#### Analyse de potentiel seule :
```bash
dist\windows\ultrahdr.exe photo.jpg --analyze
```

---

## 🛠️ Recompilation

### Sous Windows :
Double-cliquez sur `build_all.bat` ou lancez :
```cmd
build_all.bat
```

### Sous Linux :
```bash
chmod +x build_all.sh
./build_all.sh
```
