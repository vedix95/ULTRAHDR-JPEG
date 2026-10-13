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
    pub whites: f32,     // -1.0 .. +1.0 (défaut 0.0)
    pub highlights: f32, // -1.0 .. +1.0 (défaut 0.0)
    pub shadows: f32,    // -1.0 .. +1.0 (défaut 0.0)
    pub blacks: f32,     // -1.0 .. +1.0 (défaut 0.0)
}

impl Default for TonalParams {
    fn default() -> Self {
        Self {
            whites: 0.0,
            highlights: 0.0,
            shadows: 0.0,
            blacks: 0.0,
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

    pub fn label(&self) -> &'static str {
        match self {
            TonalZone::Blacks => "Noirs (Blacks)",
            TonalZone::Shadows => "Ombres (Shadows)",
            TonalZone::Highlights => "Hautes Lumières (Highlights)",
            TonalZone::Whites => "Blancs (Whites / HDR)",
        }
    }
}

#[derive(Debug, Clone)]
pub struct LuminanceHistogram {
    pub bins: [u32; 256],
    pub max_count: u32,
    pub black_pct: f32,
    pub shadow_pct: f32,
    pub highlight_pct: f32,
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

    // Modulation Tonal Pro (Style Lightroom / Photoshop) :
    // 1. Blancs (Whites) : module directement le pic de boost
    let effective_peak_boost = (base_peak_boost * (1.0 + tonal.whites * 0.45)).max(1.05);
    // 2. Hautes Lumières (Highlights) : module la progressivité du seuil et du gamma
    let thresh = (highlight_threshold - tonal.highlights * 0.22).clamp(0.01, 0.98);
    let effective_gamma = (hi_gamma * (1.0 - tonal.highlights * 0.35)).clamp(0.2, 3.5);
    // 3. Ombres (Shadows) : module l'élévation des pénombres
    let effective_s_boost = (shadow_boost + tonal.shadows * 0.40).max(1.0);

