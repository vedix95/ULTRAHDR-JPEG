//! main.rs — Point d'entrée UltraHDR Studio (Rust pur).
//!
//! 1. Mode Interface Graphique Desktop "en dur" (eframe / wgpu natif, DirectX 12 / Wayland).
//! 2. Mode Ligne de Commande (CLI) ultra-rapide (multithreadé Rayon).

mod analyzer;
mod engine;
mod gui;
mod loader;

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use clap::Parser;
use rayon::prelude::*;

use analyzer::{analyze_image_potential, get_preset_by_name};
use engine::{
    assemble_ultra_hdr_jpeg, compute_hdr_and_gain_map, encode_gain_map_jpeg, encode_rgb_jpeg,
    TonalParams,
};
use loader::load_image;

#[derive(Parser, Debug)]
#[command(name = "ultrahdr")]
#[command(version = "1.0.1")]
#[command(about = "UltraHDR Studio — Éditeur et Générateur Ultra HDR / ISO 21496-1 en Rust pur")]
struct Cli {
    /// Fichier image ou dossier d'images (si omis, ouvre l'interface graphique native)
    #[arg(value_name = "INPUT")]
    input: Option<PathBuf>,

    /// Chemin du fichier de sortie (déclenche le mode CLI)
    #[arg(short = 'o', long = "output")]
    output: Option<PathBuf>,

    /// Profil HDR à appliquer (sunset, night_neon, landscape, studio, native_hdr, moderate)
    #[arg(long = "preset")]
    preset: Option<String>,

    /// Luminance de crête en nits (ex: 1000, 1400)
    #[arg(long = "peak-nits")]
    peak_nits: Option<f32>,

    /// Luminance de référence du blanc SDR en nits (défaut: 203.0)
    #[arg(long = "sdr-white", default_value_t = 203.0)]
    sdr_white: f32,

    /// Exposant gamma de la courbe de boost
    #[arg(long = "gamma")]
    gamma: Option<f32>,

    /// Facteur de boost des ombres (ex: 1.15)
    #[arg(long = "shadow-boost")]
    shadow_boost: Option<f32>,

    /// Seuil de transition des hautes lumières [0.0..1.0]
    #[arg(long = "threshold")]
    threshold: Option<f32>,

    /// Ajustement de contraste [-1.0..1.0] (défaut: 0.0)
    #[arg(long = "contrast")]
    contrast: Option<f32>,

    /// Ajustement de saturation [-1.0..1.0] (défaut: 0.0)
    #[arg(long = "saturation")]
    saturation: Option<f32>,

    /// Ajustement de vibrance [-1.0..1.0] (défaut: 0.0)
    #[arg(long = "vibrance")]
    vibrance: Option<f32>,

    /// Qualité JPEG de l'image SDR de base (défaut: 94)
    #[arg(long = "quality", default_value_t = 94)]
    quality: u8,

    /// Qualité JPEG de la Gain Map (défaut: 88)
    #[arg(long = "gainmap-quality", default_value_t = 88)]
    gainmap_quality: u8,

    /// Enregistrer aussi la Gain Map seule en JPEG niveaux de gris
    #[arg(long = "save-gainmap")]
    save_gainmap: bool,

    /// Afficher uniquement l'analyse de potentiel HDR sans convertir
    #[arg(long = "analyze")]
    analyze: bool,

    /// Forcer le lancement de l'interface graphique native (GUI)
    #[arg(long = "gui")]
    gui: bool,

    /// Forcer le mode ligne de commande (CLI)
    #[arg(long = "cli")]
    cli: bool,
}

