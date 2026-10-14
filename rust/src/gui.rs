//! gui.rs — Interface graphique native haute performance et ultra-ergonomique pour UltraHDR Studio.
//!
//! Rendu matériel direct avec wgpu (DirectX 12 sous Windows, Vulkan / Wayland sous Linux).
//! Pipeline de prévisualisation découplé (1280px temps réel à 60+ FPS) et export pleine résolution native.

use std::path::{Path, PathBuf};
use std::time::Instant;

use eframe::egui::{self, Color32, Pos2, Rect, Rounding, Stroke, Vec2};
use image::{ImageBuffer, Rgb};
use rayon::prelude::*;

use crate::analyzer::{analyze_image_potential, get_preset_library, AnalysisResult, HDRPreset};
use crate::engine::{
    adapt_hdr_nits_for_display, assemble_ultra_hdr_jpeg, compute_display_headroom_weight,
    compute_hdr_and_gain_map, compute_luminance_histogram, encode_gain_map_jpeg, encode_rgb_jpeg,
    generate_nits_heatmap, simulate_hdr_on_sdr, LuminanceHistogram, TonalParams, TonalZone,
};
use crate::loader::{load_image, LoadedImage};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewerMode {
    Split,       // ↔ Glissière Avant / Après
    SideBySide,  // ◫ Côte à côte
    FullHdr,     // ✨ Vrai Ultra HDR Direct
    GainMap,     // ◐ Gain Map
    Heatmap,     // 🔥 Carte Nits
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigPane {
    Tonal,       // 🎨 Tonalité Pro, Courbe, Saturation
    Simulation,  // 🖥️ Simulation EDR & Tone-Mapping
    Engine,      // 📁 Source & Profils HDR
    Export,      // 💾 Export Ultra HDR & Traitement par lot
}

impl ConfigPane {
    pub fn icon(&self) -> &'static str {
        match self {
            ConfigPane::Tonal => "🎨",
            ConfigPane::Simulation => "🖥️",
            ConfigPane::Engine => "📁",
            ConfigPane::Export => "💾",
        }
    }

    pub fn title(&self) -> &'static str {
        match self {
            ConfigPane::Tonal => "Tonalité",
            ConfigPane::Simulation => "Rendu",
            ConfigPane::Engine => "Profils",
            ConfigPane::Export => "Export",
        }
    }
}

pub struct ProcessingParams {
    pub peak_nits: f32,
    pub sdr_white: f32,
    pub shadow_boost: f32,
    pub hi_gamma: f32,
    pub highlight_threshold: f32,
    pub hdr_boost: f32,
    pub highlight_rolloff: f32,
    pub tone_map_mode: String,
    // 🎨 Tonalité Pro (Style Photoshop / Lightroom)
    pub tonal_contrast: f32,   // -1.0 .. +1.0 (défaut 0.0)
    pub tonal_whites: f32,     // -1.0 .. +1.0 (défaut 0.0)
    pub tonal_highlights: f32, // -1.0 .. +1.0 (défaut 0.0)
    pub tonal_shadows: f32,    // -1.0 .. +1.0 (défaut 0.0)
    pub tonal_blacks: f32,     // -1.0 .. +1.0 (défaut 0.0)
    pub tonal_saturation: f32, // -1.0 .. +1.0 (défaut 0.0)
    pub tonal_vibrance: f32,   // -2.0 .. +2.0 (défaut 0.0)
    pub anti_halo: f32,        // 0.0 .. 1.0 (défaut 0.40)
}

impl Default for ProcessingParams {
    fn default() -> Self {
        Self {
            peak_nits: 1000.0,
            sdr_white: 203.0,
            shadow_boost: 1.0,
            hi_gamma: 1.0,
            highlight_threshold: 0.50,
            hdr_boost: 1.0,
            highlight_rolloff: 0.75,
            tone_map_mode: "vivid".to_string(),
            tonal_contrast: 0.0,
            tonal_whites: 0.0,
            tonal_highlights: 0.0,
            tonal_shadows: 0.0,
            tonal_blacks: 0.0,
            tonal_saturation: 0.0,
            tonal_vibrance: 0.0,
            anti_halo: 0.40,
        }
    }
}

/// Tampon de prévisualisation léger (1280px max) pour une réactivité 60 FPS sans latence.
pub struct PreviewBuffer {
    pub sdr_uint8: Vec<u8>,
    pub linear_rgb: Option<Vec<f32>>,
    pub width: u32,
    pub height: u32,
}

pub struct UltraHdrApp {
    // Image source pleine résolution
    loaded_image: Option<LoadedImage>,
    analysis: Option<AnalysisResult>,
    params: ProcessingParams,

    // Tampon de prévisualisation rapide
    preview_buffer: Option<PreviewBuffer>,

    // Paramètres d'exportation
    base_quality: u8,
    gainmap_quality: u8,
    save_standalone_gainmap: bool,

    // État de la visionneuse
    mode: ViewerMode,
    split_pos: f32,
    zoom: f32,
    pan: Vec2,

    // Textures GPU
    tex_original_sdr: Option<egui::TextureHandle>,
    tex_hdr_preview: Option<egui::TextureHandle>,
    tex_gain_map: Option<egui::TextureHandle>,
    tex_heatmap: Option<egui::TextureHandle>,

    // Statistiques en direct
    gain_min: f32,
    gain_max: f32,
    preview_multiplier: f32,

    // Tonalité Pro & Outil Ciblé Direct (Lightroom TAT)
    card_tonal_open: bool,
    targeted_tool_active: bool,
    hovered_zone: Option<(TonalZone, f32)>,
    cached_histogram: Option<LuminanceHistogram>,

    // État des accordéons des panneaux latéraux
    card_proposal_open: bool,
    card_settings_open: bool,

    // Traitement par lot (Batch)
    show_batch_dialog: bool,
    batch_input_dir: String,
    batch_output_dir: String,
    batch_status_msg: Option<String>,

    // Détection matérielle HDR
    is_hdr_display_active: bool,
    display_info_str: String,
    display_peak_nits: f32,

    // Barre d'état & Ergonomie
    status_message: Option<(String, Instant, Color32)>,
    open_path_input: String,
    show_open_dialog: bool,
    sidebar_visible: bool,
    left_panel_visible: bool,
    right_panel_visible: bool,
    bottom_panel_visible: bool,
    compare_toggled: bool,
    pub active_config_pane: ConfigPane,
}

impl UltraHdrApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // Détection de l'écran HDR (Windows DXGI ou Linux Wayland)
        let (is_hdr, info_str, peak_nits) = detect_hdr_display();

        // ── Police Material Design (Google Roboto) ──
        let mut fonts = egui::FontDefinitions::default();
        fonts.font_data.insert(
            "roboto_regular".to_owned(),
            egui::FontData::from_static(include_bytes!("../assets/fonts/Roboto-Regular.ttf")),
        );
        fonts.font_data.insert(
            "roboto_medium".to_owned(),
            egui::FontData::from_static(include_bytes!("../assets/fonts/Roboto-Medium.ttf")),
        );
        if let Some(family) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
            family.insert(0, "roboto_regular".to_owned());
        }
        cc.egui_ctx.set_fonts(fonts);

        // ── Thème Sombre Material Design 3 (M3 Dark Surface & Google Blue) ──
        let mut visuals = egui::Visuals::dark();
        visuals.override_text_color = Some(Color32::from_rgb(240, 243, 250));
        visuals.panel_fill = Color32::from_rgb(18, 20, 28);
        visuals.window_fill = Color32::from_rgb(24, 26, 36);
        visuals.faint_bg_color = Color32::from_rgb(28, 32, 44);
        visuals.extreme_bg_color = Color32::from_rgb(12, 14, 20);

        visuals.window_rounding = Rounding::same(12.0);
        visuals.menu_rounding = Rounding::same(10.0);
        visuals.widgets.noninteractive.rounding = Rounding::same(8.0);
        visuals.widgets.inactive.rounding = Rounding::same(8.0);
        visuals.widgets.hovered.rounding = Rounding::same(8.0);
        visuals.widgets.active.rounding = Rounding::same(8.0);
        visuals.widgets.open.rounding = Rounding::same(8.0);

        visuals.widgets.inactive.bg_fill = Color32::from_rgb(28, 32, 46);
        visuals.widgets.inactive.bg_stroke = Stroke::new(1.0f32, Color32::from_rgb(46, 52, 72));
        visuals.widgets.hovered.bg_fill = Color32::from_rgb(38, 44, 64);
        visuals.widgets.hovered.bg_stroke = Stroke::new(1.2f32, Color32::from_rgb(66, 165, 245));
        visuals.widgets.active.bg_fill = Color32::from_rgb(25, 118, 210);
        visuals.selection.bg_fill = Color32::from_rgb(25, 118, 210);

        cc.egui_ctx.set_visuals(visuals);

        Self {
            loaded_image: None,
            analysis: None,
            params: ProcessingParams::default(),
            preview_buffer: None,
            base_quality: 94,
            gainmap_quality: 88,
            save_standalone_gainmap: false,
            mode: ViewerMode::Split,
            split_pos: 0.5,
            zoom: 1.0,
            pan: Vec2::ZERO,
            tex_original_sdr: None,
            tex_hdr_preview: None,
            tex_gain_map: None,
            tex_heatmap: None,
            gain_min: 0.0,
            gain_max: 0.0,
            preview_multiplier: 1.0,
            card_tonal_open: true,
            targeted_tool_active: false,
            hovered_zone: None,
            cached_histogram: None,
            card_proposal_open: true,
            card_settings_open: true,
            show_batch_dialog: false,
            batch_input_dir: String::new(),
            batch_output_dir: String::new(),
            batch_status_msg: None,
            is_hdr_display_active: is_hdr,
            display_info_str: info_str,
            display_peak_nits: peak_nits,
            status_message: None,
            open_path_input: String::new(),
            show_open_dialog: false,
            sidebar_visible: true,
            left_panel_visible: true,
            right_panel_visible: true,
            bottom_panel_visible: true,
            compare_toggled: false,
            active_config_pane: ConfigPane::Tonal,
        }
    }

    pub fn setup_loaded_image(&mut self, img: LoadedImage, ctx: &egui::Context) {
        let analysis = analyze_image_potential(
            img.is_native_hdr,
            &img.linear_rgb,
            img.dynamic_range_stops,
            &img.format_name,
        );

        let rec = &analysis.recommended_preset;
        self.params.peak_nits = rec.peak_nits;
        self.params.sdr_white = rec.sdr_white;
        self.params.shadow_boost = rec.shadow_boost;
        self.params.hi_gamma = rec.hi_gamma;
        self.params.highlight_threshold = rec.highlight_threshold;
        self.params.hdr_boost = 1.0;
        self.params.highlight_rolloff = 0.75;
        self.params.tone_map_mode = "vivid".to_string();
        self.params.tonal_contrast = 0.0;
        self.params.tonal_whites = 0.0;
        self.params.tonal_highlights = 0.0;
        self.params.tonal_shadows = 0.0;
        self.params.tonal_blacks = 0.0;
        self.params.tonal_saturation = 0.0;
        self.params.tonal_vibrance = 0.0;
        self.params.anti_halo = 0.40;
        self.targeted_tool_active = false;
        self.hovered_zone = None;
        self.compare_toggled = false;

        // ⚡ Génération du tampon de prévisualisation rapide (1280px max)
        let preview = create_preview_buffer(&img, 1280);

        // Calcul de l'histogramme des 4 zones
        self.cached_histogram = Some(compute_luminance_histogram(&preview.sdr_uint8));

        // Texture SDR Original : mise en cache UNE SEULE FOIS (aucun recalcul inutile lors des glissements)
        let color_img_sdr = egui::ColorImage::from_rgb(
            [preview.width as usize, preview.height as usize],
            &preview.sdr_uint8,
        );
        self.tex_original_sdr = Some(ctx.load_texture("sdr_orig", color_img_sdr, egui::TextureOptions::LINEAR));

        // Invalidation des textures dépendantes
        self.tex_hdr_preview = None;
        self.tex_gain_map = None;
        self.tex_heatmap = None;

        self.preview_buffer = Some(preview);
        self.analysis = Some(analysis);
        let name = img.filename.clone();
        self.loaded_image = Some(img);
        self.zoom = 1.0;
        self.pan = Vec2::ZERO;

        // Calcul initial des textures de prévisualisation
        self.recompute_and_update_textures(ctx);
        self.set_status(&format!("Fichier chargé : {}", name), Color32::from_rgb(46, 204, 113));
    }

    pub fn load_file(&mut self, path: &Path, ctx: &egui::Context) {
        match load_image(path) {
            Ok(img) => self.setup_loaded_image(img, ctx),
            Err(e) => {
                self.set_status(&format!("Erreur : {}", e), Color32::from_rgb(231, 76, 60));
            }
        }
    }

    /// Génère instantanément une scène de test Ultra HDR de coucher de soleil en haute dynamique.
    pub fn load_demo_image(&mut self, ctx: &egui::Context) {
        let width = 1280u32;
        let height = 720u32;
        let mut sdr = Vec::with_capacity((width * height * 3) as usize);
        let mut linear = Vec::with_capacity((width * height * 3) as usize);

        for y in 0..height {
            let ny = y as f32 / height as f32;
            for x in 0..width {
                let nx = x as f32 / width as f32;
                let dx = nx - 0.5;
                let dy = (ny - 0.35) * 0.7;
                let dist = (dx * dx + dy * dy).sqrt();

                let sky_r = (1.0 - ny * 0.55).clamp(0.0, 1.0);
                let sky_g = ((1.0 - ny * 0.75) * 0.65).clamp(0.0, 1.0);
                let sky_b = (ny * 0.65 + 0.15).clamp(0.0, 1.0);

                let sun_flare = (1.0 / (dist * 10.0 + 1.0)).powf(2.0);
                let sun_core = if dist < 0.045 { 1.0 } else { 0.0 };

                let hdr_r = sky_r + sun_flare * 4.5 + sun_core * 12.0;
                let hdr_g = sky_g + sun_flare * 3.2 + sun_core * 10.0;
                let hdr_b = sky_b + sun_flare * 1.8 + sun_core * 6.5;

                linear.push(hdr_r);
                linear.push(hdr_g);
                linear.push(hdr_b);

                let sdr_r = ((hdr_r / (hdr_r + 1.0)).powf(1.0 / 2.2) * 255.0).clamp(0.0, 255.0) as u8;
                let sdr_g = ((hdr_g / (hdr_g + 1.0)).powf(1.0 / 2.2) * 255.0).clamp(0.0, 255.0) as u8;
                let sdr_b = ((hdr_b / (hdr_b + 1.0)).powf(1.0 / 2.2) * 255.0).clamp(0.0, 255.0) as u8;

                sdr.push(sdr_r);
                sdr.push(sdr_g);
                sdr.push(sdr_b);
            }
        }

        let loaded = LoadedImage {
            filename: "demo_coucher_de_soleil_ultrahdr.jpg".to_string(),
            format_name: "Ultra HDR Synthétique Démo".to_string(),
            file_size_bytes: (width * height * 3) as u64,
            source_bit_depth: 16,
            width,
            height,
            is_native_hdr: true,
            dynamic_range_stops: 10.8,
            exif_bytes: None,
            sdr_uint8: sdr,
            linear_rgb: linear,
        };

        self.setup_loaded_image(loaded, ctx);
    }

