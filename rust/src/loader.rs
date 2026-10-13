//! loader.rs — Chargeur universel d'images (OpenEXR .exr, Radiance .hdr, JPEG, PNG, WebP, TIFF 16/32-bit, BMP).

use std::fs::File;
use std::io::Read;
use std::path::Path;
use image::{DynamicImage, GenericImageView, ImageReader};
use crate::engine::{linear_to_srgb, percentile_mut, srgb_to_linear, LUM_WEIGHTS};

#[allow(dead_code)]
pub struct LoadedImage {
    pub sdr_uint8: Vec<u8>,
    pub linear_rgb: Vec<f32>,
    pub width: u32,
    pub height: u32,
    pub is_native_hdr: bool,
    pub source_bit_depth: u8,
    pub format_name: String,
    pub filename: String,
    pub file_size_bytes: u64,
    pub dynamic_range_stops: f32,
}

pub fn load_image<P: AsRef<Path>>(path: P) -> Result<LoadedImage, String> {
    let path = path.as_ref();
    if !path.exists() {
        return Err(format!("Fichier introuvable : {}", path.display()));
    }

    let file_size_bytes = std::fs::metadata(path)
        .map(|m| m.len())
        .unwrap_or(0);
    let filename = path.file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "image".to_string());

    let mut file = File::open(path).map_err(|e| format!("Erreur d'ouverture : {}", e))?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).map_err(|e| format!("Erreur de lecture : {}", e))?;

    load_image_from_bytes(&bytes, &filename, file_size_bytes)
}

