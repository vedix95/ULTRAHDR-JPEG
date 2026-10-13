"""cli.py — Interface en ligne de commande pour UltraHDR Studio.

Permet la conversion unitaire ou par lot de n'importe quel format vers JPEG Ultra HDR
sans interface graphique.
"""
from __future__ import annotations

import argparse
import sys
from pathlib import Path

# Assurer le support UTF-8 sur consoles Windows
if hasattr(sys.stdout, "reconfigure"):
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
if hasattr(sys.stderr, "reconfigure"):
    sys.stderr.reconfigure(encoding="utf-8", errors="replace")

import numpy as np
from core.image_loader import load_image, RAW_EXTENSIONS, HDR_EXTENSIONS, HEIF_EXTENSIONS
from core.analyzer import analyze_image_potential, PRESET_LIBRARY
from core.ultrahdr_engine import (
    compute_hdr_and_gain_map,
    assemble_ultra_hdr_jpeg, encode_gain_map_jpeg
)


def process_single_file(
    input_path: Path,
    output_path: Path | None = None,
    preset_name: str | None = None,
    peak_nits: float | None = None,
    sdr_white: float = 203.0,
    hi_gamma: float | None = None,
    shadow_boost: float | None = None,
    threshold: float | None = None,
    contrast: float = 0.0,
    base_quality: int = 94,
    gainmap_quality: int = 88,
    save_gainmap: bool = False,
    analyze_only: bool = False,
) -> bool:
    try:
        print(f"\n[+] Chargement de {input_path.name}...")
        img = load_image(input_path)
        print(f"    Format : {img.format_name} ({img.width}x{img.height}), Natif HDR={img.is_native_hdr}")

        analysis = analyze_image_potential(img)
        print(f"    Potentiel HDR : {analysis.potential_score}% [{analysis.category_title}]")
        print(f"    Détails : {analysis.explanation}")

        if analyze_only:
            return True

        # Détermination des paramètres
        if preset_name and preset_name in PRESET_LIBRARY:
            selected_preset = PRESET_LIBRARY[preset_name]
        else:
            selected_preset = analysis.recommended_preset

        final_peak = peak_nits or selected_preset.peak_nits
        final_gamma = hi_gamma or selected_preset.hi_gamma
        final_shadow = shadow_boost or selected_preset.shadow_boost
        final_threshold = threshold or selected_preset.highlight_threshold

        print(f"    Paramètres appliqués : Peak={final_peak:.0f} nits, Gamma={final_gamma:.2f}, ShadowBoost={final_shadow:.2f}x, Contraste={contrast:+.0%}")

        # Calcul HDR et Gain Map unifié
        hdr_nits, gain_log2, gmin, gmax = compute_hdr_and_gain_map(
            sdr_uint8=img.sdr_uint8,
            hdr_lin_input=img.linear_rgb if img.is_native_hdr else None,
            is_native_hdr=img.is_native_hdr,
            peak_nits=final_peak,
            sdr_white=sdr_white,
            shadow_boost=final_shadow,
            hi_gamma=final_gamma,
            highlight_threshold=final_threshold,
            contrast=contrast,
        )
        print(f"    Gain map générée : Min={gmin:.2f} IL, Max=+{gmax:.2f} IL (Boost max ~{2**gmax:.1f}x)")

        # Assemblage binaire Ultra HDR
        uhdr_bytes = assemble_ultra_hdr_jpeg(
            img.sdr_uint8,
            gain_log2,
            gmin,
            gmax,
            final_peak,
            sdr_white,
            base_quality=base_quality,
            gainmap_quality=gainmap_quality
        )

        # Enregistrement
        if output_path is None:
            output_path = input_path.with_name(f"{input_path.stem}_UltraHDR.jpg")

        output_path.parent.mkdir(parents=True, exist_ok=True)
        output_path.write_bytes(uhdr_bytes)
        print(f"    [✓] Enregistré : {output_path} ({len(uhdr_bytes) // 1024} Ko)")

        if save_gainmap:
            gm_bytes = encode_gain_map_jpeg(gain_log2, gmin, gmax, quality=gainmap_quality)
            gm_path = output_path.with_name(f"{output_path.stem}_gainmap.jpg")
            gm_path.write_bytes(gm_bytes)
            print(f"    [✓] Gain map standalone : {gm_path}")

        return True
    except Exception as e:
        print(f"    [!] Erreur lors du traitement de {input_path.name} : {e}", file=sys.stderr)
        return False


