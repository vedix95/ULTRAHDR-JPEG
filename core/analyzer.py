"""core/analyzer.py — Analyse intelligente du potentiel HDR et proposition automatique.

Analyse l'histogramme, la dynamique et la distribution spatiale de la luminance
pour déterminer si la photo se prête à l'Ultra HDR et proposer les réglages optimaux.
"""
from __future__ import annotations

from dataclasses import dataclass, asdict
from typing import Dict, Any, List

import numpy as np
from core.image_loader import LoadedImage


@dataclass
class HDRPreset:
    name: str
    label: str
    peak_nits: float
    sdr_white: float
    shadow_boost: float
    hi_gamma: float
    highlight_threshold: float
    description: str


@dataclass
class AnalysisResult:
    is_native_hdr: bool
    potential_score: int          # 0 à 100
    category: str                 # 'NATIVE_HDR' | 'SUNSET' | 'NIGHT_NEON' | 'LANDSCAPE' | 'STUDIO' | 'MODERATE'
    category_title: str
    explanation: str
    highlight_ratio_pct: float
    peak_detected_stops: float
    contrast_ratio: float
    recommended_preset: HDRPreset
    available_presets: List[HDRPreset]

    def to_dict(self) -> Dict[str, Any]:
        d = asdict(self)
        return d


PRESET_LIBRARY: Dict[str, HDRPreset] = {
    "NATIVE_HDR": HDRPreset(
        name="native_hdr",
        label="Capteur Haute Dynamique (RAW/EXR)",
        peak_nits=1400.0,
        sdr_white=203.0,
        shadow_boost=1.2,
        hi_gamma=1.0,
        highlight_threshold=0.50,
        description="Exploite la plage dynamique étendue native du capteur avec une restitution éclatante des hautes lumières."
    ),
    "SUNSET": HDRPreset(
        name="sunset",
        label="Coucher de Soleil & Contre-Jour",
        peak_nits=1300.0,
        sdr_white=203.0,
        shadow_boost=1.15,
        hi_gamma=1.1,
        highlight_threshold=0.55,
        description="Sublime le disque solaire, les nuages incandescents et les reflets dorés sans dénaturer les teintes chaudes."
    ),
    "NIGHT_NEON": HDRPreset(
        name="night_neon",
        label="Nuit, Éclairages & Néons",
        peak_nits=1500.0,
        sdr_white=203.0,
        shadow_boost=1.05,
        hi_gamma=0.85,
        highlight_threshold=0.65,
        description="Fait percer les sources lumineuses intenses (enseignes, phares, réverbères) sur fond nocturne profond."
    ),
    "LANDSCAPE": HDRPreset(
        name="landscape",
        label="Paysage & Ciel Lumineux",
        peak_nits=1000.0,
        sdr_white=203.0,
        shadow_boost=1.25,
        hi_gamma=1.0,
        highlight_threshold=0.60,
        description="Boost équilibré et naturel pour paysages ensoleillés, ciels bleus et reflets sur l'eau."
    ),
    "STUDIO": HDRPreset(
        name="studio",
        label="Portrait & Contraste Doux",
        peak_nits=800.0,
        sdr_white=203.0,
        shadow_boost=1.20,
        hi_gamma=1.2,
        highlight_threshold=0.70,
        description="Rehaussement soyeux des reflets spéculaires et des reflets dans les yeux tout en préservant le modelé des peaux."
    ),
    "MODERATE": HDRPreset(
        name="moderate",
        label="Subtil / Anti-Bruit",
        peak_nits=600.0,
        sdr_white=203.0,
        shadow_boost=1.10,
        hi_gamma=1.4,
        highlight_threshold=0.75,
        description="Rehaussement progressif et maîtrisé pour scènes à faible contraste, évitant toute montée de bruit."
    ),
}


