//! engine.rs — Moteur Ultra HDR, Gain Map, assemblage binaire ISO 21496-1 / Google Ultra HDR et simulation.

use image::codecs::jpeg::JpegEncoder;
use image::{ExtendedColorType, ImageError};
use rayon::prelude::*;

pub const LUM_WEIGHTS: [f32; 3] = [0.2126, 0.7152, 0.0722];

#[inline]
pub fn srgb_to_linear(x: f32) -> f32 {
    if x <= 0.04045 {
        x / 12.92
    } else {
        ((x.max(0.0) + 0.055) / 1.055).powf(2.4)
    }
}

#[inline]
pub fn linear_to_srgb(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    if x <= 0.0031308 {
        x * 12.92
    } else {
        1.055 * x.powf(1.0 / 2.4) - 0.055
    }
}

pub fn percentile_mut(v: &mut [f32], p: f32) -> f32 {
    if v.is_empty() {
        return 0.0;
    }
    v.sort_unstable_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let idx = ((v.len() - 1) as f32 * (p / 100.0)).round() as usize;
    v[idx.min(v.len() - 1)]
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TonalParams {
    pub contrast: f32,   // -1.0 .. +1.0 (défaut 0.0)
    pub whites: f32,     // -1.0 .. +1.0 (défaut 0.0)
    pub highlights: f32, // -1.0 .. +1.0 (défaut 0.0)
    pub shadows: f32,    // -1.0 .. +1.0 (défaut 0.0)
    pub blacks: f32,     // -1.0 .. +1.0 (défaut 0.0)
    pub saturation: f32, // -1.0 .. +1.0 (défaut 0.0)
    pub vibrance: f32,   // -1.0 .. +1.0 (défaut 0.0)
}

impl TonalParams {
    #[inline]
    pub fn has_any_adjustment(&self) -> bool {
        self.contrast.abs() > 1e-4
            || self.whites.abs() > 1e-4
            || self.highlights.abs() > 1e-4
            || self.shadows.abs() > 1e-4
            || self.blacks.abs() > 1e-4
            || self.saturation.abs() > 1e-4
            || self.vibrance.abs() > 1e-4
    }
}

impl Default for TonalParams {
    fn default() -> Self {
        Self {
            contrast: 0.0,
            whites: 0.0,
            highlights: 0.0,
            shadows: 0.0,
            blacks: 0.0,
            saturation: 0.0,
            vibrance: 0.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TonalZone {
    Blacks,
    Shadows,
    Highlights,
    Whites,
}

impl TonalZone {
    pub fn from_luminance_u8(lum: u8) -> Self {
        if lum < 51 {
            TonalZone::Blacks
        } else if lum < 128 {
            TonalZone::Shadows
        } else if lum < 204 {
            TonalZone::Highlights
        } else {
            TonalZone::Whites
        }
    }

    #[allow(dead_code)]
    pub fn label(&self) -> &'static str {
        match self {
            TonalZone::Blacks => "Noirs (Blacks)",
            TonalZone::Shadows => "Ombres (Shadows)",
            TonalZone::Highlights => "Hautes Lumières (Highlights)",
            TonalZone::Whites => "Blancs (Whites / HDR)",
        }
    }

    pub fn name_fr(&self) -> &'static str {
        match self {
            TonalZone::Blacks => "Noirs",
            TonalZone::Shadows => "Ombres",
            TonalZone::Highlights => "Hautes Lumières",
            TonalZone::Whites => "Blancs",
        }
    }
}

#[derive(Debug, Clone)]
pub struct LuminanceHistogram {
    pub bins: [u32; 256],
    pub max_count: u32,
    #[allow(dead_code)]
    pub black_pct: f32,
    #[allow(dead_code)]
    pub shadow_pct: f32,
    #[allow(dead_code)]
    pub highlight_pct: f32,
    #[allow(dead_code)]
    pub white_pct: f32,
}

/// Calcule l'histogramme de luminance en 256 bins avec répartition dans les 4 zones Lightroom.
pub fn compute_luminance_histogram(sdr_uint8: &[u8]) -> LuminanceHistogram {
    let mut bins = [0u32; 256];
    let n_pixels = sdr_uint8.len() / 3;
    if n_pixels == 0 {
        return LuminanceHistogram {
            bins,
            max_count: 1,
            black_pct: 0.0,
            shadow_pct: 0.0,
            highlight_pct: 0.0,
            white_pct: 0.0,
        };
    }

    let mut black_cnt = 0u32;
    let mut shadow_cnt = 0u32;
    let mut high_cnt = 0u32;
    let mut white_cnt = 0u32;

    for chunk in sdr_uint8.chunks_exact(3) {
        let lum = (chunk[0] as f32 * LUM_WEIGHTS[0]
            + chunk[1] as f32 * LUM_WEIGHTS[1]
            + chunk[2] as f32 * LUM_WEIGHTS[2])
            .round()
            .clamp(0.0, 255.0) as usize;
        bins[lum] += 1;

        if lum < 51 {
            black_cnt += 1;
        } else if lum < 128 {
            shadow_cnt += 1;
        } else if lum < 204 {
            high_cnt += 1;
        } else {
            white_cnt += 1;
        }
    }

    let max_count = *bins.iter().max().unwrap_or(&1).max(&1);
    let total = n_pixels as f32;

    LuminanceHistogram {
        bins,
        max_count,
        black_pct: (black_cnt as f32 / total) * 100.0,
        shadow_pct: (shadow_cnt as f32 / total) * 100.0,
        highlight_pct: (high_cnt as f32 / total) * 100.0,
        white_pct: (white_cnt as f32 / total) * 100.0,
    }
}

/// Lissage spatial 2D séparable pour éliminer les halos de Gain Map sur transitions contrastées.
pub fn apply_anti_halo_filter(gain: &mut [f32], width: u32, height: u32, strength: f32) {
    if strength <= 0.01 || width < 4 || height < 4 {
        return;
    }
    let radius = ((width.min(height) as f32 * 0.012 * strength).round() as i32).clamp(1, 10);
    let mut temp = gain.to_vec();
    let w = width as usize;
    let h = height as usize;

    // Passe horizontale
    temp.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
        let orig_row = &gain[y * w..(y + 1) * w];
        for x in 0..w {
            let x_start = (x as i32 - radius).max(0) as usize;
            let x_end = (x as i32 + radius).min(w as i32 - 1) as usize;
            let mut sum = 0.0;
            let count = (x_end - x_start + 1) as f32;
            for ix in x_start..=x_end {
                sum += orig_row[ix];
            }
            row[x] = sum / count;
        }
    });

    // Passe verticale et fusion progressive
    let alpha = (strength * 0.65).clamp(0.0, 0.80);
    gain.par_chunks_mut(w).enumerate().for_each(|(y, out_row)| {
        for x in 0..w {
            let y_start = (y as i32 - radius).max(0) as usize;
            let y_end = (y as i32 + radius).min(h as i32 - 1) as usize;
            let mut sum = 0.0;
            let count = (y_end - y_start + 1) as f32;
            for iy in y_start..=y_end {
                sum += temp[iy * w + x];
            }
            let blurred = sum / count;
            out_row[x] = out_row[x] * (1.0 - alpha) + blurred * alpha;
        }
    });
}

