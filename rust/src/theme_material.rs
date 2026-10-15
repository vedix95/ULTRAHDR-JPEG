//! Material Design 3 (Material You) Theme & Responsive Screen Form Factor Engine
//!
//! Provides automatic screen classification across all known Android display formats:
//! - Modern 19.5:9 and 20:9 smartphones (Samsung Galaxy S20-S24, Pixel 6-9, etc.)
//! - Ultra-tall 21:9 and 22:9 devices (Sony Xperia, cover screens)
//! - Classic 16:9 and 18:9 displays
//! - Foldable / Square screens (Galaxy Z Fold, Pixel Fold open: 4:3, 6:5, 1:1)
//! - Tablets & Desktop Dual-Pane (16:10, 3:2, 4:3 landscape)
//!
//! Enforces Material Design 3 Dark surfaces, pill chips, 16dp rounded cards,
//! and >= 48dp minimum touch targets for effortless mobile ergonomics.

#![allow(dead_code)]

use egui::{
    epaint::Shadow, Color32, Context, FontFamily, FontId, Margin, Response, Rounding, Stroke,
    TextStyle, Ui, Vec2, Visuals,
};

/// Material Design 3 Color Tokens (Dark Theme / Material You)
pub mod colors {
    use egui::Color32;

    /// Deep dark surface for OLED power saving & high contrast
    pub const SURFACE_0: Color32 = Color32::from_rgb(17, 19, 24);
    /// Elevated surface container for cards and panels
    pub const SURFACE_CONTAINER: Color32 = Color32::from_rgb(29, 32, 38);
    /// Higher elevation surface container for active cards / sheets
    pub const SURFACE_CONTAINER_HIGH: Color32 = Color32::from_rgb(38, 41, 51);

    /// Primary Material 3 accent (Soft Indigo / Material Blue)
    pub const PRIMARY: Color32 = Color32::from_rgb(138, 180, 248);
    /// Text on primary accent
    pub const ON_PRIMARY: Color32 = Color32::from_rgb(4, 30, 73);
    /// Primary container background for active chips and selected items
    pub const PRIMARY_CONTAINER: Color32 = Color32::from_rgb(47, 65, 94);
    /// Text on primary container
    pub const ON_PRIMARY_CONTAINER: Color32 = Color32::from_rgb(211, 228, 255);

    /// Secondary container for inactive chips & subtle controls
    pub const SECONDARY_CONTAINER: Color32 = Color32::from_rgb(52, 59, 70);
    pub const ON_SECONDARY_CONTAINER: Color32 = Color32::from_rgb(222, 226, 235);

    /// Tertiary accent for HDR active badge and peak luminance controls (Teal / Emerald)
    pub const TERTIARY_HDR: Color32 = Color32::from_rgb(118, 214, 213);
    pub const TERTIARY_HDR_CONTAINER: Color32 = Color32::from_rgb(28, 66, 65);

    /// Warning accent for Gain Map saturation & clipping alerts (Material Amber)
    pub const WARNING: Color32 = Color32::from_rgb(255, 213, 79);

    /// Subtle outline borders
    pub const OUTLINE: Color32 = Color32::from_rgb(68, 71, 79);
    pub const OUTLINE_VARIANT: Color32 = Color32::from_rgb(47, 50, 58);

    /// High contrast text
    pub const ON_SURFACE: Color32 = Color32::from_rgb(226, 226, 233);
    pub const ON_SURFACE_VARIANT: Color32 = Color32::from_rgb(196, 199, 208);
}

/// Known Android & Cross-Platform Screen Form Factors
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScreenFormFactor {
    /// Tall portrait smartphone (16:9, 18:9, 19.5:9, 20:9, 21:9)
    /// Top image viewer + Bottom Sheet controls with scrollable Material Chips
    SmartphonePortrait,
    /// Landscape smartphone
    /// Left image viewer + Right collapsible / scrollable control panel
    SmartphoneLandscape,
    /// Foldable phone unfolded or square tablet (4:3, 6:5, 1:1)
    /// Balanced dual-column or adaptive split-pane
    FoldableSquare,
    /// Android Tablet / Large Display (16:10, 3:2, 4:3 landscape >= 720dp)
    /// Permanent Dual-Pane layout with high-res viewer & persistent sidebar
    TabletLarge,
}

impl ScreenFormFactor {
    /// Detects the form factor based on screen dimensions and aspect ratio R = width / height
    pub fn detect(screen_size: Vec2) -> Self {
        let width = screen_size.x;
        let height = screen_size.y;
        if height <= 0.0 || width <= 0.0 {
            return Self::SmartphonePortrait;
        }

        let ratio = width / height;

        if ratio < 0.72 {
            // High portrait: 18:9, 19.5:9, 20:9, 21:9 smartphones
            Self::SmartphonePortrait
        } else if ratio <= 1.25 {
            // Quasi-square: Galaxy Z Fold, Pixel Fold open (4:3, 6:5, 1:1)
            Self::FoldableSquare
        } else if width >= 720.0 {
            // Wide tablet or desktop window (>= 720dp width)
            Self::TabletLarge
        } else {
            // Narrow landscape smartphone (< 720dp width)
            Self::SmartphoneLandscape
        }
    }

    /// Whether this form factor displays settings below the image
    pub fn is_bottom_sheet(&self) -> bool {
        matches!(self, Self::SmartphonePortrait)
    }

