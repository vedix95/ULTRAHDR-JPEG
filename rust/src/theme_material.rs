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

/// Applies Material Design 3 Dark Theme, typography, and adaptive touch targets to egui
pub fn apply_material_3_theme(ctx: &Context, is_android: bool) -> ScreenFormFactor {
    let screen_rect = ctx.screen_rect();
    let screen_size = screen_rect.size();
    let form_factor = ScreenFormFactor::detect(screen_size);

    if is_android {
        if ctx.zoom_factor() < 1.35 {
            ctx.set_zoom_factor(1.35);
        }
    }

    let mut style = (*ctx.style()).clone();

    // 1. Typography Hierarchy with readable mobile sizes
    let font_scale = if is_android {
        1.25_f32
    } else if form_factor == ScreenFormFactor::SmartphonePortrait {
        1.10_f32
    } else {
        1.0_f32
    };

    style.text_styles = [
        (
            TextStyle::Heading,
            FontId::new(22.0_f32 * font_scale, FontFamily::Proportional),
        ),
        (
            TextStyle::Name("Title".into()),
            FontId::new(18.0_f32 * font_scale, FontFamily::Proportional),
        ),
        (
            TextStyle::Body,
            FontId::new(15.5_f32 * font_scale, FontFamily::Proportional),
        ),
        (
            TextStyle::Button,
            FontId::new(15.0_f32 * font_scale, FontFamily::Proportional),
        ),
        (
            TextStyle::Small,
            FontId::new(13.0_f32 * font_scale, FontFamily::Proportional),
        ),
        (
            TextStyle::Monospace,
            FontId::new(13.0_f32 * font_scale, FontFamily::Monospace),
        ),
    ]
    .into();

    // 2. Material 3 Touch Targets & Spacing (>= 48dp touch heights)
    let touch_padding_y = if is_android || form_factor == ScreenFormFactor::SmartphonePortrait {
        12.0_f32
    } else {
        7.0_f32
    };

    style.spacing.button_padding = Vec2::new(if is_android { 18.0 } else { 14.0 }, touch_padding_y);
    style.spacing.item_spacing = Vec2::new(10.0_f32, 10.0_f32);
    style.spacing.slider_width = if is_android { 200.0 } else { 160.0 };
    // Enhanced touch slider thumb (>= 48dp)
    style.spacing.interact_size = Vec2::new(
        48.0_f32,
        if is_android { 48.0_f32 } else { 34.0_f32 },
    );
    style.spacing.scroll.bar_width = if is_android { 10.0 } else { 8.0 };

    // 3. Material Design 3 Visuals & Dark Color Palette
    let mut visuals = Visuals::dark();
    visuals.dark_mode = true;
    visuals.override_text_color = Some(colors::ON_SURFACE);

    // Surfaces
    visuals.panel_fill = colors::SURFACE_0;
    visuals.window_fill = colors::SURFACE_CONTAINER;
    visuals.faint_bg_color = colors::SURFACE_CONTAINER;
    visuals.extreme_bg_color = colors::SURFACE_0;

    // Window & Card Rounding (Material Card = 16dp)
    visuals.window_rounding = Rounding::same(16.0_f32);
    visuals.window_stroke = Stroke::new(1.0_f32, colors::OUTLINE_VARIANT);
    visuals.window_shadow = Shadow {
        offset: Vec2::new(0.0_f32, 4.0_f32),
        blur: 12.0_f32,
        spread: 0.0_f32,
        color: Color32::from_black_alpha(80),
    };

    // Button & Chip Pill Rounding (Material Pill = 24dp)
    let pill_rounding = Rounding::same(24.0_f32);

    // Inactive Widgets
    visuals.widgets.inactive.bg_fill = colors::SURFACE_CONTAINER;
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, colors::OUTLINE_VARIANT);
    visuals.widgets.inactive.rounding = pill_rounding;
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, colors::ON_SURFACE);

    // Hovered Widgets
    visuals.widgets.hovered.bg_fill = colors::SURFACE_CONTAINER_HIGH;
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.2_f32, colors::PRIMARY);
    visuals.widgets.hovered.rounding = pill_rounding;
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.2_f32, Color32::WHITE);

    // Active / Pressed Widgets
    visuals.widgets.active.bg_fill = colors::PRIMARY_CONTAINER;
    visuals.widgets.active.bg_stroke = Stroke::new(1.5_f32, colors::PRIMARY);
    visuals.widgets.active.rounding = pill_rounding;
    visuals.widgets.active.fg_stroke = Stroke::new(1.5_f32, colors::ON_PRIMARY_CONTAINER);

    // Open / Selected Widgets
    visuals.widgets.open.bg_fill = colors::PRIMARY_CONTAINER;
    visuals.widgets.open.bg_stroke = Stroke::new(1.0_f32, colors::PRIMARY);
    visuals.widgets.open.rounding = pill_rounding;
    visuals.widgets.open.fg_stroke = Stroke::new(1.0_f32, colors::ON_PRIMARY_CONTAINER);

    // Selection highlight
    visuals.selection.bg_fill = colors::PRIMARY_CONTAINER;
    visuals.selection.stroke = Stroke::new(1.5_f32, colors::PRIMARY);

    style.visuals = visuals;
    ctx.set_style(style);

    form_factor
}