pub struct HdrGainMapResult {
    pub hdr_nits: Vec<f32>,   // (width * height * 3)
    pub gain_log2: Vec<f32>,  // (width * height)
    pub gain_min: f32,
    pub gain_max: f32,
    pub sdr_processed: Option<Vec<u8>>,
}

/// Modifie la saturation et la vibrance d'un tampon SDR RGB uint8 de manière ultra-rapide et multithreadée.
/// - Saturation : ajuste proportionnellement l'intensité chromatique globale (-100% à +100%).
/// - Vibrance : protège les tons chair et les couleurs déjà saturées tout en rehaussant les teintes douces (-100% à +100%).
#[allow(dead_code)]
pub fn apply_vibrance_saturation(sdr_uint8: &[u8], saturation: f32, vibrance: f32) -> Vec<u8> {
    if saturation == 0.0 && vibrance == 0.0 {
        return sdr_uint8.to_vec();
    }
    let mut out = sdr_uint8.to_vec();
    out.par_chunks_exact_mut(3).for_each(|pixel| {
        let r = pixel[0] as f32 / 255.0;
        let g = pixel[1] as f32 / 255.0;
        let b = pixel[2] as f32 / 255.0;

        let l = r * 0.2126 + g * 0.7152 + b * 0.0722;
        let max_c = r.max(g).max(b);
        let min_c = r.min(g).min(b);
        let sat = if max_c > 1e-4 {
            (max_c - min_c) / max_c
        } else {
            0.0
        };

        let vib_mod = if vibrance > 0.0 {
            vibrance * (1.0 - sat) * 1.3
        } else {
            vibrance * sat
        };

        let total_mult = (1.0 + saturation + vib_mod).max(0.0);

        let r_out = (l + (r - l) * total_mult).clamp(0.0, 1.0);
        let g_out = (l + (g - l) * total_mult).clamp(0.0, 1.0);
        let b_out = (l + (b - l) * total_mult).clamp(0.0, 1.0);

        pixel[0] = (r_out * 255.0 + 0.5) as u8;
        pixel[1] = (g_out * 255.0 + 0.5) as u8;
        pixel[2] = (b_out * 255.0 + 0.5) as u8;
    });
    out
}