    /// Recommended height ratio of screen dedicated to image viewer
    pub fn image_height_ratio(&self) -> f32 {
        match self {
            Self::SmartphonePortrait => 0.48_f32, // 48% viewer, 52% bottom sheet
            Self::FoldableSquare => 0.58_f32,     // Generous square viewer
            Self::SmartphoneLandscape | Self::TabletLarge => 1.0_f32, // Full height in left pane
        }
    }
}

/// Dynamic Material Design 3 Palette adapted to the host device (Android Monet / Windows Accent)
#[derive(Debug, Clone, Copy)]
pub struct DynamicM3Theme {
    pub surface_0: Color32,
    pub surface_container: Color32,
    pub surface_container_high: Color32,
    pub primary: Color32,
    pub on_primary: Color32,
    pub primary_container: Color32,
    pub on_primary_container: Color32,
    pub secondary_container: Color32,
    pub on_secondary_container: Color32,
    pub tertiary_hdr: Color32,
    pub tertiary_hdr_container: Color32,
    pub warning: Color32,
    pub outline: Color32,
    pub outline_variant: Color32,
    pub on_surface: Color32,
    pub on_surface_variant: Color32,
}

impl Default for DynamicM3Theme {
    fn default() -> Self {
        Self {
            surface_0: colors::SURFACE_0,
            surface_container: colors::SURFACE_CONTAINER,
            surface_container_high: colors::SURFACE_CONTAINER_HIGH,
            primary: colors::PRIMARY,
            on_primary: colors::ON_PRIMARY,
            primary_container: colors::PRIMARY_CONTAINER,
            on_primary_container: colors::ON_PRIMARY_CONTAINER,
            secondary_container: colors::SECONDARY_CONTAINER,
            on_secondary_container: colors::ON_SECONDARY_CONTAINER,
            tertiary_hdr: colors::TERTIARY_HDR,
            tertiary_hdr_container: colors::TERTIARY_HDR_CONTAINER,
            warning: colors::WARNING,
            outline: colors::OUTLINE,
            outline_variant: colors::OUTLINE_VARIANT,
            on_surface: colors::ON_SURFACE,
            on_surface_variant: colors::ON_SURFACE_VARIANT,
        }
    }
}

impl DynamicM3Theme {
    /// Detects and extracts dynamic theme colors from the host device (Android Monet, Windows Accent, or default M3).
    pub fn detect_host() -> Self {
        #[cfg(target_os = "android")]
        {
            let mut theme = Self::default();
            let (vm_ptr, act_ptr) = crate::gui::get_android_context_ptrs();
            if !vm_ptr.is_null() && !act_ptr.is_null() {
                unsafe {
                    if let Ok(vm) = jni::JavaVM::from_raw(vm_ptr as *mut _) {
                        if let Ok(mut env) = vm.attach_current_thread() {
                            let act_obj = jni::objects::JObject::from_raw(act_ptr as *mut _);
                            if let Ok(class) = env.get_object_class(&act_obj) {
                                if let Ok(res) = env.call_static_method(&class, "getDynamicColors", "()[I", &[]) {
                                    if let Ok(obj) = res.l() {
                                        if !obj.as_raw().is_null() {
                                            let jarr = jni::objects::JIntArray::from(obj);
                                            let mut raw = [0i32; 5];
                                            if env.get_int_array_region(&jarr, 0, &mut raw).is_ok() && raw[0] != 0 {
                                                let parse = |argb: i32| -> Color32 {
                                                    let u = argb as u32;
                                                    Color32::from_rgb(((u >> 16) & 0xFF) as u8, ((u >> 8) & 0xFF) as u8, (u & 0xFF) as u8)
                                                };
                                                theme.primary = parse(raw[0]);
                                                theme.on_primary = parse(raw[1]);
                                                theme.primary_container = parse(raw[2]);
                                                theme.on_primary_container = parse(raw[3]);
                                                theme.surface_container = parse(raw[4]);
                                            }
                                        }
                                    }
                                }
                            }
                            if env.exception_check().unwrap_or(false) {
                                let _ = env.exception_clear();
                            }
                        }
                    }
                }
            }
            theme
        }

        #[cfg(target_os = "windows")]
        {
            let mut theme = Self::default();
            extern "system" {
                fn RegOpenKeyExA(hkey: isize, subkey: *const i8, opt: u32, sam: u32, res: *mut isize) -> i32;
                fn RegQueryValueExA(hkey: isize, value: *const i8, res: *mut u32, typ: *mut u32, data: *mut u8, len: *mut u32) -> i32;
                fn RegCloseKey(hkey: isize) -> i32;
            }
            const HKEY_CURRENT_USER: isize = 0x80000001u32 as isize;
            const KEY_READ: u32 = 0x20019;
            unsafe {
                let mut key = 0isize;
                let subkey = b"Software\\Microsoft\\Windows\\DWM\0";
                if RegOpenKeyExA(HKEY_CURRENT_USER, subkey.as_ptr() as *const i8, 0, KEY_READ, &mut key) == 0 {
                    let mut val = 0u32;
                    let mut size = 4u32;
                    let val_name = b"AccentColor\0";
                    let res = RegQueryValueExA(key, val_name.as_ptr() as *const i8, std::ptr::null_mut(), std::ptr::null_mut(), &mut val as *mut u32 as *mut u8, &mut size);
                    RegCloseKey(key);
                    if res == 0 && val != 0 {
                        // AccentColor est stocké en format ABGR : 0xAABBGGRR
                        let r = (val & 0xFF) as u8;
                        let g = ((val >> 8) & 0xFF) as u8;
                        let b = ((val >> 16) & 0xFF) as u8;
                        theme.primary = Color32::from_rgb(r, g, b);
                        theme.primary_container = Color32::from_rgb((r as f32 * 0.35) as u8, (g as f32 * 0.35) as u8, (b as f32 * 0.35) as u8);
                        theme.on_primary_container = Color32::from_rgb(
                            ((r as f32 * 0.45 + 140.0).min(255.0)) as u8,
                            ((g as f32 * 0.45 + 140.0).min(255.0)) as u8,
                            ((b as f32 * 0.45 + 140.0).min(255.0)) as u8,
                        );
                    }
                }
            }
            theme
        }

        #[cfg(all(not(target_os = "android"), not(target_os = "windows")))]
        {
            Self::default()
        }
    }
}