#[cfg(target_os = "android")]
pub fn trigger_android_photo_picker() {
    let ctx = ndk_context::android_context();
    let vm_ptr = ctx.vm();
    if vm_ptr.is_null() {
        return;
    }
    unsafe {
        if let Ok(vm) = jni::JavaVM::from_raw(vm_ptr as *mut _) {
            if let Ok(mut env) = vm.attach_current_thread() {
                if let Ok(class) = env.find_class("com/ultrahdr/studio/MainActivity") {
                    let _ = env.call_static_method(class, "openImagePicker", "()V", &[]);
                }
            }
        }
    }
}

    /// Déclenche l'ouverture de fichier selon la plateforme (rfd sur desktop, sélecteur photo Android + modal sur mobile).
    pub fn trigger_pick_file(&mut self, ctx: &egui::Context) {
        #[cfg(not(target_os = "android"))]
        {
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("Images (*.jpg, *.png, *.webp, *.hdr, *.exr, *.tif)", &["jpg", "jpeg", "png", "webp", "hdr", "exr", "tif", "tiff", "bmp"])
                .pick_file()
            {
                self.load_file(&path, ctx);
            }
        }
        #[cfg(target_os = "android")]
        {
            Self::trigger_android_photo_picker();
            self.show_open_dialog = true;
        }
    }

    fn set_status(&mut self, msg: &str, color: Color32) {
        self.status_message = Some((msg.to_string(), Instant::now(), color));
    }

    /// Recalcul ultra-rapide (< 4ms) sur le tampon de prévisualisation 1280px.
    fn recompute_and_update_textures(&mut self, ctx: &egui::Context) {
        let preview = match &self.preview_buffer {
            Some(p) => p,
            None => return,
        };
        let is_native_hdr = self.loaded_image.as_ref().map(|i| i.is_native_hdr).unwrap_or(false);

        let w = preview.width;
        let h = preview.height;
        let p = &self.params;

        let tonal = TonalParams {
            contrast: p.tonal_contrast,
            whites: p.tonal_whites,
            highlights: p.tonal_highlights,
            shadows: p.tonal_shadows,
            blacks: p.tonal_blacks,
            saturation: p.tonal_saturation,
            vibrance: p.tonal_vibrance,
        };

        // 1. Calcul Gain Map et HDR ultra-rapide sur la taille de prévisualisation
        let res = compute_hdr_and_gain_map(
            &preview.sdr_uint8,
            preview.linear_rgb.as_deref(),
            is_native_hdr,
            w,
            h,
            p.peak_nits,
            p.sdr_white,
            p.shadow_boost,
            p.hi_gamma,
            p.highlight_threshold,
            tonal,
            p.anti_halo,
        );

        self.gain_min = res.gain_min;
        self.gain_max = res.gain_max;
        self.preview_multiplier = (res.gain_max.exp2() * 10.0).round() / 10.0;

        // Adaptation proportionnelle ISO 21496-1 pour la prévisualisation écran:
        // Si la cible d'export dépasse les capacités physiques de l'écran (ex: 10 000 nits sur écran 400 nits),
        // on adapte proportionnellement la dynamique affichée pour éliminer tout cramage en prévisualisation,
        // tandis que le fichier exporté conserve rigoureusement la pleine dynamique mathématique.
        let sdr_source = res.sdr_processed.as_deref().unwrap_or(&preview.sdr_uint8);
        let preview_hdr_nits = adapt_hdr_nits_for_display(
            &res.hdr_nits,
            &res.gain_log2,
            sdr_source,
            w,
            h,
            p.peak_nits,
            self.display_peak_nits,
            p.sdr_white,
        );

        // 2. Ne régénérer QUE la texture du mode actif pour garantir 60 FPS constants
        match self.mode {
            ViewerMode::Split | ViewerMode::SideBySide | ViewerMode::FullHdr => {
                let tone_mode = if self.is_hdr_display_active { "linear" } else { &p.tone_map_mode };
                let sim_u8 = simulate_hdr_on_sdr(
                    &preview_hdr_nits,
                    w,
                    h,
                    p.sdr_white,
                    p.hdr_boost,
                    p.highlight_rolloff,
                    tone_mode,
                );
                let color_img_hdr = egui::ColorImage::from_rgb([w as usize, h as usize], &sim_u8);
                self.tex_hdr_preview = Some(ctx.load_texture("hdr_prev", color_img_hdr, egui::TextureOptions::LINEAR));
            }
            ViewerMode::GainMap => {
                let gain_range = (res.gain_max - res.gain_min).max(1e-3);
                let mut gm_rgb = Vec::with_capacity((w * h * 3) as usize);
                for &g in &res.gain_log2 {
                    let val = (((g - res.gain_min) / gain_range * 255.0 + 0.5).clamp(0.0, 255.0)) as u8;
                    gm_rgb.push(val);
                    gm_rgb.push(val);
                    gm_rgb.push(val);
                }
                let color_img_gm = egui::ColorImage::from_rgb([w as usize, h as usize], &gm_rgb);
                self.tex_gain_map = Some(ctx.load_texture("gain_map", color_img_gm, egui::TextureOptions::LINEAR));
            }
            ViewerMode::Heatmap => {
                let heat_u8 = generate_nits_heatmap(&preview_hdr_nits, w, h, p.sdr_white, p.peak_nits);
                let color_img_heat = egui::ColorImage::from_rgb([w as usize, h as usize], &heat_u8);
                self.tex_heatmap = Some(ctx.load_texture("heatmap", color_img_heat, egui::TextureOptions::LINEAR));
            }
        }
    }

    /// Exportation binaire Ultra HDR de PLEINE RÉSOLUTION (conforme ISO 21496-1 / Google Ultra HDR).
    fn export_ultrahdr(&mut self) {
        let (stem, w, h) = match &self.loaded_image {
            Some(i) => {
                let stem = Path::new(&i.filename).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "image".into());
                (stem, i.width, i.height)
            }
            None => return,
        };

        let default_out = format!("{}_UltraHDR.jpg", stem);

        #[cfg(not(target_os = "android"))]
        let target_path = match rfd::FileDialog::new()
            .set_file_name(&default_out)
            .add_filter("JPEG Ultra HDR (*.jpg)", &["jpg", "jpeg"])
            .save_file()
        {
            Some(p) => p,
            None => return,
        };

        #[cfg(target_os = "android")]
        let target_path = {
            let pictures_dir = PathBuf::from("/sdcard/Pictures/UltraHDR");
            let _ = std::fs::create_dir_all(&pictures_dir);
            pictures_dir.join(&default_out)
        };

        self.set_status("Calcul et encodage pleine résolution en cours...", Color32::from_rgb(0, 200, 255));

        let img = self.loaded_image.as_ref().unwrap();
        let p = &self.params;

        let tonal = TonalParams {
            contrast: p.tonal_contrast,
            whites: p.tonal_whites,
            highlights: p.tonal_highlights,
            shadows: p.tonal_shadows,
            blacks: p.tonal_blacks,
            saturation: p.tonal_saturation,
            vibrance: p.tonal_vibrance,
        };

        // Calcul sur la pleine résolution native de l'image
        let res = compute_hdr_and_gain_map(
            &img.sdr_uint8,
            if img.is_native_hdr { Some(&img.linear_rgb) } else { None },
            img.is_native_hdr,
            w,
            h,
            p.peak_nits,
            p.sdr_white,
            p.shadow_boost,
            p.hi_gamma,
            p.highlight_threshold,
            tonal,
            p.anti_halo,
        );

        let sdr_to_encode = res.sdr_processed.as_deref().unwrap_or(&img.sdr_uint8);
        let base_jpeg = match encode_rgb_jpeg(sdr_to_encode, w, h, self.base_quality) {
            Ok(b) => b,
            Err(e) => {
                self.set_status(&format!("Erreur encodage base JPEG : {}", e), Color32::from_rgb(231, 76, 60));
                return;
            }
        };

        let gm_jpeg = match encode_gain_map_jpeg(&res.gain_log2, res.gain_min, res.gain_max, w, h, self.gainmap_quality) {
            Ok(b) => b,
            Err(e) => {
                self.set_status(&format!("Erreur encodage Gain Map : {}", e), Color32::from_rgb(231, 76, 60));
                return;
            }
        };

        let uhdr_bytes = assemble_ultra_hdr_jpeg(
            &base_jpeg,
            &gm_jpeg,
            res.gain_min,
            res.gain_max,
            p.peak_nits,
            p.sdr_white,
            img.exif_bytes.as_deref(),
        );

        if let Err(e) = std::fs::write(&target_path, &uhdr_bytes) {
            self.set_status(&format!("Erreur écriture : {}", e), Color32::from_rgb(231, 76, 60));
            return;
        }

        if self.save_standalone_gainmap {
            let gm_path = target_path.with_file_name(format!("{}_gainmap.jpg", stem));
            let _ = std::fs::write(&gm_path, &gm_jpeg);
        }

        self.set_status(
            &format!("Enregistré avec succès : {} ({} Ko, {}×{})", target_path.display(), uhdr_bytes.len() / 1024, w, h),
            Color32::from_rgb(46, 204, 113),
        );
    }

    /// Traitement par lot multithreadé (Rayon) de tout un dossier en JPEG Ultra HDR.
    fn run_batch_conversion(&mut self) {
        let in_dir = PathBuf::from(self.batch_input_dir.trim());
        if !in_dir.is_dir() {
            self.batch_status_msg = Some("Dossier source invalide ou introuvable.".to_string());
            return;
        }

        let out_dir = if self.batch_output_dir.trim().is_empty() {
            in_dir.clone()
        } else {
            PathBuf::from(self.batch_output_dir.trim())
        };
        let _ = std::fs::create_dir_all(&out_dir);

        let valid_exts = [
            "jpg", "jpeg", "png", "webp", "tif", "tiff", "bmp", "hdr", "exr", "dng", "cr2", "nef", "arw"
        ];
        let files: Vec<PathBuf> = match std::fs::read_dir(&in_dir) {
            Ok(entries) => entries
                .flatten()
                .map(|e| e.path())
                .filter(|p| {
                    p.is_file()
                        && p.extension()
                            .and_then(|ext| ext.to_str())
                            .map(|ext| valid_exts.contains(&ext.to_lowercase().as_str()))
                            .unwrap_or(false)
                })
                .collect(),
            Err(e) => {
                self.batch_status_msg = Some(format!("Erreur lecture dossier : {}", e));
                return;
            }
        };

        if files.is_empty() {
            self.batch_status_msg = Some("Aucun fichier image compatible trouvé dans ce dossier.".to_string());
            return;
        }

        let total = files.len();
        let p_copy = &self.params;
        let base_q = self.base_quality;
        let gm_q = self.gainmap_quality;

        let success_count = files
            .into_par_iter()
            .map(|file_path| {
                if let Ok(img) = load_image(&file_path) {
                    let tonal = TonalParams {
                        contrast: p_copy.tonal_contrast,
                        whites: p_copy.tonal_whites,
                        highlights: p_copy.tonal_highlights,
                        shadows: p_copy.tonal_shadows,
                        blacks: p_copy.tonal_blacks,
                        saturation: p_copy.tonal_saturation,
                        vibrance: p_copy.tonal_vibrance,
                    };
                    let res = compute_hdr_and_gain_map(
                        &img.sdr_uint8,
                        if img.is_native_hdr { Some(&img.linear_rgb) } else { None },
                        img.is_native_hdr,
                        img.width,
                        img.height,
                        p_copy.peak_nits,
                        p_copy.sdr_white,
                        p_copy.shadow_boost,
                        p_copy.hi_gamma,
                        p_copy.highlight_threshold,
                        tonal,
                        p_copy.anti_halo,
                    );
                    let sdr_to_encode = res.sdr_processed.as_deref().unwrap_or(&img.sdr_uint8);
                    if let (Ok(base_jpg), Ok(gm_jpg)) = (
                        encode_rgb_jpeg(sdr_to_encode, img.width, img.height, base_q),
                        encode_gain_map_jpeg(&res.gain_log2, res.gain_min, res.gain_max, img.width, img.height, gm_q),
                    ) {
                        let uhdr = assemble_ultra_hdr_jpeg(
                            &base_jpg,
                            &gm_jpg,
                            res.gain_min,
                            res.gain_max,
                            p_copy.peak_nits,
                            p_copy.sdr_white,
                            img.exif_bytes.as_deref(),
                        );
                        let stem = file_path.file_stem().map(|s| s.to_string_lossy()).unwrap_or_else(|| "img".into());
                        let out_file = out_dir.join(format!("{}_UltraHDR.jpg", stem));
                        if std::fs::write(&out_file, &uhdr).is_ok() {
                            return 1;
                        }
                    }
                }
                0
            })
            .sum::<usize>();

        self.batch_status_msg = Some(format!("Terminé avec succès : {}/{} photos converties !", success_count, total));
        self.set_status(
            &format!("Traitement par lot terminé : {}/{} images converties.", success_count, total),
            Color32::from_rgb(46, 204, 113),
        );
    }
}