/// Applique les ajustements tonals (Contraste, Blancs, Hautes Lumières, Ombres, Noirs)
/// et chromatiques (Saturation, Vibrance) sur le tampon SDR de manière ultra-rapide et multithreadée.
pub fn apply_tonal_and_color_adjustments(sdr_uint8: &[u8], tonal: &TonalParams) -> Vec<u8> {
    if !tonal.has_any_adjustment() {
        return sdr_uint8.to_vec();
    }
    let mut out = sdr_uint8.to_vec();
    out.par_chunks_exact_mut(3).for_each(|pixel| {
        let r = pixel[0] as f32 / 255.0;
        let g = pixel[1] as f32 / 255.0;
        let b = pixel[2] as f32 / 255.0;

        let lum = r * LUM_WEIGHTS[0] + g * LUM_WEIGHTS[1] + b * LUM_WEIGHTS[2];
        let (r1, g1, b1, l1) = if lum > 1e-5 {
            // Courbe tonale paramétrique (Contraste, Blancs, Hautes Lumières, Ombres, Noirs)
            // 1. Contraste : S-curve sinusoïdale continue autour du pivot moyen
            let d_contrast = -tonal.contrast * 0.22 * (std::f32::consts::TAU * lum).sin();

            // 2. Noirs : module l'ancrage des ombres très profondes (lum < 0.25)
            let d_blacks = if lum < 0.25 {
                let wb = (1.0 - lum / 0.25) * (1.0 - lum / 0.25);
                tonal.blacks * 0.20 * wb
            } else {
                0.0
            };

            // 3. Ombres : élévation / densification des pénombres (lum < 0.55)
            let d_shadows = if lum < 0.55 {
                let ws = (std::f32::consts::PI * (lum / 0.55)).sin();
                tonal.shadows * 0.30 * ws
            } else {
                0.0
            };

            // 4. Hautes Lumières : récupération ou rehaussement des teintes hautes (lum > 0.45)
            let d_highlights = if lum > 0.45 {
                let wh = (std::f32::consts::PI * ((lum - 0.45) / 0.55)).sin();
                tonal.highlights * 0.30 * wh
            } else {
                0.0
            };

            // 5. Blancs : expansion des crêtes lumineuses (lum > 0.70)
            let d_whites = if lum > 0.70 {
                let ww = ((lum - 0.70) / 0.30) * ((lum - 0.70) / 0.30);
                tonal.whites * 0.25 * ww
            } else {
                0.0
            };

            let lum_adj = (lum + d_contrast + d_blacks + d_shadows + d_highlights + d_whites).clamp(0.0, 1.0);
            let s_tone = lum_adj / lum;
            let r_tone = (r * s_tone).clamp(0.0, 1.0);
            let g_tone = (g * s_tone).clamp(0.0, 1.0);
            let b_tone = (b * s_tone).clamp(0.0, 1.0);
            let l_tone = r_tone * LUM_WEIGHTS[0] + g_tone * LUM_WEIGHTS[1] + b_tone * LUM_WEIGHTS[2];
            (r_tone, g_tone, b_tone, l_tone)
        } else {
            (r, g, b, lum)
        };

        // 6. Modulation chromatique (Saturation & Vibrance)
        let max_c = r1.max(g1).max(b1);
        let min_c = r1.min(g1).min(b1);
        let sat = if max_c > 1e-4 {
            (max_c - min_c) / max_c
        } else {
            0.0
        };

        let vib_mod = if tonal.vibrance > 0.0 {
            tonal.vibrance * (1.0 - sat).powf(1.5) * 1.5
        } else {
            tonal.vibrance * sat * 0.9
        };

        let total_mult = (1.0 + tonal.saturation * 1.2 + vib_mod).max(0.0);

        let r_out = (l1 + (r1 - l1) * total_mult).clamp(0.0, 1.0);
        let g_out = (l1 + (g1 - l1) * total_mult).clamp(0.0, 1.0);
        let b_out = (l1 + (b1 - l1) * total_mult).clamp(0.0, 1.0);

        pixel[0] = (r_out * 255.0 + 0.5) as u8;
        pixel[1] = (g_out * 255.0 + 0.5) as u8;
        pixel[2] = (b_out * 255.0 + 0.5) as u8;
    });
    out
}