fn process_single_file(
    input_path: &Path,
    output_path: Option<&Path>,
    preset_name: Option<&str>,
    peak_nits: Option<f32>,
    sdr_white: f32,
    gamma: Option<f32>,
    shadow_boost: Option<f32>,
    threshold: Option<f32>,
    contrast: Option<f32>,
    saturation: Option<f32>,
    vibrance: Option<f32>,
    quality: u8,
    gainmap_quality: u8,
    save_gainmap: bool,
    analyze_only: bool,
) -> bool {
    let filename = input_path.file_name().map(|s| s.to_string_lossy()).unwrap_or_default();
    println!("\n[+] Chargement de {}...", filename);

    let t0 = Instant::now();
    let loaded = match load_image(input_path) {
        Ok(img) => img,
        Err(e) => {
            eprintln!("    [!] Erreur chargement : {}", e);
            return false;
        }
    };

    println!(
        "    Format : {} ({}x{}), Profondeur : {}-bit, Natif HDR : {}",
        loaded.format_name, loaded.width, loaded.height, loaded.source_bit_depth, loaded.is_native_hdr
    );

    let analysis = analyze_image_potential(
        loaded.is_native_hdr,
        &loaded.linear_rgb,
        loaded.dynamic_range_stops,
        &loaded.format_name,
    );

    println!(
        "    Potentiel HDR : {}% [{}]",
        analysis.potential_score, analysis.category_title
    );
    println!("    Détails : {}", analysis.explanation);

    if analyze_only {
        return true;
    }

    let selected_preset = if let Some(name) = preset_name {
        get_preset_by_name(name).unwrap_or(analysis.recommended_preset)
    } else {
        analysis.recommended_preset
    };

    let final_peak = peak_nits.unwrap_or(selected_preset.peak_nits);
    let final_gamma = gamma.unwrap_or(selected_preset.hi_gamma);
    let final_shadow = shadow_boost.unwrap_or(selected_preset.shadow_boost);
    let final_thresh = threshold.unwrap_or(selected_preset.highlight_threshold);
    let final_contrast = contrast.unwrap_or(0.0);
    let final_saturation = saturation.unwrap_or(0.0);
    let final_vibrance = vibrance.unwrap_or(0.0);

    println!(
        "    Paramètres appliqués : Peak={:.0} nits, Gamma={:.2}, ShadowBoost={:.2}x, Thresh={:.2}, Contraste={:+.0}%, Sat={:+.0}%, Vib={:+.0}%",
        final_peak, final_gamma, final_shadow, final_thresh, final_contrast * 100.0, final_saturation * 100.0, final_vibrance * 100.0
    );

    // 1. Calcul HDR et Gain Map
    let res = compute_hdr_and_gain_map(
        &loaded.sdr_uint8,
        if loaded.is_native_hdr { Some(&loaded.linear_rgb) } else { None },
        loaded.is_native_hdr,
        loaded.width,
        loaded.height,
        final_peak,
        sdr_white,
        final_shadow,
        final_gamma,
        final_thresh,
        TonalParams {
            contrast: final_contrast,
            saturation: final_saturation,
            vibrance: final_vibrance,
            ..Default::default()
        },
        0.40,
    );

    println!(
        "    Gain map générée : Min={:.2} IL, Max=+{:.2} IL (Boost max ~{:.1}x)",
        res.gain_min, res.gain_max, (2.0f32).powf(res.gain_max)
    );

    // 2. Encodage JPEG SDR de base
    let sdr_to_encode = res.sdr_processed.as_deref().unwrap_or(&loaded.sdr_uint8);
    let base_jpeg = match encode_rgb_jpeg(sdr_to_encode, loaded.width, loaded.height, quality) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("    [!] Erreur encodage base JPEG : {}", e);
            return false;
        }
    };

    // 3. Encodage JPEG Gain Map
    let gm_jpeg = match encode_gain_map_jpeg(&res.gain_log2, res.gain_min, res.gain_max, loaded.width, loaded.height, gainmap_quality) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("    [!] Erreur encodage Gain Map : {}", e);
            return false;
        }
    };

    // 4. Assemblage binaire Ultra HDR ISO 21496-1 / Google Ultra HDR (avec préservation EXIF)
    let uhdr_bytes = assemble_ultra_hdr_jpeg(
        &base_jpeg,
        &gm_jpeg,
        res.gain_min,
        res.gain_max,
        final_peak,
        sdr_white,
        loaded.exif_bytes.as_deref(),
    );

    // 5. Enregistrement fichier
    let out_path = if let Some(p) = output_path {
        p.to_path_buf()
    } else {
        let stem = input_path.file_stem().map(|s| s.to_string_lossy()).unwrap_or_else(|| "out".into());
        input_path.with_file_name(format!("{}_UltraHDR.jpg", stem))
    };

    if let Some(parent) = out_path.parent() {
        let _ = fs::create_dir_all(parent);
    }

    if let Err(e) = fs::write(&out_path, &uhdr_bytes) {
        eprintln!("    [!] Erreur écriture {} : {}", out_path.display(), e);
        return false;
    }

    let elapsed = t0.elapsed();
    println!(
        "    [✓] Enregistré : {} ({} Ko) en {:.2}s",
        out_path.display(),
        uhdr_bytes.len() / 1024,
        elapsed.as_secs_f32()
    );

    if save_gainmap {
        let stem = out_path.file_stem().map(|s| s.to_string_lossy()).unwrap_or_else(|| "out".into());
        let gm_path = out_path.with_file_name(format!("{}_gainmap.jpg", stem));
        let _ = fs::write(&gm_path, &gm_jpeg);
        println!("    [✓] Gain map standalone : {}", gm_path.display());
    }

    true
}