def main():
    parser = argparse.ArgumentParser(description="UltraHDR Studio CLI — Conversion vers JPEG Ultra HDR")
    parser.add_argument("input", type=Path, help="Fichier image ou dossier d'images à convertir")
    parser.add_argument("-o", "--output", type=Path, default=None, help="Chemin du fichier de sortie")
    parser.add_argument("--preset", type=str, choices=list(PRESET_LIBRARY.keys()), default=None,
                        help="Nom du profil (sunset, night_neon, landscape, studio, native_hdr, moderate)")
    parser.add_argument("--peak-nits", type=float, default=None, help="Luminance de crête en nits (ex: 1200)")
    parser.add_argument("--sdr-white", type=float, default=203.0, help="Luminance blanc SDR de référence (défaut: 203)")
    parser.add_argument("--gamma", type=float, default=None, help="Exposant de courbe de boost")
    parser.add_argument("--shadow-boost", type=float, default=None, help="Facteur de boost des ombres (ex: 1.2)")
    parser.add_argument("--threshold", type=float, default=None, help="Seuil de hautes lumières [0..1]")
    parser.add_argument("--contrast", type=float, default=0.0, help="Ajustement de contraste [-1.0..1.0] (défaut: 0.0)")
    parser.add_argument("--quality", type=int, default=94, help="Qualité JPEG base (défaut: 94)")
    parser.add_argument("--gainmap-quality", type=int, default=88, help="Qualité JPEG gain map (défaut: 88)")
    parser.add_argument("--save-gainmap", action="store_true", help="Enregistrer aussi la gain map en JPEG niveaux de gris")
    parser.add_argument("--analyze", action="store_true", help="Afficher uniquement l'analyse de potentiel HDR sans convertir")

    args = parser.parse_args()

    input_path = args.input
    if not input_path.exists():
        print(f"Erreur : Le chemin {input_path} n'existe pas.", file=sys.stderr)
        sys.exit(1)

    if input_path.is_file():
        success = process_single_file(
            input_path,
            output_path=args.output,
            preset_name=args.preset,
            peak_nits=args.peak_nits,
            sdr_white=args.sdr_white,
            hi_gamma=args.gamma,
            shadow_boost=args.shadow_boost,
            threshold=args.threshold,
            contrast=args.contrast,
            base_quality=args.quality,
            gainmap_quality=args.gainmap_quality,
            save_gainmap=args.save_gainmap,
            analyze_only=args.analyze
        )
        sys.exit(0 if success else 1)
    elif input_path.is_dir():
        # Traitement par lot
        all_exts = {'.jpg', '.jpeg', '.png', '.webp', '.tif', '.tiff', '.bmp'} | RAW_EXTENSIONS | HDR_EXTENSIONS | HEIF_EXTENSIONS
        files = [p for p in input_path.iterdir() if p.is_file() and p.suffix.lower() in all_exts]
        print(f"[*] Traitement par lot de {len(files)} fichiers dans {input_path}...")
        count_ok = 0
        for f in files:
            if process_single_file(
                f,
                output_path=None,
                preset_name=args.preset,
                peak_nits=args.peak_nits,
                sdr_white=args.sdr_white,
                hi_gamma=args.gamma,
                shadow_boost=args.shadow_boost,
                threshold=args.threshold,
                contrast=args.contrast,
                base_quality=args.quality,
                gainmap_quality=args.gainmap_quality,
                save_gainmap=args.save_gainmap,
                analyze_only=args.analyze
            ):
                count_ok += 1
        print(f"\n[✓] Terminé : {count_ok}/{len(files)} fichiers convertis avec succès.")


if __name__ == "__main__":
    main()