/// Calcule l'image HDR (en nits) et la Gain Map (log2) associée à l'image SDR avec contrôle Tonal Pro et Anti-Halo.
pub fn compute_hdr_and_gain_map(
    sdr_uint8: &[u8],
    hdr_lin_input: Option<&[f32]>,
    is_native_hdr: bool,
    width: u32,
    height: u32,
    peak_nits: f32,
    sdr_white: f32,
    shadow_boost: f32,
    hi_gamma: f32,
    highlight_threshold: f32,
    tonal: TonalParams,
    anti_halo_strength: f32,
) -> HdrGainMapResult {
    let n_pixels = (width * height) as usize;
    let base_peak_boost = (peak_nits / sdr_white).max(1.05);

    let processed_sdr = if tonal.has_any_adjustment() {
        Some(apply_tonal_and_color_adjustments(sdr_uint8, &tonal))
    } else {
        None
    };
    let sdr_source: &[u8] = match &processed_sdr {
        Some(s) => s.as_slice(),
        None => sdr_uint8,
    };

    // Modulation Tonal Pro (Style Lightroom / Photoshop) amplifiée et responsive :
    // 1. Blancs (Whites) : module directement le pic de boost (jusqu'à x2.7 à +200%)
    let effective_peak_boost = (base_peak_boost * (1.0 + tonal.whites * 0.85)).max(1.02);
    // 2. Hautes Lumières (Highlights) : module la progressivité du seuil et du gamma
    let thresh = (highlight_threshold - tonal.highlights * 0.35).clamp(0.01, 0.98);
    let effective_gamma = (hi_gamma * (1.0 - tonal.highlights * 0.45)).clamp(0.1, 4.5);
    // 3. Ombres (Shadows) : module l'élévation ou la densification des pénombres
    let effective_s_boost = (shadow_boost + tonal.shadows * 0.85).max(0.4);

    // Calcul de p99_headroom si native_hdr
    let (use_raw_mod, p99_headroom) = if is_native_hdr && hdr_lin_input.is_some() {
        let raw_lin = hdr_lin_input.unwrap();
        let mut headrooms = Vec::with_capacity(n_pixels);
        for i in 0..n_pixels {
            let r_sdr = srgb_to_linear(sdr_source[i * 3] as f32 / 255.0);
            let g_sdr = srgb_to_linear(sdr_source[i * 3 + 1] as f32 / 255.0);
            let b_sdr = srgb_to_linear(sdr_source[i * 3 + 2] as f32 / 255.0);
            let l_sdr = (r_sdr * LUM_WEIGHTS[0] + g_sdr * LUM_WEIGHTS[1] + b_sdr * LUM_WEIGHTS[2]).clamp(0.0, 1.0);

            let r_raw = raw_lin[i * 3].max(0.0);
            let g_raw = raw_lin[i * 3 + 1].max(0.0);
            let b_raw = raw_lin[i * 3 + 2].max(0.0);
            let l_raw = (r_raw * LUM_WEIGHTS[0] + g_raw * LUM_WEIGHTS[1] + b_raw * LUM_WEIGHTS[2]).max(0.0);

            headrooms.push((l_raw - l_sdr).max(0.0));
        }
        let p99 = percentile_mut(&mut headrooms, 99.5);
        (p99 > 1e-3, p99)
    } else {
        (false, 0.0)
    };

    let mut gain_log2 = vec![0.0f32; n_pixels];
    let mut hdr_nits = vec![0.0f32; n_pixels * 3];

    // Calcul multithreadé parallèle avec Rayon
    gain_log2
        .par_chunks_mut(1024)
        .zip(hdr_nits.par_chunks_mut(1024 * 3))
        .enumerate()
        .for_each(|(chunk_idx, (gain_chunk, hdr_chunk))| {
            let start_pixel = chunk_idx * 1024;
            for (i, gain_val) in gain_chunk.iter_mut().enumerate() {
                let pixel_idx = start_pixel + i;
                let r_sdr = srgb_to_linear(sdr_source[pixel_idx * 3] as f32 / 255.0);
                let g_sdr = srgb_to_linear(sdr_source[pixel_idx * 3 + 1] as f32 / 255.0);
                let b_sdr = srgb_to_linear(sdr_source[pixel_idx * 3 + 2] as f32 / 255.0);
                let l_sdr = (r_sdr * LUM_WEIGHTS[0] + g_sdr * LUM_WEIGHTS[1] + b_sdr * LUM_WEIGHTS[2]).clamp(0.0, 1.0);

                let t = ((l_sdr - thresh) / (1.0 - thresh).max(1e-4)).clamp(0.0, 1.0);
                let w_high = t * t * (3.0 - 2.0 * t);

                let highlight_weight = if use_raw_mod {
                    let raw_lin = hdr_lin_input.unwrap();
                    let r_raw = raw_lin[pixel_idx * 3].max(0.0);
                    let g_raw = raw_lin[pixel_idx * 3 + 1].max(0.0);
                    let b_raw = raw_lin[pixel_idx * 3 + 2].max(0.0);
                    let l_raw = (r_raw * LUM_WEIGHTS[0] + g_raw * LUM_WEIGHTS[1] + b_raw * LUM_WEIGHTS[2]).max(0.0);
                    let raw_headroom = (l_raw - l_sdr).max(0.0);
                    let raw_mod = (raw_headroom / p99_headroom).clamp(0.0, 1.0);
                    (w_high * (0.6 * w_high + 0.4 * raw_mod)).powf(effective_gamma)
                } else {
                    w_high.powf(effective_gamma)
                };

                // Interpolation logarithmique (en EV / stops) pour une progressivité
                // uniforme et indépendante de peak_nits, garantissant que les très hautes
                // lumières conservent toute leur dynamique sans écrasement au seuil
                let log_min = effective_s_boost.log2();
                let log_max = effective_peak_boost.log2().max(log_min + 0.001);
                let gain_stops = log_min + (log_max - log_min) * highlight_weight;
                let mut boost = gain_stops.exp2().clamp(0.3, effective_peak_boost);

                // 4. Noirs (Blacks) : modulation du contraste dans les basses lumières (l_sdr < 0.30)
                if tonal.blacks != 0.0 && l_sdr < 0.30 {
                    let black_weight = (1.0 - l_sdr / 0.30).clamp(0.0, 1.0);
                    let black_mod = (1.0 + tonal.blacks * 0.70 * black_weight).max(0.15);
                    boost = (boost * black_mod).max(0.15);
                }

                // 5. Contraste Global S-Curve : modifie la transition dynamique autour du pivot moyen (0.45)
                if tonal.contrast != 0.0 {
                    let contrast_factor = 1.0 + (l_sdr - 0.45) * tonal.contrast * 0.90;
                    boost = (boost * contrast_factor.max(0.15)).clamp(0.15, effective_peak_boost * 3.0);
                }

                hdr_chunk[i * 3] = r_sdr * sdr_white * boost;
                hdr_chunk[i * 3 + 1] = g_sdr * sdr_white * boost;
                hdr_chunk[i * 3 + 2] = b_sdr * sdr_white * boost;

                *gain_val = boost.log2().max(0.0);
            }
        });

    // 5. Filtrage spatial Anti-Halo bilatéral sur la Gain Map si activé
    if anti_halo_strength > 0.01 {
        apply_anti_halo_filter(&mut gain_log2, width, height, anti_halo_strength);
        // Réactualisation de hdr_nits avec la Gain Map adoucie
        hdr_nits
            .par_chunks_mut(1024 * 3)
            .zip(gain_log2.par_chunks(1024))
            .enumerate()
            .for_each(|(chunk_idx, (hdr_chunk, gain_chunk))| {
                let start_pixel = chunk_idx * 1024;
                for (i, &g) in gain_chunk.iter().enumerate() {
                    let pixel_idx = start_pixel + i;
                    let r_sdr = srgb_to_linear(sdr_source[pixel_idx * 3] as f32 / 255.0);
                    let g_sdr = srgb_to_linear(sdr_source[pixel_idx * 3 + 1] as f32 / 255.0);
                    let b_sdr = srgb_to_linear(sdr_source[pixel_idx * 3 + 2] as f32 / 255.0);
                    let boost = g.exp2();
                    hdr_chunk[i * 3] = r_sdr * sdr_white * boost;
                    hdr_chunk[i * 3 + 1] = g_sdr * sdr_white * boost;
                    hdr_chunk[i * 3 + 2] = b_sdr * sdr_white * boost;
                }
            });
    }

    let mut gain_min = f32::MAX;
    let mut gain_max = f32::MIN;
    for &g in &gain_log2 {
        if g < gain_min {
            gain_min = g;
        }
        if g > gain_max {
            gain_max = g;
        }
    }
    if gain_max < 0.05 {
        gain_max = effective_peak_boost.log2();
    }

    HdrGainMapResult {
        hdr_nits,
        gain_log2,
        gain_min,
        gain_max,
        sdr_processed: processed_sdr,
    }
}