fn main() {
    // 🐧 Forcer le backend Wayland sous Linux pour garantir la compatibilité HDR et le protocole couleur
    #[cfg(target_os = "linux")]
    {
        if std::env::var_os("WINIT_UNIX_BACKEND").is_none() {
            if std::env::var_os("WAYLAND_DISPLAY").is_some()
                || std::env::var("XDG_SESSION_TYPE").map(|v| v == "wayland").unwrap_or(false)
            {
                std::env::set_var("WINIT_UNIX_BACKEND", "wayland");
            }
        }
    }

    let cli = Cli::parse();

    // Déterminer s'il faut lancer l'interface graphique native
    let is_dir = cli.input.as_ref().map(|p| p.is_dir()).unwrap_or(false);
    let wants_cli = cli.cli || cli.output.is_some() || cli.analyze || is_dir;

    if !wants_cli || cli.gui {
        // Lancer l'interface native Desktop "en dur" (wgpu / DirectX 12 / Wayland)
        if let Err(e) = gui::run_native_gui(cli.input) {
            eprintln!("Erreur lors du lancement de l'interface graphique : {}", e);
            std::process::exit(1);
        }
        return;
    }

    let input_path = cli.input.unwrap();
    if !input_path.exists() {
        eprintln!("Erreur : Le chemin spécifié n'existe pas : {}", input_path.display());
        std::process::exit(1);
    }

    if input_path.is_file() {
        let ok = process_single_file(
            &input_path,
            cli.output.as_deref(),
            cli.preset.as_deref(),
            cli.peak_nits,
            cli.sdr_white,
            cli.gamma,
            cli.shadow_boost,
            cli.threshold,
            cli.contrast,
            cli.saturation,
            cli.vibrance,
            cli.quality,
            cli.gainmap_quality,
            cli.save_gainmap,
            cli.analyze,
        );
        std::process::exit(if ok { 0 } else { 1 });
    } else if input_path.is_dir() {
        let valid_exts = [
            "jpg", "jpeg", "png", "webp", "tif", "tiff", "bmp", "hdr", "pfm", "exr", "dng", "cr2", "nef", "arw"
        ];

        let files: Vec<PathBuf> = fs::read_dir(&input_path)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                p.is_file()
                    && p.extension()
                        .and_then(|ext| ext.to_str())
                        .map(|ext| valid_exts.contains(&ext.to_lowercase().as_str()))
                        .unwrap_or(false)
            })
            .collect();

        println!(
            "[*] Traitement par lot de {} fichiers dans {} (multithreadé Rayon)...",
            files.len(),
            input_path.display()
        );

        let t_start = Instant::now();
        let success_count = files
            .par_iter()
            .map(|f| {
                if process_single_file(
                    f,
                    None,
                    cli.preset.as_deref(),
                    cli.peak_nits,
                    cli.sdr_white,
                    cli.gamma,
                    cli.shadow_boost,
                    cli.threshold,
                    cli.contrast,
                    cli.saturation,
                    cli.vibrance,
                    cli.quality,
                    cli.gainmap_quality,
                    cli.save_gainmap,
                    cli.analyze,
                ) {
                    1
                } else {
                    0
                }
            })
            .sum::<usize>();

        println!(
            "\n[✓] Traitement terminé : {}/{} fichiers convertis avec succès en {:.2}s.",
            success_count,
            files.len(),
            t_start.elapsed().as_secs_f32()
        );
    }
}