/// Adaptive UI Metrics calculated from host screen resolution, window size, and OS scaling multiplier.
#[derive(Debug, Clone, Copy)]
pub struct AdaptiveUiMetrics {
    /// Overall scaling factor (1.00x to 1.40x)
    pub scale: f32,
    /// Standard button height (38px to 46px)
    pub btn_height: f32,
    /// Prominent action button height (e.g. Export: 44px to 52px)
    pub btn_large_height: f32,
    /// Header / toolbar button height (36px to 44px)
    pub btn_header_height: f32,
    /// Square icon button size (40px to 48px)
    pub icon_btn_size: f32,
    /// Icon glyph font size (16px to 21px)
    pub icon_font_size: f32,
    /// Width of a segment in icon segmented bar (46px to 54px)
    pub segmented_item_w: f32,
    /// Height of a segment in icon segmented bar (36px to 44px)
    pub segmented_item_h: f32,
    /// Slider track interaction height (18px to 24px)
    pub slider_track_h: f32,
    /// Minimum touch target dimension according to Material 3 standards (>= 44dp to 48dp)
    pub min_touch_target: f32,
    /// Button typography font size (13px to 16px)
    pub font_btn_size: f32,
    /// Body typography font size (13px to 15.5px)
    pub font_body_size: f32,
    /// Small pill badge font size (11.5px to 13px)
    pub pill_font_size: f32,
    /// Side panel resize handle grab radius (12px to 16px)
    pub resize_grab_radius: f32,
}

impl AdaptiveUiMetrics {
    /// Computes adaptive metrics based on physical display resolution, window dimensions, and OS DPI multiplier.
    pub fn compute(ctx: &Context, is_android: bool) -> Self {
        let ppp = ctx.pixels_per_point().max(1.0);
        let screen_rect = ctx.screen_rect();
        let phys_w = screen_rect.width() * ppp;
        let phys_h = screen_rect.height() * ppp;
        let max_dim = phys_w.max(phys_h);

        // Resolution-based scale factor:
        // - 4K / UHD (>= 3200px): 1.25
        // - 1440p QHD (>= 2200px): 1.15
        // - 1080p FHD (>= 1800px): 1.08
        // - Compact / HD (< 1800px): 1.02
        let res_scale = if max_dim >= 3200.0 {
            1.25_f32
        } else if max_dim >= 2200.0 {
            1.15_f32
        } else if max_dim >= 1800.0 {
            1.08_f32
        } else {
            1.02_f32
        };

        // Window size adjustment:
        let win_w = screen_rect.width();
        let win_scale = if win_w >= 1600.0 {
            1.08_f32
        } else if win_w >= 1200.0 {
            1.04_f32
        } else {
            1.00_f32
        };

        // OS scaling multiplier boost (e.g. Windows 125%, 150%, 200%):
        let ppp_boost = if ppp >= 1.75 {
            1.12_f32
        } else if ppp >= 1.25 {
            1.06_f32
        } else {
            1.00_f32
        };

        let raw_scale = res_scale * win_scale * ppp_boost;

        // Android touchscreens always prioritize generous touch targets (>= 1.18)
        let scale = if is_android {
            raw_scale.clamp(1.18_f32, 1.40_f32)
        } else {
            // Desktop: comfortable mouse aiming and hybrid touch support, cleanly normalized to 1.00 base
            (res_scale * win_scale * ppp_boost * 0.90).clamp(1.00_f32, 1.25_f32)
        };

        Self {
            scale,
            btn_height: (38.0_f32 * scale).round(),
            btn_large_height: (44.0_f32 * scale).round(),
            btn_header_height: (36.0_f32 * scale).round(),
            icon_btn_size: (40.0_f32 * scale).round(),
            icon_font_size: (16.5_f32 * scale).round(),
            segmented_item_w: (46.0_f32 * scale).round(),
            segmented_item_h: (36.0_f32 * scale).round(),
            slider_track_h: (18.0_f32 * scale).round(),
            min_touch_target: (44.0_f32 * scale).round(),
            font_btn_size: (13.0_f32 * scale).round(),
            font_body_size: (13.0_f32 * scale).round(),
            pill_font_size: (11.5_f32 * scale).round(),
            resize_grab_radius: (12.0_f32 * scale).round(),
        }
    }
}

