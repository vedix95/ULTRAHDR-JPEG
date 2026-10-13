//! analyzer.rs — Analyse intelligente du potentiel HDR et proposition automatique de profil.

use serde::{Deserialize, Serialize};
use crate::engine::{percentile_mut, LUM_WEIGHTS};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HDRPreset {
    pub name: String,
    pub label: String,
    pub peak_nits: f32,
    pub sdr_white: f32,
    pub shadow_boost: f32,
    pub hi_gamma: f32,
    pub highlight_threshold: f32,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisResult {
    pub is_native_hdr: bool,
    pub potential_score: u32,
    pub category: String,
    pub category_title: String,
    pub explanation: String,
    pub highlight_ratio_pct: f32,
    pub peak_detected_stops: f32,
    pub contrast_ratio: f32,
    pub recommended_preset: HDRPreset,
    pub available_presets: Vec<HDRPreset>,
}

pub fn get_preset_library() -> Vec<HDRPreset> {
    vec![
        HDRPreset {
            name: "native_hdr".to_string(),
            label: "Capteur RAW / Haute Fidélité".to_string(),
            peak_nits: 1400.0,
            sdr_white: 203.0,
            shadow_boost: 1.0,
            hi_gamma: 1.0,
            highlight_threshold: 0.50,
            description: "Exploite la plage dynamique étendue native du capteur avec une restitution éclatante des hautes lumières.".to_string(),
        },
        HDRPreset {
            name: "sunset".to_string(),
            label: "Coucher de Soleil & Contre-Jour".to_string(),
            peak_nits: 1300.0,
            sdr_white: 203.0,
            shadow_boost: 1.0,
            hi_gamma: 1.1,
            highlight_threshold: 0.55,
            description: "Sublime le disque solaire, les nuages incandescents et les reflets dorés sans dénaturer les teintes chaudes.".to_string(),
        },
        HDRPreset {
            name: "night_neon".to_string(),
            label: "Nuit, Éclairages & Néons".to_string(),
            peak_nits: 1500.0,
            sdr_white: 203.0,
            shadow_boost: 1.0,
            hi_gamma: 0.85,
            highlight_threshold: 0.65,
            description: "Fait percer les sources lumineuses intenses (enseignes, phares, réverbères) sur fond nocturne profond.".to_string(),
        },
        HDRPreset {
            name: "landscape".to_string(),
            label: "Paysage & Ciel Lumineux".to_string(),
            peak_nits: 1000.0,
            sdr_white: 203.0,
            shadow_boost: 1.0,
            hi_gamma: 1.0,
            highlight_threshold: 0.60,
            description: "Boost équilibré et naturel pour paysages ensoleillés, ciels bleus et reflets sur l'eau.".to_string(),
        },
        HDRPreset {
            name: "studio".to_string(),
            label: "Portrait & Reflets Doux".to_string(),
            peak_nits: 800.0,
            sdr_white: 203.0,
            shadow_boost: 1.0,
            hi_gamma: 1.2,
            highlight_threshold: 0.70,
            description: "Rehaussement soyeux des reflets spéculaires et des reflets dans les yeux tout en préservant le modelé des peaux.".to_string(),
        },
        HDRPreset {
            name: "moderate".to_string(),
            label: "Subtil / Anti-Bruit".to_string(),
            peak_nits: 600.0,
            sdr_white: 203.0,
            shadow_boost: 1.0,
            hi_gamma: 1.4,
            highlight_threshold: 0.75,
            description: "Rehaussement progressif et maîtrisé pour scènes à faible contraste, évitant toute montée de bruit.".to_string(),
        },
    ]
}

pub fn get_preset_by_name(name: &str) -> Option<HDRPreset> {
    let lower = name.to_lowercase();
    get_preset_library().into_iter().find(|p| p.name == lower || p.name.replace('_', "") == lower.replace('_', ""))
}

/// Analyse le potentiel HDR d'une image chargée et propose les paramètres optimaux.
pub fn analyze_image_potential(
    is_native_hdr: bool,
    linear_rgb: &[f32],
    dynamic_range_stops: f32,
    format_name: &str,
) -> AnalysisResult {
    let presets = get_preset_library();

    if is_native_hdr {
        let mut preset = presets[0].clone();
        if dynamic_range_stops > 12.0 {
            preset.peak_nits = 1600.0;
        } else if dynamic_range_stops > 10.0 {
            preset.peak_nits = 1400.0;
        } else {
            preset.peak_nits = 1100.0;
        }

        let explanation = format!(
            "Fichier haute dynamique native ({}) contenant ~{:.1} stops de dynamique. Recommandation : Ultra HDR Haute Fidélité pour déployer tout le potentiel du capteur.",
            format_name, dynamic_range_stops
        );

        return AnalysisResult {
            is_native_hdr: true,
            potential_score: 98,
            category: "NATIVE_HDR".to_string(),
            category_title: preset.label.clone(),
            explanation,
            highlight_ratio_pct: 15.0,
            peak_detected_stops: dynamic_range_stops,
            contrast_ratio: 150.0,
            recommended_preset: preset,
            available_presets: presets,
        };
    }

    let n_pixels = linear_rgb.len() / 3;
    let mut lum_samples = Vec::with_capacity(n_pixels);
    let mut high_count = 0usize;
    let mut intense_count = 0usize;

    let mut r_high_sum = 0.0f32;
    let mut b_high_sum = 0.0f32;
    let mut high_color_count = 0usize;

    for i in 0..n_pixels {
        let r = linear_rgb[i * 3];
        let g = linear_rgb[i * 3 + 1];
        let b = linear_rgb[i * 3 + 2];
        let lum = r * LUM_WEIGHTS[0] + g * LUM_WEIGHTS[1] + b * LUM_WEIGHTS[2];
        lum_samples.push(lum);

        if lum > 0.45 {
            high_count += 1;
        }
        if lum > 0.75 {
            intense_count += 1;
        }
        if lum > 0.40 {
            r_high_sum += r;
            b_high_sum += b;
            high_color_count += 1;
        }
    }

    let p05 = percentile_mut(&mut lum_samples, 5.0);
    let p50 = percentile_mut(&mut lum_samples, 50.0);
    let p99 = percentile_mut(&mut lum_samples, 99.0);

    let high_lights_pct = (high_count as f32 / n_pixels.max(1) as f32) * 100.0;
    let intense_lights_pct = (intense_count as f32 / n_pixels.max(1) as f32) * 100.0;
    let contrast_ratio = (p99 + 1e-4) / (p05 + 1e-4);

    let is_warm_sunset = if high_color_count > 0 {
        let r_mean = r_high_sum / high_color_count as f32;
        let b_mean = b_high_sum / high_color_count as f32;
        r_mean > (b_mean * 1.35)
    } else {
        false
    };

    let (category, preset_idx, score, explanation) = if p50 < 0.15 && intense_lights_pct >= 0.2 && p99 > 0.85 {
        (
            "NIGHT_NEON",
            2,
            92,
            "Scène nocturne avec fortes sources d'éclairage ponctuelles (phares, néons, lampions). Le profil Nuit & Néons donnera un éclat saisissant à ces points lumineux.".to_string(),
        )
    } else if is_warm_sunset && high_lights_pct > 3.0 {
        (
            "SUNSET",
            1,
            94,
            "Hautes lumières chaleureuses détectées (coucher de soleil, contre-jour ou reflets dorés). Le profil Coucher de Soleil fera rayonner les teintes chaudes et le ciel.".to_string(),
        )
    } else if high_lights_pct >= 6.0 && contrast_ratio >= 15.0 {
        (
            "LANDSCAPE",
            3,
            88,
            format!(
                "Image lumineuse avec {:.1}% de hautes lumières et bon contraste ({:.0}:1). Le profil Paysage accentuera la profondeur du ciel et la lumière naturelle.",
                high_lights_pct, contrast_ratio
            ),
        )
    } else if contrast_ratio >= 8.0 && intense_lights_pct >= 0.5 {
        (
            "STUDIO",
            4,
            78,
            "Contraste équilibré avec reflets modérés. Le profil Doux rehausse les reflets avec naturel sans surcharger.".to_string(),
        )
    } else {
        (
            "MODERATE",
            5,
            62,
            "Image à contraste modéré sans sources lumineuses éclatantes. Le profil Subtil applique un rehaussement progressif tout en évitant le bruit.".to_string(),
        )
    };

    let recommended = presets[preset_idx].clone();
    let stops_est = contrast_ratio.max(2.0).log2();

    AnalysisResult {
        is_native_hdr: false,
        potential_score: score,
        category: category.to_string(),
        category_title: recommended.label.clone(),
        explanation,
        highlight_ratio_pct: (high_lights_pct * 10.0).round() / 10.0,
        peak_detected_stops: (stops_est * 10.0).round() / 10.0,
        contrast_ratio: (contrast_ratio * 10.0).round() / 10.0,
        recommended_preset: recommended,
        available_presets: presets,
    }
}