impl eframe::App for UltraHdrApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // ── Raccourcis Clavier Ergonomiques ──
        if ctx.input(|i| i.key_pressed(egui::Key::Tab)) {
            let any_open = self.left_panel_visible || self.right_panel_visible || self.bottom_panel_visible;
            self.left_panel_visible = !any_open;
            self.right_panel_visible = !any_open;
            self.bottom_panel_visible = !any_open;
            self.sidebar_visible = !any_open;
        }
        if ctx.input(|i| i.key_pressed(egui::Key::F1)) {
            self.left_panel_visible = !self.left_panel_visible;
        }
        if ctx.input(|i| i.key_pressed(egui::Key::F2)) {
            self.right_panel_visible = !self.right_panel_visible;
        }
        if ctx.input(|i| i.key_pressed(egui::Key::F3)) {
            self.bottom_panel_visible = !self.bottom_panel_visible;
        }
        if ctx.input(|i| i.key_pressed(egui::Key::F11)) {
            let is_fs = ctx.input(|i| i.viewport().fullscreen.unwrap_or(false));
            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(!is_fs));
        }
        // Touche M : cycle les modes d'affichage
        if ctx.input(|i| i.key_pressed(egui::Key::M)) {
            self.mode = match self.mode {
                ViewerMode::Split => ViewerMode::SideBySide,
                ViewerMode::SideBySide => ViewerMode::FullHdr,
                ViewerMode::FullHdr => ViewerMode::GainMap,
                ViewerMode::GainMap => ViewerMode::Heatmap,
                ViewerMode::Heatmap => ViewerMode::Split,
            };
            self.recompute_and_update_textures(ctx);
        }
        // Touche Backslash ou Slash : bascule le mode comparaison instantanée
        if ctx.input(|i| i.key_pressed(egui::Key::Backslash) || i.key_pressed(egui::Key::Slash)) {
            self.compare_toggled = !self.compare_toggled;
        }
        if ctx.input(|i| i.key_pressed(egui::Key::T)) {
            self.targeted_tool_active = !self.targeted_tool_active;
            if !self.targeted_tool_active {
                self.hovered_zone = None;
            }
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            if self.targeted_tool_active {
                self.targeted_tool_active = false;
                self.hovered_zone = None;
            }
            self.compare_toggled = false;
        }

        // Glisser-Déposer de fichier (Drag & Drop)
        if !ctx.input(|i| i.raw.dropped_files.is_empty()) {
            let dropped = ctx.input(|i| i.raw.dropped_files.clone());
            if let Some(first) = dropped.first() {
                if let Some(path) = &first.path {
                    self.load_file(path, ctx);
                }
            }
        }

        // ── Importation automatique depuis la galerie Android (via MainActivity) ──
        #[cfg(target_os = "android")]
        {
            let cache_dirs = [
                PathBuf::from("/data/data/com.ultrahdr.studio/cache"),
                PathBuf::from("/data/user/0/com.ultrahdr.studio/cache"),
            ];
            for c_dir in &cache_dirs {
                let flag = c_dir.join("photo_ready.flag");
                let photo = c_dir.join("imported_photo.jpg");
                if flag.exists() && photo.exists() {
                    let _ = std::fs::remove_file(&flag);
                    self.load_file(&photo, ctx);
                    self.show_open_dialog = false;
                    self.set_status("Photo importée depuis la galerie Android !", Color32::from_rgb(118, 214, 213));
                    break;
                }
            }
        }

        // ── Détection de l'orientation et mode responsive multi-formats Android ──
        let is_android = cfg!(target_os = "android");
        let form_factor = crate::theme_material::apply_material_3_theme(ctx, is_android);
        let screen_rect = ctx.screen_rect();
        let is_vertical = form_factor.is_bottom_sheet() || (screen_rect.width() < screen_rect.height()) || screen_rect.width() < 840.0;

        // ── 1. Barre Supérieure (Header Responsive Material Design) ──
        egui::TopBottomPanel::top("top_panel")
            .frame(egui::Frame::none().fill(Color32::from_rgb(15, 17, 24)).inner_margin(egui::Margin::symmetric(if is_vertical { 8.0 } else { 14.0 }, 6.0)))
            .show(ctx, |ui| {
                if is_vertical {
                    // En-tête compact optimisé mobile / écran vertical : AUCUNE superposition
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("✨ UltraHDR").size(15.0).strong().color(Color32::from_rgb(0, 200, 255)));

                        if let Some(img) = &self.loaded_image {
                            let display_name = if img.filename.len() > 10 {
                                format!("{}...", &img.filename[..8])
                            } else {
                                img.filename.clone()
                            };
                            draw_pill(ui, &display_name, Color32::from_rgb(0, 120, 212), Color32::WHITE);
                        }

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let export_enabled = self.loaded_image.is_some();
                            let btn_export = egui::Button::new(
                                egui::RichText::new("💾").strong().color(Color32::WHITE).size(13.0),
                            )
                            .fill(if export_enabled { Color32::from_rgb(0, 120, 212) } else { Color32::from_rgb(40, 45, 55) })
                            .rounding(Rounding::same(6.0));

                            if ui.add_enabled(export_enabled, btn_export).on_hover_text("Exporter JPEG Ultra HDR").clicked() {
                                self.export_ultrahdr();
                            }

                            let btn_open = egui::Button::new(
                                egui::RichText::new("📂").color(Color32::WHITE).size(13.0),
                            )
                            .fill(Color32::from_rgb(35, 40, 55))
                            .rounding(Rounding::same(6.0));

                            if ui.add(btn_open).on_hover_text("Ouvrir une image...").clicked() {
                                self.trigger_pick_file(ctx);
                            }

                            let btn_batch = egui::Button::new(
                                egui::RichText::new("⚡").size(12.0).color(Color32::from_rgb(210, 220, 240)),
                            )
                            .fill(Color32::from_rgb(32, 36, 50))
                            .rounding(Rounding::same(6.0));

                            if ui.add(btn_batch).on_hover_text("Traitement par lot...").clicked() {
                                self.show_batch_dialog = true;
                            }
                        });
                    });
                } else {
                    // En-tête standard bureau étendu
                    ui.horizontal(|ui| {
                        // Logo de marque avec numéro de version officiel
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new("✨").size(18.0));
                            ui.label(egui::RichText::new("Ultra").size(17.0).strong().color(Color32::WHITE));
                            ui.label(egui::RichText::new("HDR").size(17.0).strong().color(Color32::from_rgb(0, 200, 255)));
                            ui.label(egui::RichText::new("Studio").size(17.0).color(Color32::from_rgb(180, 190, 210)));
                            draw_pill(ui, &format!("v{}", env!("CARGO_PKG_VERSION")), Color32::from_rgb(0, 110, 220), Color32::WHITE);
                        });

                        ui.add_space(8.0);
                        ui.separator();
                        ui.add_space(8.0);

                        // Badge d'état matériel physique fixe (indicateur d'état système non commutable par clic)
                        let (badge_bg, badge_border, badge_text_col, badge_text, hover_tip) = if self.is_hdr_display_active {
                            (
                                Color32::from_rgb(14, 45, 26),
                                Color32::from_rgb(34, 197, 94),
                                Color32::from_rgb(220, 252, 231),
                                format!("🟢 ÉCRAN HDR ACTIF • {} (Pic {:.0} nits)", self.display_info_str, self.display_peak_nits),
                                "Écran HDR matériel détecté et actif dans Windows (DirectX scRGB). Rendu physique direct.",
                            )
                        } else {
                            (
                                Color32::from_rgb(20, 32, 52),
                                Color32::from_rgb(56, 189, 248),
                                Color32::from_rgb(224, 242, 254),
                                format!("🖥️ ÉCRAN SDR DÉTECTÉ • {} (Capacité {:.0} nits)", self.display_info_str, self.display_peak_nits),
                                "Écran SDR standard détecté (ou HDR désactivé dans Windows). La simulation et le tonemapping sont actifs.",
                            )
                        };

                        egui::Frame::none()
                            .fill(badge_bg)
                            .stroke(Stroke::new(1.0f32, badge_border))
                            .rounding(Rounding::same(14.0))
                            .inner_margin(egui::Margin::symmetric(12.0, 5.0))
                            .show(ui, |ui| {
                                ui.label(egui::RichText::new(badge_text).size(12.0).strong().color(badge_text_col));
                            })
                            .response
                            .on_hover_text(hover_tip);

                        // Pills d'informations sur l'image chargée
                        if let Some(img) = &self.loaded_image {
                            ui.add_space(10.0);
                            ui.separator();
                            ui.add_space(10.0);

                            draw_pill(ui, &img.filename, Color32::from_rgb(0, 120, 212), Color32::WHITE);
                            draw_pill(ui, &img.format_name, Color32::from_rgb(35, 40, 55), Color32::from_rgb(200, 210, 230));
                            draw_pill(ui, &format!("{}×{}", img.width, img.height), Color32::from_rgb(35, 40, 55), Color32::from_rgb(200, 210, 230));
                            draw_pill(
                                ui,
                                &format!("+{} IL Boost", (self.gain_max * 10.0).round() / 10.0),
                                Color32::from_rgb(60, 45, 15),
                                Color32::from_rgb(255, 195, 40),
                            );
                        }

                        // Boutons d'actions et bascules de panneaux à droite
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            // Bascule Panneau Droit [F2]
                            let btn_right = egui::Button::new(
                                egui::RichText::new("◨ Tonalité [F2]").size(11.5).color(if self.right_panel_visible { Color32::WHITE } else { Color32::from_rgb(140, 150, 175) }),
                            )
                            .fill(if self.right_panel_visible { Color32::from_rgb(30, 42, 65) } else { Color32::from_rgb(20, 22, 30) })
                            .stroke(Stroke::new(1.0f32, if self.right_panel_visible { Color32::from_rgb(0, 180, 255) } else { Color32::from_rgb(40, 45, 60) }))
                            .rounding(Rounding::same(6.0));

                            if ui.add(btn_right).on_hover_text("Afficher/Masquer le panneau droit : Tonalité Pro, Courbe, Vibrance [F2]").clicked() {
                                self.right_panel_visible = !self.right_panel_visible;
                            }

                            // Bascule Dock Inférieur [F3]
                            let btn_bottom = egui::Button::new(
                                egui::RichText::new("⬓ Export & Simu [F3]").size(11.5).color(if self.bottom_panel_visible { Color32::WHITE } else { Color32::from_rgb(140, 150, 175) }),
                            )
                            .fill(if self.bottom_panel_visible { Color32::from_rgb(30, 42, 65) } else { Color32::from_rgb(20, 22, 30) })
                            .stroke(Stroke::new(1.0f32, if self.bottom_panel_visible { Color32::from_rgb(0, 180, 255) } else { Color32::from_rgb(40, 45, 60) }))
                            .rounding(Rounding::same(6.0));

                            if ui.add(btn_bottom).on_hover_text("Afficher/Masquer le panneau inférieur : Simulation SDR, Tone-Mapping, Export [F3]").clicked() {
                                self.bottom_panel_visible = !self.bottom_panel_visible;
                            }

                            // Bascule Panneau Gauche [F1]
                            let btn_left = egui::Button::new(
                                egui::RichText::new("◧ Source & Profils [F1]").size(11.5).color(if self.left_panel_visible { Color32::WHITE } else { Color32::from_rgb(140, 150, 175) }),
                            )
                            .fill(if self.left_panel_visible { Color32::from_rgb(30, 42, 65) } else { Color32::from_rgb(20, 22, 30) })
                            .stroke(Stroke::new(1.0f32, if self.left_panel_visible { Color32::from_rgb(0, 180, 255) } else { Color32::from_rgb(40, 45, 60) }))
                            .rounding(Rounding::same(6.0));

                            if ui.add(btn_left).on_hover_text("Afficher/Masquer le panneau gauche : Analyse, Profils HDR, Moteur [F1]").clicked() {
                                self.left_panel_visible = !self.left_panel_visible;
                            }

                            ui.add_space(4.0);
                            ui.separator();
                            ui.add_space(4.0);

                            // Bouton Traitement par lot
                            let btn_batch = egui::Button::new(
                                egui::RichText::new("⚡ Lot...").size(12.0).color(Color32::from_rgb(210, 220, 240)),
                            )
                            .fill(Color32::from_rgb(32, 36, 50))
                            .rounding(Rounding::same(6.0));

                            if ui.add(btn_batch).on_hover_text("Convertit en masse tout un répertoire en JPEG Ultra HDR.").clicked() {
                                self.show_batch_dialog = true;
                            }

                            let export_enabled = self.loaded_image.is_some();
                            let btn_export = egui::Button::new(
                                egui::RichText::new("💾 Exporter").strong().color(Color32::WHITE).size(13.0),
                            )
                            .fill(if export_enabled { Color32::from_rgb(0, 120, 212) } else { Color32::from_rgb(40, 45, 55) })
                            .rounding(Rounding::same(6.0));

                            if ui.add_enabled(export_enabled, btn_export).clicked() {
                                self.export_ultrahdr();
                            }

                            let btn_open = egui::Button::new(
                                egui::RichText::new("📂 Ouvrir").color(Color32::WHITE).size(13.0),
                            )
                            .fill(Color32::from_rgb(35, 40, 55))
                            .rounding(Rounding::same(6.0));

                            if ui.add(btn_open).clicked() {
                                self.trigger_pick_file(ctx);
                            }
                        });
                    });
                }
            });

        // Boîte de dialogue chemin manuel si nécessaire
        if self.show_open_dialog {
            egui::Window::new("Ouvrir un fichier image")
                .collapsible(false)
                .resizable(false)
                .default_width(360.0)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.vertical_centered(|ui| {
                        #[cfg(target_os = "android")]
                        {
                            let btn_picker = egui::Button::new(
                                egui::RichText::new("📱 Ouvrir Galerie Photos & Fichiers").strong().size(15.0).color(Color32::WHITE),
                            )
                            .fill(Color32::from_rgb(0, 150, 136))
                            .rounding(Rounding::same(24.0));

                            if ui.add_sized([ui.available_width(), 48.0], btn_picker).clicked() {
                                Self::trigger_android_photo_picker();
                            }

                            ui.add_space(8.0);
                        }

                        let btn_demo = egui::Button::new(
                            egui::RichText::new("🌅 Charger la Démo Ultra HDR (1000+ nits)").strong().size(14.0).color(Color32::WHITE),
                        )
                        .fill(Color32::from_rgb(0, 120, 212))
                        .rounding(Rounding::same(24.0));

                        if ui.add_sized([ui.available_width(), 48.0], btn_demo).clicked() {
                            self.load_demo_image(ctx);
                            self.show_open_dialog = false;
                        }
                    });

                    ui.add_space(8.0);
                    ui.separator();
                    ui.add_space(8.0);

                    // Dossiers photos Android standards
                    let standard_dirs = [
                        ("/sdcard/DCIM/Camera", "📸 Photos Appareil (DCIM)"),
                        ("/sdcard/Pictures", "🖼️ Dossier Images"),
                        ("/sdcard/Download", "📥 Téléchargements"),
                    ];

                    let mut file_to_load = None;
                    for (dir_path, dir_label) in standard_dirs {
                        let p = Path::new(dir_path);
                        if p.exists() && p.is_dir() {
                            ui.label(egui::RichText::new(dir_label).strong().size(12.5).color(Color32::from_rgb(138, 180, 248)));
                            if let Ok(entries) = std::fs::read_dir(p) {
                                let mut images: Vec<PathBuf> = entries
                                    .filter_map(|e| e.ok().map(|e| e.path()))
                                    .filter(|p| {
                                        p.extension()
                                            .and_then(|ext| ext.to_str())
                                            .map(|ext| matches!(ext.to_lowercase().as_str(), "jpg" | "jpeg" | "png" | "webp" | "hdr" | "exr"))
                                            .unwrap_or(false)
                                    })
                                    .take(6)
                                    .collect();
                                images.sort_by(|a, b| b.cmp(a));
                                for img_path in images {
                                    let fname = img_path.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default();
                                    if ui.button(format!("📷 {}", fname)).clicked() {
                                        file_to_load = Some(img_path);
                                    }
                                }
                            }
                            ui.add_space(4.0);
                        }
                    }

                    if let Some(target) = file_to_load {
                        self.load_file(&target, ctx);
                        self.show_open_dialog = false;
                    }

                    ui.label("Ou entrez un chemin complet :");
                    ui.text_edit_singleline(&mut self.open_path_input);
                    ui.add_space(4.0);

                    ui.horizontal(|ui| {
                        if ui.button("Ouvrir ce chemin").clicked() {
                            let path = PathBuf::from(self.open_path_input.trim());
                            if path.exists() {
                                self.load_file(&path, ctx);
                                self.show_open_dialog = false;
                            } else {
                                self.set_status("Chemin introuvable", Color32::from_rgb(231, 76, 60));
                            }
                        }
                        if ui.button("Fermer").clicked() {
                            self.show_open_dialog = false;
                        }
                    });
                });
        }

        // Boîte de dialogue Traitement par lot
        if self.show_batch_dialog {
            egui::Window::new("⚡ Traitement par lot (Dossier complet)")
                .collapsible(false)
                .resizable(false)
                .default_width(460.0)
                .show(ctx, |ui| {
                    ui.label("Convertit automatiquement toutes les photos d'un dossier en JPEG Ultra HDR.");
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        ui.label("Dossier source :");
                        ui.text_edit_singleline(&mut self.batch_input_dir);
                        #[cfg(not(target_os = "android"))]
                        if ui.button("Parcourir...").clicked() {
                            if let Some(path) = rfd::FileDialog::new().pick_folder() {
                                self.batch_input_dir = path.to_string_lossy().to_string();
                            }
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label("Dossier cible (opt.) :");
                        ui.text_edit_singleline(&mut self.batch_output_dir);
                        #[cfg(not(target_os = "android"))]
                        if ui.button("Parcourir...").clicked() {
                            if let Some(path) = rfd::FileDialog::new().pick_folder() {
                                self.batch_output_dir = path.to_string_lossy().to_string();
                            }
                        }
                    });
                    ui.add_space(8.0);
                    if let Some(msg) = &self.batch_status_msg {
                        ui.colored_label(Color32::from_rgb(0, 200, 255), msg);
                        ui.add_space(6.0);
                    }
                    ui.horizontal(|ui| {
                        let can_run = !self.batch_input_dir.trim().is_empty();
                        let btn_run = egui::Button::new(
                            egui::RichText::new("🚀 Lancer la conversion").strong().color(Color32::WHITE),
                        )
                        .fill(if can_run { Color32::from_rgb(0, 120, 212) } else { Color32::from_rgb(45, 50, 65) });

                        if ui.add_enabled(can_run, btn_run).clicked() {
                            self.run_batch_conversion();
                        }
                        if ui.button("Fermer").clicked() {
                            self.show_batch_dialog = false;
                        }
                    });
                });
        }

        // ── 2. Barre d'État Inférieure (Footer Bureau Uniquement) ──
        if !is_vertical {
            egui::TopBottomPanel::bottom("bottom_status_bar")
                .frame(egui::Frame::none().fill(Color32::from_rgb(14, 15, 20)).inner_margin(egui::Margin::symmetric(14.0, 5.0)))
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        if let Some((msg, time, col)) = &self.status_message {
                            if time.elapsed().as_secs() < 10 {
                                ui.colored_label(*col, msg);
                            } else {
                                ui.label(egui::RichText::new("Glissez-déposez n'importe quelle photo directement dans la fenêtre.").color(Color32::from_rgb(140, 145, 165)));
                            }
                        } else {
                            ui.label(egui::RichText::new("Glissez-déposez n'importe quelle photo directement dans la fenêtre.").color(Color32::from_rgb(140, 145, 165)));
                        }

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(egui::RichText::new(format!(
                                "UltraHDR Studio v{} • Norme ISO 21496-1 • Adobe XMP (hdrgm) • CIPA DC-007 MPF",
                                env!("CARGO_PKG_VERSION")
                            )).size(11.0).color(Color32::from_rgb(110, 115, 135)));
                        });
                    });
                });
        }

        // ── 3. Panneau Latéral Gauche (Source, Analyse, Profils HDR, Moteur) ──
        if !is_vertical && self.left_panel_visible {
            egui::SidePanel::left("left_panel")
                .min_width(280.0)
                .default_width(315.0)
                .max_width(380.0)
                .frame(egui::Frame::none().fill(Color32::from_rgb(18, 20, 28)).inner_margin(egui::Margin::symmetric(10.0, 8.0)))
                .show(ctx, |ui| {
                    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                        self.ui_left_panel(ui, ctx);
                    });
                });
        }

        // ── 4. Panneau Latéral Droit (Tonalité Pro, Courbe, Vibrance/Saturation) ──
        if !is_vertical && self.right_panel_visible {
            egui::SidePanel::right("right_panel")
                .min_width(290.0)
                .default_width(325.0)
                .max_width(400.0)
                .frame(egui::Frame::none().fill(Color32::from_rgb(18, 20, 28)).inner_margin(egui::Margin::symmetric(10.0, 8.0)))
                .show(ctx, |ui| {
                    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                        self.ui_right_panel(ui, ctx);
                    });
                });
        }

        // ── 5. Panneau Inférieur (Deck de Commande Vertical ou Dock Bureau) ──
        if is_vertical {
            let total_h = screen_rect.height();
            let deck_h = (total_h * 0.40).clamp(200.0, 310.0);
            egui::TopBottomPanel::bottom("vertical_control_deck")
                .exact_height(deck_h)
                .resizable(false)
                .frame(egui::Frame::none().fill(Color32::from_rgb(16, 18, 26)).inner_margin(egui::Margin::symmetric(8.0, 6.0)))
                .show(ctx, |ui| {
                    self.ui_vertical_control_deck(ui, ctx);
                });
        } else if self.bottom_panel_visible {
            egui::TopBottomPanel::bottom("bottom_dock")
                .min_height(148.0)
                .max_height(185.0)
                .frame(egui::Frame::none().fill(Color32::from_rgb(16, 18, 26)).inner_margin(egui::Margin::symmetric(14.0, 10.0)))
                .show(ctx, |ui| {
                    self.ui_bottom_dock(ui, ctx);
                });
        }

        // ── 6. Panneau Central (Visionneuse d'Image Haute Performance) ──
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(Color32::from_rgb(12, 13, 18)))
            .show(ctx, |ui| {
                self.ui_center_viewport(ui, ctx, is_vertical);
            });
    }
}