/// Applies Material Design 3 Dark Theme, typography, and adaptive touch targets to egui
pub fn apply_material_3_theme(ctx: &Context, theme: &DynamicM3Theme, is_android: bool) -> (ScreenFormFactor, AdaptiveUiMetrics) {
    let screen_rect = ctx.screen_rect();
    let screen_size = screen_rect.size();
    let form_factor = ScreenFormFactor::detect(screen_size);
    let metrics = AdaptiveUiMetrics::compute(ctx, is_android);

    let mut style = (*ctx.style()).clone();

    // 1. Typography Hierarchy with readable, properly proportioned sizes
    let font_scale = metrics.scale;

    style.text_styles = [
        (
            TextStyle::Heading,
            FontId::new(19.0_f32 * font_scale, FontFamily::Proportional),
        ),
        (
            TextStyle::Name("Title".into()),
            FontId::new(15.5_f32 * font_scale, FontFamily::Proportional),
        ),
        (
            TextStyle::Body,
            FontId::new(metrics.font_body_size, FontFamily::Proportional),
        ),
        (
            TextStyle::Button,
            FontId::new(metrics.font_btn_size, FontFamily::Proportional),
        ),
        (
            TextStyle::Small,
            FontId::new(11.5_f32 * font_scale, FontFamily::Proportional),
        ),
        (
            TextStyle::Monospace,
            FontId::new(12.0_f32 * font_scale, FontFamily::Monospace),
        ),
    ]
    .into();

    // 2. Material 3 Touch Targets & Spacing (generous padding, easy clicking & tapping)
    style.spacing.button_padding = Vec2::new(12.0_f32 * metrics.scale, 7.0_f32 * metrics.scale);
    style.spacing.item_spacing = Vec2::new(8.0_f32 * metrics.scale, 8.0_f32 * metrics.scale);
    style.spacing.slider_width = 180.0_f32 * metrics.scale;
    style.spacing.interact_size = Vec2::new(metrics.min_touch_target, (metrics.btn_height * 0.9).round());
    style.spacing.scroll.bar_width = (8.0_f32 * metrics.scale).round();

    // Élargir la zone de capture des poignées de redimensionnement de panneaux
    style.interaction.resize_grab_radius_side = metrics.resize_grab_radius;

    // 3. Material Design 3 Visuals & Dynamic Color Palette (from host Android Monet or Windows Accent)
    let mut visuals = Visuals::dark();
    visuals.dark_mode = true;
    visuals.override_text_color = Some(theme.on_surface);

    // Surfaces
    visuals.panel_fill = theme.surface_0;
    visuals.window_fill = theme.surface_container;
    visuals.faint_bg_color = theme.surface_container;
    visuals.extreme_bg_color = theme.surface_0;

    // Window & Card Rounding (Material Card = 16dp)
    visuals.window_rounding = Rounding::same(14.0_f32);
    visuals.window_stroke = Stroke::new(1.0_f32, theme.outline_variant);
    visuals.window_shadow = Shadow {
        offset: Vec2::new(0.0_f32, 4.0_f32),
        blur: 12.0_f32,
        spread: 0.0_f32,
        color: Color32::from_black_alpha(80),
    };

    // Button & Chip Pill Rounding (Material Pill = 20dp)
    let pill_rounding = Rounding::same(16.0_f32);

    // Inactive & Noninteractive Widgets (Panels Separators / Resizers)
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, Color32::from_rgb(45, 50, 70));
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, theme.on_surface_variant);
    visuals.widgets.inactive.bg_fill = theme.surface_container;
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, theme.outline_variant);
    visuals.widgets.inactive.rounding = pill_rounding;
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, theme.on_surface);

    // Hovered Widgets
    visuals.widgets.hovered.bg_fill = theme.surface_container_high;
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.2_f32, theme.primary);
    visuals.widgets.hovered.rounding = pill_rounding;
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.2_f32, Color32::WHITE);

    // Active / Pressed Widgets
    visuals.widgets.active.bg_fill = theme.primary_container;
    visuals.widgets.active.bg_stroke = Stroke::new(1.5_f32, theme.primary);
    visuals.widgets.active.rounding = pill_rounding;
    visuals.widgets.active.fg_stroke = Stroke::new(1.5_f32, theme.on_primary_container);

    // Open / Selected Widgets
    visuals.widgets.open.bg_fill = theme.primary_container;
    visuals.widgets.open.bg_stroke = Stroke::new(1.0_f32, theme.primary);
    visuals.widgets.open.rounding = pill_rounding;
    visuals.widgets.open.fg_stroke = Stroke::new(1.0_f32, theme.on_primary_container);

    // Selection highlight
    visuals.selection.bg_fill = theme.primary_container;
    visuals.selection.stroke = Stroke::new(1.5_f32, theme.primary);

    style.visuals = visuals;
    ctx.set_style(style);

    (form_factor, metrics)
}

/// Helper to render a Material 3 Pill Filter Chip (used for mode and filter selection)
pub fn material_chip(ui: &mut Ui, theme: &DynamicM3Theme, selected: bool, label: &str) -> Response {
    let (bg_fill, fg_color, stroke) = if selected {
        (
            theme.primary_container,
            theme.on_primary_container,
            Stroke::new(1.2_f32, theme.primary),
        )
    } else {
        (
            theme.surface_container,
            theme.on_surface_variant,
            Stroke::new(1.0_f32, theme.outline_variant),
        )
    };

    let padding = Vec2::new(8.0_f32, 4.0_f32);
    let rounding = Rounding::same(14.0_f32);

    let font_id = FontId::new(12.0_f32, FontFamily::Proportional);
    let galley = ui.painter().layout_no_wrap(label.to_string(), font_id, fg_color);
    let text_size = galley.size();
    let desired_size = text_size + padding * 2.0_f32;

    let (rect, response) = ui.allocate_exact_size(
        desired_size.max(Vec2::new(32.0_f32, 28.0_f32)),
        egui::Sense::click(),
    );

    if ui.is_rect_visible(rect) {
        let hover_adjust = if response.hovered() {
            theme.surface_container_high
        } else {
            bg_fill
        };

        ui.painter().rect(rect, rounding, hover_adjust, stroke);
        let text_pos = rect.center() - text_size / 2.0_f32;
        ui.painter().galley(text_pos, galley, fg_color);
    }

    response
}