/// Quantifie la Gain Map log2 en JPEG niveaux de gris 8-bit.
pub fn encode_gain_map_jpeg(
    gain_log2: &[f32],
    gain_min: f32,
    gain_max: f32,
    width: u32,
    height: u32,
    quality: u8,
) -> Result<Vec<u8>, ImageError> {
    let gain_range = (gain_max - gain_min).max(1e-3);
    let n_pixels = (width * height) as usize;
    let mut gain_u8 = vec![0u8; n_pixels];

    gain_u8
        .par_chunks_mut(1024)
        .zip(gain_log2.par_chunks(1024))
        .for_each(|(u8_chunk, log2_chunk)| {
            for (out, &g) in u8_chunk.iter_mut().zip(log2_chunk.iter()) {
                let v = ((g - gain_min) / gain_range * 255.0 + 0.5).clamp(0.0, 255.0);
                *out = v as u8;
            }
        });

    let mut buf = Vec::new();
    let mut encoder = JpegEncoder::new_with_quality(&mut buf, quality);
    encoder.encode(&gain_u8, width, height, ExtendedColorType::L8)?;
    Ok(buf)
}

/// Encodage JPEG standard d'une image RGB uint8.
pub fn encode_rgb_jpeg(
    rgb: &[u8],
    width: u32,
    height: u32,
    quality: u8,
) -> Result<Vec<u8>, ImageError> {
    let mut buf = Vec::new();
    let mut encoder = JpegEncoder::new_with_quality(&mut buf, quality);
    encoder.encode(rgb, width, height, ExtendedColorType::Rgb8)?;
    Ok(buf)
}

/// Encapsule un document XML dans un marqueur JPEG APP1 (0xFF 0xE1) avec espace de nom XMP.
pub fn wrap_xmp_payload(xml: &str) -> Vec<u8> {
    let mut payload = b"http://ns.adobe.com/xap/1.0/\0".to_vec();
    payload.extend_from_slice(xml.as_bytes());
    let length = (payload.len() + 2) as u16;

    let mut out = Vec::with_capacity(4 + payload.len());
    out.extend_from_slice(&[0xff, 0xe1]); // Marqueur APP1
    out.extend_from_slice(&length.to_be_bytes()); // Longueur en Big Endian
    out.extend_from_slice(&payload);
    out
}

/// Génère le segment XMP APP1 pour l'image primaire conforme Google Ultra HDR v1.1 et GContainer.
pub fn build_primary_xmp(gain_min: f32, gain_max: f32, hdr_capacity: f32, gainmap_size: usize) -> Vec<u8> {
    let xml = format!(
        "<?xpacket begin=\"\u{feff}\" id=\"W5M0MpCehiHzreSzNTczkc9d\"?>\n\
<x:xmpmeta xmlns:x=\"adobe:ns:meta/\" x:xmptk=\"XMP Core 5.5.0\">\n\
 <rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\">\n\
  <rdf:Description rdf:about=\"\"\n\
    xmlns:Container=\"http://ns.google.com/photos/1.0/container/\"\n\
    xmlns:Item=\"http://ns.google.com/photos/1.0/container/item/\"\n\
    xmlns:hdrgm=\"http://ns.adobe.com/hdr-gain-map/1.0/\"\n\
   hdrgm:Version=\"1.0\"\n\
   hdrgm:GainMapMin=\"{gain_min:.6}\"\n\
   hdrgm:GainMapMax=\"{gain_max:.6}\"\n\
   hdrgm:Gamma=\"1.0\"\n\
   hdrgm:OffsetSDR=\"0.015625\"\n\
   hdrgm:OffsetHDR=\"0.015625\"\n\
   hdrgm:HDRCapacityMin=\"0.000000\"\n\
   hdrgm:HDRCapacityMax=\"{hdr_capacity:.6}\"\n\
   hdrgm:BaseRenditionIsHDR=\"False\">\n\
   <Container:Directory>\n\
    <rdf:Seq>\n\
     <rdf:li rdf:parseType=\"Resource\">\n\
      <Container:Item Item:Semantic=\"Primary\" Item:Mime=\"image/jpeg\"/>\n\
     </rdf:li>\n\
     <rdf:li rdf:parseType=\"Resource\">\n\
      <Container:Item Item:Semantic=\"GainMap\" Item:Mime=\"image/jpeg\" Item:Length=\"{gainmap_size}\"/>\n\
     </rdf:li>\n\
    </rdf:Seq>\n\
   </Container:Directory>\n\
  </rdf:Description>\n\
 </rdf:RDF>\n\
</x:xmpmeta>\n\
<?xpacket end=\"w\"?>"
    );
    wrap_xmp_payload(&xml)
}