impl UltraHdrApp {
    fn ui_left_panel(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        // En-tête du panneau gauche
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("📁 Source & Profils").strong().size(13.5).color(Color32::WHITE));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.link(egui::RichText::new("⮜ Masquer [F1]").size(11.0).color(Color32::from_rgb(140, 150, 175))).clicked() {
                    self.left_panel_visible = false;
                }
            });
        });
        ui.add_space(4.0);
        self.ui_panel_engine_content(ui, ctx, false);
    }

    fn ui_panel_engine_content(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, is_vertical: bool) {
        let mut params_changed = false;
        let mut preset_to_apply = None;

        let card_frame = egui::Frame::none()
            .fill(Color32::from_rgb(22, 24, 33))
            .stroke(Stroke::new(1.0f32, Color32::from_rgb(42, 46, 64)))
            .rounding(Rounding::same(8.0))
            .inner_margin(egui::Margin::symmetric(12.0, 10.0));

        // ── 1. CARTE : 💡 Proposition Automatique & Analyse ──
        let proposal_score = self.analysis.as_ref().map(|a| a.potential_score).unwrap_or(0);
        let badge_text = if self.analysis.is_some() { format!("{}%", proposal_score) } else { "--%".to_string() };

        card_frame.show(ui, |ui| {
            let chevron = if self.card_proposal_open { "▾" } else { "▸" };
            let header_resp = ui.horizontal(|ui| {
                ui.label(egui::RichText::new(chevron).size(13.0).color(Color32::from_rgb(150, 155, 175)));
                ui.label(egui::RichText::new("💡").size(14.0));
                ui.label(egui::RichText::new("Proposition Automatique").strong().size(13.0).color(Color32::WHITE));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    draw_pill(ui, &badge_text, Color32::from_rgb(30, 35, 48), Color32::from_rgb(46, 204, 113));
                });
            });

            if header_resp.response.interact(egui::Sense::click()).clicked() {
                self.card_proposal_open = !self.card_proposal_open;
            }

            if self.card_proposal_open {
                ui.add_space(6.0);
                ui.separator();
                ui.add_space(6.0);

                if let Some(analysis) = &self.analysis {
                    ui.label(egui::RichText::new(&analysis.category_title).strong().size(13.0).color(Color32::from_rgb(0, 200, 255)));
                    ui.label(egui::RichText::new(&analysis.explanation).small().color(Color32::from_rgb(180, 185, 200)));

                    ui.add_space(6.0);
                    // Grille de métadonnées (verticale si écran portrait)
                    if is_vertical {
                        draw_meta_box(ui, "Hautes Lumières", &format!("{:.1}%", analysis.highlight_ratio_pct));
                        ui.add_space(3.0);
                        draw_meta_box(ui, "Contraste", &format!("{:.0}:1", analysis.contrast_ratio));
                    } else {
                        ui.horizontal(|ui| {
                            draw_meta_box(ui, "Hautes Lumières", &format!("{:.1}%", analysis.highlight_ratio_pct));
                            draw_meta_box(ui, "Contraste", &format!("{:.0}:1", analysis.contrast_ratio));
                        });
                    }

                    ui.add_space(8.0);
                    let btn_apply = egui::Button::new(
                        egui::RichText::new("✨ Appliquer la proposition recommandée").strong().color(Color32::WHITE),
                    )
                    .fill(Color32::from_rgb(190, 130, 0))
                    .rounding(Rounding::same(6.0));

                    if ui.add_sized([ui.available_width(), 28.0], btn_apply).clicked() {
                        preset_to_apply = Some(analysis.recommended_preset.clone());
                    }

                    ui.add_space(8.0);
                    ui.label(egui::RichText::new("Profils prédéfinis :").size(11.5).color(Color32::from_rgb(160, 165, 180)));

                    // Boutons sélecteurs de profils
                    for preset in get_preset_library() {
                        let is_cur = (self.params.peak_nits - preset.peak_nits).abs() < 1.0
                            && (self.params.highlight_threshold - preset.highlight_threshold).abs() < 0.02;

                        let btn = egui::Button::new(
                            egui::RichText::new(&preset.label).size(11.5).color(if is_cur { Color32::WHITE } else { Color32::from_rgb(190, 195, 215) }),
                        )
                        .fill(if is_cur { Color32::from_rgb(0, 120, 212) } else { Color32::from_rgb(26, 29, 39) })
                        .stroke(Stroke::new(1.0f32, if is_cur { Color32::from_rgb(0, 180, 255) } else { Color32::from_rgb(45, 50, 68) }))
                        .rounding(Rounding::same(5.0));

                        if ui.add_sized([ui.available_width(), 24.0], btn).on_hover_text(&preset.description).clicked() {
                            preset_to_apply = Some(preset);
                        }
                        ui.add_space(2.0);
                    }
                } else {
                    ui.label(egui::RichText::new("En attente d'image...").color(Color32::GRAY));
                    ui.label(egui::RichText::new("Chargez une photo pour analyser son potentiel Ultra HDR.").small().color(Color32::DARK_GRAY));
                }
            }
        });
        ui.add_space(8.0);

        if let Some(preset) = preset_to_apply {
            self.apply_preset(&preset);
            params_changed = true;
        }

        // ── 2. CARTE : ⚙️ Réglages Moteur Ultra HDR ──
        card_frame.show(ui, |ui| {
            let chevron = if self.card_settings_open { "▾" } else { "▸" };
            let header_resp = ui.horizontal(|ui| {
                ui.label(egui::RichText::new(chevron).size(13.0).color(Color32::from_rgb(150, 155, 175)));
                ui.label(egui::RichText::new("⚙️").size(14.0));
                ui.label(egui::RichText::new("Moteur Ultra HDR").strong().size(13.0).color(Color32::WHITE));
            });

            if header_resp.response.interact(egui::Sense::click()).clicked() {
                self.card_settings_open = !self.card_settings_open;
            }

            if self.card_settings_open {
                ui.spacing_mut().slider_width = (ui.available_width() - 4.0).max(180.0);
                ui.add_space(6.0);
                ui.separator();
                ui.add_space(6.0);

                // Peak Nits
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Luminance Crête :").size(12.0).color(Color32::WHITE));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        draw_val_pill(ui, &format!("{:.0} nits", self.params.peak_nits));
                    });
                });
                if ui.add(egui::Slider::new(&mut self.params.peak_nits, 200.0..=10000.0).step_by(50.0).show_value(false)).changed() {
                    params_changed = true;
                }

                ui.add_space(6.0);

                // Gamma
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Courbe Boost (Gamma) :").size(12.0).color(Color32::WHITE));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        draw_val_pill(ui, &format!("{:.2}", self.params.hi_gamma));
                    });
                });
                if ui.add(egui::Slider::new(&mut self.params.hi_gamma, 0.10..=4.00).step_by(0.05).show_value(false)).changed() {
                    params_changed = true;
                }

                ui.add_space(6.0);

                // Préservation Ombres
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Préservation Ombres :").size(12.0).color(Color32::WHITE));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        draw_val_pill(ui, &format!("{:.2}×", self.params.shadow_boost));
                    });
                });
                if ui.add(egui::Slider::new(&mut self.params.shadow_boost, 0.50..=4.00).step_by(0.02).show_value(false)).changed() {
                    params_changed = true;
                }

                ui.add_space(6.0);

                // Seuil Hautes Lumières
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Seuil d'Élévation :").size(12.0).color(Color32::WHITE));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        draw_val_pill(ui, &format!("{:.2}", self.params.highlight_threshold));
                    });
                });
                if ui.add(egui::Slider::new(&mut self.params.highlight_threshold, 0.00..=0.98).step_by(0.02).show_value(false)).changed() {
                    params_changed = true;
                }

                ui.add_space(8.0);

                // Boîte de statistiques
                egui::Frame::none()
                    .fill(Color32::from_rgb(16, 17, 24))
                    .stroke(Stroke::new(1.0f32, Color32::from_rgb(45, 50, 70)))
                    .rounding(Rounding::same(6.0))
                    .inner_margin(egui::Margin::symmetric(8.0, 6.0))
                    .show(ui, |ui| {
                        ui.columns(3, |cols| {
                            cols[0].vertical_centered(|ui| {
                                ui.label(egui::RichText::new("Gain Min").size(10.0).color(Color32::GRAY));
                                ui.label(egui::RichText::new(format!("{:.2} IL", self.gain_min)).strong().size(12.0).color(Color32::WHITE));
                            });
                            cols[1].vertical_centered(|ui| {
                                ui.label(egui::RichText::new("Gain Max").size(10.0).color(Color32::GRAY));
                                ui.label(egui::RichText::new(format!("+{:.2} IL", self.gain_max)).strong().size(12.0).color(Color32::from_rgb(0, 200, 255)));
                            });
                            cols[2].vertical_centered(|ui| {
                                ui.label(egui::RichText::new("Multiplicateur").size(10.0).color(Color32::GRAY));
                                ui.label(egui::RichText::new(format!("{:.1}×", self.preview_multiplier)).strong().size(12.0).color(Color32::from_rgb(255, 195, 40)));
                            });
                        });
                    });
            }
        });

        if params_changed {
            self.recompute_and_update_textures(ctx);
        }
    }

    fn ui_right_panel(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        // En-tête du panneau droit
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("🎨 Tonalité Pro & Couleur").strong().size(13.5).color(Color32::WHITE));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.link(egui::RichText::new("Masquer ⮞ [F2]").size(11.0).color(Color32::from_rgb(140, 150, 175))).clicked() {
                    self.right_panel_visible = false;
                }
            });
        });
        ui.add_space(4.0);
        self.ui_panel_tonal_content(ui, ctx, false);
    }

    fn ui_panel_tonal_content(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, _is_vertical: bool) {
        let mut params_changed = false;

        let card_frame = egui::Frame::none()
            .fill(Color32::from_rgb(22, 24, 33))
            .stroke(Stroke::new(1.0f32, Color32::from_rgb(42, 46, 64)))
            .rounding(Rounding::same(8.0))
            .inner_margin(egui::Margin::symmetric(12.0, 10.0));

        // ── 1. CARTE : 🎨 Tonalité Pro (Lightroom) ──
        card_frame.show(ui, |ui| {
            let chevron = if self.card_tonal_open { "▾" } else { "▸" };
            let header_resp = ui.horizontal(|ui| {
                ui.label(egui::RichText::new(chevron).size(13.0).color(Color32::from_rgb(150, 155, 175)));
                ui.label(egui::RichText::new("🎨").size(14.0));
                ui.label(egui::RichText::new("Tonalité (Courbe S-Curve)").strong().size(13.0).color(Color32::WHITE));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if let Some((zone, _lum)) = self.hovered_zone {
                        draw_pill(ui, zone.name_fr(), Color32::from_rgb(0, 120, 212), Color32::WHITE);
                    } else {
                        draw_pill(ui, "4 Zones", Color32::from_rgb(25, 30, 42), Color32::from_rgb(160, 175, 210));
                    }
                });
            });

            if header_resp.response.interact(egui::Sense::click()).clicked() {
                self.card_tonal_open = !self.card_tonal_open;
            }

            if self.card_tonal_open {
                ui.spacing_mut().slider_width = (ui.available_width() - 8.0).max(120.0);
                ui.add_space(6.0);
                ui.separator();
                ui.add_space(6.0);

                // 🎯 Bouton Outil Ciblé Direct (TAT)
                let tat_active = self.targeted_tool_active;
                let tat_btn = egui::Button::new(
                    egui::RichText::new(if tat_active {
                        "🎯 Outil Ciblé ACTIF [T]"
                    } else {
                        "🎯 Ajuster sur l'image [T]"
                    })
                    .strong()
                    .size(12.0)
                    .color(if tat_active { Color32::BLACK } else { Color32::WHITE }),
                )
                .fill(if tat_active { Color32::from_rgb(0, 220, 255) } else { Color32::from_rgb(30, 36, 52) })
                .stroke(Stroke::new(1.0f32, if tat_active { Color32::WHITE } else { Color32::from_rgb(0, 180, 255) }))
                .rounding(Rounding::same(6.0));

                if ui.add_sized([ui.available_width(), 30.0], tat_btn).on_hover_text("Active l'ajustement direct : survolez et glissez verticalement pour moduler Noirs, Ombres, Hautes Lumières ou Blancs.").clicked() {
                    self.targeted_tool_active = !self.targeted_tool_active;
                    if !self.targeted_tool_active {
                        self.hovered_zone = None;
                    }
                }

                ui.add_space(6.0);

                // Widget Histogramme 4 Zones & Courbe Tonale
                let tonal_for_curve = TonalParams {
                    contrast: self.params.tonal_contrast,
                    whites: self.params.tonal_whites,
                    highlights: self.params.tonal_highlights,
                    shadows: self.params.tonal_shadows,
                    blacks: self.params.tonal_blacks,
                    saturation: self.params.tonal_saturation,
                    vibrance: self.params.tonal_vibrance,
                };
                draw_tonal_histogram(ui, self.cached_histogram.as_ref(), self.hovered_zone.map(|(z, _)| z), &tonal_for_curve);

                ui.add_space(8.0);

                // 0. Contraste Global
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Contraste :").strong().color(Color32::WHITE));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        draw_val_pill(ui, &format!("{:+.0}%", self.params.tonal_contrast * 100.0));
                    });
                });
                if ui.add(egui::Slider::new(&mut self.params.tonal_contrast, -2.0..=2.0).step_by(0.01).show_value(false)).changed() {
                    params_changed = true;
                }

                ui.add_space(6.0);

                // 1. Blancs
                ui.horizontal(|ui| {
                    let is_active = self.hovered_zone.map(|(z, _)| z == TonalZone::Whites).unwrap_or(false);
                    let col = if is_active { Color32::from_rgb(0, 220, 255) } else { Color32::WHITE };
                    ui.label(egui::RichText::new("Blancs (Crêtes) :").color(col));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        draw_val_pill(ui, &format!("{:+.0}%", self.params.tonal_whites * 100.0));
                    });
                });
                if ui.add(egui::Slider::new(&mut self.params.tonal_whites, -2.0..=2.0).step_by(0.01).show_value(false)).changed() {
                    params_changed = true;
                }

                ui.add_space(6.0);

                // 2. Hautes Lumières
                ui.horizontal(|ui| {
                    let is_active = self.hovered_zone.map(|(z, _)| z == TonalZone::Highlights).unwrap_or(false);
                    let col = if is_active { Color32::from_rgb(0, 220, 255) } else { Color32::WHITE };
                    ui.label(egui::RichText::new("Hautes Lumières :").color(col));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        draw_val_pill(ui, &format!("{:+.0}%", self.params.tonal_highlights * 100.0));
                    });
                });
                if ui.add(egui::Slider::new(&mut self.params.tonal_highlights, -2.0..=2.0).step_by(0.01).show_value(false)).changed() {
                    params_changed = true;
                }

                ui.add_space(6.0);

                // 3. Ombres
                ui.horizontal(|ui| {
                    let is_active = self.hovered_zone.map(|(z, _)| z == TonalZone::Shadows).unwrap_or(false);
                    let col = if is_active { Color32::from_rgb(0, 220, 255) } else { Color32::WHITE };
                    ui.label(egui::RichText::new("Ombres :").color(col));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        draw_val_pill(ui, &format!("{:+.0}%", self.params.tonal_shadows * 100.0));
                    });
                });
                if ui.add(egui::Slider::new(&mut self.params.tonal_shadows, -2.0..=2.0).step_by(0.01).show_value(false)).changed() {
                    params_changed = true;
                }

                ui.add_space(6.0);

                // 4. Noirs
                ui.horizontal(|ui| {
                    let is_active = self.hovered_zone.map(|(z, _)| z == TonalZone::Blacks).unwrap_or(false);
                    let col = if is_active { Color32::from_rgb(0, 220, 255) } else { Color32::WHITE };
                    ui.label(egui::RichText::new("Noirs (Point Noir) :").color(col));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        draw_val_pill(ui, &format!("{:+.0}%", self.params.tonal_blacks * 100.0));
                    });
                });
                if ui.add(egui::Slider::new(&mut self.params.tonal_blacks, -2.0..=2.0).step_by(0.01).show_value(false)).changed() {
                    params_changed = true;
                }
            }
        });
        ui.add_space(8.0);

        // ── 2. CARTE : 🌈 Couleur & Finition HDR ──
        card_frame.show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("🌈").size(14.0));
                ui.label(egui::RichText::new("Couleur & Finition HDR").strong().size(13.0).color(Color32::WHITE));
            });
            ui.add_space(6.0);
            ui.separator();
            ui.add_space(6.0);

            ui.spacing_mut().slider_width = (ui.available_width() - 4.0).max(200.0);

            // Vibrance HDR
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Vibrance HDR :").color(Color32::from_rgb(255, 170, 200)));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    draw_val_pill(ui, &format!("{:+.0}%", self.params.tonal_vibrance * 100.0));
                });
            });
            if ui.add(egui::Slider::new(&mut self.params.tonal_vibrance, -2.0..=2.0).step_by(0.01).show_value(false)).changed() {
                params_changed = true;
            }

            ui.add_space(6.0);

            // Saturation HDR
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Saturation HDR :").color(Color32::from_rgb(255, 140, 160)));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    draw_val_pill(ui, &format!("{:+.0}%", self.params.tonal_saturation * 100.0));
                });
            });
            if ui.add(egui::Slider::new(&mut self.params.tonal_saturation, -2.0..=2.0).step_by(0.01).show_value(false)).changed() {
                params_changed = true;
            }

            ui.add_space(6.0);

            // Anti-Halo
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Filtre Anti-Halo :").size(12.0).color(Color32::from_rgb(180, 190, 215)));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    draw_val_pill(ui, &format!("{:.0}%", self.params.anti_halo * 100.0));
                });
            });
            if ui.add(egui::Slider::new(&mut self.params.anti_halo, 0.0..=1.0).step_by(0.05).show_value(false)).changed() {
                params_changed = true;
            }

            ui.add_space(8.0);

            // Bouton réinitialiser
            let btn_reset_tonal = egui::Button::new(
                egui::RichText::new("↺ Réinitialiser Tonalité & Couleur").size(11.5).color(Color32::from_rgb(170, 180, 205)),
            )
            .fill(Color32::from_rgb(26, 29, 39))
            .stroke(Stroke::new(1.0f32, Color32::from_rgb(45, 50, 70)))
            .rounding(Rounding::same(5.0));

            if ui.add_sized([ui.available_width(), 24.0], btn_reset_tonal).clicked() {
                self.params.tonal_contrast = 0.0;
                self.params.tonal_whites = 0.0;
                self.params.tonal_highlights = 0.0;
                self.params.tonal_shadows = 0.0;
                self.params.tonal_blacks = 0.0;
                self.params.tonal_saturation = 0.0;
                self.params.tonal_vibrance = 0.0;
                params_changed = true;
            }
        });

        if params_changed {
            self.recompute_and_update_textures(ctx);
        }
    }

    fn ui_card_simulation(&mut self, ui: &mut egui::Ui, card_frame: &egui::Frame, is_vertical: bool) -> bool {
        let mut params_changed = false;
        card_frame.show(ui, |ui| {
            ui.spacing_mut().slider_width = (ui.available_width() - 8.0).max(120.0);

            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("🖥️").size(15.0));
                ui.label(egui::RichText::new("Simulation SDR & Rendu").strong().size(13.0).color(Color32::WHITE));
            });
            ui.add_space(4.0);

            if self.is_hdr_display_active {
                // Si HDR natif est actif : bandeau explicatif et simulation grisée
                egui::Frame::none()
                    .fill(Color32::from_rgb(16, 34, 24))
                    .stroke(Stroke::new(1.0f32, Color32::from_rgb(34, 197, 94)))
                    .rounding(Rounding::same(6.0))
                    .inner_margin(egui::Margin::symmetric(10.0, 7.0))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new("🔒 Mode HDR Matériel Actif").strong().size(11.5).color(Color32::from_rgb(74, 222, 128)));
                        });
                        ui.label(egui::RichText::new("Affichage physique direct. Tonemapping SDR désactivé.").size(10.0).color(Color32::from_rgb(180, 210, 190)));
                    });
                ui.add_space(6.0);
                ui.add_enabled_ui(false, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("Éclat HDR :").size(11.5).color(Color32::GRAY));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            draw_val_pill(ui, &format!("{:.0}%", self.params.hdr_boost * 100.0));
                        });
                    });
                    ui.add(egui::Slider::new(&mut self.params.hdr_boost, 0.0..=1.5).step_by(0.05).show_value(false));

                    ui.add_space(3.0);
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("Épaulement :").size(11.5).color(Color32::GRAY));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            draw_val_pill(ui, &format!("{:.0}% seuil", self.params.highlight_rolloff * 100.0));
                        });
                    });
                    ui.add(egui::Slider::new(&mut self.params.highlight_rolloff, 0.50..=0.90).step_by(0.02).show_value(false));
                });
            } else {
                // Mode SDR Actif : Contrôles de simulation EDR intelligents sans cramage
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Éclat HDR (Punch) :").size(12.0).color(Color32::WHITE).strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let pct = (self.params.hdr_boost * 100.0).round();
                        let tag = if pct <= 5.0 {
                            " (SDR pur)"
                        } else if (pct - 100.0).abs() < 5.0 {
                            " (100% HDR)"
                        } else if pct >= 140.0 {
                            " (Spéculaire)"
                        } else {
                            ""
                        };
                        draw_val_pill(ui, &format!("{:.0}%{}", pct, tag));
                    });
                });
                if ui.add(egui::Slider::new(&mut self.params.hdr_boost, 0.0..=1.5).step_by(0.05).show_value(false)).changed() {
                    params_changed = true;
                }

                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Épaulement / Rolloff :").size(11.5).color(Color32::from_rgb(180, 190, 215)));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        draw_val_pill(ui, &format!("{:.0}% seuil", self.params.highlight_rolloff * 100.0));
                    });
                });
                if ui.add(egui::Slider::new(&mut self.params.highlight_rolloff, 0.50..=0.90).step_by(0.02).show_value(false)).changed() {
                    params_changed = true;
                }

                ui.add_space(4.0);
                ui.label(egui::RichText::new("Mode Tone-Mapping :").size(11.0).color(Color32::from_rgb(170, 175, 195)));
                let modes = [
                    ("vivid", "✨ Éclat"),
                    ("natural", "🌿 Naturel"),
                    ("aces", "🎬 ACES"),
                    ("reinhard", "⚖ Reinhard"),
                    ("linear", "⚡ Direct"),
                ];

                if is_vertical {
                    // Si mode vertical / mobile : empiler les modes verticalement en boutons pleine largeur
                    for (mode_key, mode_title) in modes {
                        let is_cur = self.params.tone_map_mode == mode_key;
                        let btn = egui::Button::new(
                            egui::RichText::new(mode_title).size(11.5).color(if is_cur { Color32::WHITE } else { Color32::from_rgb(180, 185, 205) }),
                        )
                        .fill(if is_cur { Color32::from_rgb(0, 120, 212) } else { Color32::from_rgb(30, 34, 48) })
                        .stroke(Stroke::new(1.0f32, if is_cur { Color32::from_rgb(0, 180, 255) } else { Color32::from_rgb(50, 56, 76) }))
                        .rounding(Rounding::same(5.0));

                        if ui.add_sized([ui.available_width(), 26.0], btn).clicked() {
                            self.params.tone_map_mode = mode_key.to_string();
                            params_changed = true;
                        }
                        ui.add_space(2.0);
                    }
                } else {
                    ui.horizontal_wrapped(|ui| {
                        for (mode_key, mode_title) in modes {
                            let is_cur = self.params.tone_map_mode == mode_key;
                            let btn = egui::Button::new(
                                egui::RichText::new(mode_title).size(11.0).color(if is_cur { Color32::WHITE } else { Color32::from_rgb(180, 185, 205) }),
                            )
                            .fill(if is_cur { Color32::from_rgb(0, 120, 212) } else { Color32::from_rgb(30, 34, 48) })
                            .stroke(Stroke::new(1.0f32, if is_cur { Color32::from_rgb(0, 180, 255) } else { Color32::from_rgb(50, 56, 76) }))
                            .rounding(Rounding::same(5.0));

                            if ui.add(btn).clicked() {
                                self.params.tone_map_mode = mode_key.to_string();
                                params_changed = true;
                            }
                        }
                    });
                }
            }
        });
        params_changed
    }

    fn ui_card_headroom(&mut self, ui: &mut egui::Ui, card_frame: &egui::Frame, _is_vertical: bool) -> bool {
        let mut params_changed = false;
        card_frame.show(ui, |ui| {
            ui.spacing_mut().slider_width = (ui.available_width() - 8.0).max(120.0);

            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("📐").size(15.0));
                ui.label(egui::RichText::new("Capacité Écran & Export").strong().size(13.0).color(Color32::WHITE));
            });
            ui.add_space(2.0);

            // Slider Cible Export Peak Nits
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Luminance Crête Export :").size(12.0).color(Color32::WHITE));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    draw_val_pill(ui, &format!("{:.0} nits", self.params.peak_nits));
                });
            });
            if ui.add(egui::Slider::new(&mut self.params.peak_nits, 200.0..=10000.0).step_by(50.0).show_value(false)).changed() {
                params_changed = true;
            }

            ui.add_space(4.0);

            // Indicateur d'adaptation proportionnelle ISO 21496-1
            let weight = compute_display_headroom_weight(self.params.peak_nits, self.display_peak_nits, self.params.sdr_white);
            let effective_nits = (self.params.sdr_white * (self.params.peak_nits / self.params.sdr_white).powf(weight)).min(self.display_peak_nits);

            egui::Frame::none()
                .fill(Color32::from_rgb(17, 21, 30))
                .stroke(Stroke::new(1.0f32, Color32::from_rgb(40, 48, 68)))
                .rounding(Rounding::same(6.0))
                .inner_margin(egui::Margin::symmetric(10.0, 6.0))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("💡").size(12.0));
                        if self.params.peak_nits > self.display_peak_nits {
                            ui.label(
                                egui::RichText::new(format!(
                                    "Adaptation : {:.0} nits sur votre écran (Échelle {:.0}%)",
                                    effective_nits, weight * 100.0
                                ))
                                .size(11.0)
                                .color(Color32::from_rgb(0, 200, 255)),
                            );
                        } else {
                            ui.label(
                                egui::RichText::new(format!(
                                    "Écran 100% capable ({:.0} nits)",
                                    self.display_peak_nits
                                ))
                                .size(11.0)
                                .color(Color32::from_rgb(46, 204, 113)),
                            );
                        }
                    });
                    ui.label(
                        egui::RichText::new("Zéro cramage : prévisualisation proportionnelle conforme ISO 21496-1.")
                            .size(9.5)
                            .color(Color32::from_rgb(150, 160, 180)),
                    );
                });
        });
        params_changed
    }

    fn ui_card_export(&mut self, ui: &mut egui::Ui, card_frame: &egui::Frame, is_vertical: bool) -> (bool, bool) {
        let mut trigger_export = false;
        card_frame.show(ui, |ui| {
            ui.spacing_mut().slider_width = (ui.available_width() - 8.0).max(120.0);

            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("💾").size(15.0));
                ui.label(egui::RichText::new("Qualité & Export Ultra HDR").strong().size(13.0).color(Color32::WHITE));
            });
            ui.add_space(2.0);

            if is_vertical {
                // Si vertical : placer les sliders Base et Gain Map verticalement
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("Base :").size(11.5).color(Color32::WHITE));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            draw_val_pill(ui, &format!("{}%", self.base_quality));
                        });
                    });
                    ui.add(egui::Slider::new(&mut self.base_quality, 75..=100).show_value(false));

                    ui.add_space(4.0);

                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("Gain Map :").size(11.5).color(Color32::WHITE));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            draw_val_pill(ui, &format!("{}%", self.gainmap_quality));
                        });
                    });
                    ui.add(egui::Slider::new(&mut self.gainmap_quality, 65..=100).show_value(false));
                });
            } else {
                ui.columns(2, |sub_cols| {
                    sub_cols[0].vertical(|ui| {
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new("Base :").size(11.5).color(Color32::WHITE));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                draw_val_pill(ui, &format!("{}%", self.base_quality));
                            });
                        });
                        ui.add(egui::Slider::new(&mut self.base_quality, 75..=100).show_value(false));
                    });

                    sub_cols[1].vertical(|ui| {
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new("Gain Map :").size(11.5).color(Color32::WHITE));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                draw_val_pill(ui, &format!("{}%", self.gainmap_quality));
                            });
                        });
                        ui.add(egui::Slider::new(&mut self.gainmap_quality, 65..=100).show_value(false));
                    });
                });
            }

            ui.add_space(4.0);

            let export_enabled = self.loaded_image.is_some();
            let btn_export = egui::Button::new(
                egui::RichText::new("💾 Exporter JPEG Ultra HDR (F12)").strong().size(12.5).color(Color32::WHITE),
            )
            .fill(if export_enabled { Color32::from_rgb(0, 120, 212) } else { Color32::from_rgb(38, 42, 54) })
            .rounding(Rounding::same(6.0));

            if ui.add_sized([ui.available_width(), 30.0], btn_export).clicked() {
                trigger_export = true;
            }

            ui.add_space(2.0);
            if is_vertical {
                // En mode vertical, empiler la case et le bouton lot verticalement
                ui.checkbox(&mut self.save_standalone_gainmap, egui::RichText::new("Gain Map seule").size(11.0));
                ui.add_space(3.0);
                let btn_batch = egui::Button::new(
                    egui::RichText::new("⚡ Traitement par lot...").size(11.5).color(Color32::from_rgb(210, 220, 240)),
                )
                .fill(Color32::from_rgb(32, 36, 50))
                .rounding(Rounding::same(5.0));

                if ui.add_sized([ui.available_width(), 26.0], btn_batch).on_hover_text("Convertit en masse tout un répertoire en JPEG Ultra HDR.").clicked() {
                    self.show_batch_dialog = true;
                }
            } else {
                ui.horizontal(|ui| {
                    ui.checkbox(&mut self.save_standalone_gainmap, egui::RichText::new("Gain Map seule").size(10.5));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let btn_batch = egui::Button::new(
                            egui::RichText::new("⚡ Lot...").size(10.5).color(Color32::from_rgb(210, 220, 240)),
                        )
                        .fill(Color32::from_rgb(32, 36, 50))
                        .rounding(Rounding::same(5.0));

                        if ui.add(btn_batch).on_hover_text("Convertit en masse tout un répertoire en JPEG Ultra HDR.").clicked() {
                            self.show_batch_dialog = true;
                        }
                    });
                });
            }
        });
        (false, trigger_export)
    }

    fn ui_panel_simulation_content(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, is_vertical: bool) {
        let card_frame = egui::Frame::none()
            .fill(Color32::from_rgb(22, 25, 35))
            .stroke(Stroke::new(1.0f32, Color32::from_rgb(44, 49, 68)))
            .rounding(Rounding::same(8.0))
            .inner_margin(egui::Margin::symmetric(14.0, 10.0));

        let mut changed = false;
        changed |= self.ui_card_simulation(ui, &card_frame, is_vertical);
        ui.add_space(8.0);
        changed |= self.ui_card_headroom(ui, &card_frame, is_vertical);

        if changed {
            self.recompute_and_update_textures(ctx);
        }
    }

    fn ui_panel_export_content(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, is_vertical: bool) {
        let card_frame = egui::Frame::none()
            .fill(Color32::from_rgb(22, 25, 35))
            .stroke(Stroke::new(1.0f32, Color32::from_rgb(44, 49, 68)))
            .rounding(Rounding::same(8.0))
            .inner_margin(egui::Margin::symmetric(14.0, 10.0));

        let mut changed = false;
        let (_p_ch, trigger_export) = self.ui_card_export(ui, &card_frame, is_vertical);
        ui.add_space(8.0);
        changed |= self.ui_card_headroom(ui, &card_frame, is_vertical);

        if trigger_export {
            self.export_ultrahdr();
        }
        if changed {
            self.recompute_and_update_textures(ctx);
        }
    }

    fn ui_bottom_dock(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let mut params_changed = false;
        let mut trigger_export = false;

        let card_frame = egui::Frame::none()
            .fill(Color32::from_rgb(22, 25, 35))
            .stroke(Stroke::new(1.0f32, Color32::from_rgb(44, 49, 68)))
            .rounding(Rounding::same(8.0))
            .inner_margin(egui::Margin::symmetric(14.0, 10.0));

        ui.columns(3, |cols| {
            // ── COLONNE 1 : Simulation Écran SDR & Tone-Mapping ──
            if self.ui_card_simulation(&mut cols[0], &card_frame, false) {
                params_changed = true;
            }

            // ── COLONNE 2 : Capacité Écran & Luminance Cible (ISO 21496-1) ──
            if self.ui_card_headroom(&mut cols[1], &card_frame, false) {
                params_changed = true;
            }

            // ── COLONNE 3 : Paramètres d'Encodage & Export ──
            let (_c, trig) = self.ui_card_export(&mut cols[2], &card_frame, false);
            if trig {
                trigger_export = true;
            }
        });

        if trigger_export {
            self.export_ultrahdr();
        }

        if params_changed {
            self.recompute_and_update_textures(ctx);
        }
    }

    fn ui_vertical_control_deck(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let panes = [
            ConfigPane::Tonal,
            ConfigPane::Simulation,
            ConfigPane::Engine,
            ConfigPane::Export,
        ];

        let avail_w = ui.available_width();
        if avail_w < 540.0 {
            // Disposition Smartphone Portrait Étroite (16:9, 18:9, 19.5:9, 20:9, 21:9) :
            // 4 Onglets Material 3 en 1 SEULE ligne horizontale parfaitement répartie
            ui.vertical(|ui| {
                let tab_w = ((avail_w - 3.0 * 6.0) / 4.0).max(45.0);
                ui.horizontal(|ui| {
                    for pane in panes {
                        let is_active = self.active_config_pane == pane;
                        if crate::theme_material::material_tab(ui, is_active, pane.icon(), pane.title(), tab_w).clicked() {
                            self.active_config_pane = pane;
                        }
                    }
                });

                ui.add_space(4.0);
                ui.separator();
                ui.add_space(4.0);

                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.vertical(|ui| {
                            match self.active_config_pane {
                                ConfigPane::Tonal => self.ui_panel_tonal_content(ui, ctx, true),
                                ConfigPane::Simulation => self.ui_panel_simulation_content(ui, ctx, true),
                                ConfigPane::Engine => self.ui_panel_engine_content(ui, ctx, true),
                                ConfigPane::Export => self.ui_panel_export_content(ui, ctx, true),
                            }
                        });
                    });
            });
        } else {
            // Disposition Foldable dépliée / Tablette : Rail vertical gauche + contenu droit
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(110.0);
                    for pane in panes {
                        let is_active = self.active_config_pane == pane;
                        let label = format!("{} {}", pane.icon(), pane.title());
                        if crate::theme_material::material_chip(ui, is_active, &label).clicked() {
                            self.active_config_pane = pane;
                        }
                        ui.add_space(4.0);
                    }
                });

                ui.add_space(4.0);
                ui.separator();
                ui.add_space(4.0);

                ui.vertical(|ui| {
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.vertical(|ui| {
                                match self.active_config_pane {
                                    ConfigPane::Tonal => self.ui_panel_tonal_content(ui, ctx, true),
                                    ConfigPane::Simulation => self.ui_panel_simulation_content(ui, ctx, true),
                                    ConfigPane::Engine => self.ui_panel_engine_content(ui, ctx, true),
                                    ConfigPane::Export => self.ui_panel_export_content(ui, ctx, true),
                                }
                            });
                        });
                });
            });
        }
    }

    fn apply_preset(&mut self, preset: &HDRPreset) {
        self.params.peak_nits = preset.peak_nits;
        self.params.sdr_white = preset.sdr_white;
        self.params.shadow_boost = preset.shadow_boost;
        self.params.hi_gamma = preset.hi_gamma;
        self.params.highlight_threshold = preset.highlight_threshold;
    }

    fn ui_center_viewport(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, is_vertical: bool) {
        // Barre d'outils de la visionneuse
        egui::Frame::none()
            .fill(Color32::from_rgb(18, 19, 26))
            .inner_margin(egui::Margin::symmetric(if is_vertical { 8.0 } else { 12.0 }, 6.0))
            .show(ui, |ui| {
                let mut mode_changed = false;
                let modes: &[(ViewerMode, &str)] = if is_vertical {
                    &[
                        (ViewerMode::Split, "↔ Glissière"),
                        (ViewerMode::SideBySide, "◫ Côte à côte"),
                        (ViewerMode::FullHdr, "✨ Ultra HDR"),
                        (ViewerMode::GainMap, "◐ Gain Map"),
                        (ViewerMode::Heatmap, "🔥 Carte Nits"),
                    ]
                } else {
                    &[
                        (ViewerMode::Split, "↔ Glissière Avant / Après"),
                        (ViewerMode::SideBySide, "◫ Côte à côte"),
                        (ViewerMode::FullHdr, "✨ Vrai Ultra HDR Direct"),
                        (ViewerMode::GainMap, "◐ Gain Map"),
                        (ViewerMode::Heatmap, "🔥 Carte Nits"),
                    ]
                };

                let is_compare_active = self.compare_toggled
                    || ctx.input(|i| {
                        i.key_down(egui::Key::Space)
                            || i.key_down(egui::Key::Backslash)
                            || i.key_down(egui::Key::Slash)
                    });

                let compare_label = if is_vertical { "👁 Original" } else { "👁 Original [Espace / \\]" };
                let btn_compare = egui::Button::new(
                    egui::RichText::new(compare_label)
                        .size(if is_vertical { 13.0 } else { 12.0 })
                        .color(if is_compare_active {
                            Color32::BLACK
                        } else {
                            Color32::from_rgb(255, 215, 0)
                        }),
                )
                .fill(if is_compare_active {
                    Color32::from_rgb(255, 215, 0)
                } else {
                    Color32::from_rgb(34, 30, 20)
                })
                .stroke(Stroke::new(
                    1.0f32,
                    if is_compare_active {
                        Color32::WHITE
                    } else {
                        Color32::from_rgb(180, 150, 40)
                    },
                ))
                .rounding(Rounding::same(8.0));

                if is_vertical {
                    // ── Mode Vertical / Mobile : Défilement horizontal des modes + barre de zoom séparée ──
                    egui::ScrollArea::horizontal()
                        .id_source("viewer_modes_scroll")
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                for &(m, label) in modes {
                                    let is_active = self.mode == m;
                                    let btn = egui::Button::new(
                                        egui::RichText::new(label)
                                            .size(13.0)
                                            .color(if is_active { Color32::WHITE } else { Color32::from_rgb(180, 185, 205) }),
                                    )
                                    .fill(if is_active { Color32::from_rgb(0, 120, 212) } else { Color32::from_rgb(26, 29, 39) })
                                    .stroke(Stroke::new(1.0f32, if is_active { Color32::from_rgb(0, 180, 255) } else { Color32::from_rgb(45, 50, 68) }))
                                    .rounding(Rounding::same(8.0));

                                    if ui.add(btn).clicked() {
                                        self.mode = m;
                                        mode_changed = true;
                                    }
                                }

                                ui.add_space(6.0);
                                ui.separator();
                                ui.add_space(6.0);

                                if ui.button(egui::RichText::new("⛶").size(11.5)).clicked() {
                                    self.zoom = 1.0;
                                    self.pan = Vec2::ZERO;
                                }
                                if ui.button(egui::RichText::new("➖").size(10.5)).clicked() {
                                    self.zoom = (self.zoom / 1.25).clamp(0.2, 8.0);
                                }
                                ui.label(egui::RichText::new(format!("{:.0}%", self.zoom * 100.0)).strong().size(11.0).color(Color32::from_rgb(138, 180, 248)));
                                if ui.button(egui::RichText::new("➕").size(10.5)).clicked() {
                                    self.zoom = (self.zoom * 1.25).clamp(0.2, 8.0);
                                }
                            });
                        });
                } else {
                    // ── Mode Bureau / Tablette Large : 1 seule barre spacieuse ──
                    ui.horizontal(|ui| {
                        for &(m, label) in modes {
                            let is_active = self.mode == m;
                            let btn = egui::Button::new(
                                egui::RichText::new(label).size(12.0).color(if is_active { Color32::WHITE } else { Color32::from_rgb(180, 185, 205) }),
                            )
                            .fill(if is_active { Color32::from_rgb(0, 120, 212) } else { Color32::from_rgb(26, 29, 39) })
                            .stroke(Stroke::new(1.0f32, if is_active { Color32::from_rgb(0, 180, 255) } else { Color32::from_rgb(45, 50, 68) }))
                            .rounding(Rounding::same(6.0));

                            if ui.add(btn).clicked() {
                                self.mode = m;
                                mode_changed = true;
                            }
                        }

                        ui.add_space(8.0);
                        ui.separator();
                        ui.add_space(8.0);

                        if ui.add(btn_compare).clicked() {
                            self.compare_toggled = !self.compare_toggled;
                        }

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button(egui::RichText::new("+").size(12.5)).clicked() {
                                self.zoom = (self.zoom * 1.2).clamp(0.2, 8.0);
                            }
                            ui.label(egui::RichText::new(format!("{:.0}%", self.zoom * 100.0)).size(11.5).color(Color32::from_rgb(200, 205, 225)));
                            if ui.button(egui::RichText::new("-").size(12.5)).clicked() {
                                self.zoom = (self.zoom / 1.2).clamp(0.2, 8.0);
                            }
                            if ui.button("1:1").clicked() {
                                self.zoom = 1.0;
                                self.pan = Vec2::ZERO;
                            }
                            if ui.button("⛶ Ajuster").clicked() {
                                self.zoom = 1.0;
                                self.pan = Vec2::ZERO;
                            }
                        });
                    });
                }

                if mode_changed {
                    self.recompute_and_update_textures(ctx);
                }
            });

        if self.loaded_image.is_none() {
            // Zone de dépôt vide élégante
            ui.centered_and_justified(|ui| {
                egui::Frame::none()
                    .fill(Color32::from_rgb(20, 22, 30))
                    .stroke(Stroke::new(1.5f32, Color32::from_rgb(50, 56, 78)))
                    .rounding(Rounding::same(12.0))
                    .inner_margin(egui::Margin::symmetric(40.0, 36.0))
                    .show(ui, |ui| {
                        ui.vertical_centered(|ui| {
                            ui.label(egui::RichText::new("📷").size(48.0));
                            ui.add_space(8.0);
                            ui.heading(egui::RichText::new("Glissez-déposez n'importe quelle photo ici").size(20.0).color(Color32::WHITE));
                            ui.add_space(4.0);
                            ui.label(
                                egui::RichText::new(
                                    "Prend en charge tous les formats : Camera RAW, OpenEXR (.exr), Radiance HDR (.hdr), Apple HEIC, TIFF 16-bit, JPEG, PNG, WebP",
                                )
                                .size(13.0)
                                .color(Color32::from_rgb(160, 165, 185)),
                            );
                            ui.add_space(16.0);
                            let btn_browse = egui::Button::new(
                                egui::RichText::new("Sélectionner une photo...").strong().size(14.0).color(Color32::WHITE),
                            )
                            .fill(Color32::from_rgb(0, 120, 212))
                            .rounding(Rounding::same(24.0));

                            if ui.add_sized([220.0, 42.0], btn_browse).clicked() {
                                self.trigger_pick_file(ctx);
                            }

                            ui.add_space(8.0);
                            let btn_demo = egui::Button::new(
                                egui::RichText::new("🌅 Charger la Démo Ultra HDR").strong().size(13.0).color(Color32::from_rgb(220, 240, 255)),
                            )
                            .fill(Color32::from_rgb(38, 55, 80))
                            .rounding(Rounding::same(24.0));

                            if ui.add_sized([220.0, 38.0], btn_demo).clicked() {
                                self.load_demo_image(ctx);
                            }
                        });
                    });
            });
            return;
        }

        // Zone d'affichage interactive avec Zoom et Pan
        let available_size = ui.available_size();
        let (rect, response) = ui.allocate_exact_size(available_size, egui::Sense::click_and_drag());

        // Zoom à la molette centré sur le curseur de la souris
        let scroll_delta = ctx.input(|i| i.raw_scroll_delta.y);
        if scroll_delta != 0.0 && response.hovered() {
            let zoom_factor = if scroll_delta > 0.0 { 1.12 } else { 0.88 };
            let new_zoom = (self.zoom * zoom_factor).clamp(0.15, 10.0);
            if let Some(mouse_pos) = ctx.input(|i| i.pointer.hover_pos()) {
                let center = rect.center() + self.pan;
                let ratio = new_zoom / self.zoom;
                self.pan = ((center - mouse_pos) * ratio) + (mouse_pos - rect.center());
            }
            self.zoom = new_zoom;
        }

        // Double-clic pour réinitialiser le zoom & centrage à 1:1
        if response.double_clicked() {
            self.zoom = 1.0;
            self.pan = Vec2::ZERO;
        }

        // Déplacement (Pan) au clic droit, clic molette, ou clic gauche (si hors outil TAT et hors split)
        let can_drag_pan = response.dragged_by(egui::PointerButton::Secondary)
            || response.dragged_by(egui::PointerButton::Middle)
            || (!self.targeted_tool_active && self.mode != ViewerMode::Split && response.dragged_by(egui::PointerButton::Primary));

        if can_drag_pan {
            self.pan += response.drag_delta();
        }

        let painter = ui.painter_at(rect);
        let preview = self.preview_buffer.as_ref().unwrap();
        let img_w = preview.width as f32;
        let img_h = preview.height as f32;
        let aspect = (img_w / img_h).max(1e-4);

        // Calcul STRICT de la taille pour préserver le ratio exact à TOUS les niveaux de zoom
        let (base_w, base_h) = if rect.width() / rect.height() > aspect {
            (rect.height() * aspect, rect.height())
        } else {
            (rect.width(), rect.width() / aspect)
        };

        let draw_w = base_w * self.zoom;
        let draw_h = base_h * self.zoom;

        let center = rect.center() + self.pan;
        let img_rect = Rect::from_center_size(center, Vec2::new(draw_w, draw_h));

        let is_comparing = self.compare_toggled
            || ctx.input(|i| {
                i.key_down(egui::Key::Space)
                    || i.key_down(egui::Key::Backslash)
                    || i.key_down(egui::Key::Slash)
            });

        if is_comparing {
            if let Some(tex_sdr) = &self.tex_original_sdr {
                painter.image(tex_sdr.id(), img_rect, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
                draw_floating_badge(
                    &painter,
                    Pos2::new(img_rect.left() + 16.0, img_rect.top() + 16.0),
                    "👀 Mode Comparaison : Image SDR Originale [Espace / \\]",
                    Color32::from_rgb(255, 215, 0),
                );
            }
        } else {
            match self.mode {
                ViewerMode::Split => {
                    if let (Some(tex_sdr), Some(tex_hdr)) = (&self.tex_original_sdr, &self.tex_hdr_preview) {
                        let split_x = img_rect.left() + img_rect.width() * self.split_pos;

                        // Moitié gauche : SDR Original (AVANT)
                        let left_clip = Rect::from_min_max(img_rect.min, Pos2::new(split_x, img_rect.max.y));
                        let left_painter = painter.with_clip_rect(left_clip);
                        left_painter.image(tex_sdr.id(), img_rect, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);

                        // Moitié droite : Ultra HDR (APRÈS)
                        let right_clip = Rect::from_min_max(Pos2::new(split_x, img_rect.min.y), img_rect.max);
                        let right_painter = painter.with_clip_rect(right_clip);
                        right_painter.image(tex_hdr.id(), img_rect, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);

                        // Ligne de séparation blanche
                        painter.line_segment([Pos2::new(split_x, img_rect.top()), Pos2::new(split_x, img_rect.bottom())], Stroke::new(2.0f32, Color32::WHITE));

                        // Poignée circulaire élégante avec flèches
                        let handle_pos = Pos2::new(split_x, img_rect.center().y);
                        painter.circle_filled(handle_pos, 14.0, Color32::from_rgb(0, 120, 212));
                        painter.circle_stroke(handle_pos, 14.0, Stroke::new(2.5f32, Color32::WHITE));
                        painter.text(handle_pos, egui::Align2::CENTER_CENTER, "◀▶", egui::FontId::proportional(10.0), Color32::WHITE);

                        // Badges semi-transparents élégants (compacts sur mobile pour ne jamais entrer en collision)
                        let sdr_txt = if is_vertical { "SDR" } else { "Avant : SDR Original" };
                        let hdr_txt = if is_vertical { "Ultra HDR" } else { "Après : Ultra HDR" };
                        let r_offset = if is_vertical { 80.0 } else { 150.0 };
                        draw_floating_badge(&painter, Pos2::new(img_rect.left() + 8.0, img_rect.top() + 8.0), sdr_txt, Color32::WHITE);
                        draw_floating_badge(&painter, Pos2::new((img_rect.right() - r_offset).max(split_x + 8.0), img_rect.top() + 8.0), hdr_txt, Color32::from_rgb(0, 220, 255));

                        // Drag de la barre (désactivé si l'outil ciblé TAT est actif)
                        if !self.targeted_tool_active && response.dragged_by(egui::PointerButton::Primary) {
                            if let Some(pos) = response.interact_pointer_pos() {
                                let pct = ((pos.x - img_rect.left()) / img_rect.width()).clamp(0.0, 1.0);
                                self.split_pos = pct;
                            }
                        }
                    }
                }
                ViewerMode::FullHdr => {
                    if let Some(tex) = &self.tex_hdr_preview {
                        painter.image(tex.id(), img_rect, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
                        draw_floating_badge(&painter, Pos2::new(img_rect.left() + 16.0, img_rect.top() + 16.0), "Rendu Ultra HDR Direct", Color32::from_rgb(0, 220, 255));
                    }
                }
                ViewerMode::GainMap => {
                    if let Some(tex) = &self.tex_gain_map {
                        painter.image(tex.id(), img_rect, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
                        draw_floating_badge(&painter, Pos2::new(img_rect.left() + 16.0, img_rect.top() + 16.0), "Gain Map (Niveaux de Gris)", Color32::WHITE);
                    }
                }
                ViewerMode::Heatmap => {
                    if let Some(tex) = &self.tex_heatmap {
                        painter.image(tex.id(), img_rect, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
                        draw_floating_badge(&painter, Pos2::new(img_rect.left() + 16.0, img_rect.top() + 16.0), "Carte Thermique des Nits (Turbo)", Color32::from_rgb(255, 180, 50));
                    }
                }
                ViewerMode::SideBySide => {
                    if let (Some(tex_sdr), Some(tex_hdr)) = (&self.tex_original_sdr, &self.tex_hdr_preview) {
                        let gap = 12.0;
                        let half_avail_w = ((rect.width() - gap) * 0.5).max(10.0);
                        let (base_single_w, base_single_h) = if half_avail_w / rect.height() > aspect {
                            (rect.height() * aspect, rect.height())
                        } else {
                            (half_avail_w, half_avail_w / aspect)
                        };
                        let single_w = base_single_w * self.zoom;
                        let single_h = base_single_h * self.zoom;
                        let left_rect = Rect::from_center_size(
                            Pos2::new(center.x - (single_w + gap) * 0.5, center.y),
                            Vec2::new(single_w, single_h),
                        );
                        let right_rect = Rect::from_center_size(
                            Pos2::new(center.x + (single_w + gap) * 0.5, center.y),
                            Vec2::new(single_w, single_h),
                        );

                        painter.image(tex_sdr.id(), left_rect, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
                        painter.image(tex_hdr.id(), right_rect, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);

                        draw_floating_badge(&painter, Pos2::new(left_rect.left() + 8.0, left_rect.top() + 8.0), "Avant : SDR Original", Color32::WHITE);
                        draw_floating_badge(&painter, Pos2::new(right_rect.left() + 8.0, right_rect.top() + 8.0), "Après : Ultra HDR", Color32::from_rgb(0, 220, 255));
                    }
                }
            }
        }

        // ── 🎯 Outil Ciblé Direct (Lightroom TAT) sur l'image ──
        if self.targeted_tool_active {
            let hover_pos = ctx.input(|i| i.pointer.hover_pos());
            let (in_img, norm_x, norm_y) = if self.mode == ViewerMode::SideBySide {
                let gap = 12.0;
                let half_avail_w = ((rect.width() - gap) * 0.5).max(10.0);
                let (base_single_w, base_single_h) = if half_avail_w / rect.height() > aspect {
                    (rect.height() * aspect, rect.height())
                } else {
                    (half_avail_w, half_avail_w / aspect)
                };
                let single_w = base_single_w * self.zoom;
                let single_h = base_single_h * self.zoom;
                let left_rect = Rect::from_center_size(
                    Pos2::new(center.x - (single_w + gap) * 0.5, center.y),
                    Vec2::new(single_w, single_h),
                );
                let right_rect = Rect::from_center_size(
                    Pos2::new(center.x + (single_w + gap) * 0.5, center.y),
                    Vec2::new(single_w, single_h),
                );
                if let Some(pos) = hover_pos {
                    if left_rect.contains(pos) {
                        (true, ((pos.x - left_rect.left()) / left_rect.width()).clamp(0.0, 1.0), ((pos.y - left_rect.top()) / left_rect.height()).clamp(0.0, 1.0))
                    } else if right_rect.contains(pos) {
                        (true, ((pos.x - right_rect.left()) / right_rect.width()).clamp(0.0, 1.0), ((pos.y - right_rect.top()) / right_rect.height()).clamp(0.0, 1.0))
                    } else {
                        (false, 0.0, 0.0)
                    }
                } else {
                    (false, 0.0, 0.0)
                }
            } else {
                if let Some(pos) = hover_pos {
                    if img_rect.contains(pos) {
                        (true, ((pos.x - img_rect.left()) / img_rect.width()).clamp(0.0, 1.0), ((pos.y - img_rect.top()) / img_rect.height()).clamp(0.0, 1.0))
                    } else {
                        (false, 0.0, 0.0)
                    }
                } else {
                    (false, 0.0, 0.0)
                }
            };

            if in_img {
                let px = ((norm_x * (preview.width - 1) as f32).round() as usize).min(preview.width as usize - 1);
                let py = ((norm_y * (preview.height - 1) as f32).round() as usize).min(preview.height as usize - 1);
                let idx = (py * preview.width as usize + px) * 3;
                if idx + 2 < preview.sdr_uint8.len() {
                    let r = preview.sdr_uint8[idx] as f32;
                    let g = preview.sdr_uint8[idx + 1] as f32;
                    let b = preview.sdr_uint8[idx + 2] as f32;
                    let lum_u8 = (0.2126 * r + 0.7152 * g + 0.0722 * b + 0.5).clamp(0.0, 255.0) as u8;
                    let zone = TonalZone::from_luminance_u8(lum_u8);
                    let lum_pct = lum_u8 as f32 / 255.0;
                    self.hovered_zone = Some((zone, lum_pct));

                    // Clic gauche + glisser verticalement : modulation en direct
                    if response.dragged_by(egui::PointerButton::Primary) {
                        let dy = response.drag_delta().y;
                        if dy != 0.0 {
                            let delta = -dy * 0.008; // Glisser vers le haut = +, vers le bas = -
                            match zone {
                                TonalZone::Blacks => {
                                    self.params.tonal_blacks = (self.params.tonal_blacks + delta).clamp(-2.0, 2.0);
                                }
                                TonalZone::Shadows => {
                                    self.params.tonal_shadows = (self.params.tonal_shadows + delta).clamp(-2.0, 2.0);
                                }
                                TonalZone::Highlights => {
                                    self.params.tonal_highlights = (self.params.tonal_highlights + delta).clamp(-2.0, 2.0);
                                }
                                TonalZone::Whites => {
                                    self.params.tonal_whites = (self.params.tonal_whites + delta).clamp(-2.0, 2.0);
                                }
                            }
                            self.recompute_and_update_textures(ctx);
                        }
                    }

                    if let Some(pos) = hover_pos {
                        draw_tat_hud(&painter, pos, zone, lum_pct, &self.params);
                    }
                }
            } else {
                self.hovered_zone = None;
            }
        }
    }
}

// ── Fonctions Utilitaires d'Affichage ──

/// Évalue la fonction de transfert de tonalité Pro (Courbe S-Curve paramétrique Lightroom).
pub fn eval_tone_curve(x: f32, t: &TonalParams) -> f32 {
    let mut y = x;
    // 1. Noirs (x < 0.25)
    if t.blacks != 0.0 && x < 0.25 {
        let w = (1.0 - x / 0.25).clamp(0.0, 1.0);
        y += t.blacks * 0.18 * w * (1.0 - x);
    }
    // 2. Ombres (0.0 < x < 0.50)
    if t.shadows != 0.0 && x < 0.50 {
        let w = (1.0 - ((x - 0.25) / 0.25).abs()).max(0.0);
        y += t.shadows * 0.20 * w * (1.0 - x);
    }
    // 3. Contraste Global S-Curve (pivot à 0.45)
    if t.contrast != 0.0 {
        let pivot = 0.45;
        let s = (x - pivot) * t.contrast * 0.35;
        y += s * (x * (1.0 - x) * 4.0);
    }
    // 4. Hautes Lumières (0.50 < x < 1.0)
    if t.highlights != 0.0 && x > 0.50 {
        let w = (1.0 - ((x - 0.70) / 0.25).abs()).max(0.0);
        y += t.highlights * 0.20 * w * x;
    }
    // 5. Blancs (x > 0.75)
    if t.whites != 0.0 && x > 0.75 {
        let w = ((x - 0.75) / 0.25).clamp(0.0, 1.0);
        y += t.whites * 0.22 * w * x;
    }
    y.clamp(0.0, 1.0)
}

/// Affiche l'histogramme de luminance subdivisé en 4 zones avec le tracé interactif de la Courbe Tonale (Tone Curve S-Curve).
fn draw_tonal_histogram(
    ui: &mut egui::Ui,
    hist: Option<&LuminanceHistogram>,
    active_zone: Option<TonalZone>,
    tonal: &TonalParams,
) {
    let desired_size = Vec2::new(ui.available_width(), 82.0);
    let (rect, _response) = ui.allocate_exact_size(desired_size, egui::Sense::hover());
    let painter = ui.painter_at(rect);

    painter.rect_filled(rect, Rounding::same(6.0), Color32::from_rgb(14, 15, 22));
    painter.rect_stroke(rect, Rounding::same(6.0), Stroke::new(1.0f32, Color32::from_rgb(38, 42, 58)));

    let w = rect.width();
    let h = rect.height() - 18.0;

    let zones = [
        (TonalZone::Blacks, 0.0, 0.25, Color32::from_rgb(45, 55, 80), "Noirs"),
        (TonalZone::Shadows, 0.25, 0.50, Color32::from_rgb(30, 80, 120), "Ombres"),
        (TonalZone::Highlights, 0.50, 0.75, Color32::from_rgb(140, 100, 30), "Hautes Lum."),
        (TonalZone::Whites, 0.75, 1.0, Color32::from_rgb(160, 160, 170), "Blancs"),
    ];

    for (z, start_pct, end_pct, zone_col, label) in zones {
        let z_left = rect.left() + w * start_pct;
        let z_right = rect.left() + w * end_pct;
        let z_rect = Rect::from_min_max(Pos2::new(z_left, rect.top()), Pos2::new(z_right, rect.top() + h));

        let is_active = active_zone == Some(z);
        if is_active {
            painter.rect_filled(z_rect, Rounding::ZERO, Color32::from_rgba_premultiplied(0, 190, 255, 45));
            painter.rect_stroke(z_rect, Rounding::ZERO, Stroke::new(1.5f32, Color32::from_rgb(0, 210, 255)));
        }

        if start_pct > 0.0 {
            painter.line_segment(
                [Pos2::new(z_left, rect.top()), Pos2::new(z_left, rect.top() + h)],
                Stroke::new(1.0f32, Color32::from_rgb(35, 38, 52)),
            );
        }

        let text_col = if is_active {
            Color32::from_rgb(0, 220, 255)
        } else {
            zone_col
        };
        painter.text(
            Pos2::new((z_left + z_right) * 0.5, rect.top() + h + 9.0),
            egui::Align2::CENTER_CENTER,
            label,
            egui::FontId::proportional(9.5),
            text_col,
        );
    }

    // 1. Histogramme de luminance en barres semi-transparentes
    if let Some(hist) = hist {
        let max_val = hist.max_count.max(1) as f32;
        let num_bins = 256;
        for i in 0..num_bins {
            let bin_count = hist.bins[i];
            if bin_count == 0 {
                continue;
            }
            let x = rect.left() + (i as f32 / 255.0) * w;
            let bar_h = ((bin_count as f32 / max_val).sqrt() * (h - 2.0)).clamp(1.0, h);
            let y_top = rect.top() + h - bar_h;

            let col = if i < 64 {
                Color32::from_rgba_premultiplied(70, 95, 140, 180)
            } else if i < 128 {
                Color32::from_rgba_premultiplied(60, 140, 190, 180)
            } else if i < 192 {
                Color32::from_rgba_premultiplied(200, 155, 50, 180)
            } else {
                Color32::from_rgba_premultiplied(210, 210, 230, 200)
            };

            painter.line_segment(
                [Pos2::new(x, rect.top() + h), Pos2::new(x, y_top)],
                Stroke::new(1.0f32, col),
            );
        }
    }

    // 2. Tracé de la diagonale de référence (Ligne 45°)
    painter.line_segment(
        [Pos2::new(rect.left(), rect.top() + h), Pos2::new(rect.left() + w, rect.top())],
        Stroke::new(1.0f32, Color32::from_rgba_premultiplied(100, 110, 140, 80)),
    );

    // 3. Tracé de la Courbe Tonale (Tone Curve S-Curve)
    let n_pts = 64;
    let mut curve_pts = Vec::with_capacity(n_pts);
    for k in 0..n_pts {
        let x_norm = k as f32 / (n_pts - 1) as f32;
        let y_norm = eval_tone_curve(x_norm, tonal);
        let px = rect.left() + x_norm * w;
        let py = rect.top() + h - y_norm * h;
        curve_pts.push(Pos2::new(px, py));
    }
    for i in 0..curve_pts.len() - 1 {
        painter.line_segment(
            [curve_pts[i], curve_pts[i + 1]],
            Stroke::new(2.2f32, Color32::from_rgb(0, 225, 255)),
        );
    }

    // 4. Points d'ancrage sur les centres des 4 zones Lightroom
    let anchors = [
        (0.125f32, TonalZone::Blacks),
        (0.375f32, TonalZone::Shadows),
        (0.625f32, TonalZone::Highlights),
        (0.875f32, TonalZone::Whites),
    ];
    for &(ax, zone) in &anchors {
        let ay = eval_tone_curve(ax, tonal);
        let pt = Pos2::new(rect.left() + ax * w, rect.top() + h - ay * h);
        let is_zone_active = active_zone == Some(zone);
        if is_zone_active {
            painter.circle_stroke(pt, 5.0, Stroke::new(1.5f32, Color32::WHITE));
            painter.circle_filled(pt, 3.5, Color32::from_rgb(0, 220, 255));
        } else {
            painter.circle_filled(pt, 2.5, Color32::from_rgb(0, 180, 240));
            painter.circle_stroke(pt, 2.5, Stroke::new(1.0f32, Color32::WHITE));
        }
    }
}

/// Dessine le réticule et la bulle d'informations de l'outil d'ajustement ciblé direct (Lightroom TAT).
fn draw_tat_hud(painter: &egui::Painter, pos: Pos2, zone: TonalZone, lum_pct: f32, params: &ProcessingParams) {
    let (zone_val, zone_col) = match zone {
        TonalZone::Blacks => (params.tonal_blacks, Color32::from_rgb(100, 140, 220)),
        TonalZone::Shadows => (params.tonal_shadows, Color32::from_rgb(0, 190, 255)),
        TonalZone::Highlights => (params.tonal_highlights, Color32::from_rgb(255, 185, 45)),
        TonalZone::Whites => (params.tonal_whites, Color32::from_rgb(255, 255, 255)),
    };

    // 1. Cercle cible avec réticule
    painter.circle_filled(pos, 3.0, Color32::WHITE);
    painter.circle_stroke(pos, 16.0, Stroke::new(1.8f32, Color32::from_rgb(0, 220, 255)));
    painter.circle_stroke(pos, 8.0, Stroke::new(1.0f32, Color32::from_rgba_premultiplied(255, 255, 255, 160)));

    // Ticks du réticule
    painter.line_segment([Pos2::new(pos.x - 22.0, pos.y), Pos2::new(pos.x - 17.0, pos.y)], Stroke::new(2.0f32, Color32::from_rgb(0, 220, 255)));
    painter.line_segment([Pos2::new(pos.x + 17.0, pos.y), Pos2::new(pos.x + 22.0, pos.y)], Stroke::new(2.0f32, Color32::from_rgb(0, 220, 255)));
    painter.line_segment([Pos2::new(pos.x, pos.y - 22.0), Pos2::new(pos.x, pos.y - 17.0)], Stroke::new(2.0f32, Color32::from_rgb(0, 220, 255)));
    painter.line_segment([Pos2::new(pos.x, pos.y + 17.0), Pos2::new(pos.x, pos.y + 22.0)], Stroke::new(2.0f32, Color32::from_rgb(0, 220, 255)));

    // 2. Bulle flottante (HUD Pill)
    let hud_pos = Pos2::new(pos.x + 26.0, pos.y - 14.0);
    let text = format!("🎯 {} : {:+.0}% (L {:.0}%)", zone.name_fr(), zone_val * 100.0, lum_pct * 100.0);
    let badge_rect = Rect::from_min_size(hud_pos, Vec2::new(180.0, 28.0));

    painter.rect_filled(badge_rect, Rounding::same(6.0), Color32::from_rgba_premultiplied(12, 14, 22, 230));
    painter.rect_stroke(badge_rect, Rounding::same(6.0), Stroke::new(1.2f32, Color32::from_rgb(0, 200, 255)));
    painter.text(badge_rect.center(), egui::Align2::CENTER_CENTER, text, egui::FontId::proportional(11.5), zone_col);
}

/// Crée un tampon de prévisualisation rapide et léger (max_dim pixels max) avec filtre bilinéaire propre.
fn create_preview_buffer(img: &LoadedImage, max_dim: u32) -> PreviewBuffer {
    let orig_w = img.width;
    let orig_h = img.height;
    let max_side = orig_w.max(orig_h);

    if max_side <= max_dim {
        return PreviewBuffer {
            sdr_uint8: img.sdr_uint8.clone(),
            linear_rgb: if img.is_native_hdr { Some(img.linear_rgb.clone()) } else { None },
            width: orig_w,
            height: orig_h,
        };
    }

    let scale = max_dim as f32 / max_side as f32;
    let new_w = ((orig_w as f32 * scale).round() as u32).max(1);
    let new_h = ((orig_h as f32 * scale).round() as u32).max(1);

    // 1. Redimensionner l'image SDR 8-bit
    let sdr_img = ImageBuffer::<Rgb<u8>, _>::from_raw(orig_w, orig_h, img.sdr_uint8.clone())
        .expect("Buffer SDR invalide");
    let resized_sdr = image::imageops::resize(&sdr_img, new_w, new_h, image::imageops::FilterType::Triangle);
    let preview_sdr = resized_sdr.into_raw();

    // 2. Redimensionner les données linéaires float si HDR natif
    let preview_lin = if img.is_native_hdr {
        let lin_img = ImageBuffer::<Rgb<f32>, _>::from_raw(orig_w, orig_h, img.linear_rgb.clone())
            .expect("Buffer Linear invalide");
        let resized_lin = image::imageops::resize(&lin_img, new_w, new_h, image::imageops::FilterType::Triangle);
        Some(resized_lin.into_raw())
    } else {
        None
    };

    PreviewBuffer {
        sdr_uint8: preview_sdr,
        linear_rgb: preview_lin,
        width: new_w,
        height: new_h,
    }
}

fn draw_pill(ui: &mut egui::Ui, text: &str, bg: Color32, fg: Color32) {
    egui::Frame::none()
        .fill(bg)
        .rounding(Rounding::same(10.0))
        .inner_margin(egui::Margin::symmetric(8.0, 3.0))
        .show(ui, |ui| {
            ui.label(egui::RichText::new(text).size(11.0).color(fg).strong());
        });
}

fn draw_val_pill(ui: &mut egui::Ui, text: &str) {
    egui::Frame::none()
        .fill(Color32::from_rgb(30, 34, 48))
        .stroke(Stroke::new(1.0f32, Color32::from_rgb(50, 56, 78)))
        .rounding(Rounding::same(6.0))
        .inner_margin(egui::Margin::symmetric(6.0, 2.0))
        .show(ui, |ui| {
            ui.label(egui::RichText::new(text).size(11.5).strong().color(Color32::from_rgb(0, 200, 255)));
        });
}

fn draw_meta_box(ui: &mut egui::Ui, label: &str, val: &str) {
    egui::Frame::none()
        .fill(Color32::from_rgb(16, 17, 24))
        .stroke(Stroke::new(1.0f32, Color32::from_rgb(40, 44, 60)))
        .rounding(Rounding::same(6.0))
        .inner_margin(egui::Margin::symmetric(8.0, 5.0))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(label).size(11.0).color(Color32::GRAY));
                ui.label(egui::RichText::new(val).strong().size(11.5).color(Color32::WHITE));
            });
        });
}