/// Helper to render a Material 3 Navigation Tab spanning a defined width
pub fn material_tab(ui: &mut Ui, theme: &DynamicM3Theme, selected: bool, icon: &str, label: &str, width: f32) -> Response {
    let (bg_fill, fg_color, stroke) = if selected {
        (
            theme.primary_container,
            theme.on_primary_container,
            Stroke::new(1.2_f32, theme.primary),
        )
    } else {
        (
            theme.surface_container,
            theme.on_surface_variant,
            Stroke::new(1.0_f32, theme.outline_variant),
        )
    };

    let rounding = Rounding::same(10.0_f32);
    let (rect, response) = ui.allocate_exact_size(
        Vec2::new(width.max(34.0_f32), 32.0_f32),
        egui::Sense::click(),
    );

    if ui.is_rect_visible(rect) {
        let hover_adjust = if response.hovered() {
            theme.surface_container_high
        } else {
            bg_fill
        };

        ui.painter().rect(rect, rounding, hover_adjust, stroke);

        let text = format!("{} {}", icon, label);
        let font_id = FontId::new(11.0_f32, FontFamily::Proportional);
        let galley = ui.painter().layout_no_wrap(text, font_id, fg_color);
        let text_pos = rect.center() - galley.size() / 2.0_f32;
        ui.painter().galley(text_pos, galley, fg_color);
    }

    response
}

/// Helper to render a Material 3 Card Container
pub fn material_card<R>(
    ui: &mut Ui,
    theme: &DynamicM3Theme,
    title: Option<&str>,
    add_contents: impl FnOnce(&mut Ui) -> R,
) -> R {
    let frame = egui::Frame::none()
        .fill(theme.surface_container)
        .stroke(Stroke::new(1.0_f32, theme.outline_variant))
        .rounding(Rounding::same(16.0_f32))
        .inner_margin(Margin::same(12.0_f32));

    frame
        .show(ui, |ui| {
            if let Some(t) = title {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(t)
                            .font(FontId::new(14.5_f32, FontFamily::Proportional))
                            .color(theme.primary)
                            .strong(),
                    );
                });
                ui.add_space(6.0_f32);
            }
            add_contents(ui)
        })
        .inner
}

/// Helper to render a Material 3 Prominent Button / FAB
pub fn material_button(ui: &mut Ui, theme: &DynamicM3Theme, label: &str, primary: bool) -> Response {
    let (bg, fg, stroke) = if primary {
        (
            theme.primary,
            theme.on_primary,
            Stroke::new(1.0_f32, theme.primary),
        )
    } else {
        (
            theme.surface_container_high,
            theme.on_surface,
            Stroke::new(1.0_f32, theme.outline_variant),
        )
    };

    let btn = egui::Button::new(
        egui::RichText::new(label)
            .color(fg)
            .font(FontId::new(14.0_f32, FontFamily::Proportional))
            .strong(),
    )
    .fill(bg)
    .stroke(stroke)
    .rounding(Rounding::same(24.0_f32))
    .min_size(Vec2::new(48.0_f32, 42.0_f32));

    ui.add(btn)
}

// ─────────────────────────────────────────────────────────────────────────────
// 🌟 COMPOSANTS OFFICIELS MATERIAL DESIGN 3 (m3.material.io)
// ─────────────────────────────────────────────────────────────────────────────

/// 1. Material 3 Filled Button (High Emphasis — Pill 20dp, Primary Fill)
/// https://m3.material.io/components/buttons/overview
pub fn m3_filled_button(ui: &mut Ui, theme: &DynamicM3Theme, icon: &str, label: &str, height: f32) -> Response {
    let metrics = AdaptiveUiMetrics::compute(ui.ctx(), cfg!(target_os = "android"));
    let actual_h = height.max(metrics.btn_height);
    let text = if icon.is_empty() { label.to_string() } else { format!("{} {}", icon, label) };
    let min_w = if ui.layout().is_horizontal() { 0.0_f32 } else { ui.available_width() };

    let btn = egui::Button::new(
        egui::RichText::new(text)
            .color(theme.on_primary)
            .font(FontId::new(metrics.font_btn_size, FontFamily::Proportional))
            .strong(),
    )
    .fill(theme.primary)
    .stroke(Stroke::NONE)
    .rounding(Rounding::same(actual_h / 2.0))
    .min_size(Vec2::new(min_w, actual_h));

    ui.add(btn)
}