/// Helper to render a Material 3 Pill Filter Chip (used for tab selection)
pub fn material_chip(ui: &mut Ui, selected: bool, label: &str) -> Response {
    let (bg_fill, fg_color, stroke) = if selected {
        (
            colors::PRIMARY_CONTAINER,
            colors::ON_PRIMARY_CONTAINER,
            Stroke::new(1.5_f32, colors::PRIMARY),
        )
    } else {
        (
            colors::SURFACE_CONTAINER,
            colors::ON_SURFACE_VARIANT,
            Stroke::new(1.0_f32, colors::OUTLINE_VARIANT),
        )
    };

    let padding = Vec2::new(14.0_f32, 7.0_f32);
    let rounding = Rounding::same(20.0_f32);

    let font_id = FontId::new(13.5_f32, FontFamily::Proportional);
    let galley = ui.painter().layout_no_wrap(label.to_string(), font_id, fg_color);
    let text_size = galley.size();
    let desired_size = text_size + padding * 2.0_f32;

    let (rect, response) = ui.allocate_exact_size(
        desired_size.max(Vec2::new(44.0_f32, 36.0_f32)),
        egui::Sense::click(),
    );

    if ui.is_rect_visible(rect) {
        let hover_adjust = if response.hovered() {
            colors::SURFACE_CONTAINER_HIGH
        } else {
            bg_fill
        };

        ui.painter().rect(rect, rounding, hover_adjust, stroke);
        let text_pos = rect.center() - text_size / 2.0_f32;
        ui.painter().galley(text_pos, galley, fg_color);
    }

    response
}

/// Helper to render a Material 3 Card Container
pub fn material_card<R>(
    ui: &mut Ui,
    title: Option<&str>,
    add_contents: impl FnOnce(&mut Ui) -> R,
) -> R {
    let frame = egui::Frame::none()
        .fill(colors::SURFACE_CONTAINER)
        .stroke(Stroke::new(1.0_f32, colors::OUTLINE_VARIANT))
        .rounding(Rounding::same(16.0_f32))
        .inner_margin(Margin::same(12.0_f32));

    frame
        .show(ui, |ui| {
            if let Some(t) = title {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(t)
                            .font(FontId::new(14.5_f32, FontFamily::Proportional))
                            .color(colors::PRIMARY)
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
pub fn material_button(ui: &mut Ui, label: &str, primary: bool) -> Response {
    let (bg, fg, stroke) = if primary {
        (
            colors::PRIMARY,
            colors::ON_PRIMARY,
            Stroke::new(1.0_f32, colors::PRIMARY),
        )
    } else {
        (
            colors::SURFACE_CONTAINER_HIGH,
            colors::ON_SURFACE,
            Stroke::new(1.0_f32, colors::OUTLINE_VARIANT),
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
