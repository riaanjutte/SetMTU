//! Colours, fonts and widget styling: neutral light and dark palettes with a
//! single restrained accent, tight corners and no decoration.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use eframe::egui::{
    self, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, RichText, Stroke,
};

pub struct Palette {
    pub bg: Color32,
    pub panel: Color32,
    pub bar: Color32,
    pub border: Color32,
    pub accent: Color32,
    pub apply: Color32,
    pub on_apply: Color32,
    pub text: Color32,
    pub muted: Color32,
    pub faint: Color32,
    pub good: Color32,
    pub warn: Color32,
    pub bad: Color32,
    // Button backgrounds: idle, hovered, pressed.
    widget: Color32,
    widget_hover: Color32,
    widget_active: Color32,
    widget_hover_edge: Color32,
}

const LIGHT: Palette = Palette {
    bg: Color32::from_rgb(0xf4, 0xf5, 0xf7),
    panel: Color32::from_rgb(0xff, 0xff, 0xff),
    bar: Color32::from_rgb(0xea, 0xec, 0xef),
    border: Color32::from_rgb(0xd3, 0xd7, 0xdc),
    accent: Color32::from_rgb(0x1c, 0x8a, 0x94),
    apply: Color32::from_rgb(0x23, 0x87, 0x4f),
    on_apply: Color32::WHITE,
    text: Color32::from_rgb(0x1c, 0x20, 0x24),
    muted: Color32::from_rgb(0x58, 0x60, 0x6a),
    faint: Color32::from_rgb(0x8a, 0x91, 0x9a),
    good: Color32::from_rgb(0x1e, 0x86, 0x4a),
    warn: Color32::from_rgb(0xa8, 0x6c, 0x0c),
    bad: Color32::from_rgb(0xc2, 0x3e, 0x37),
    widget: Color32::from_rgb(0xe8, 0xeb, 0xee),
    widget_hover: Color32::from_rgb(0xde, 0xe2, 0xe6),
    widget_active: Color32::from_rgb(0xd2, 0xd7, 0xdc),
    widget_hover_edge: Color32::from_rgb(0xb4, 0xbb, 0xc3),
};

const DARK: Palette = Palette {
    bg: Color32::from_rgb(0x15, 0x17, 0x1a),
    panel: Color32::from_rgb(0x1b, 0x1e, 0x22),
    bar: Color32::from_rgb(0x11, 0x13, 0x15),
    border: Color32::from_rgb(0x2b, 0x2f, 0x35),
    accent: Color32::from_rgb(0x38, 0xb2, 0xbc),
    apply: Color32::from_rgb(0x3f, 0xb3, 0x6f),
    on_apply: Color32::from_rgb(0x05, 0x1c, 0x0e),
    text: Color32::from_rgb(0xd8, 0xdb, 0xdf),
    muted: Color32::from_rgb(0x86, 0x8d, 0x96),
    faint: Color32::from_rgb(0x5c, 0x62, 0x6a),
    good: Color32::from_rgb(0x4c, 0xb7, 0x82),
    warn: Color32::from_rgb(0xd9, 0xa4, 0x41),
    bad: Color32::from_rgb(0xe0, 0x62, 0x5a),
    widget: Color32::from_rgb(0x24, 0x28, 0x2d),
    widget_hover: Color32::from_rgb(0x2c, 0x31, 0x37),
    widget_active: Color32::from_rgb(0x33, 0x39, 0x40),
    widget_hover_edge: Color32::from_rgb(0x44, 0x4a, 0x52),
};

static IS_DARK: AtomicBool = AtomicBool::new(false);

/// The palette for the current mode.
pub fn p() -> &'static Palette {
    if IS_DARK.load(Ordering::Relaxed) { &DARK } else { &LIGHT }
}

pub fn is_dark() -> bool {
    IS_DARK.load(Ordering::Relaxed)
}

pub const RADIUS: u8 = 4;