fn draw_floating_badge(painter: &egui::Painter, pos: Pos2, text: &str, text_col: Color32) {
    let rect = Rect::from_min_size(pos, Vec2::new(140.0, 26.0));
    painter.rect_filled(rect, Rounding::same(6.0), Color32::from_rgba_premultiplied(12, 14, 20, 210));
    painter.rect_stroke(rect, Rounding::same(6.0), Stroke::new(1.0f32, Color32::from_rgba_premultiplied(60, 65, 85, 180)));
    painter.text(rect.center(), egui::Align2::CENTER_CENTER, text, egui::FontId::proportional(12.0), text_col);
}

#[derive(Debug, Clone)]
struct ParsedEdid {
    model_name: String,
    bits_per_channel: u8,
    is_hdr: bool,
    max_lum_nits: u32,
    min_lum_nits: f32,
    is_bt2020: bool,
}

/// Analyse les blocs EDID matériels bruts (CTA-861 / DisplayPort / HDMI) pour extraire les métadonnées HDR.
fn parse_drm_edid(data: &[u8]) -> Option<ParsedEdid> {
    if data.len() < 128 {
        return None;
    }
    // 1. Nom du modèle depuis les descripteurs du bloc 0 (offsets 54, 72, 90, 108)
    let mut model_name = String::new();
    for offset in [54, 72, 90, 108] {
        if offset + 18 <= data.len() {
            let desc = &data[offset..offset + 18];
            if desc[0] == 0 && desc[1] == 0 && desc[2] == 0 && (desc[3] == 0xfc || desc[3] == 0xfe) {
                let s = String::from_utf8_lossy(&desc[5..18]);
                let trimmed = s.trim();
                if !trimmed.is_empty() {
                    model_name = trimmed.to_string();
                    break;
                }
            }
        }
    }
    if model_name.is_empty() {
        model_name = "Écran Principal".to_string();
    }

    // 2. Profondeur de couleur (Byte 20 bits 6-4)
    let bpc_code = (data[20] >> 4) & 0x07;
    let bits_per_channel = match bpc_code {
        1 => 6,
        2 => 8,
        3 => 10,
        4 => 12,
        5 => 14,
        6 => 16,
        _ => 8,
    };

    // 3. Blocs d'extension CTA-861 (Bloc 1 et suivants)
    let mut is_hdr = false;
    let mut max_lum_nits = 0u32;
    let mut min_lum_nits = 0.0f32;
    let mut is_bt2020 = false;

    let n_blocks = data.len() / 128;
    for b in 1..n_blocks {
        let ext = &data[b * 128..(b + 1) * 128];
        if ext[0] == 0x02 { // Extension CTA-861
            let dtd_offset = (ext[2] as usize).min(ext.len());
            let mut pos = 4;
            while pos < dtd_offset && pos < ext.len() {
                let header = ext[pos];
                let tag = header >> 5;
                let length = (header & 0x1f) as usize;
                pos += 1;
                if tag == 7 && length >= 2 && pos + length <= ext.len() {
                    let ext_tag = ext[pos];
                    if ext_tag == 5 {
                        // Colorimetry Data Block (BT.2020)
                        if ext[pos + 1] & 0x80 != 0 {
                            is_bt2020 = true;
                        }
                    } else if ext_tag == 6 {
                        // HDR Static Metadata Data Block (ST2084 / HLG)
                        let eotf = ext[pos + 1];
                        if eotf & 0x04 != 0 || eotf & 0x08 != 0 {
                            is_hdr = true;
                        }
                        if length >= 4 {
                            let max_lum_byte = ext[pos + 3];
                            if max_lum_byte > 0 {
                                max_lum_nits = (50.0 * (2.0f32).powf(max_lum_byte as f32 / 32.0)).round() as u32;
                            }
                        }
                        if length >= 6 {
                            let min_lum_byte = ext[pos + 5];
                            if min_lum_byte > 0 && max_lum_nits > 0 {
                                min_lum_nits = (max_lum_nits as f32 * (min_lum_byte as f32 / 255.0).powi(2) / 100.0 * 1000.0).round() / 1000.0;
                            }
                        }
                    }
                }
                pos += length;
            }
        }
    }

    Some(ParsedEdid {
        model_name,
        bits_per_channel,
        is_hdr,
        max_lum_nits,
        min_lum_nits,
        is_bt2020,
    })
}