/// 2. Material 3 Filled Tonal Button (Medium-High Emphasis — Tonal Surface Fill)
pub fn m3_tonal_button(ui: &mut Ui, theme: &DynamicM3Theme, icon: &str, label: &str, selected: bool) -> Response {
    let metrics = AdaptiveUiMetrics::compute(ui.ctx(), cfg!(target_os = "android"));
    let actual_h = metrics.btn_height;
    let text = if icon.is_empty() { label.to_string() } else { format!("{} {}", icon, label) };
    let (bg, fg, stroke) = if selected {
        (
            theme.primary_container,
            theme.on_primary_container,
            Stroke::new(1.2_f32, theme.primary),
        )
    } else {
        (
            theme.secondary_container,
            theme.on_secondary_container,
            Stroke::new(1.0_f32, theme.outline_variant),
        )
    };

    let min_w = if ui.layout().is_horizontal() { 0.0_f32 } else { ui.available_width() };
    let btn = egui::Button::new(
        egui::RichText::new(text)
            .color(fg)
            .font(FontId::new(metrics.font_btn_size, FontFamily::Proportional))
            .strong(),
    )
    .fill(bg)
    .stroke(stroke)
    .rounding(Rounding::same(actual_h / 2.0))
    .min_size(Vec2::new(min_w, actual_h));

    ui.add(btn)
}

/// 3. Material 3 Outlined Button (Medium Emphasis — Border & Transparent/Surface)
pub fn m3_outlined_button(ui: &mut Ui, theme: &DynamicM3Theme, icon: &str, label: &str) -> Response {
    let metrics = AdaptiveUiMetrics::compute(ui.ctx(), cfg!(target_os = "android"));
    let actual_h = (metrics.btn_height * 0.92).round().max(34.0);
    let text = if icon.is_empty() { label.to_string() } else { format!("{} {}", icon, label) };
    let min_w = if ui.layout().is_horizontal() { 0.0_f32 } else { ui.available_width() };

    let btn = egui::Button::new(
        egui::RichText::new(text)
            .color(theme.on_surface)
            .font(FontId::new(metrics.font_btn_size, FontFamily::Proportional)),
    )
    .fill(theme.surface_container)
    .stroke(Stroke::new(1.0_f32, theme.outline))
    .rounding(Rounding::same(actual_h / 2.0))
    .min_size(Vec2::new(min_w, actual_h));

    ui.add(btn)
}

/// 4. Material 3 Text Button (Low Emphasis — Clean Text with Icon)
pub fn m3_text_button(ui: &mut Ui, theme: &DynamicM3Theme, icon: &str, label: &str) -> Response {
    let metrics = AdaptiveUiMetrics::compute(ui.ctx(), cfg!(target_os = "android"));
    let text = if icon.is_empty() { label.to_string() } else { format!("{} {}", icon, label) };
    let btn = egui::Button::new(
        egui::RichText::new(text)
            .color(theme.primary)
            .font(FontId::new(metrics.font_btn_size, FontFamily::Proportional))
            .strong(),
    )
    .fill(Color32::TRANSPARENT)
    .stroke(Stroke::NONE)
    .rounding(Rounding::same(8.0_f32))
    .min_size(Vec2::new(0.0_f32, (metrics.btn_height * 0.85).round()));

    ui.add(btn)
}

/// 5. Material 3 Icon Button (Circular / Rounded Touch Target >= 40-48dp)
pub fn m3_icon_button(ui: &mut Ui, theme: &DynamicM3Theme, icon: &str, tooltip: &str) -> Response {
    let metrics = AdaptiveUiMetrics::compute(ui.ctx(), cfg!(target_os = "android"));
    let size = metrics.icon_btn_size;
    let btn = egui::Button::new(
        egui::RichText::new(icon)
            .color(theme.on_surface)
            .size(metrics.icon_font_size),
    )
    .fill(theme.surface_container_high)
    .stroke(Stroke::new(1.0_f32, theme.outline_variant))
    .rounding(Rounding::same(size / 2.0))
    .min_size(Vec2::new(size, size));

    let resp = ui.add(btn);
    if !tooltip.is_empty() {
        resp.on_hover_text(tooltip)
    } else {
        resp
    }
}

/// 6. Material 3 Value Pill Badge (utilisé au-dessus des curseurs et dans les en-têtes)
pub fn m3_value_pill(ui: &mut Ui, theme: &DynamicM3Theme, value_str: &str) {
    let metrics = AdaptiveUiMetrics::compute(ui.ctx(), cfg!(target_os = "android"));
    let padding = Vec2::new(8.0_f32 * metrics.scale, 3.0_f32 * metrics.scale);
    let rounding = Rounding::same(12.0_f32);
    let font_id = FontId::new(metrics.pill_font_size, FontFamily::Proportional);
    let galley = ui.painter().layout_no_wrap(value_str.to_string(), font_id, theme.on_primary_container);
    let text_size = galley.size();
    let desired_size = text_size + padding * 2.0_f32;

    let (rect, _) = ui.allocate_exact_size(desired_size.max(Vec2::new(36.0_f32 * metrics.scale, 22.0_f32 * metrics.scale)), egui::Sense::hover());
    if ui.is_rect_visible(rect) {
        ui.painter().rect(rect, rounding, theme.primary_container, Stroke::NONE);
        let text_pos = rect.center() - text_size / 2.0_f32;
        ui.painter().galley(text_pos, galley, theme.on_primary_container);
    }
}