/// Génère le segment XMP APP1 de la Gain Map conforme Adobe / Google Ultra HDR v1.1.
pub fn build_xmp_hdrgm(gain_min: f32, gain_max: f32, hdr_capacity: f32) -> Vec<u8> {
    let xml = format!(
        "<?xpacket begin=\"\u{feff}\" id=\"W5M0MpCehiHzreSzNTczkc9d\"?>\n\
<x:xmpmeta xmlns:x=\"adobe:ns:meta/\" x:xmptk=\"XMP Core 5.5.0\">\n\
 <rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\">\n\
  <rdf:Description rdf:about=\"\"\n\
    xmlns:hdrgm=\"http://ns.adobe.com/hdr-gain-map/1.0/\"\n\
   hdrgm:Version=\"1.0\"\n\
   hdrgm:GainMapMin=\"{gain_min:.6}\"\n\
   hdrgm:GainMapMax=\"{gain_max:.6}\"\n\
   hdrgm:Gamma=\"1.0\"\n\
   hdrgm:OffsetSDR=\"0.015625\"\n\
   hdrgm:OffsetHDR=\"0.015625\"\n\
   hdrgm:HDRCapacityMin=\"0.000000\"\n\
   hdrgm:HDRCapacityMax=\"{hdr_capacity:.6}\"\n\
   hdrgm:BaseRenditionIsHDR=\"False\"/>\n\
 </rdf:RDF>\n\
</x:xmpmeta>\n\
<?xpacket end=\"w\"?>"
    );
    wrap_xmp_payload(&xml)
}

/// Génère le segment CIPA DC-007 Multi-Picture Format APP2 (90 octets).
pub fn build_mpf_app2(primary_size: u32, gainmap_size: u32, gainmap_offset: u32) -> Vec<u8> {
    let mut data = Vec::with_capacity(86);
    data.extend_from_slice(b"MPF\0");
    // En-tête TIFF Little Endian : 'II' + 42 + offset IFD 8
    data.extend_from_slice(b"II");
    data.extend_from_slice(&42u16.to_le_bytes());
    data.extend_from_slice(&8u32.to_le_bytes());

    // Nombre d'entrées IFD : 3
    data.extend_from_slice(&3u16.to_le_bytes());

    // 0xB000 (MPF Version): type 7 (UNDEFINED), count 4, value '0100'
    data.extend_from_slice(&0xB000u16.to_le_bytes());
    data.extend_from_slice(&7u16.to_le_bytes());
    data.extend_from_slice(&4u32.to_le_bytes());
    data.extend_from_slice(b"0100");

    // 0xB001 (Number of Images): type 4 (LONG), count 1, value 2
    data.extend_from_slice(&0xB001u16.to_le_bytes());
    data.extend_from_slice(&4u16.to_le_bytes());
    data.extend_from_slice(&1u32.to_le_bytes());
    data.extend_from_slice(&2u32.to_le_bytes());

    // 0xB002 (MP Entry offset): type 7 (UNDEFINED), count 32, offset 50
    data.extend_from_slice(&0xB002u16.to_le_bytes());
    data.extend_from_slice(&7u16.to_le_bytes());
    data.extend_from_slice(&32u32.to_le_bytes());
    data.extend_from_slice(&50u32.to_le_bytes());

    // Offset prochain IFD : 0
    data.extend_from_slice(&0u32.to_le_bytes());

    // MP Entry 1 (Primary Image)
    data.extend_from_slice(&0x00030000u32.to_le_bytes()); // Type
    data.extend_from_slice(&primary_size.to_le_bytes());
    data.extend_from_slice(&0u32.to_le_bytes()); // Offset 0
    data.extend_from_slice(&0u16.to_le_bytes());
    data.extend_from_slice(&0u16.to_le_bytes());

    // MP Entry 2 (Gain Map Image)
    data.extend_from_slice(&0x00000000u32.to_le_bytes()); // Type
    data.extend_from_slice(&gainmap_size.to_le_bytes());
    data.extend_from_slice(&gainmap_offset.to_le_bytes());
    data.extend_from_slice(&0u16.to_le_bytes());
    data.extend_from_slice(&0u16.to_le_bytes());

    assert_eq!(data.len(), 86);

    let mut out = Vec::with_capacity(90);
    out.extend_from_slice(&[0xff, 0xe2]); // Marqueur APP2
    out.extend_from_slice(&88u16.to_be_bytes()); // Longueur 88 Big Endian
    out.extend_from_slice(&data);
    out
}