/// Détecte les capacités HDR physiques de l'écran (Windows DXGI / WDDM ou Linux DRM sysfs / Wayland).
fn detect_hdr_display() -> (bool, String, f32) {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;

        #[repr(C)]
        struct DisplayDeviceW {
            cb: u32,
            device_name: [u16; 32],
            device_string: [u16; 128],
            state_flags: u32,
            device_id: [u16; 128],
            device_key: [u16; 128],
        }

        extern "system" {
            fn EnumDisplayDevicesW(
                lp_device: *const u16,
                i_dev_num: u32,
                lp_display_device: *mut DisplayDeviceW,
                dw_flags: u32,
            ) -> i32;
        }

        // 1. Énumération des écrans actuellement connectés et attachés au bureau Windows
        let mut primary_device_id = String::new();
        let mut attached_device_ids = Vec::new();

        unsafe {
            let mut dev = DisplayDeviceW {
                cb: std::mem::size_of::<DisplayDeviceW>() as u32,
                device_name: [0; 32],
                device_string: [0; 128],
                state_flags: 0,
                device_id: [0; 128],
                device_key: [0; 128],
            };

            let mut i = 0;
            while EnumDisplayDevicesW(std::ptr::null(), i, &mut dev, 0) != 0 {
                if (dev.state_flags & 1) != 0 {
                    let is_primary_adapter = (dev.state_flags & 4) != 0;
                    let mut mon = DisplayDeviceW {
                        cb: std::mem::size_of::<DisplayDeviceW>() as u32,
                        device_name: [0; 32],
                        device_string: [0; 128],
                        state_flags: 0,
                        device_id: [0; 128],
                        device_key: [0; 128],
                    };
                    let mut j = 0;
                    while EnumDisplayDevicesW(dev.device_name.as_ptr(), j, &mut mon, 0) != 0 {
                        if (mon.state_flags & 1) != 0 {
                            let id = String::from_utf16_lossy(&mon.device_id)
                                .trim_matches('\0')
                                .to_string();
                            let parts: Vec<&str> = id.split('\\').collect();
                            if parts.len() >= 2 {
                                let mon_id = parts[1].to_string();
                                if is_primary_adapter && primary_device_id.is_empty() {
                                    primary_device_id = mon_id.clone();
                                }
                                attached_device_ids.push(mon_id);
                            }
                        }
                        j += 1;
                    }
                }
                i += 1;
            }
        }

        let target_mon_id = if !primary_device_id.is_empty() {
            primary_device_id
        } else if let Some(first) = attached_device_ids.first() {
            first.clone()
        } else {
            String::new()
        };

        let mut is_hdr = false;
        let mut model_name = String::new();
        let mut max_lum = 0u32;
        let mut min_lum = 0.0f32;
        let mut bpc = 8u8;
        let mut is_bt2020 = false;

        // 2. Récupération de l'EDID physique de l'écran connecté dans le registre
        if !target_mon_id.is_empty() {
            let reg_path = format!(r"HKLM\SYSTEM\CurrentControlSet\Enum\DISPLAY\{}", target_mon_id);

            if let Ok(output) = std::process::Command::new("reg")
                .args(["query", &reg_path, "/s", "/v", "EDID"])
                .creation_flags(CREATE_NO_WINDOW)
                .output()
            {
                let text = String::from_utf8_lossy(&output.stdout);
                for line in text.lines() {
                    let trimmed = line.trim();
                    if trimmed.starts_with("EDID") && trimmed.contains("REG_BINARY") {
                        if let Some(pos) = trimmed.find("REG_BINARY") {
                            let hex_str = trimmed[pos + 10..].trim();
                            let mut edid_bytes = Vec::new();
                            let chars: Vec<char> = hex_str.chars().filter(|c| c.is_ascii_hexdigit()).collect();
                            for chunk in chars.chunks(2) {
                                if chunk.len() == 2 {
                                    if let Ok(byte) = u8::from_str_radix(&format!("{}{}", chunk[0], chunk[1]), 16) {
                                        edid_bytes.push(byte);
                                    }
                                }
                            }
                            if let Some(parsed) = parse_drm_edid(&edid_bytes) {
                                model_name = parsed.model_name;
                                bpc = parsed.bits_per_channel;
                                max_lum = parsed.max_lum_nits;
                                min_lum = parsed.min_lum_nits;
                                is_bt2020 = parsed.is_bt2020;

                                // L'écran n'est HDR QUE si son matériel supporte le HDR ET que Windows l'a activé
                                if parsed.is_hdr {
                                    if let Ok(adv_out) = std::process::Command::new("reg")
                                        .args(["query", r"HKLM\SYSTEM\CurrentControlSet\Control\GraphicsDrivers\MonitorDataStore", "/s"])
                                        .creation_flags(CREATE_NO_WINDOW)
                                        .output()
                                    {
                                        let adv_text = String::from_utf8_lossy(&adv_out.stdout);
                                        let mut in_target = false;
                                        for adv_line in adv_text.lines() {
                                            if adv_line.starts_with("HKEY_LOCAL_MACHINE") {
                                                in_target = adv_line.contains(&target_mon_id);
                                            }
                                            if in_target && adv_line.contains("AdvancedColorEnabled") && adv_line.contains("0x1") {
                                                is_hdr = true;
                                                break;
                                            }
                                        }
                                    }
                                }
                                break;
                            }
                        }
                    }
                }
            }
        }

        if model_name.is_empty() {
            model_name = "Écran SDR Standard".to_string();
        }

        let peak_nits = if max_lum > 0 {
            max_lum as f32
        } else if is_hdr {
            400.0
        } else {
            300.0 // Standard SDR
        };

        let gamut_str = if is_bt2020 { "BT.2020" } else { "sRGB" };
        let info = if is_hdr {
            let min_str = if min_lum > 0.0 {
                format!("{:.3} nits", min_lum)
            } else {
                "0.005 nits".to_string()
            };
            format!(
                "{} (Pic {:.0} nits, Noir {}, {}-bit {})",
                model_name, peak_nits, min_str, bpc.max(10), gamut_str
            )
        } else {
            format!(
                "{} ({}-bit, {})",
                model_name, bpc, gamut_str
            )
        };

        (is_hdr, info, peak_nits)
    }

    #[cfg(target_os = "android")]
    {
        (
            true,
            "Écran Mobile OLED HDR (Samsung AMOLED / Google Pixel Ultra HDR ~1200 nits, DCI-P3)".to_string(),
            1200.0,
        )
    }

    #[cfg(all(not(target_os = "windows"), not(target_os = "android")))]
    {
        let is_wayland = std::env::var_os("WAYLAND_DISPLAY").is_some()
            || std::env::var("XDG_SESSION_TYPE").map(|v| v == "wayland").unwrap_or(false);

        let mut detected_info = None;
        if let Ok(entries) = std::fs::read_dir("/sys/class/drm") {
            for entry in entries.flatten() {
                let path = entry.path();
                let status_path = path.join("status");
                if let Ok(status) = std::fs::read_to_string(&status_path) {
                    if status.trim() == "connected" {
                        let edid_path = path.join("edid");
                        if let Ok(edid_bytes) = std::fs::read(&edid_path) {
                            if let Some(parsed) = parse_drm_edid(&edid_bytes) {
                                detected_info = Some(parsed);
                                break;
                            }
                        }
                    }
                }
            }
        }

        if let Some(info) = detected_info {
            let backend_str = if is_wayland { "Wayland natif forcé" } else { "X11" };
            let peak_nits = if info.max_lum_nits > 0 {
                info.max_lum_nits as f32
            } else if info.is_hdr {
                400.0
            } else {
                300.0
            };

            if info.is_hdr {
                let min_str = if info.min_lum_nits > 0.0 {
                    format!("{:.3} nits", info.min_lum_nits)
                } else {
                    "0.005 nits (Noir OLED pur)".to_string()
                };
                let gamut_str = if info.is_bt2020 { "BT.2020" } else { "DCI-P3" };
                (
                    true,
                    format!(
                        "Écran OLED HDR {} (Pic {:.0} nits, Noir {}, {}-bit {}) • {}",
                        info.model_name, peak_nits, min_str, info.bits_per_channel, gamut_str, backend_str
                    ),
                    peak_nits,
                )
            } else {
                (
                    false,
                    format!(
                        "Écran SDR {} ({}-bit, {:.0} nits) • Rendu avec simulation • {}",
                        info.model_name, info.bits_per_channel, peak_nits, backend_str
                    ),
                    peak_nits,
                )
            }
        } else if is_wayland {
            (
                true,
                "Linux (Wayland natif forcé) : Protocole Color Management / Vulkan scRGB actif".to_string(),
                400.0,
            )
        } else {
            (
                false,
                "Linux (X11) : Rendu SDR avec simulation de tone-mapping dynamique actif".to_string(),
                300.0,
            )
        }
    }
}