/// 7. Material 3 Slider avec Label, Badge de valeur et piste ergonomique
/// https://m3.material.io/components/sliders/overview
pub fn m3_slider<F>(
    ui: &mut Ui,
    theme: &DynamicM3Theme,
    icon: &str,
    label: &str,
    val: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    step: f64,
    formatter: F,
) -> bool
where
    F: FnOnce(f32) -> String,
{
    let metrics = AdaptiveUiMetrics::compute(ui.ctx(), cfg!(target_os = "android"));
    let mut changed = false;
    let formatted_val = formatter(*val);

    // Ligne 1 : Label avec icône et Badge Pilule M3
    ui.horizontal(|ui| {
        let title_text = if icon.is_empty() {
            label.to_string()
        } else {
            format!("{} {}", icon, label)
        };
        ui.label(
            egui::RichText::new(title_text)
                .color(theme.on_surface)
                .font(FontId::new(metrics.font_btn_size, FontFamily::Proportional))
                .strong(),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            m3_value_pill(ui, theme, &formatted_val);
        });
    });

    // Ligne 2 : Slider tactile pleine largeur avec poignée de préhension confortable
    let slider_w = (ui.available_width() - 4.0_f32).max(120.0_f32);
    ui.spacing_mut().slider_width = slider_w;
    ui.spacing_mut().interact_size.y = metrics.slider_track_h.max(22.0);

    let slider = egui::Slider::new(val, range)
        .step_by(step)
        .show_value(false);

    if ui.add(slider).changed() {
        changed = true;
    }
    ui.add_space(5.0_f32 * metrics.scale);

    changed
}

/// Helper pour slider M3 sur valeur entière u8 (qualité JPEG 0..100)
pub fn m3_slider_u8(
    ui: &mut Ui,
    theme: &DynamicM3Theme,
    icon: &str,
    label: &str,
    val: &mut u8,
    range: std::ops::RangeInclusive<u8>,
) -> bool {
    let mut float_val = *val as f32;
    let min = *range.start() as f32;
    let max = *range.end() as f32;
    let changed = m3_slider(ui, theme, icon, label, &mut float_val, min..=max, 1.0, |v| format!("{:.0}%", v));
    if changed {
        *val = float_val.round().clamp(min, max) as u8;
    }
    changed
}

/// 8. Material 3 Segmented Button (Barre de sélection connectée pour les modes)
/// https://m3.material.io/components/segmented-buttons/overview
pub fn m3_segmented_button<T: PartialEq + Copy>(
    ui: &mut Ui,
    theme: &DynamicM3Theme,
    options: &[(T, &str, &str)],
    current: &mut T,
) -> bool {
    let metrics = AdaptiveUiMetrics::compute(ui.ctx(), cfg!(target_os = "android"));
    let mut changed = false;
    let n = options.len();
    if n == 0 {
        return false;
    }

    let total_w = ui.available_width();
    let item_w = (total_w / n as f32).max(42.0_f32);
    let item_h = metrics.segmented_item_h;

    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing = Vec2::ZERO;

        for (idx, (val, icon, label)) in options.iter().enumerate() {
            let is_selected = *current == *val;
            let (bg, fg, stroke) = if is_selected {
                (
                    theme.primary_container,
                    theme.on_primary_container,
                    Stroke::new(1.2_f32, theme.primary),
                )
            } else {
                (
                    theme.surface_container,
                    theme.on_surface_variant,
                    Stroke::new(1.0_f32, theme.outline_variant),
                )
            };

            let rounding = if idx == 0 {
                Rounding { nw: item_h / 2.0, sw: item_h / 2.0, ne: 0.0, se: 0.0 }
            } else if idx == n - 1 {
                Rounding { nw: 0.0, sw: 0.0, ne: item_h / 2.0, se: item_h / 2.0 }
            } else {
                Rounding::ZERO
            };

            let (rect, resp) = ui.allocate_exact_size(Vec2::new(item_w, item_h), egui::Sense::click());
            if ui.is_rect_visible(rect) {
                let hover_adjust = if resp.hovered() {
                    theme.surface_container_high
                } else {
                    bg
                };
                ui.painter().rect(rect, rounding, hover_adjust, stroke);

                let display_text = if item_w < 55.0 {
                    icon.to_string()
                } else {
                    format!("{} {}", icon, label)
                };
                let font_id = FontId::new(metrics.font_btn_size * 0.9, FontFamily::Proportional);
                let galley = ui.painter().layout_no_wrap(display_text, font_id, fg);
                let text_pos = rect.center() - galley.size() / 2.0_f32;
                ui.painter().galley(text_pos, galley, fg);
            }

            if resp.clicked() && !is_selected {
                *current = *val;
                changed = true;
            }
        }
    });

    changed
}

/// 9. Material 3 Icon-Only Segmented Button (Compact pour barre d'outils, avec infobulles riches)
pub fn m3_icon_segmented_button<T: PartialEq + Copy>(
    ui: &mut Ui,
    theme: &DynamicM3Theme,
    options: &[(T, &str, &str)],
    current: &mut T,
) -> bool {
    let metrics = AdaptiveUiMetrics::compute(ui.ctx(), cfg!(target_os = "android"));
    let mut changed = false;
    let n = options.len();
    if n == 0 {
        return false;
    }

    let avail = ui.available_width();
    let max_w = metrics.segmented_item_w;
    let item_w = ((avail - 4.0) / n as f32).min(max_w).max(28.0_f32);
    let item_h = metrics.segmented_item_h;

    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing = Vec2::ZERO;

        for (idx, (val, icon, tooltip)) in options.iter().enumerate() {
            let is_selected = *current == *val;
            let (bg, fg, stroke) = if is_selected {
                (
                    theme.primary_container,
                    theme.on_primary_container,
                    Stroke::new(1.2_f32, theme.primary),
                )
            } else {
                (
                    theme.surface_container,
                    theme.on_surface_variant,
                    Stroke::new(1.0_f32, theme.outline_variant),
                )
            };

            let rounding = if idx == 0 {
                Rounding { nw: item_h / 2.0, sw: item_h / 2.0, ne: 0.0, se: 0.0 }
            } else if idx == n - 1 {
                Rounding { nw: 0.0, sw: 0.0, ne: item_h / 2.0, se: item_h / 2.0 }
            } else {
                Rounding::ZERO
            };

            let (rect, resp) = ui.allocate_exact_size(Vec2::new(item_w, item_h), egui::Sense::click());
            let resp = resp.on_hover_text(*tooltip);

            if ui.is_rect_visible(rect) {
                let hover_adjust = if resp.hovered() {
                    theme.surface_container_high
                } else {
                    bg
                };
                ui.painter().rect(rect, rounding, hover_adjust, stroke);

                let font_id = FontId::new(metrics.icon_font_size, FontFamily::Proportional);
                let galley = ui.painter().layout_no_wrap(icon.to_string(), font_id, fg);
                let text_pos = rect.center() - galley.size() / 2.0_f32;
                ui.painter().galley(text_pos, galley, fg);
            }

            if resp.clicked() && !is_selected {
                *current = *val;
                changed = true;
            }
        }
    });

    changed
}