/// Assemble le conteneur JPEG Ultra HDR complet (ISO 21496-1 / Google Ultra HDR) avec préservation EXIF.
pub fn assemble_ultra_hdr_jpeg(
    base_jpeg: &[u8],
    gainmap_raw_jpeg: &[u8],
    gain_min: f32,
    gain_max: f32,
    peak_nits: f32,
    sdr_white: f32,
    exif_segment: Option<&[u8]>,
) -> Vec<u8> {
    let hdr_capacity = (peak_nits / sdr_white).max(1.01).log2();
    let gm_xmp_app1 = build_xmp_hdrgm(gain_min, gain_max, hdr_capacity);

    // Gain map JPEG avec injection du XMP après SOI (0xFF 0xD8)
    let mut gainmap_jpeg = Vec::with_capacity(gainmap_raw_jpeg.len() + gm_xmp_app1.len());
    gainmap_jpeg.extend_from_slice(&gainmap_raw_jpeg[..2]);
    gainmap_jpeg.extend_from_slice(&gm_xmp_app1);
    gainmap_jpeg.extend_from_slice(&gainmap_raw_jpeg[2..]);

    let g = gainmap_jpeg.len();
    let primary_xmp = build_primary_xmp(gain_min, gain_max, hdr_capacity, g);

    let e = exif_segment.map(|s| s.len()).unwrap_or(0);
    let a = primary_xmp.len();
    let b = base_jpeg.len();

    // Spécification CIPA DC-007 MPF :
    // - primary_size : taille totale de la première image du conteneur (de 0 à l'EOI de l'image de base).
    // - gainmap_offset : distance relative depuis le début de l'en-tête TIFF MPF ("II") jusqu'au SOI de la Gain Map.
    // Dans final_jpeg :
    //   L'en-tête "II" est à l'offset : 2 (SOI) + e (EXIF) + a (XMP) + 8 = e + a + 10.
    //   La Gain Map commence à l'offset : 2 + e + a + 90 (MPF) + (b - 2) = e + a + b + 90.
    //   Offset relatif = (e + a + b + 90) - (e + a + 10) = b + 80 (Strictement indépendant de e et a !).
    let primary_size = (e + a + b + 90) as u32;
    let gainmap_offset = (b + 80) as u32;

    let mpf_app2 = build_mpf_app2(primary_size, g as u32, gainmap_offset);

    let mut final_jpeg = Vec::with_capacity(2 + e + a + mpf_app2.len() + b - 2 + g);
    final_jpeg.extend_from_slice(&[0xff, 0xd8]);
    if let Some(exif) = exif_segment {
        final_jpeg.extend_from_slice(exif);
    }
    final_jpeg.extend_from_slice(&primary_xmp);
    final_jpeg.extend_from_slice(&mpf_app2);
    final_jpeg.extend_from_slice(&base_jpeg[2..]);
    final_jpeg.extend_from_slice(&gainmap_jpeg);

    final_jpeg
}

/// Simule le rendu HDR sur écran SDR standard avec protection totale contre le cramage des hautes lumières.
///
/// Contrairement à un simple décalage EV qui surexpose et brûle les blancs, cet algorithme EDR (Extended Dynamic Range) :
/// - Préserve à 100% l'intégrité des tons sombres, tons moyens et blanc diffus SDR (base_x <= 1.0).
/// - Module la puissance du headroom HDR via `hdr_boost` (0.0 = SDR pur, 1.0 = plein HDR, 1.5 = pop spéculaire).
/// - Compresse asymptotiquement les crêtes lumineuses (> seuil `highlight_rolloff`) via une courbe rationnelle C1-continue.
/// Aucun pixel ne peut être écrêté ou brûlé à (255, 255, 255), même à 10 000 nits.
pub fn simulate_hdr_on_sdr(
    hdr_nits: &[f32],
    width: u32,
    height: u32,
    sdr_white: f32,
    hdr_boost: f32,
    highlight_rolloff: f32,
    tone_map_mode: &str,
) -> Vec<u8> {
    let sdr_ref = sdr_white.max(50.0);
    let knee = highlight_rolloff.clamp(0.40, 0.95);
    let m = 1.0 - knee;
    let boost = hdr_boost.max(0.0);

    let n_pixels = (width * height) as usize;
    let mut out_u8 = vec![0u8; n_pixels * 3];

    let wp = (1.5 + boost * 2.0).max(1.2);
    let wp2 = wp * wp;

    out_u8
        .par_chunks_mut(1024 * 3)
        .zip(hdr_nits.par_chunks(1024 * 3))
        .for_each(|(out_chunk, nits_chunk)| {
            for (out_pixel, nits_pixel) in out_chunk.chunks_exact_mut(3).zip(nits_chunk.chunks_exact(3)) {
                for c in 0..3 {
                    let base_x = (nits_pixel[c] / sdr_ref).max(0.0);
                    // Règle d'or EDR : tons sombres, moyens et blanc diffus (base_x <= 1.0)
                    // sont strictement ancrés et ne peuvent JAMAIS cramer.
                    // Seul le surplus de dynamique HDR (base_x > 1.0) est amplifié par hdr_boost.
                    let x = if base_x <= 1.0 {
                        base_x
                    } else {
                        1.0 + (base_x - 1.0) * boost
                    };

                    let tm = match tone_map_mode {
                        // Mode Éclat Dynamique (Vivid Pop) :
                        // Parfaite fidélité sur les tons moyens/sombres (x <= knee),
                        // puis compression rationnelle progressive à grand éclat spéculaire sans écrêtage plat.
                        "vivid" | "eclat" | "hdr_pop" => {
                            if x <= knee {
                                x
                            } else {
                                let y = x - knee;
                                let s = 1.0 + m * 1.5;
                                knee + m * (y / (y + s))
                            }
                        }
                        // Mode Naturel : C1-continu, préserve 100% de la texture des hautes lumières
                        "natural" | "naturel" | "preserved" => {
                            if x <= knee {
                                x
                            } else {
                                let y = x - knee;
                                let s = 1.8 + m * 2.0;
                                knee + m * (y / (y + s))
                            }
                        }
                        // Mode ACES Filmic normalisé
                        "aces" | "filmic" => {
                            let num = x * (2.51 * x + 0.03);
                            let den = (x * (2.43 * x + 0.59) + 0.14).max(1e-6);
                            (num / den).clamp(0.0, 1.0)
                        }
                        // Mode Reinhard avec point blanc adaptatif
                        "reinhard" => {
                            let num = x * (1.0 + x / wp2);
                            let den = (1.0 + x).max(1e-6);
                            (num / den).clamp(0.0, 1.0)
                        }
                        // Mode Linéaire direct (écrêtage à 1.0, utilisé pour affichage matériel HDR ou diagnostic)
                        _ => x.clamp(0.0, 1.0),
                    };

                    let srgb = linear_to_srgb(tm.clamp(0.0, 1.0));
                    out_pixel[c] = (srgb * 255.0 + 0.5).clamp(0.0, 255.0) as u8;
                }
            }
        });

    out_u8
}