def analyze_image_potential(image: LoadedImage) -> AnalysisResult:
    """Analyse l'image chargée et produit une recommandation automatique personnalisée."""
    available_list = list(PRESET_LIBRARY.values())

    # 1. Cas d'un fichier nativement HDR (RAW, EXR, HDR 32-bit float, TIFF 16-bit)
    if image.is_native_hdr:
        score = 98
        category = "NATIVE_HDR"
        preset = PRESET_LIBRARY["NATIVE_HDR"]
        # Ajuster le peak nits selon les stops réels détectés
        if image.dynamic_range_stops > 12.0:
            preset.peak_nits = 1600.0
        elif image.dynamic_range_stops > 10.0:
            preset.peak_nits = 1400.0
        else:
            preset.peak_nits = 1100.0

        explanation = (
            f"Fichier haute dynamique native ({image.format_name}) contenant ~{image.dynamic_range_stops:.1f} stops "
            f"de dynamique. Recommandation : Ultra HDR Haute Fidélité pour déployer tout le potentiel du capteur."
        )
        return AnalysisResult(
            is_native_hdr=True,
            potential_score=score,
            category=category,
            category_title=preset.label,
            explanation=explanation,
            highlight_ratio_pct=15.0,
            peak_detected_stops=image.dynamic_range_stops,
            contrast_ratio=150.0,
            recommended_preset=preset,
            available_presets=available_list
        )

    # 2. Cas d'une image SDR standard (analyse d'histogramme)
    # Calcul de la luminance relative Y standard BT.709
    lin = image.linear_rgb
    w = np.array([0.2126, 0.7152, 0.0722], dtype=np.float32)
    lum = (lin * w).sum(axis=-1)

    # Statistiques
    p05 = float(np.percentile(lum, 5))
    p50 = float(np.percentile(lum, 50))
    p90 = float(np.percentile(lum, 90))
    p95 = float(np.percentile(lum, 95))
    p99 = float(np.percentile(lum, 99))
    max_lum = float(np.max(lum))

    # Taux de hautes lumières en linéaire (0.50 en linéaire correspond à ~190 en sRGB 8-bit)
    high_lights_pct = float(np.mean(lum > 0.45)) * 100.0
    intense_lights_pct = float(np.mean(lum > 0.75)) * 100.0
    contrast_ratio = (p99 + 1e-4) / (p05 + 1e-4)

    # Détection des dominantes colorées dans les hautes lumières
    mask_high = lum > 0.40
    is_warm_sunset = False
    if np.any(mask_high):
        r_high = float(np.mean(lin[:, :, 0][mask_high]))
        b_high = float(np.mean(lin[:, :, 2][mask_high]))
        if r_high > (b_high * 1.35):
            is_warm_sunset = True

    # Classification
    # A. Scène nocturne / néons : médiane sombre mais hautes lumières très isolées
    if p50 < 0.15 and intense_lights_pct >= 0.2 and p99 > 0.85:
        category = "NIGHT_NEON"
        preset = PRESET_LIBRARY["NIGHT_NEON"]
        score = 92
        explanation = (
            "Scène nocturne avec fortes sources d'éclairage ponctuelles (phares, néons, lampions). "
            "Le profil Nuit & Néons donnera un éclat saisissant à ces points lumineux."
        )

    # B. Coucher de soleil / Contre-jour chaleureux
    elif is_warm_sunset and high_lights_pct > 3.0:
        category = "SUNSET"
        preset = PRESET_LIBRARY["SUNSET"]
        score = 94
        explanation = (
            "Hautes lumières chaleureuses détectées (coucher de soleil, contre-jour ou reflets dorés). "
            "Le profil Coucher de Soleil fera rayonner les teintes chaudes et le ciel."
        )

    # C. Paysage ensoleillé / ciel lumineux
    elif high_lights_pct >= 6.0 and contrast_ratio >= 15.0:
        category = "LANDSCAPE"
        preset = PRESET_LIBRARY["LANDSCAPE"]
        score = 88
        explanation = (
            f"Image lumineuse avec {high_lights_pct:.1f}% de hautes lumières et bon contraste ({contrast_ratio:.0f}:1). "
            "Le profil Paysage accentuera la profondeur du ciel et la lumière naturelle."
        )

    # D. Portrait ou sujet contrasté
    elif contrast_ratio >= 8.0 and intense_lights_pct >= 0.5:
        category = "STUDIO"
        preset = PRESET_LIBRARY["STUDIO"]
        score = 78
        explanation = (
            "Contraste équilibré avec reflets modérés. Le profil Doux rehausse les reflets avec naturel sans surcharger."
        )

    # E. Scène modérée / faible dynamique
    else:
        category = "MODERATE"
        preset = PRESET_LIBRARY["MODERATE"]
        score = 62
        explanation = (
            "Image à contraste modéré sans sources lumineuses éclatantes. "
            "Le profil Subtil applique un rehaussement progressif tout en évitant le bruit."
        )

    stops_est = float(np.log2(max(contrast_ratio, 2.0)))

    return AnalysisResult(
        is_native_hdr=False,
        potential_score=score,
        category=category,
        category_title=preset.label,
        explanation=explanation,
        highlight_ratio_pct=round(high_lights_pct, 1),
        peak_detected_stops=round(stops_est, 1),
        contrast_ratio=round(contrast_ratio, 1),
        recommended_preset=preset,
        available_presets=available_list
    )


if __name__ == "__main__":
    from core.image_loader import load_image
    import sys
    test_path = sys.argv[1] if len(sys.argv) > 1 else None
    if test_path:
        img = load_image(test_path)
        res = analyze_image_potential(img)
        print("Analyse:", res.category_title, "Score:", res.potential_score, "Preset:", res.recommended_preset.name)
    else:
        print("Usage: python core/analyzer.py <image_path>")