/// 10. Material 3 Status Badge (Taille exacte allouée, zéro débordement, infobulle détaillée)
pub fn m3_status_badge(
    ui: &mut Ui,
    theme: &DynamicM3Theme,
    text: &str,
    is_hdr: bool,
    tooltip: &str,
) -> egui::Response {
    let metrics = AdaptiveUiMetrics::compute(ui.ctx(), cfg!(target_os = "android"));
    let padding = Vec2::new(12.0_f32 * metrics.scale, 5.0_f32 * metrics.scale);
    let rounding = Rounding::same(14.0_f32);
    let font_id = FontId::new(metrics.pill_font_size.max(12.0), FontFamily::Proportional);

    let (bg, fg, stroke) = if is_hdr {
        (
            Color32::from_rgb(14, 45, 26),
            Color32::from_rgb(220, 252, 231),
            Stroke::new(1.0_f32, Color32::from_rgb(34, 197, 94)),
        )
    } else {
        (
            Color32::from_rgb(20, 32, 52),
            Color32::from_rgb(224, 242, 254),
            Stroke::new(1.0_f32, theme.primary),
        )
    };

    let galley = ui.painter().layout_no_wrap(text.to_string(), font_id, fg);
    let desired_size = galley.size() + padding * 2.0_f32;

    let (rect, resp) = ui.allocate_exact_size(desired_size, egui::Sense::hover());
    if ui.is_rect_visible(rect) {
        ui.painter().rect(rect, rounding, bg, stroke);
        let text_pos = rect.center() - galley.size() / 2.0_f32;
        ui.painter().galley(text_pos, galley, fg);
    }

    if !tooltip.is_empty() {
        resp.on_hover_text(tooltip)
    } else {
        resp
    }
}

/// 11. Material 3 Stepper pour le zoom de l'interface (+ / - et bouton 100%)
/// Remplace avantageusement les curseurs à glissière en éliminant tout artefact
/// de redimensionnement de coordonnées sous le pointeur de souris.
pub fn m3_zoom_stepper(ui: &mut Ui, theme: &DynamicM3Theme, scale: &mut f32) -> bool {
    let mut changed = false;
    let rounding = Rounding::same(13.0_f32);
    let border = Stroke::new(1.0_f32, theme.outline_variant);
    let btn_size = Vec2::new(26.0_f32, 26.0_f32);

    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(3.0_f32, 0.0_f32);

        ui.label(egui::RichText::new("🔍").size(13.0));

        // Bouton Moins (-)
        let minus_btn = egui::Button::new(
            egui::RichText::new("➖")
                .size(11.0)
                .color(theme.on_surface)
        )
        .fill(theme.surface_container_high)
        .stroke(border)
        .rounding(rounding)
        .min_size(btn_size);

        if ui.add(minus_btn)
            .on_hover_text("Réduire la taille de l'interface (-10%)")
            .clicked()
        {
            let next = (*scale - 0.10).max(0.70);
            *scale = (next * 100.0).round() / 100.0;
            changed = true;
        }

        // Pilule centrale cliquable affichant le pourcentage actuel (clic = reset 100%)
        let scale_pct = format!("{:.0}%", *scale * 100.0);
        let pill_btn = egui::Button::new(
            egui::RichText::new(scale_pct)
                .strong()
                .size(11.5)
                .color(theme.on_primary_container)
        )
        .fill(theme.primary_container)
        .stroke(Stroke::NONE)
        .rounding(rounding)
        .min_size(Vec2::new(44.0_f32, 26.0_f32));

        if ui.add(pill_btn)
            .on_hover_text("Cliquer pour réinitialiser la taille de l'interface à 100%")
            .clicked()
        {
            if (*scale - 1.0).abs() > 0.01 {
                *scale = 1.0;
                changed = true;
            }
        }

        // Bouton Plus (+)
        let plus_btn = egui::Button::new(
            egui::RichText::new("➕")
                .size(11.0)
                .color(theme.on_surface)
        )
        .fill(theme.surface_container_high)
        .stroke(border)
        .rounding(rounding)
        .min_size(btn_size);

        if ui.add(plus_btn)
            .on_hover_text("Agrandir la taille de l'interface (+10%)")
            .clicked()
        {
            let next = (*scale + 0.10).min(1.50);
            *scale = (next * 100.0).round() / 100.0;
            changed = true;
        }
    });

    changed
}
