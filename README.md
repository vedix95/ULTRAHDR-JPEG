# UltraHDR Studio (Google Ultra HDR & ISO 21496-1)

Générateur et visualiseur **Ultra HDR** conforme aux normes **ISO 21496-1** et **Google Ultra HDR (v1.0)**.
Disponible avec une **interface graphique de bureau 100% native en Rust pur** (zéro HTML, zéro navigateur) et une suite d'outils **CLI / Python**.

---

## ⚡ Fonctionnalités Clés

- **Norme ISO 21496-1 & Google Ultra HDR** :
  - Métadonnées Adobe XMP (`hdrgm:Version="1.0"`, `GainMapMin`, `GainMapMax`, `HDRCapacityMin/Max`).
  - Marqueur MPF CIPA DC-007 APP2 (Multi-Picture Format) pour l'encapsulation de la Gain Map en flux secondaire.
  - Image de base JPEG 8-bit standard 100% compatible avec tous les visualiseurs et navigateurs SDR existants.
- **Support HDR Matériel Natif Direct** :
  - **Windows** : Pipeline DirectX 12 / scRGB (`Rgba16Float`) exploitant directement la luminance maximale des écrans HDR (jusqu'à 1000–4000 nits).
  - **Linux (Wayland)** : Détection et présentation native via Wayland Color Management et Vulkan.
- **Simulation Réaliste sur Écrans SDR** :
  - Tone-mapping **ACES Filmic** cinématographique (défaut) sans aucun écrêtage.
  - Mode **Naturel (SDR Préservé)** avec transition continue $C^1$.
  - Ajustement d'exposition virtuelle en temps réel (-3 EV à +3 EV).
- **Visionneuse Interactive Complète** :
  - **↔ Glissière Avant / Après (Split-Screen)** : Comparaison temps réel interactive.
  - **◫ Côte à côte (Side by Side)** : Vue simultanée SDR original vs Ultra HDR.
  - **✨ Vrai Ultra HDR Direct** : Exploitation maximale de la dalle d'affichage.
  - **◐ Gain Map** : Visualisation de la carte de gain en niveaux de gris.
  - **🔥 Carte Nits** : Carte thermique Turbo en fausses couleurs des niveaux de luminance.
  - **Zoom & Pan** : Zoom continu (20% à 800%) et déplacement au clic droit.
- **Prise en charge universelle des formats** :
  - OpenEXR (`.exr`), Radiance HDR (`.hdr`), Camera RAW, TIFF 16-bit, Apple HEIC, JPEG, PNG, WebP.
- **Double Mode d'Exécution** :
  - Interface graphique de bureau native.
  - Mode ligne de commande (CLI) multithreadé haute performance avec Rayon pour conversion par lot.

---

## 🚀 Démarrage Rapide

### Binaires Précompilés Prêts à l'Emploi

Les exécutables autonomes sont disponibles dans le dossier [`dist/`](dist/) :

| Plateforme | Fichier | Rendu Graphique |
| :--- | :--- | :--- |
| **Windows** | `dist/windows/ultrahdr.exe` | DirectX 12 / Vulkan, PE 64-bit autonome |
| **Linux** | `dist/linux/ultrahdr` | Wayland / X11, ELF 64-bit statique musl |

* **Sous Windows** : Double-cliquez sur `dist\windows\ultrahdr.exe` ou lancez `run_rust.bat`.
* **Sous Linux** : Lancez simplement `./dist/linux/ultrahdr`.

---

## 🛠️ Utilisation en Ligne de Commande (CLI)

```bash
# Analyse intelligente du potentiel HDR sans convertir
ultrahdr photo.jpg --analyze

# Conversion automatique avec profil recommandé
ultrahdr photo.jpg -o photo_UltraHDR.jpg

# Conversion avec profil spécifique (sunset, night_neon, landscape, studio, native_hdr, moderate)
ultrahdr coucher_soleil.jpg -o output.jpg --preset sunset

# Paramétrage sur mesure
ultrahdr rendu.exr -o output.jpg --peak-nits 1600 --gamma 1.1 --quality 95
```

---

## 📦 Compilation des Sources

### Version Rust Native
```bash
cd rust
cargo build --release
```

Pour compiler la version Linux statique musl depuis Windows :
```bash
cargo build --release --target x86_64-unknown-linux-musl
```

### Version Python (Backend FastAPI & Web)
```bash
pip install -r requirements.txt
python main.py
```

---

## 📄 Licence

Projet distribué sous licence MIT.
