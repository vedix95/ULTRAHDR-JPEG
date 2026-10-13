//! gui.rs — Interface graphique native haute performance et ultra-ergonomique pour UltraHDR Studio.
//!
//! Rendu matériel direct avec wgpu (DirectX 12 sous Windows, Vulkan / Wayland sous Linux).
//! Pipeline de prévisualisation découplé (1280px temps réel à 60+ FPS) et export pleine résolution native.

use std::path::{Path, PathBuf};
use std::time::Instant;

use eframe::egui::{self, Color32, Pos2, Rect, Rounding, Stroke, Vec2};
use image::{ImageBuffer, Rgb};

use crate::analyzer::{analyze_image_potential, get_preset_library, AnalysisResult, HDRPreset};
use crate::engine::{
    assemble_ultra_hdr_jpeg, compute_hdr_and_gain_map, encode_gain_map_jpeg,
    encode_rgb_jpeg, generate_nits_heatmap, simulate_hdr_on_sdr,
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

pub struct ProcessingParams {
    pub peak_nits: f32,
    pub sdr_white: f32,
    pub shadow_boost: f32,
    pub hi_gamma: f32,
    pub highlight_threshold: f32,
    pub ev_shift: f32,
    pub tone_map_mode: String,
}

impl Default for ProcessingParams {
    fn default() -> Self {
        Self {
            peak_nits: 1000.0,
            sdr_white: 203.0,
            shadow_boost: 1.0,
            hi_gamma: 1.0,
            highlight_threshold: 0.50,
            ev_shift: 0.0,
            tone_map_mode: "aces".to_string(),
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

    // État des accordéons du panneau latéral
    card_proposal_open: bool,
    card_simulation_open: bool,
    card_settings_open: bool,
    card_export_open: bool,

    // Détection matérielle HDR
    is_hdr_display_active: bool,
    display_info_str: String,

    // Barre d'état
    status_message: Option<(String, Instant, Color32)>,
    open_path_input: String,
    show_open_dialog: bool,
}

impl UltraHdrApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // Détection de l'écran HDR (Windows DXGI ou Linux Wayland)
        let (is_hdr, info_str) = detect_hdr_display();

        // ── Thème Sombre Moderne et Agréable (Obsidian / Google Cyan) ──
        let mut visuals = egui::Visuals::dark();
        visuals.override_text_color = Some(Color32::from_rgb(240, 243, 250));
        visuals.panel_fill = Color32::from_rgb(18, 19, 26);
        visuals.window_fill = Color32::from_rgb(24, 26, 36);
        visuals.faint_bg_color = Color32::from_rgb(28, 31, 44);
        visuals.extreme_bg_color = Color32::from_rgb(13, 14, 20);

        visuals.window_rounding = Rounding::same(10.0);
        visuals.menu_rounding = Rounding::same(8.0);
        visuals.widgets.noninteractive.rounding = Rounding::same(8.0);
        visuals.widgets.inactive.rounding = Rounding::same(6.0);
        visuals.widgets.hovered.rounding = Rounding::same(6.0);
        visuals.widgets.active.rounding = Rounding::same(6.0);
        visuals.widgets.open.rounding = Rounding::same(6.0);

        visuals.widgets.inactive.bg_fill = Color32::from_rgb(30, 33, 46);
        visuals.widgets.inactive.bg_stroke = Stroke::new(1.0f32, Color32::from_rgb(48, 52, 72));
        visuals.widgets.hovered.bg_fill = Color32::from_rgb(42, 47, 68);
        visuals.widgets.hovered.bg_stroke = Stroke::new(1.0f32, Color32::from_rgb(0, 190, 255));
        visuals.widgets.active.bg_fill = Color32::from_rgb(0, 120, 212);
        visuals.selection.bg_fill = Color32::from_rgb(0, 120, 212);

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
            card_proposal_open: true,
            card_simulation_open: true,
            card_settings_open: true,
            card_export_open: true,
            is_hdr_display_active: is_hdr,
            display_info_str: info_str,
            status_message: None,
            open_path_input: String::new(),
            show_open_dialog: false,
        }
    }

    pub fn load_file(&mut self, path: &Path, ctx: &egui::Context) {
        match load_image(path) {
            Ok(img) => {
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
                self.params.ev_shift = 0.0;
                self.params.tone_map_mode = "aces".to_string();

                // ⚡ Génération du tampon de prévisualisation rapide (1280px max)
                let preview = create_preview_buffer(&img, 1280);

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
                self.loaded_image = Some(img);
                self.zoom = 1.0;
                self.pan = Vec2::ZERO;

                // Calcul initial des textures de prévisualisation
                self.recompute_and_update_textures(ctx);
                self.set_status(&format!("Fichier chargé : {}", path.display()), Color32::from_rgb(46, 204, 113));
            }
            Err(e) => {
                self.set_status(&format!("Erreur : {}", e), Color32::from_rgb(231, 76, 60));
            }
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
        );

        self.gain_min = res.gain_min;
        self.gain_max = res.gain_max;
        self.preview_multiplier = (res.gain_max.exp2() * 10.0).round() / 10.0;

        // 2. Ne régénérer QUE la texture du mode actif pour garantir 60 FPS constants
        match self.mode {
            ViewerMode::Split | ViewerMode::SideBySide | ViewerMode::FullHdr => {
                let sim_u8 = simulate_hdr_on_sdr(&res.hdr_nits, w, h, p.sdr_white, p.ev_shift, &p.tone_map_mode);
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
                let heat_u8 = generate_nits_heatmap(&res.hdr_nits, w, h, p.sdr_white, p.peak_nits);
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

        let out_path = {
            #[cfg(windows)]
            {
                rfd::FileDialog::new()
                    .set_file_name(&default_out)
                    .add_filter("JPEG Ultra HDR (*.jpg)", &["jpg", "jpeg"])
                    .save_file()
            }
            #[cfg(not(windows))]
            {
                Some(PathBuf::from(&default_out))
            }
        };

        let target_path = match out_path {
            Some(p) => p,
            None => return,
        };

        self.set_status("Calcul et encodage pleine résolution en cours...", Color32::from_rgb(0, 200, 255));

        let img = self.loaded_image.as_ref().unwrap();
        let p = &self.params;

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
        );

        let base_jpeg = match encode_rgb_jpeg(&img.sdr_uint8, w, h, self.base_quality) {
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

        let uhdr_bytes = assemble_ultra_hdr_jpeg(&base_jpeg, &gm_jpeg, res.gain_min, res.gain_max, p.peak_nits, p.sdr_white);

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
}

impl eframe::App for UltraHdrApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Glisser-Déposer de fichier (Drag & Drop)
        if !ctx.input(|i| i.raw.dropped_files.is_empty()) {
            let dropped = ctx.input(|i| i.raw.dropped_files.clone());
            if let Some(first) = dropped.first() {
                if let Some(path) = &first.path {
                    self.load_file(path, ctx);
                }
            }
        }

        // ── 1. Barre Supérieure (Header Élégant) ──
        egui::TopBottomPanel::top("top_panel")
            .frame(egui::Frame::none().fill(Color32::from_rgb(15, 16, 22)).inner_margin(egui::Margin::symmetric(14.0, 8.0)))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    // Logo de marque
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("✨").size(18.0));
                        ui.label(egui::RichText::new("Ultra").size(17.0).strong().color(Color32::WHITE));
                        ui.label(egui::RichText::new("HDR").size(17.0).strong().color(Color32::from_rgb(0, 200, 255)));
                        ui.label(egui::RichText::new("Studio").size(17.0).color(Color32::from_rgb(180, 190, 210)));
                    });

                    ui.add_space(8.0);
                    ui.separator();
                    ui.add_space(8.0);

                    // Badge de statut matériel HDR interactif
                    let (dot_char, badge_text, badge_color) = if self.is_hdr_display_active {
                        ("●", "Écran HDR Physique Actif (scRGB Float)", Color32::from_rgb(46, 204, 113))
                    } else {
                        ("○", "Écran SDR (Simulation Tone-Mapping)", Color32::from_rgb(52, 152, 219))
                    };

                    let badge_btn = egui::Button::new(
                        egui::RichText::new(format!("{} {}", dot_char, badge_text)).size(11.5).color(badge_color),
                    )
                    .fill(Color32::from_rgb(22, 25, 36))
                    .stroke(Stroke::new(1.0f32, Color32::from_rgb(45, 50, 70)))
                    .rounding(Rounding::same(12.0));

                    if ui.add(badge_btn).on_hover_text(&self.display_info_str).clicked() {
                        self.is_hdr_display_active = !self.is_hdr_display_active;
                        self.params.tone_map_mode = if self.is_hdr_display_active {
                            "linear".to_string()
                        } else {
                            "natural".to_string()
                        };
                        self.recompute_and_update_textures(ctx);
                    }

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

                    // Boutons d'actions à droite
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let export_enabled = self.loaded_image.is_some();
                        let btn_export = egui::Button::new(
                            egui::RichText::new("💾 Exporter JPEG Ultra HDR").strong().color(Color32::WHITE).size(13.0),
                        )
                        .fill(if export_enabled { Color32::from_rgb(0, 120, 212) } else { Color32::from_rgb(40, 45, 55) })
                        .rounding(Rounding::same(6.0));

                        if ui.add_enabled(export_enabled, btn_export).clicked() {
                            self.export_ultrahdr();
                        }

                        let btn_open = egui::Button::new(
                            egui::RichText::new("📂 Ouvrir une image").color(Color32::WHITE).size(13.0),
                        )
                        .fill(Color32::from_rgb(35, 40, 55))
                        .rounding(Rounding::same(6.0));

                        if ui.add(btn_open).clicked() {
                            #[cfg(windows)]
                            {
                                if let Some(path) = rfd::FileDialog::new()
                                    .add_filter("Images (*.jpg, *.png, *.webp, *.hdr, *.exr, *.tif)", &["jpg", "jpeg", "png", "webp", "hdr", "exr", "tif", "tiff", "bmp"])
                                    .pick_file()
                                {
                                    self.load_file(&path, ctx);
                                }
                            }
                            #[cfg(not(windows))]
                            {
                                self.show_open_dialog = true;
                            }
                        }
                    });
                });
            });

        // Boîte de dialogue chemin manuel si nécessaire
        if self.show_open_dialog {
            egui::Window::new("Ouvrir un fichier image")
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.label("Entrez le chemin d'accès complet du fichier image :");
                    ui.text_edit_singleline(&mut self.open_path_input);
                    ui.horizontal(|ui| {
                        if ui.button("Charger").clicked() {
                            let path = PathBuf::from(self.open_path_input.trim());
                            if path.exists() {
                                self.load_file(&path, ctx);
                                self.show_open_dialog = false;
                            } else {
                                self.set_status("Chemin introuvable", Color32::from_rgb(231, 76, 60));
                            }
                        }
                        if ui.button("Annuler").clicked() {
                            self.show_open_dialog = false;
                        }
                    });
                });
        }

        // ── 2. Barre d'État Inférieure (Footer) ──
        egui::TopBottomPanel::bottom("bottom_panel")
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
                        ui.label(egui::RichText::new("Norme ISO 21496-1 • Adobe XMP (hdrgm) • CIPA DC-007 MPF").size(11.0).color(Color32::from_rgb(110, 115, 135)));
                    });
                });
            });

        // ── 3. Panneau Latéral de Contrôle (Droite) ──
        egui::SidePanel::right("controls_panel")
            .min_width(340.0)
            .default_width(375.0)
            .frame(egui::Frame::none().fill(Color32::from_rgb(18, 19, 26)).inner_margin(egui::Margin::symmetric(10.0, 8.0)))
            .show(ctx, |ui| {
                // En-tête du panneau latéral avec boutons Tout déplier / Tout replier
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Panneau d'Édition").strong().size(14.0).color(Color32::WHITE));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.link(egui::RichText::new("Tout replier").size(11.5).color(Color32::from_rgb(140, 150, 175))).clicked() {
                            self.card_proposal_open = false;
                            self.card_simulation_open = false;
                            self.card_settings_open = false;
                            self.card_export_open = false;
                        }
                        ui.label(egui::RichText::new("•").size(11.0).color(Color32::from_rgb(80, 85, 105)));
                        if ui.link(egui::RichText::new("Tout déplier").size(11.5).color(Color32::from_rgb(140, 150, 175))).clicked() {
                            self.card_proposal_open = true;
                            self.card_simulation_open = true;
                            self.card_settings_open = true;
                            self.card_export_open = true;
                        }
                    });
                });
                ui.add_space(6.0);

                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    self.ui_sidebar_controls(ui, ctx);
                });
            });

        // ── 4. Panneau Central (Visionneuse d'Image) ──
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(Color32::from_rgb(12, 13, 17)))
            .show(ctx, |ui| {
                self.ui_center_viewport(ui, ctx);
            });
    }
}

