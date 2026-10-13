# ✨ UltraHDR Studio — Moteur & Studio Photo Ultra HDR en Rust Pur

[![CI / CD Build & Multiplatform Release](https://github.com/vedix95/ULTRAHDR-JPEG/actions/workflows/build.yml/badge.svg)](https://github.com/vedix95/ULTRAHDR-JPEG/actions/workflows/build.yml)
[![GitHub Release](https://img.shields.io/github/v/release/vedix95/ULTRAHDR-JPEG?color=blue&label=Release%20v1.0.0)](https://github.com/vedix95/ULTRAHDR-JPEG/releases/tag/v1.0.0)
[![Rust](https://img.shields.io/badge/Rust-1.75+-orange.svg)](https://www.rust-lang.org/)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)
[![Platform](https://img.shields.io/badge/Platform-Linux%20%7C%20Windows-blue.svg)](https://github.com/vedix95/ULTRAHDR-JPEG/releases/tag/v1.0.0)

> **Éditeur et générateur photographique Ultra HDR professionnel en Rust pur**, 100% conforme aux standards internationaux **ISO 21496-1** et **Google Ultra HDR (v1.0)**.
> Développé avec une **architecture 100% native** (zéro dépendance Python, zéro Electron, zéro WebView, rendu direct matériel GPU via `wgpu` / DirectX 12 / Vulkan / Wayland) et un moteur **CLI multithreadé ultra-rapide (Rayon)**.

---

## 📸 Qu'est-ce que le format Ultra HDR (ISO 21496-1) ?

Le format **JPEG Ultra HDR** réconcilie la compatibilité universelle et l'imagerie moderne à haute dynamique :
1. **Compatibilité universelle SDR** : Tout visualiseur classique, navigateur web ou écran SDR affiche l'image JPEG de base sans aucune altération.
2. **Gain Map ISO 21496-1 intégrée** : Une carte de gain secondaire encodée en niveaux de gris est encapsulée dans le fichier via les métadonnées Adobe XMP (`hdrgm`) et le format multi-images CIPA DC-007 MPF APP2.
3. **Rendu HDR Éclatant** : Sur les appareils compatibles (écrans OLED/Mini-LED HDR, smartphones Android récents, Google Chrome, iOS 18+, macOS, Windows 11 HDR), la carte de gain est appliquée dynamiquement pour restituer jusqu'à **10 000 nits** d'éclat lumineux dans les reflets et les crêtes spéculaires.

```mermaid
flowchart LR
    A["Image SDR ou RAW (JPEG, PNG, DNG, EXR)"] --> B["Moteur Rust Multithreadé"]
    B --> C["JPEG SDR de base (Qualité 94)"]
    B --> D["Gain Map Log2 (ISO 21496-1)"]
    C & D --> E["Assemblage Binaire UltraHDR"]
    E --> F["Fichier final JPEG Ultra HDR"]
    F --> G["Écrans SDR : Affichage JPEG normal 100% fidèle"]
    F --> H["Écrans HDR : Éclat spéculaire jusqu'à 10 000 nits"]
```

---

## ⚡ Fonctionnalités Clés

### 🎨 Tonalité Pro Lightroom-grade (-200% à +200%)
- **Contraste Paramétrique S-Curve** : modulation sigmoïde de la pente dynamique avec pivot perceptuel (0.45), donnant du relief et de la profondeur sans brûler les blancs ni boucher les ombres.
- **Réglages par Zones Tonalité Pro** :
  - **Blancs (Whites)** : module directement l'intensité du pic de boost HDR sur les éclats spéculaires (jusqu'à x2.7 de boost).
  - **Hautes Lumières (Highlights)** : abaisse ou relève le seuil de transition vers les hautes luminances.
  - **Ombres (Shadows)** : rehausse ou densifie les pénombres.
  - **Noirs (Blacks)** : ancre la densité du point noir absolu pour exploiter les noirs profonds OLED.
- **Vibrance & Saturation HDR** :
  - **Saturation HDR** : ajuste uniformément l'intensité chromatique globale.
  - **Vibrance HDR** : protège les tons chair et les couleurs déjà très vives tout en rehaussant les teintes douces, empêchant le délavage blanc des crêtes lumineuses HDR.
- **Filtre Anti-Halo Bilatéral** : supprime les artefacts de liseré et de halo autour des silhouettes à fort contraste.

### 🖥️ Support Matériel HDR & Linux Wayland Natif
- **Détection Automatique Matérielle EDID (Linux & Windows)** :
  - Analyse directe des blocs EDID CTA-861 dans `/sys/class/drm` sous Linux (détecte la dalle OLED, la luminance maximale de crête en nits, les noirs absolus et l'espace BT.2020 10-bit).
  - Badge interactif dans l'en-tête indiquant en temps réel l'état matériel de l'écran.
- **Forçage Wayland Natif** :
  - Active directement le protocole Wayland (`WINIT_UNIX_BACKEND=wayland`) sans passer par XWayland pour garantir la fidélité colorimétrique 10-bit.
- **Moteur d'Aperçu « Éclat Ultra HDR » (Vivid Pop)** :
  - Préserve à 100% la luminosité SDR des tons moyens et sombres (aucun assombrissement terne).
  - Éclat dynamique progressif sur les hautes lumières pour visualiser instantanément l'effet HDR dès que vous manipulez les curseurs.

### 📈 Courbe Tonale Interactive (*Tone Curve*)
- **Tracé Spline en Temps Réel** : visualisation dynamique de la courbe de réponse tonale superposée à l'histogramme Lightroom à 4 zones.
- **Ligne de Référence 45°** : repère visuel immédiat montrant si une zone est rehaussée ou atténuée.
- **Points d'Ancrage Interactifs** : suivi visuel direct des zones *Noirs*, *Ombres*, *Hautes Lumières* et *Blancs*.

### 🎯 Outil Ciblé Direct (*Targeted Adjustment Tool - TAT*)
- Cliquez sur l'icône **🎯 Outil Ciblé** (ou touche <kbd>T</kbd>) : survolez directement n'importe quelle zone de la photo et glissez verticalement avec la souris pour ajuster immédiatement la zone tonale correspondante.

### 👁️ Comparaison Instantanée Ultra-Rapide
- **Hold-to-Compare** : maintenez enfoncée la touche <kbd>Espace</kbd>, <kbd>\</kbd> ou <kbd>/</kbd> pour basculer instantanément sur l'image originale SDR brute à 60/144 FPS sans latence. Relâchez pour revenir à la vue Ultra HDR traitée.

### 📁 Traitement par Lot Multithreadé (*Batch Conversion*)
- Convertissez des répertoires entiers d'images en JPEG Ultra HDR en quelques secondes grâce à la parallélisation multithread avec **Rayon**.
- Conserve automatiquement les métadonnées **EXIF** d'origine de vos photos.

---

## ⌨️ Raccourcis Clavier Essentiels

| Raccourci | Action |
| :--- | :--- |
| **<kbd>Ctrl</kbd> + <kbd>O</kbd>** | Ouvrir une image |
| **<kbd>Ctrl</kbd> + <kbd>S</kbd>** | Exporter l'image Ultra HDR finale |
| **<kbd>Espace</kbd>** / **<kbd>\</kbd>** *(maintenir)* | **Afficher instantanément l'image originale SDR** (Hold to Compare) |
| **<kbd>\</kbd>** / **<kbd>/</kbd>** *(appui simple)* | Basculer la vue comparaison Original SDR / Ultra HDR |
| **<kbd>M</kbd>** | Changer de mode d'affichage (`Split` → `SideBySide` → `FullHdr` → `GainMap` → `Heatmap`) |
| **<kbd>T</kbd>** | Activer / désactiver l'**Outil Ciblé Direct (TAT)** sur l'image |
| **<kbd>Tab</kbd>** | Afficher / masquer le panneau latéral d'outils |
| **<kbd>F11</kbd>** | Basculer en mode Plein Écran |
| **<kbd>R</kbd>** | Réinitialiser tous les réglages par défaut |
| **<kbd>Échap</kbd>** | Quitter l'outil ciblé ou réinitialiser le mode comparaison |
| **Molette Souris** | Zoom avant / arrière fluide (20% à 800%) |
| **Clic Droit / Milieu + Glisser** | Déplacement panoramique (Pan) dans l'image |

---

## 🚀 Téléchargement & Installation

Les binaires autonomes multiplateformes (sans dépendance externe requise) sont générés automatiquement par le pipeline CI/CD GitHub Actions.

👉 **[Télécharger la version officielle (Releases)](https://github.com/vedix95/ULTRAHDR-JPEG/releases/tag/v1.0.0)**

### Packages Disponibles :
- **Linux 64-bit** : [`ultrahdr-linux-x64.tar.gz`](https://github.com/vedix95/ULTRAHDR-JPEG/releases/download/v1.0.0/ultrahdr-linux-x64.tar.gz)
- **Windows 64-bit** : [`ultrahdr-windows-x64.zip`](https://github.com/vedix95/ULTRAHDR-JPEG/releases/download/v1.0.0/ultrahdr-windows-x64.zip)

### Lancement Rapide :
- **Sous Linux** :
  ```bash
  tar -xzf ultrahdr-linux-x64.tar.gz
  chmod +x ultrahdr
  ./ultrahdr
  ```
- **Sous Windows** :
  Décompressez l'archive et lancez `ultrahdr.exe` (ou double-cliquez sur `run.bat`).

---

## 🛠️ Utilisation en Ligne de Commande (CLI)

UltraHDR Studio peut être utilisé comme outil en ligne de commande ultra-rapide et scriptable :

```bash
# Ouvrir l'interface graphique avec une photo préchargée
ultrahdr photo.jpg

# Convertir avec le profil automatique recommandé
ultrahdr photo.jpg -o photo_UltraHDR.jpg

# Conversion avec étalonnage complet personnalisé
ultrahdr photo.jpg -o output.jpg \
  --peak-nits 1400 \
  --contrast 0.50 \
  --vibrance 0.40 \
  --saturation 0.20 \
  --shadow-boost 1.15 \
  --quality 95 \
  --gainmap-quality 90

# Appliquer un profil prédéfini (sunset, night_neon, landscape, studio, native_hdr, moderate)
ultrahdr coucher_soleil.jpg -o output.jpg --preset sunset

# Analyse approfondie du potentiel HDR sans conversion
ultrahdr photo.raw --analyze

# Traitement par lot multithreadé de tout un dossier
ultrahdr /chemin/vers/dossier/ -o /chemin/vers/sortie/ --preset landscape
```

### Profils Prédéfinis Disponibles :
- `sunset` : Coucher de soleil (teintes chaudes, hautes lumières intenses, reflets solaires).
- `night_neon` : Nuit & Néons (noirs profonds, néons et lumières artificielles survoltées).
- `landscape` : Paysage naturel (ciel préservé, ombres débouchées, contraste équilibré).
- `studio` : Portrait & Studio (préservation des tons chair, reflets doux et naturels).
- `native_hdr` : Capture HDR native (pour fichiers RAW, DNG ou OpenEXR avec dynamique étendue).
- `moderate` : Rehaussement discret et naturel pour tout type d'image.

---

## 📦 Compilation depuis les Sources

### Prérequis
- [Rust & Cargo](https://rustup.rs/) (version 1.75+)
- Sous Linux (Ubuntu / Debian) :
  ```bash
  sudo apt-get update && sudo apt-get install -y libxkbcommon-dev libwayland-dev
  ```

### Scripts Prêts à l'Emploi
- **Sous Linux** :
  ```bash
  chmod +x build_all.sh
  ./build_all.sh
  ```
- **Sous Windows** :
  Double-cliquez sur `build_all.bat`.

### Compilation Manuelle avec Cargo
```bash
git clone https://github.com/vedix95/ULTRAHDR-JPEG.git
cd ULTRAHDR-JPEG/rust
cargo build --release
```
L'exécutable optimisé se trouve dans `rust/target/release/ultrahdr` (ou `dist/linux/ultrahdr`).

### Tests Unitaires
```bash
cd rust
cargo test
```

---

## 📋 Formats d'Images Pris en Charge

- **Photos Standards** : JPEG, PNG, WebP, BMP
- **Haute Dynamique & HDR Natif** : Radiance HDR (`.hdr`), OpenEXR (`.exr`), TIFF 16-bit
- **Fichiers RAW Appareils Photo** : Adobe DNG, Canon CR2, Nikon NEF, Sony ARW
- **Mobiles** : Apple HEIC / HEIF

---

## 📄 Licence

Ce projet est sous licence **MIT**. Développé et maintenu par [@vedix95](https://github.com/vedix95).