pub fn load_image_from_bytes(bytes: &[u8], filename: &str, file_size_bytes: u64) -> Result<LoadedImage, String> {
    let mut reader = match ImageReader::new(std::io::Cursor::new(bytes)).with_guessed_format() {
        Ok(r) => r,
        Err(_) => ImageReader::new(std::io::Cursor::new(bytes)),
    };

    let lower_fn = filename.to_lowercase();
    if reader.format().is_none() {
        if lower_fn.ends_with(".exr") {
            reader.set_format(image::ImageFormat::OpenExr);
        } else if lower_fn.ends_with(".hdr") {
            reader.set_format(image::ImageFormat::Hdr);
        } else if lower_fn.ends_with(".tif") || lower_fn.ends_with(".tiff") {
            reader.set_format(image::ImageFormat::Tiff);
        } else if lower_fn.ends_with(".jpg") || lower_fn.ends_with(".jpeg") {
            reader.set_format(image::ImageFormat::Jpeg);
        } else if lower_fn.ends_with(".png") {
            reader.set_format(image::ImageFormat::Png);
        } else if lower_fn.ends_with(".webp") {
            reader.set_format(image::ImageFormat::WebP);
        }
    }

    let format_guessed = reader.format();
    let format_str = format_guessed
        .map(|f| format!("{:?}", f))
        .unwrap_or_else(|| "Inconnu".to_string());

    let dyn_img = reader.decode().map_err(|e| format!("Erreur de décodage ({}) : {}", format_str, e))?;
    let (width, height) = dyn_img.dimensions();
    let n_pixels = (width * height) as usize;

    match dyn_img {
        // ── Formats 32-bit Float HDR (OpenEXR .exr, Radiance .hdr, TIFF 32F) ──
        DynamicImage::ImageRgb32F(rgb32f) => {
            let mut raw_lin = Vec::with_capacity(n_pixels * 3);
            for p in rgb32f.pixels() {
                raw_lin.push(if p[0].is_finite() { p[0].max(0.0) } else { 0.0 });
                raw_lin.push(if p[1].is_finite() { p[1].max(0.0) } else { 0.0 });
                raw_lin.push(if p[2].is_finite() { p[2].max(0.0) } else { 0.0 });
            }
            process_float_hdr(raw_lin, width, height, &format_str, filename, file_size_bytes)
        }
        DynamicImage::ImageRgba32F(rgba32f) => {
            let mut raw_lin = Vec::with_capacity(n_pixels * 3);
            for p in rgba32f.pixels() {
                raw_lin.push(if p[0].is_finite() { p[0].max(0.0) } else { 0.0 });
                raw_lin.push(if p[1].is_finite() { p[1].max(0.0) } else { 0.0 });
                raw_lin.push(if p[2].is_finite() { p[2].max(0.0) } else { 0.0 });
            }
            process_float_hdr(raw_lin, width, height, &format_str, filename, file_size_bytes)
        }

        // ── Formats 16-bit Entier (TIFF 16-bit, PNG 16-bit) ──
        DynamicImage::ImageRgb16(rgb16) => {
            let mut linear_rgb = vec![0.0f32; n_pixels * 3];
            let mut sdr_uint8 = vec![0u8; n_pixels * 3];

            for (i, pixel) in rgb16.pixels().enumerate() {
                let r = pixel[0] as f32 / 65535.0;
                let g = pixel[1] as f32 / 65535.0;
                let b = pixel[2] as f32 / 65535.0;
                linear_rgb[i * 3] = r;
                linear_rgb[i * 3 + 1] = g;
                linear_rgb[i * 3 + 2] = b;

                sdr_uint8[i * 3] = (linear_to_srgb(r) * 255.0 + 0.5).clamp(0.0, 255.0) as u8;
                sdr_uint8[i * 3 + 1] = (linear_to_srgb(g) * 255.0 + 0.5).clamp(0.0, 255.0) as u8;
                sdr_uint8[i * 3 + 2] = (linear_to_srgb(b) * 255.0 + 0.5).clamp(0.0, 255.0) as u8;
            }

            let stops = compute_stops(&linear_rgb);

            Ok(LoadedImage {
                sdr_uint8,
                linear_rgb,
                width,
                height,
                is_native_hdr: true,
                source_bit_depth: 16,
                format_name: format!("16-bit RGB ({})", format_str),
                filename: filename.to_string(),
                file_size_bytes,
                dynamic_range_stops: stops.clamp(8.0, 14.0),
            })
        }
        DynamicImage::ImageRgba16(rgba16) => {
            let mut linear_rgb = vec![0.0f32; n_pixels * 3];
            let mut sdr_uint8 = vec![0u8; n_pixels * 3];

            for (i, pixel) in rgba16.pixels().enumerate() {
                let r = pixel[0] as f32 / 65535.0;
                let g = pixel[1] as f32 / 65535.0;
                let b = pixel[2] as f32 / 65535.0;
                linear_rgb[i * 3] = r;
                linear_rgb[i * 3 + 1] = g;
                linear_rgb[i * 3 + 2] = b;

                sdr_uint8[i * 3] = (linear_to_srgb(r) * 255.0 + 0.5).clamp(0.0, 255.0) as u8;
                sdr_uint8[i * 3 + 1] = (linear_to_srgb(g) * 255.0 + 0.5).clamp(0.0, 255.0) as u8;
                sdr_uint8[i * 3 + 2] = (linear_to_srgb(b) * 255.0 + 0.5).clamp(0.0, 255.0) as u8;
            }

            let stops = compute_stops(&linear_rgb);

            Ok(LoadedImage {
                sdr_uint8,
                linear_rgb,
                width,
                height,
                is_native_hdr: true,
                source_bit_depth: 16,
                format_name: format!("16-bit RGBA ({})", format_str),
                filename: filename.to_string(),
                file_size_bytes,
                dynamic_range_stops: stops.clamp(8.0, 14.0),
            })
        }

        // ── Formats SDR Standard 8-bit (JPEG, PNG, WebP, BMP, TIFF 8-bit) ──
        _ => {
            let rgb8 = dyn_img.to_rgb8();
            let raw_bytes = rgb8.into_raw();
            let mut linear_rgb = vec![0.0f32; n_pixels * 3];

            for i in 0..n_pixels {
                let r = raw_bytes[i * 3] as f32 / 255.0;
                let g = raw_bytes[i * 3 + 1] as f32 / 255.0;
                let b = raw_bytes[i * 3 + 2] as f32 / 255.0;
                linear_rgb[i * 3] = srgb_to_linear(r);
                linear_rgb[i * 3 + 1] = srgb_to_linear(g);
                linear_rgb[i * 3 + 2] = srgb_to_linear(b);
            }

            let stops = compute_stops(&linear_rgb);

            Ok(LoadedImage {
                sdr_uint8: raw_bytes,
                linear_rgb,
                width,
                height,
                is_native_hdr: false,
                source_bit_depth: 8,
                format_name: format_str,
                filename: filename.to_string(),
                file_size_bytes,
                dynamic_range_stops: stops.clamp(6.0, 10.0),
            })
        }
    }
}