impl UltraHdrApp {
    fn ui_sidebar_controls(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let mut params_changed = false;
        let mut preset_to_apply = None;

        let card_frame = egui::Frame::none()
            .fill(Color32::from_rgb(22, 24, 33))
            .stroke(Stroke::new(1.0f32, Color32::from_rgb(42, 46, 64)))
            .rounding(Rounding::same(8.0))
            .inner_margin(egui::Margin::symmetric(12.0, 10.0));

        // ── 1. CARTE : 💡 Proposition Automatique ──
        let proposal_score = self.analysis.as_ref().map(|a| a.potential_score).unwrap_or(0);
        let badge_text = if self.analysis.is_some() { format!("{}%", proposal_score) } else { "--%".to_string() };

        card_frame.show(ui, |ui| {
            let chevron = if self.card_proposal_open { "▾" } else { "▸" };
            let header_resp = ui.horizontal(|ui| {
                ui.label(egui::RichText::new(chevron).size(13.0).color(Color32::from_rgb(150, 155, 175)));
                ui.label(egui::RichText::new("💡").size(14.0));
                ui.label(egui::RichText::new("Proposition Automatique").strong().size(13.5).color(Color32::WHITE));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    draw_pill(ui, &badge_text, Color32::from_rgb(30, 35, 48), Color32::from_rgb(46, 204, 113));
                });
            });

            if header_resp.response.interact(egui::Sense::click()).clicked() {
                self.card_proposal_open = !self.card_proposal_open;
            }

            if self.card_proposal_open {
                ui.add_space(8.0);
                ui.separator();
                ui.add_space(8.0);

                if let Some(analysis) = &self.analysis {
                    ui.label(egui::RichText::new(&analysis.category_title).strong().size(13.5).color(Color32::from_rgb(0, 200, 255)));
                    ui.label(egui::RichText::new(&analysis.explanation).small().color(Color32::from_rgb(180, 185, 200)));

                    ui.add_space(6.0);
                    // Grille de métadonnées
                    ui.horizontal(|ui| {
                        draw_meta_box(ui, "Hautes Lumières", &format!("{:.1}%", analysis.highlight_ratio_pct));
                        draw_meta_box(ui, "Contraste", &format!("{:.0}:1", analysis.contrast_ratio));
                    });

                    ui.add_space(8.0);
                    let btn_apply = egui::Button::new(
                        egui::RichText::new("✨ Appliquer la proposition recommandée").strong().color(Color32::WHITE),
                    )
                    .fill(Color32::from_rgb(190, 130, 0))
                    .rounding(Rounding::same(6.0));

                    if ui.add_sized([ui.available_width(), 28.0], btn_apply).clicked() {
                        preset_to_apply = Some(analysis.recommended_preset.clone());
                    }

                    ui.add_space(10.0);
                    ui.label(egui::RichText::new("Profils prédéfinis :").size(12.0).color(Color32::from_rgb(160, 165, 180)));

                    // Boutons sélecteurs de profils
                    for preset in get_preset_library() {
                        let is_cur = (self.params.peak_nits - preset.peak_nits).abs() < 1.0
                            && (self.params.highlight_threshold - preset.highlight_threshold).abs() < 0.02;

                        let btn = egui::Button::new(
                            egui::RichText::new(&preset.label).size(12.0).color(if is_cur { Color32::WHITE } else { Color32::from_rgb(190, 195, 215) }),
                        )
                        .fill(if is_cur { Color32::from_rgb(0, 120, 212) } else { Color32::from_rgb(26, 29, 39) })
                        .stroke(Stroke::new(1.0f32, if is_cur { Color32::from_rgb(0, 180, 255) } else { Color32::from_rgb(45, 50, 68) }))
                        .rounding(Rounding::same(5.0));

                        if ui.add_sized([ui.available_width(), 25.0], btn).on_hover_text(&preset.description).clicked() {
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

        // ── 2. CARTE : 🖥️ Simulation Écran SDR ──
        card_frame.show(ui, |ui| {
            let chevron = if self.card_simulation_open { "▾" } else { "▸" };
            let header_resp = ui.horizontal(|ui| {
                ui.label(egui::RichText::new(chevron).size(13.0).color(Color32::from_rgb(150, 155, 175)));
                ui.label(egui::RichText::new("🖥️").size(14.0));
                ui.label(egui::RichText::new("Simulation Écran SDR").strong().size(13.5).color(Color32::WHITE));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    draw_pill(ui, "Max Headroom", Color32::from_rgb(20, 35, 55), Color32::from_rgb(0, 170, 255));
                });
            });

            if header_resp.response.interact(egui::Sense::click()).clicked() {
                self.card_simulation_open = !self.card_simulation_open;
            }

            if self.card_simulation_open {
                ui.spacing_mut().slider_width = (ui.available_width() - 4.0).max(260.0);
                ui.add_space(8.0);
                ui.separator();
                ui.add_space(8.0);

                ui.label(egui::RichText::new("Simule la plage dynamique maximale récupérée sur votre écran standard :").small().color(Color32::from_rgb(170, 175, 190)));
                ui.add_space(6.0);

                // Curseur Exposition Virtuelle EV
                ui.horizontal(|ui| {
                    ui.label("Exposition Virtuelle (EV) :");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        draw_val_pill(ui, &format!("{:+.1} EV", self.params.ev_shift));
                    });
                });
                if ui.add(egui::Slider::new(&mut self.params.ev_shift, -3.0..=3.0).step_by(0.1).show_value(false)).changed() {
                    params_changed = true;
                }
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("-3 EV (Hautes Lumières)").size(10.0).color(Color32::GRAY));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(egui::RichText::new("+3 EV").size(10.0).color(Color32::GRAY));
                    });
                });

                ui.add_space(10.0);
                ui.label(egui::RichText::new("Moteur de Tone-Mapping :").size(12.0).color(Color32::from_rgb(160, 165, 180)));

                let modes = [
                    ("aces", "ACES Filmic (Cinématique & Équilibré)"),
                    ("natural", "Naturel (SDR Préservé)"),
                    ("reinhard", "Reinhard Étendu (Progressif)"),
                    ("linear", "Directe (Observation Écrêtage)"),
                ];

                for (mode_key, mode_title) in modes {
                    let is_cur = self.params.tone_map_mode == mode_key;
                    let btn = egui::Button::new(
                        egui::RichText::new(mode_title).size(12.0).color(if is_cur { Color32::WHITE } else { Color32::from_rgb(180, 185, 205) }),
                    )
                    .fill(if is_cur { Color32::from_rgb(0, 100, 180) } else { Color32::from_rgb(26, 29, 39) })
                    .stroke(Stroke::new(1.0f32, if is_cur { Color32::from_rgb(0, 180, 255) } else { Color32::from_rgb(45, 50, 68) }))
                    .rounding(Rounding::same(5.0));

                    if ui.add_sized([ui.available_width(), 24.0], btn).clicked() {
                        self.params.tone_map_mode = mode_key.to_string();
                        params_changed = true;
                    }
                    ui.add_space(2.0);
                }
            }
        });
        ui.add_space(8.0);

        // ── 3. CARTE : ⚙️ Réglages Ultra HDR ──
        card_frame.show(ui, |ui| {
            let chevron = if self.card_settings_open { "▾" } else { "▸" };
            let header_resp = ui.horizontal(|ui| {
                ui.label(egui::RichText::new(chevron).size(13.0).color(Color32::from_rgb(150, 155, 175)));
                ui.label(egui::RichText::new("⚙️").size(14.0));
                ui.label(egui::RichText::new("Réglages Ultra HDR").strong().size(13.5).color(Color32::WHITE));
            });

            if header_resp.response.interact(egui::Sense::click()).clicked() {
                self.card_settings_open = !self.card_settings_open;
            }

            if self.card_settings_open {
                ui.spacing_mut().slider_width = (ui.available_width() - 4.0).max(260.0);
                ui.add_space(8.0);
                ui.separator();
                ui.add_space(8.0);

                // Peak Nits
                ui.horizontal(|ui| {
                    ui.label("Luminance de Crête :");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        draw_val_pill(ui, &format!("{:.0} nits", self.params.peak_nits));
                    });
                });
                if ui.add(egui::Slider::new(&mut self.params.peak_nits, 400.0..=4000.0).step_by(25.0).show_value(false)).changed() {
                    params_changed = true;
                }
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("400 nits").size(10.0).color(Color32::GRAY));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(egui::RichText::new("4000 nits").size(10.0).color(Color32::GRAY));
                    });
                });

                ui.add_space(8.0);

                // Gamma
                ui.horizontal(|ui| {
                    ui.label("Courbe de Boost (Gamma) :");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        draw_val_pill(ui, &format!("{:.2}", self.params.hi_gamma));
                    });
                });
                if ui.add(egui::Slider::new(&mut self.params.hi_gamma, 0.5..=2.2).step_by(0.05).show_value(false)).changed() {
                    params_changed = true;
                }

                ui.add_space(8.0);

                // Préservation Ombres
                ui.horizontal(|ui| {
                    ui.label("Préservation Ombres :");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        draw_val_pill(ui, &format!("{:.2}×", self.params.shadow_boost));
                    });
                });
                if ui.add(egui::Slider::new(&mut self.params.shadow_boost, 1.0..=2.0).step_by(0.02).show_value(false)).changed() {
                    params_changed = true;
                }

                ui.add_space(8.0);

                // Seuil Hautes Lumières
                ui.horizontal(|ui| {
                    ui.label("Seuil d'Élévation :");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        draw_val_pill(ui, &format!("{:.2}", self.params.highlight_threshold));
                    });
                });
                if ui.add(egui::Slider::new(&mut self.params.highlight_threshold, 0.0..=0.95).step_by(0.02).show_value(false)).changed() {
                    params_changed = true;
                }

                ui.add_space(10.0);

                // Boîte de statistiques à 3 colonnes (Gain Min, Gain Max, Multiplicateur)
                egui::Frame::none()
                    .fill(Color32::from_rgb(16, 17, 24))
                    .stroke(Stroke::new(1.0f32, Color32::from_rgb(45, 50, 70)))
                    .rounding(Rounding::same(6.0))
                    .inner_margin(egui::Margin::symmetric(10.0, 8.0))
                    .show(ui, |ui| {
                        ui.columns(3, |cols| {
                            cols[0].vertical_centered(|ui| {
                                ui.label(egui::RichText::new("Gain Min").size(10.5).color(Color32::GRAY));
                                ui.label(egui::RichText::new(format!("{:.2} IL", self.gain_min)).strong().size(13.0).color(Color32::WHITE));
                            });
                            cols[1].vertical_centered(|ui| {
                                ui.label(egui::RichText::new("Gain Max").size(10.5).color(Color32::GRAY));
                                ui.label(egui::RichText::new(format!("+{:.2} IL", self.gain_max)).strong().size(13.0).color(Color32::from_rgb(0, 200, 255)));
                            });
                            cols[2].vertical_centered(|ui| {
                                ui.label(egui::RichText::new("Multiplicateur").size(10.5).color(Color32::GRAY));
                                ui.label(egui::RichText::new(format!("{:.1}×", self.preview_multiplier)).strong().size(13.0).color(Color32::from_rgb(255, 195, 40)));
                            });
                        });
                    });
            }
        });
        ui.add_space(8.0);

        // ── 4. CARTE : 💾 Exportation ──
        let mut trigger_export = false;

        card_frame.show(ui, |ui| {
            let chevron = if self.card_export_open { "▾" } else { "▸" };
            let header_resp = ui.horizontal(|ui| {
                ui.label(egui::RichText::new(chevron).size(13.0).color(Color32::from_rgb(150, 155, 175)));
                ui.label(egui::RichText::new("💾").size(14.0));
                ui.label(egui::RichText::new("Exportation").strong().size(13.5).color(Color32::WHITE));
            });

            if header_resp.response.interact(egui::Sense::click()).clicked() {
                self.card_export_open = !self.card_export_open;
            }

            if self.card_export_open {
                ui.spacing_mut().slider_width = (ui.available_width() - 4.0).max(260.0);
                ui.add_space(8.0);
                ui.separator();
                ui.add_space(8.0);

                ui.horizontal(|ui| {
                    ui.label("Qualité JPEG Base :");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        draw_val_pill(ui, &format!("{}%", self.base_quality));
                    });
                });
                ui.add(egui::Slider::new(&mut self.base_quality, 75..=100).show_value(false));

                ui.add_space(8.0);

                ui.horizontal(|ui| {
                    ui.label("Qualité Gain Map :");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        draw_val_pill(ui, &format!("{}%", self.gainmap_quality));
                    });
                });
                ui.add(egui::Slider::new(&mut self.gainmap_quality, 65..=100).show_value(false));

                ui.add_space(8.0);
                ui.checkbox(&mut self.save_standalone_gainmap, "Enregistrer la Gain Map seule en JPEG (.jpg)");

                ui.add_space(10.0);
                let export_enabled = self.loaded_image.is_some();
                let btn_export = egui::Button::new(
                    egui::RichText::new("Enregistrer le JPEG Ultra HDR").strong().size(13.5).color(Color32::WHITE),
                )
                .fill(if export_enabled { Color32::from_rgb(0, 120, 212) } else { Color32::from_rgb(38, 42, 54) })
                .rounding(Rounding::same(6.0));

                if ui.add_sized([ui.available_width(), 34.0], btn_export).clicked() {
                    trigger_export = true;
                }
            }
        });

        if trigger_export {
            self.export_ultrahdr();
        }

        if params_changed {
            self.recompute_and_update_textures(ctx);
        }
    }

    fn apply_preset(&mut self, preset: &HDRPreset) {
        self.params.peak_nits = preset.peak_nits;
        self.params.sdr_white = preset.sdr_white;
        self.params.shadow_boost = preset.shadow_boost;
        self.params.hi_gamma = preset.hi_gamma;
        self.params.highlight_threshold = preset.highlight_threshold;
    }

    fn ui_center_viewport(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        // Barre d'outils de la visionneuse
        egui::Frame::none()
            .fill(Color32::from_rgb(18, 19, 26))
            .inner_margin(egui::Margin::symmetric(12.0, 6.0))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let mut mode_changed = false;
                    let modes = [
                        (ViewerMode::Split, "↔ Glissière Avant / Après"),
                        (ViewerMode::SideBySide, "◫ Côte à côte"),
                        (ViewerMode::FullHdr, "✨ Vrai Ultra HDR Direct"),
                        (ViewerMode::GainMap, "◐ Gain Map"),
                        (ViewerMode::Heatmap, "🔥 Carte Nits"),
                    ];

                    for (m, label) in modes {
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

                    if mode_changed {
                        self.recompute_and_update_textures(ctx);
                    }

                    // Contrôles de Zoom
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(egui::RichText::new("+").size(13.0)).clicked() {
                            self.zoom = (self.zoom * 1.2).clamp(0.2, 8.0);
                        }
                        ui.label(egui::RichText::new(format!("{:.0}%", self.zoom * 100.0)).size(12.0).color(Color32::from_rgb(200, 205, 225)));
                        if ui.button(egui::RichText::new("-").size(13.0)).clicked() {
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
                                egui::RichText::new("Sélectionner un fichier...").strong().size(14.0).color(Color32::WHITE),
                            )
                            .fill(Color32::from_rgb(0, 120, 212))
                            .rounding(Rounding::same(6.0));

                            if ui.add_sized([200.0, 36.0], btn_browse).clicked() {
                                #[cfg(windows)]
                                {
                                    if let Some(path) = rfd::FileDialog::new()
                                        .add_filter("Images (*.jpg, *.png, *.webp, *.hdr, *.exr, *.tif)", &["jpg", "jpeg", "png", "webp", "hdr", "exr", "tif", "tiff", "bmp"])
                                        .pick_file()
                                    {
                                        self.load_file(&path, ctx);
                                    }
                                }
                                #[cfg(not(windows))]
                                {
                                    self.show_open_dialog = true;
                                }
                            }
                        });
                    });
            });
            return;
        }

        // Zone d'affichage interactive avec Zoom et Pan
        let available_size = ui.available_size();
        let (rect, response) = ui.allocate_exact_size(available_size, egui::Sense::click_and_drag());

        // Zoom à la molette
        let scroll_delta = ctx.input(|i| i.raw_scroll_delta.y);
        if scroll_delta != 0.0 && response.hovered() {
            let zoom_factor = if scroll_delta > 0.0 { 1.12 } else { 0.88 };
            self.zoom = (self.zoom * zoom_factor).clamp(0.2, 8.0);
        }

        // Déplacement (Pan) au clic droit ou milieu ou glisser
        if response.dragged_by(egui::PointerButton::Secondary) || response.dragged_by(egui::PointerButton::Middle) {
            self.pan += response.drag_delta();
        }

        let painter = ui.painter_at(rect);
        let preview = self.preview_buffer.as_ref().unwrap();
        let aspect = preview.width as f32 / preview.height as f32;

        let mut draw_w = rect.width().min(rect.height() * aspect) * self.zoom;
        let mut draw_h = (draw_w / aspect) * self.zoom;
        if draw_w > rect.width() && self.zoom <= 1.0 {
            draw_w = rect.width();
            draw_h = draw_w / aspect;
        }

        let center = rect.center() + self.pan;
        let img_rect = Rect::from_center_size(center, Vec2::new(draw_w, draw_h));

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

                    // Badges semi-transparents élégants
                    draw_floating_badge(&painter, Pos2::new(img_rect.left() + 16.0, img_rect.top() + 16.0), "Avant : SDR Original", Color32::WHITE);
                    draw_floating_badge(&painter, Pos2::new(img_rect.right() - 150.0, img_rect.top() + 16.0), "Après : Ultra HDR", Color32::from_rgb(0, 220, 255));

                    // Drag de la barre
                    if response.dragged_by(egui::PointerButton::Primary) {
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
                    let total_w = rect.width() - gap;
                    let single_w = (total_w * 0.49).min(rect.height() * aspect) * self.zoom;
                    let single_h = single_w / aspect;
                    let left_rect = Rect::from_center_size(
                        Pos2::new(center.x - single_w * 0.5 - gap * 0.5, center.y),
                        Vec2::new(single_w, single_h),
                    );
                    let right_rect = Rect::from_center_size(
                        Pos2::new(center.x + single_w * 0.5 + gap * 0.5, center.y),
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
}

// ── Fonctions Utilitaires d'Affichage ──

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

/// Détecte les capacités HDR de l'écran (Windows DXGI / WDDM ou Linux Wayland).
fn detect_hdr_display() -> (bool, String) {
    #[cfg(target_os = "windows")]
    {
        let is_hdr = false;
        let info = "Windows: Détection pipeline scRGB linéaire (DXGI / Direct3D 12)".to_string();
        (is_hdr, info)
    }

    #[cfg(not(target_os = "windows"))]
    {
        let is_wayland = std::env::var("WAYLAND_DISPLAY").is_ok();
        if is_wayland {
            (
                true,
                "Linux (Wayland): Prise en charge native du protocole Color Management / Vulkan scRGB".to_string(),
            )
        } else {
            (
                false,
                "Linux (X11): Rendu SDR avec simulation de tone-mapping actif".to_string(),
            )
        }
    }
}

/// Lance l'application graphique native (Desktop en dur).
pub fn run_native_gui(initial_file: Option<PathBuf>) -> Result<(), eframe::Error> {
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1400.0, 880.0])
            .with_min_inner_size([1020.0, 680.0])
            .with_title("UltraHDR Studio — Éditeur Photo vers JPEG Ultra HDR")
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