/// Lance l'application graphique native (Desktop en dur).
pub fn run_native_gui(initial_file: Option<PathBuf>) -> Result<(), eframe::Error> {
    // 🐧 Forcer le backend Wayland sous Linux pour garantir la compatibilité HDR et les fenêtres natives
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

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1420.0, 900.0])
            .with_min_inner_size([400.0, 520.0])
            .with_title(format!("UltraHDR Studio v{} — Éditeur Photo vers JPEG Ultra HDR", env!("CARGO_PKG_VERSION")))
            .with_drag_and_drop(true),
        ..Default::default()
    };

    eframe::run_native(
        "UltraHDR Studio",
        native_options,
        Box::new(move |cc| {
            let mut app = UltraHdrApp::new(cc);
            if let Some(file) = initial_file {
                if file.exists() {
                    app.load_file(&file, &cc.egui_ctx);
                }
            }
            Box::new(app)
        }),
    )
}

/// Point d'entrée natif pour l'application Android (NativeActivity / eframe).
#[cfg(target_os = "android")]
#[no_mangle]
pub fn android_main(app: android_activity::AndroidApp) {
    use winit::platform::android::EventLoopBuilderExtAndroid;

    let _ = android_logger::init_once(
        android_logger::Config::default().with_max_level(log::LevelFilter::Info),
    );

    let mut native_options = eframe::NativeOptions::default();
    native_options.event_loop_builder = Some(Box::new(move |builder| {
        builder.with_android_app(app);
    }));

    let _ = eframe::run_native(
        "UltraHDR Studio",
        native_options,
        Box::new(|cc| {
            let mut app = UltraHdrApp::new(cc);
            // Charge automatiquement la scène de test Ultra HDR au démarrage sur Android
            app.load_demo_image(&cc.egui_ctx);
            Box::new(app)
        }),
    );
}