    // Calcul de p99_headroom si native_hdr
    let (use_raw_mod, p99_headroom) = if is_native_hdr && hdr_lin_input.is_some() {
        let raw_lin = hdr_lin_input.unwrap();
        let mut headrooms = Vec::with_capacity(n_pixels);
        for i in 0..n_pixels {
            let r_sdr = srgb_to_linear(sdr_uint8[i * 3] as f32 / 255.0);
            let g_sdr = srgb_to_linear(sdr_uint8[i * 3 + 1] as f32 / 255.0);
            let b_sdr = srgb_to_linear(sdr_uint8[i * 3 + 2] as f32 / 255.0);
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
                let r_sdr = srgb_to_linear(sdr_uint8[pixel_idx * 3] as f32 / 255.0);
                let g_sdr = srgb_to_linear(sdr_uint8[pixel_idx * 3 + 1] as f32 / 255.0);
                let b_sdr = srgb_to_linear(sdr_uint8[pixel_idx * 3 + 2] as f32 / 255.0);
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

                let mut boost = (effective_s_boost + (effective_peak_boost - effective_s_boost) * highlight_weight)
                    .clamp(1.0, effective_peak_boost);

                // 4. Noirs (Blacks) : modulation du contraste dans les basses lumières (l_sdr < 0.25)
                if tonal.blacks != 0.0 && l_sdr < 0.25 {
                    let black_weight = (1.0 - l_sdr / 0.25).clamp(0.0, 1.0);
                    let black_mod = 1.0 + tonal.blacks * 0.35 * black_weight;
                    boost = (boost * black_mod).max(1.0);
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
                    let r_sdr = srgb_to_linear(sdr_uint8[pixel_idx * 3] as f32 / 255.0);
                    let g_sdr = srgb_to_linear(sdr_uint8[pixel_idx * 3 + 1] as f32 / 255.0);
                    let b_sdr = srgb_to_linear(sdr_uint8[pixel_idx * 3 + 2] as f32 / 255.0);
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

/// Génère le segment XMP APP1 conforme Adobe / Google Ultra HDR.
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
   hdrgm:HDRCapacityMin=\"0.000000\"\n\
   hdrgm:HDRCapacityMax=\"{hdr_capacity:.6}\"\n\
   hdrgm:OffsetHDR=\"0.000000\"\n\
   hdrgm:OffsetSDR=\"0.000000\"/>\n\
 </rdf:RDF>\n\
</x:xmpmeta>\n\
<?xpacket end=\"w\"?>"
    );

    let mut payload = b"http://ns.adobe.com/xap/1.0/\0".to_vec();
    payload.extend_from_slice(xml.as_bytes());
    let length = (payload.len() + 2) as u16;

    let mut out = Vec::with_capacity(4 + payload.len());
    out.extend_from_slice(&[0xff, 0xe1]); // Marqueur APP1
    out.extend_from_slice(&length.to_be_bytes()); // Longueur en Big Endian
    out.extend_from_slice(&payload);
    out
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
    let xmp_app1 = build_xmp_hdrgm(gain_min, gain_max, hdr_capacity);

    // Gain map JPEG avec injection du XMP après SOI (0xFF 0xD8)
    let mut gainmap_jpeg = Vec::with_capacity(gainmap_raw_jpeg.len() + xmp_app1.len());
    gainmap_jpeg.extend_from_slice(&gainmap_raw_jpeg[..2]);
    gainmap_jpeg.extend_from_slice(&xmp_app1);
    gainmap_jpeg.extend_from_slice(&gainmap_raw_jpeg[2..]);

    let e = exif_segment.map(|s| s.len()).unwrap_or(0);
    let a = xmp_app1.len();
    let b = base_jpeg.len();
    let g = gainmap_jpeg.len();

    let primary_size = (e + a + b + 90) as u32;
    let gainmap_offset = (e + b + 80) as u32;

    let mpf_app2 = build_mpf_app2(primary_size, g as u32, gainmap_offset);

    let mut final_jpeg = Vec::with_capacity(2 + e + a + mpf_app2.len() + b - 2 + g);
    final_jpeg.extend_from_slice(&[0xff, 0xd8]);
    if let Some(exif) = exif_segment {
        final_jpeg.extend_from_slice(exif);
    }
    final_jpeg.extend_from_slice(&xmp_app1);
    final_jpeg.extend_from_slice(&mpf_app2);
    final_jpeg.extend_from_slice(&base_jpeg[2..]);
    final_jpeg.extend_from_slice(&gainmap_jpeg);

    final_jpeg
}

/// Simule le rendu HDR sur écran SDR standard avec décalage EV et modes de tone-mapping préservant l'intégrité de l'image.
pub fn simulate_hdr_on_sdr(
    hdr_nits: &[f32],
    width: u32,
    height: u32,
    sdr_white: f32,
    ev_shift: f32,
    tone_map_mode: &str,
) -> Vec<u8> {
    let exposure_scale = (2.0f32).powf(ev_shift) / sdr_white;
    let n_pixels = (width * height) as usize;
    let mut out_u8 = vec![0u8; n_pixels * 3];

    let white_point = if tone_map_mode == "reinhard" {
        let mut samples: Vec<f32> = hdr_nits.iter().step_by(3).map(|&v| v * exposure_scale).collect();
        percentile_mut(&mut samples, 99.5).max(1.5)
    } else {
        1.5
    };
    let wp2 = white_point * white_point;

    out_u8
        .par_chunks_mut(1024 * 3)
        .zip(hdr_nits.par_chunks(1024 * 3))
        .for_each(|(out_chunk, nits_chunk)| {
            for (out_pixel, nits_pixel) in out_chunk.chunks_exact_mut(3).zip(nits_chunk.chunks_exact(3)) {
                for c in 0..3 {
                    let lin_exposed = nits_pixel[c] * exposure_scale;
                    let tm = match tone_map_mode {
                        // Mode Filmic ACES classique (conforme version Python originale)
                        "aces" | "filmic" => {
                            let x = lin_exposed.max(0.0);
                            let num = x * (2.51 * x + 0.03);
                            let den = (x * (2.43 * x + 0.59) + 0.14).max(1e-6);
                            (num / den).clamp(0.0, 1.0)
                        }
                        // Mode Naturel : préserve 100% de la texture sans aucun cramage.
                        // Les tons sombres et moyens (x <= 0.65) restent strictement identiques.
                        // Les hautes lumières (x > 0.65) bénéficient d'une compression progressive C1 continue
                        // qui donne tout son éclat au HDR sans jamais écraser les blancs à 255.
                        "natural" | "naturel" | "preserved" => {
                            let x = lin_exposed.max(0.0);
                            let knee = 0.65f32;
                            if x <= knee {
                                x
                            } else {
                                let y = x - knee;
                                let m = 1.0 - knee;
                                knee + m * (y / (y + m))
                            }
                        }
                        // Mode Reinhard avec point blanc étendu
                        "reinhard" => {
                            let x = lin_exposed.max(0.0);
                            let num = x * (1.0 + x / wp2);
                            let den = (1.0 + x).max(1e-6);
                            (num / den).clamp(0.0, 1.0)
                        }
                        // Mode Linéaire pur (écrêtage à 1.0, permet d'inspecter les hautes lumières via le curseur EV)
                        _ => lin_exposed.clamp(0.0, 1.0),
                    };
                    let srgb = linear_to_srgb(tm.clamp(0.0, 1.0));
                    out_pixel[c] = (srgb * 255.0 + 0.5).clamp(0.0, 255.0) as u8;
                }
            }
        });

    out_u8
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