/// Calcule le facteur de pondération de dynamique d'affichage W selon la norme ISO 21496-1.
///
/// Si l'écran a une capacité d'affichage inférieure à la luminance cible d'export
/// (ex: cible 10 000 nits sur un écran 400 nits), cette formule adapte proportionnellement
/// la dynamique perçue à l'écran pour éviter tout cramage des hautes lumières en prévisualisation,
/// tout en conservant le fichier exporté à pleine puissance mathématique.
pub fn compute_display_headroom_weight(
    content_peak_nits: f32,
    display_peak_nits: f32,
    sdr_white: f32,
) -> f32 {
    let sdr_ref = sdr_white.max(50.0);
    let h_content = (content_peak_nits / sdr_ref).max(1.001).log2();
    let h_display = (display_peak_nits / sdr_ref).max(1.001).log2();
    if h_content <= 1e-4 {
        1.0
    } else {
        (h_display / h_content).clamp(0.0, 1.0)
    }
}

/// Adapte les luminances HDR (en nits) aux capacités réelles de l'écran de prévisualisation (ISO 21496-1).
#[allow(dead_code)]
pub fn adapt_hdr_nits_for_display(
    hdr_nits: &[f32],
    gain_log2: &[f32],
    sdr_uint8: &[u8],
    width: u32,
    height: u32,
    content_peak_nits: f32,
    display_peak_nits: f32,
    sdr_white: f32,
) -> Vec<f32> {
    let weight = compute_display_headroom_weight(content_peak_nits, display_peak_nits, sdr_white);
    if (weight - 1.0).abs() < 1e-4 {
        return hdr_nits.to_vec();
    }

    let n_pixels = (width * height) as usize;
    let mut adapted = vec![0.0f32; n_pixels * 3];

    adapted
        .par_chunks_mut(1024 * 3)
        .zip(gain_log2.par_chunks(1024))
        .enumerate()
        .for_each(|(chunk_idx, (out_chunk, gain_chunk))| {
            let start_pixel = chunk_idx * 1024;
            for (i, &g) in gain_chunk.iter().enumerate() {
                let pixel_idx = start_pixel + i;
                let r_sdr = srgb_to_linear(sdr_uint8[pixel_idx * 3] as f32 / 255.0);
                let g_sdr = srgb_to_linear(sdr_uint8[pixel_idx * 3 + 1] as f32 / 255.0);
                let b_sdr = srgb_to_linear(sdr_uint8[pixel_idx * 3 + 2] as f32 / 255.0);

                let display_boost = (g * weight).exp2();
                out_chunk[i * 3] = r_sdr * sdr_white * display_boost;
                out_chunk[i * 3 + 1] = g_sdr * sdr_white * display_boost;
                out_chunk[i * 3 + 2] = b_sdr * sdr_white * display_boost;
            }
        });

    adapted
}

/// Approximation polynomiale ultra-rapide de la colormap Turbo (Google).
#[inline]
fn turbo_colormap(x: f32) -> [u8; 3] {
    let x = x.clamp(0.0, 1.0);
    let r = 0.1357 + x * (4.5974 - x * (42.3277 - x * (130.5887 - x * (150.5667 - x * 58.1375))));
    let g = 0.0914 + x * (2.1856 + x * (4.8052 - x * (14.0195 - x * (4.2109 - x * 2.7747))));
    let b = 0.1067 + x * (12.5732 - x * (62.4920 - x * (114.6126 - x * (78.3658 - x * 15.5457))));
    [
        (r.clamp(0.0, 1.0) * 255.0 + 0.5) as u8,
        (g.clamp(0.0, 1.0) * 255.0 + 0.5) as u8,
        (b.clamp(0.0, 1.0) * 255.0 + 0.5) as u8,
    ]
}

/// Carte thermique en fausses couleurs visualisant la répartition des nits.
pub fn generate_nits_heatmap(
    hdr_nits: &[f32],
    width: u32,
    height: u32,
    sdr_white: f32,
    peak_nits: f32,
) -> Vec<u8> {
    let n_pixels = (width * height) as usize;
    let mut out_u8 = vec![0u8; n_pixels * 3];
    let max_stops = (peak_nits / sdr_white).max(1.0).log2();

    out_u8
        .par_chunks_mut(1024 * 3)
        .zip(hdr_nits.par_chunks(1024 * 3))
        .for_each(|(out_chunk, nits_chunk)| {
            for (out_pixel, nits_pixel) in out_chunk.chunks_exact_mut(3).zip(nits_chunk.chunks_exact(3)) {
                let lum = nits_pixel[0] * LUM_WEIGHTS[0]
                    + nits_pixel[1] * LUM_WEIGHTS[1]
                    + nits_pixel[2] * LUM_WEIGHTS[2];

                if lum <= sdr_white * 1.05 {
                    // Zone SDR : atténuée en niveaux de gris
                    let gray = ((lum / (sdr_white * 1.05)).clamp(0.0, 1.0) * 60.0) as u8;
                    out_pixel[0] = gray;
                    out_pixel[1] = gray;
                    out_pixel[2] = gray;
                } else {
                    let ratio = lum / sdr_white;
                    let stops_above = ratio.log2();
                    let norm = (stops_above / max_stops).clamp(0.0, 1.0);
                    let [r, g, b] = turbo_colormap(norm);
                    out_pixel[0] = r;
                    out_pixel[1] = g;
                    out_pixel[2] = b;
                }
            }
        });

    out_u8
}