/// Traitement des images float 32-bit (OpenEXR, Radiance HDR, TIFF Float).
/// Étalonne l'exposition diffuse pour que le gris moyen soit à 18% (évite toute surexposition)
/// et génère l'image de base SDR via ACES Filmic tone mapping.
fn process_float_hdr(
    mut raw_lin: Vec<f32>,
    width: u32,
    height: u32,
    format_str: &str,
    filename: &str,
    file_size_bytes: u64,
) -> Result<LoadedImage, String> {
    let n_pixels = (width * height) as usize;

    // 1. Calcul de la médiane de luminance pour calibration de l'exposition
    let mut lum_samples = Vec::with_capacity(n_pixels);
    for i in 0..n_pixels {
        let l = raw_lin[i * 3] * LUM_WEIGHTS[0]
            + raw_lin[i * 3 + 1] * LUM_WEIGHTS[1]
            + raw_lin[i * 3 + 2] * LUM_WEIGHTS[2];
        if l.is_finite() && l > 1e-4 {
            lum_samples.push(l);
        }
    }

    let exposure_scale = if !lum_samples.is_empty() {
        let med_lum = percentile_mut(&mut lum_samples, 50.0).max(1e-4);
        // Alignement du gris moyen à 0.18 (18%)
        (0.18 / med_lum).clamp(0.0001, 1000.0)
    } else {
        1.0
    };

    // Mise à l'échelle des valeurs linéaires
    for val in &mut raw_lin {
        *val = (*val * exposure_scale).max(0.0);
    }

    // 2. Base SDR par ACES Filmic tone mapping
    let mut sdr_uint8 = vec![0u8; n_pixels * 3];
    for i in 0..n_pixels {
        let r_tm = aces_tonemap(raw_lin[i * 3]);
        let g_tm = aces_tonemap(raw_lin[i * 3 + 1]);
        let b_tm = aces_tonemap(raw_lin[i * 3 + 2]);
        sdr_uint8[i * 3] = (linear_to_srgb(r_tm) * 255.0 + 0.5).clamp(0.0, 255.0) as u8;
        sdr_uint8[i * 3 + 1] = (linear_to_srgb(g_tm) * 255.0 + 0.5).clamp(0.0, 255.0) as u8;
        sdr_uint8[i * 3 + 2] = (linear_to_srgb(b_tm) * 255.0 + 0.5).clamp(0.0, 255.0) as u8;
    }

    let stops = compute_stops(&raw_lin);

    Ok(LoadedImage {
        sdr_uint8,
        linear_rgb: raw_lin,
        width,
        height,
        is_native_hdr: true,
        source_bit_depth: 32,
        format_name: format!("HDR 32-bit ({})", format_str),
        filename: filename.to_string(),
        file_size_bytes,
        dynamic_range_stops: stops.clamp(8.0, 20.0),
    })
}

#[inline]
fn aces_tonemap(x: f32) -> f32 {
    let x = x.max(0.0);
    let num = x * (2.51 * x + 0.03);
    let den = (x * (2.43 * x + 0.59) + 0.14).max(1e-6);
    (num / den).clamp(0.0, 1.0)
}

fn compute_stops(linear_rgb: &[f32]) -> f32 {
    let n = linear_rgb.len() / 3;
    let mut lum = Vec::with_capacity(n);
    for i in 0..n {
        let l = linear_rgb[i * 3] * LUM_WEIGHTS[0]
            + linear_rgb[i * 3 + 1] * LUM_WEIGHTS[1]
            + linear_rgb[i * 3 + 2] * LUM_WEIGHTS[2];
        if l > 1e-4 {
            lum.push(l);
        }
    }
    if lum.is_empty() {
        return 8.0;
    }
    let p05 = percentile_mut(&mut lum, 5.0).max(1e-4);
    let p99 = percentile_mut(&mut lum, 99.5);
    (p99 / p05).max(2.0).log2()
}
