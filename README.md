# ✨ UltraHDR Studio (Google Ultra HDR & ISO 21496-1)

[![CI / CD Build & Release](https://github.com/vedix95/ULTRAHDR-JPEG/actions/workflows/build.yml/badge.svg)](https://github.com/vedix95/ULTRAHDR-JPEG/actions/workflows/build.yml)
[![GitHub Release](https://img.shields.io/github/v/release/vedix95/ULTRAHDR-JPEG?color=blue&label=Release)](https://github.com/vedix95/ULTRAHDR-JPEG/releases)
[![Rust](https://img.shields.io/badge/Rust-1.75+-orange.svg)](https://www.rust-lang.org/)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)
[![Platform](https://img.shields.io/badge/Platform-Linux%20%7C%20Windows-blue.svg)](https://github.com/vedix95/ULTRAHDR-JPEG/releases)

> **Éditeur et générateur photographique Ultra HDR professionnel**, 100% conforme aux standards internationaux **ISO 21496-1** et **Google Ultra HDR (v1.0)**.
> Disponible avec une **interface graphique de bureau native haute performance en Rust pur** (zéro Electron, zéro WebView, rendu WGPU DirectX 12 / Vulkan / Wayland) et une suite complète d'outils **CLI (Rust & Python)**.

---

## 📸 Qu'est-ce que le format Ultra HDR (ISO 21496-1) ?

Le format **JPEG Ultra HDR** réconcilie la compatibilité universelle et l'avenir de l'imagerie à haute dynamique :
1. **Compatibilité universelle SDR** : Tout visualiseur classique, navigateur web ou écran SDR affiche l'image JPEG de base sans aucune altération.
2. **Gain Map ISO 21496-1 intégrée** : Une carte de gain secondaire encodée en niveaux de gris est encapsulée dans le fichier via les métadonnées Adobe XMP (`hdrgm`) et le format multi-images CIPA DC-007 MPF APP2.
3. **Rendu HDR Éclatant** : Sur les appareils compatibles (écrans OLED/Mini-LED HDR, smartphones Android récents, Google Chrome, iOS 18+, macOS, Windows 11 HDR), la carte de gain est appliquée dynamiquement pour restituer jusqu'à **4 000 nits** d'éclat lumineux dans les reflets et les crêtes spéculaires.

---

## ⚡ Fonctionnalités Clés

### 🎨 Tonalité Pro & Étalonnage Lightroom / Photoshop
- **Contraste Paramétrique S-Curve** : modulation sigmoïde de la pente dynamique autour du pivot médian (0.45), donnant du relief et de la profondeur sans brûler les blancs ni noyer les ombres.
- **Réglages par Zones Tonalité Pro** :
  - **Blancs (Whites)** : module directement l'intensité du pic de boost HDR sur les éclats spéculaires.
  - **Hautes Lumières (Highlights)** : ajuste la progressivité et la courbe de transition vers les nits élevés.
  - **Ombres (Shadows)** : rehausse les pénombres sans bruit numérique.
  - **Noirs (Blacks)** : ancre la densité du point noir.
- **Vibrance & Saturation HDR** :
  - **Saturation HDR** : ajuste uniformément l'intensité chromatique globale.
  - **Vibrance HDR** : protège intelligemment les tons chair et les couleurs déjà très vives tout en rehaussant les teintes douces, empêchant le délavage blanc des crêtes lumineuses HDR.
- **Filtre Anti-Halo Bilatéral** : supprime les artefacts de liseré et de halo autour des silhouettes à fort contraste.

### 📈 Courbe Tonale Interactive (*Tone Curve*)
- **Tracé Spline en Temps Réel** : visualisation dynamique de la courbe de réponse tonale superposée à l'histogramme Lightroom à 4 zones.
- **Ligne de Référence 45°** : repère visuel immédiat montrant si une zone est rehaussée ou atténuée.
- **Points d'Ancrage Interactifs** : suivi visuel direct des zones *Noirs*, *Ombres*, *Hautes Lumières* et *Blancs*.

### 🎯 Outil Ciblé Direct (*Targeted Adjustment Tool - TAT*)
- Cliquez sur l'icône **🎯 Outil Ciblé** (ou appuyez sur <kbd>T</kbd>) : survolez directement n'importe quelle zone de la photo et glissez verticalement avec la souris pour ajuster immédiatement la zone tonale sous le curseur.

### 👁️ Comparaison Instantanée Ultra-Rapide
- **Hold-to-Compare** : maintenez enfoncée la touche <kbd>Espace</kbd>, <kbd>\</kbd> ou <kbd>/</kbd> pour basculer instantanément sur l'image originale SDR brute à 60/144 FPS sans latence. Relâchez pour revenir à la vue Ultra HDR traitée.
- **Bouton Dédié** : bascule de comparaison accessible en un clic dans la barre d'outils.

### 🖥️ Moteur de Rendu & Visionneuse
- **Multi-Modes d'Affichage** (cycle rapide avec la touche <kbd>M</kbd>) :
  - **↔ Glissière Avant / Après (Split-Screen)** : ligne de séparation interactive avec poignée draggable.
  - **◫ Côte à côte (Side by Side)** : vue synchronisée SDR original vs Ultra HDR.
  - **✨ Vrai Ultra HDR Direct** : affichage pleine intensité sur dalles HDR compatibles.
  - **◐ Gain Map** : inspection de la carte de gain log2 en niveaux de gris.
  - **🔥 Carte Thermique Nits** : carte Turbo en fausses couleurs pour cartographier la luminance en nits.
- **Simulation Écran SDR** :
  - Modes de tone-mapping : **ACES Filmic** (cinéma), **Naturel** (continuité $C^1$ sans cramage), **Reinhard**, et **Linéaire**.
  - Curseurs de décalage d'exposition virtuelle (**EV Shift** de -3 à +3 IL) et point blanc SDR (80 à 300 nits).
- **Navigation Fluide** : Zoom molette (20% à 800%), déplacement panoramique par clic droit ou clic milieu.
- **Sélecteur de Fichiers Natif** :
  - **Linux** : intégration transparente XDG Desktop Portal (GNOME, KDE Plasma, Wayland).
  - **Windows** : dialogue explorateur natif Win32.

### 📁 Traitement par Lot Multithreadé (*Batch Conversion*)
- Convertissez des répertoires entiers d'images en JPEG Ultra HDR en quelques secondes grâce à la parallélisation multithread avec **Rayon**.
- Conserve automatiquement les métadonnées **EXIF** d'origine de vos photos.

---

## ⌨️ Raccourcis Clavier Essentiels

| Raccourci | Action |
| :--- | :--- |
| **<kbd>Espace</kbd>** / **<kbd>\</kbd>** *(maintenir)* | **Afficher instantanément l'image originale SDR** (Hold to Compare) |
| **<kbd>\</kbd>** / **<kbd>/</kbd>** *(appui simple)* | Basculer la vue comparaison Original SDR / Ultra HDR |
| **<kbd>M</kbd>** | Changer de mode d'affichage (`Split` → `SideBySide` → `FullHdr` → `GainMap` → `Heatmap`) |
| **<kbd>T</kbd>** | Activer / désactiver l'**Outil Ciblé Direct (TAT)** sur l'image |
| **<kbd>Tab</kbd>** | Afficher / masquer le panneau latéral d'outils |
| **<kbd>F11</kbd>** | Basculer en mode Plein Écran |
| **<kbd>Échap</kbd>** | Quitter l'outil ciblé ou réinitialiser le mode comparaison |
| **Molette Souris** | Zoom avant / arrière fluide |
| **Clic Droit / Milieu + Glisser** | Déplacement panoramique (Pan) dans l'image |

---

## 🚀 Téléchargement & Installation

Les binaires autonomes multiplateformes (sans dépendance externe) sont compilés automatiquement par le pipeline CI/CD GitHub Actions à chaque version.

👉 **[Télécharger la dernière version (Releases)](https://github.com/vedix95/ULTRAHDR-JPEG/releases)**

### Packages Disponibles :
- **Linux 64-bit** : `ultrahdr-linux-x64.tar.gz` (exécutable binaire natif)
- **Windows 64-bit** : `ultrahdr-windows-x64.zip` (`ultrahdr.exe`)

### Exécution :
- **Sous Linux** :
  ```bash
  tar -xzf ultrahdr-linux-x64.tar.gz
  chmod +x ultrahdr
  ./ultrahdr
  ```
- **Sous Windows** :
  Extrayez l'archive `ultrahdr-windows-x64.zip` et double-cliquez sur `ultrahdr.exe`.

---

## 🛠️ Utilisation en Ligne de Commande (CLI)

UltraHDR Studio peut être utilisé comme outil en ligne de commande ultra-rapide pour des scripts d'automatisation ou des serveurs.

```bash
# Ouvrir l'interface graphique avec une photo préchargée
ultrahdr photo.jpg

# Convertir avec le profil recommandé déterminé automatiquement
ultrahdr photo.jpg -o photo_UltraHDR.jpg

# Conversion avec étalonnage complet personnalisé
ultrahdr photo.jpg -o output.jpg \
  --peak-nits 1400 \
  --contrast 0.25 \
  --vibrance 0.30 \
  --saturation 0.10 \
  --shadow-boost 1.15 \
  --quality 95 \
  --gainmap-quality 90

# Appliquer un profil prédéfini (sunset, night_neon, landscape, studio, native_hdr, moderate)
ultrahdr coucher_soleil.jpg -o output.jpg --preset sunset

# Analyse approfondie du potentiel HDR sans conversion
ultrahdr photo.raw --analyze

# Traitement par lot de tout un dossier
ultrahdr /chemin/vers/dossier/ -o /chemin/vers/sortie/ --preset landscape
```

---

## 📦 Compilation depuis les Sources

### Prérequis
- [Rust & Cargo](https://rustup.rs/) (version 1.75 ou supérieure)
- Sous Linux (Ubuntu / Debian) :
  ```bash
  sudo apt-get update && sudo apt-get install -y libxkbcommon-dev libwayland-dev
  ```

### Compilation de la Version Native Rust
```bash
git clone https://github.com/vedix95/ULTRAHDR-JPEG.git
cd ULTRAHDR-JPEG/rust
cargo build --release
```
L'exécutable optimisé se trouve dans `rust/target/release/ultrahdr` (ou `ultrahdr.exe`).

### Lancement des Tests Unitaires
```bash
cargo test
```

### Version Python & CLI Python
```bash
pip install -r requirements.txt
python -m unittest tests/test_all.py
python cli.py input.jpg -o output.jpg --contrast 0.2 --vibrance 0.3
```

---

## 📋 Formats d'Images Pris en Charge

- **Photos Standards** : JPEG, PNG, WebP, BMP
- **Haute Dynamique & HDR Natif** : Radiance HDR (`.hdr`), OpenEXR (`.exr`), TIFF 16-bit
- **Fichiers RAW Appareils Photo** : Adobe DNG, Canon CR2, Nikon NEF, Sony ARW
- **Mobiles** : Apple HEIC / HEIF

---

## 📄 Licence

Ce projet est sous licence **MIT**. Vous êtes libre de l'utiliser, le modifier et le distribuer à des fins personnelles et commerciales.