// Inter, the same typeface PRISM uses. egui picks fonts by family rather than
// weight, so each weight is registered as its own family.
const INTER: &str = "inter";
const INTER_SEMIBOLD: &str = "inter-semibold";

pub fn semibold(text: impl Into<String>, size: f32) -> RichText {
    RichText::new(text).font(FontId::new(size, FontFamily::Name(INTER_SEMIBOLD.into())))
}

pub fn mono(text: impl Into<String>, size: f32) -> RichText {
    RichText::new(text).font(FontId::monospace(size))
}

/// Small uppercase section heading.
pub fn section(text: &str) -> RichText {
    semibold(text, 11.0).color(p().muted).extra_letter_spacing(1.0)
}

pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    for (name, bytes) in [
        (INTER, &include_bytes!("../assets/Inter-Regular.ttf")[..]),
        (INTER_SEMIBOLD, &include_bytes!("../assets/Inter-SemiBold.ttf")[..]),
    ] {
        fonts.font_data.insert(name.into(), Arc::new(FontData::from_static(bytes)));
    }
    // egui's built-in fonts stay as fallbacks for symbols Inter lacks.
    let fallbacks = fonts.families[&FontFamily::Proportional].clone();
    fonts
        .families
        .get_mut(&FontFamily::Proportional)
        .unwrap()
        .insert(0, INTER.into());
    let mut family = vec![INTER_SEMIBOLD.to_owned()];
    family.extend(fallbacks);
    fonts.families.insert(FontFamily::Name(INTER_SEMIBOLD.into()), family);
    ctx.set_fonts(fonts);
}

/// Switches between the light and dark palettes.
pub fn set_mode(ctx: &egui::Context, dark: bool) {
    IS_DARK.store(dark, Ordering::Relaxed);
    let c = p();

    let mut v = if dark { egui::Visuals::dark() } else { egui::Visuals::light() };
    v.panel_fill = c.bg;
    v.window_fill = c.panel;
    v.window_stroke = Stroke::new(1.0, c.border);
    v.extreme_bg_color = c.bar;
    v.faint_bg_color = c.bar;
    v.selection.bg_fill = c.accent.gamma_multiply(if dark { 0.45 } else { 0.25 });
    v.selection.stroke = Stroke::new(1.0, c.text);
    v.hyperlink_color = c.accent;
    v.window_shadow = egui::Shadow::NONE;
    v.popup_shadow = egui::Shadow::NONE;
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, c.text);
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, c.border);
    for w in [
        &mut v.widgets.inactive,
        &mut v.widgets.hovered,
        &mut v.widgets.active,
        &mut v.widgets.open,
    ] {
        w.corner_radius = CornerRadius::same(RADIUS);
        w.expansion = 0.0;
        w.fg_stroke = Stroke::new(1.0, c.text);
    }
    v.widgets.inactive.weak_bg_fill = c.widget;
    v.widgets.inactive.bg_fill = c.widget;
    v.widgets.inactive.bg_stroke = Stroke::new(1.0, c.border);
    v.widgets.hovered.weak_bg_fill = c.widget_hover;
    v.widgets.hovered.bg_fill = c.widget_hover;
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, c.widget_hover_edge);
    v.widgets.active.weak_bg_fill = c.widget_active;
    v.widgets.active.bg_fill = c.widget_active;
    v.widgets.active.bg_stroke = Stroke::new(1.0, c.accent);
    v.widgets.open.weak_bg_fill = c.widget_hover;
    v.widgets.open.bg_stroke = Stroke::new(1.0, c.widget_hover_edge);
    v.disabled_alpha = 0.4;

    // Same look whatever mode Windows itself is in.
    ctx.all_styles_mut(|s| {
        s.visuals = v.clone();
        s.spacing.item_spacing = egui::vec2(8.0, 6.0);
        s.spacing.button_padding = egui::vec2(12.0, 5.0);
        s.spacing.interact_size.y = 26.0;
    });
}
