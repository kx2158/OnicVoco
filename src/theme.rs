//! Monochrome light and dark themes, plus native Windows fonts.

use eframe::egui::{self, Color32, FontData, FontDefinitions, FontFamily, Rounding, Stroke};

#[derive(Clone, Copy)]
pub struct Pal {
    pub dark: bool,
    pub bg: Color32,
    pub card: Color32,
    pub card2: Color32,
    pub stroke: Color32,
    pub text: Color32,
    pub muted: Color32,
    pub faint: Color32,
    /// Text drawn on top of a `text`-coloured fill (inverted).
    pub on_text: Color32,
}

pub const LIGHT: Pal = Pal {
    dark: false,
    bg: Color32::from_rgb(0xF3, 0xF3, 0xF2),
    card: Color32::from_rgb(0xFF, 0xFF, 0xFF),
    card2: Color32::from_rgb(0xEC, 0xEC, 0xEB),
    stroke: Color32::from_rgb(0xDD, 0xDD, 0xDB),
    text: Color32::from_rgb(0x11, 0x11, 0x11),
    muted: Color32::from_rgb(0x6B, 0x6B, 0x6B),
    faint: Color32::from_rgb(0xB8, 0xB8, 0xB6),
    on_text: Color32::from_rgb(0xFA, 0xFA, 0xFA),
};

pub const DARK: Pal = Pal {
    dark: true,
    bg: Color32::from_rgb(0x0D, 0x0D, 0x0E),
    card: Color32::from_rgb(0x17, 0x17, 0x19),
    card2: Color32::from_rgb(0x21, 0x21, 0x24),
    stroke: Color32::from_rgb(0x2B, 0x2B, 0x2F),
    text: Color32::from_rgb(0xF2, 0xF2, 0xF2),
    muted: Color32::from_rgb(0x9A, 0x9A, 0x9C),
    faint: Color32::from_rgb(0x4C, 0x4C, 0x50),
    on_text: Color32::from_rgb(0x0D, 0x0D, 0x0E),
};

pub fn resolve(pref: &str) -> Pal {
    match pref {
        "light" => LIGHT,
        "dark" => DARK,
        _ => match dark_light::detect() {
            dark_light::Mode::Light => LIGHT,
            _ => DARK,
        },
    }
}

pub fn visuals(p: &Pal) -> egui::Visuals {
    let mut v = if p.dark { egui::Visuals::dark() } else { egui::Visuals::light() };
    v.override_text_color = Some(p.text);
    v.panel_fill = p.bg;
    v.window_fill = p.card;
    v.window_stroke = Stroke::new(1.0_f32, p.stroke);
    v.extreme_bg_color = p.card2;
    v.faint_bg_color = p.card2;
    v.code_bg_color = p.card2;
    v.hyperlink_color = p.text;
    v.window_rounding = Rounding::same(14.0);
    v.menu_rounding = Rounding::same(10.0);
    v.selection.bg_fill = p.faint;
    v.selection.stroke = Stroke::new(1.0_f32, p.text);
    v.slider_trailing_fill = true;

    let r = Rounding::same(10.0);
    let w = &mut v.widgets;
    w.noninteractive.bg_fill = p.card;
    w.noninteractive.weak_bg_fill = p.card;
    w.noninteractive.bg_stroke = Stroke::new(1.0_f32, p.stroke);
    w.noninteractive.fg_stroke = Stroke::new(1.0_f32, p.text);
    w.noninteractive.rounding = r;
    for (ws, fill) in [(&mut w.inactive, p.card2), (&mut w.hovered, p.stroke), (&mut w.active, p.faint), (&mut w.open, p.card2)] {
        ws.bg_fill = fill;
        ws.weak_bg_fill = fill;
        ws.bg_stroke = Stroke::new(1.0_f32, p.stroke);
        ws.fg_stroke = Stroke::new(1.5_f32, p.text);
        ws.rounding = r;
        ws.expansion = 0.0;
    }
    v
}

pub fn install(ctx: &egui::Context, p: &Pal) {
    ctx.set_visuals(visuals(p));
    ctx.style_mut(|s| {
        s.spacing.item_spacing = egui::vec2(10.0, 10.0);
        s.spacing.button_padding = egui::vec2(14.0, 7.0);
        s.spacing.interact_size.y = 32.0;
        s.text_styles.insert(egui::TextStyle::Body, egui::FontId::proportional(15.0));
        s.text_styles.insert(egui::TextStyle::Button, egui::FontId::proportional(15.0));
        s.text_styles.insert(egui::TextStyle::Small, egui::FontId::proportional(12.5));
        s.text_styles.insert(egui::TextStyle::Heading, egui::FontId::new(22.0, bold()));
    });
}

pub fn bold() -> FontFamily {
    FontFamily::Name("bold".into())
}

/// Native Windows fonts: Segoe UI for body text, and Bahnschrift (a crisp,
/// DIN-style face that ships with Windows 10/11) for headings and numbers.
pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let load = |paths: &[&str]| paths.iter().find_map(|p| std::fs::read(p).ok());

    if let Some(bytes) = load(&["C:\\Windows\\Fonts\\segoeui.ttf"]) {
        fonts.font_data.insert("segoe".into(), FontData::from_owned(bytes));
        fonts.families.get_mut(&FontFamily::Proportional).unwrap().insert(0, "segoe".into());
    }
    let mut bold_family = Vec::new();
    if let Some(bytes) = load(&["C:\\Windows\\Fonts\\bahnschrift.ttf"]) {
        fonts.font_data.insert("bahn".into(), FontData::from_owned(bytes));
        bold_family.push("bahn".to_string());
    }
    if let Some(bytes) = load(&["C:\\Windows\\Fonts\\seguisb.ttf", "C:\\Windows\\Fonts\\segoeuib.ttf"]) {
        fonts.font_data.insert("segoe-bold".into(), FontData::from_owned(bytes));
        bold_family.push("segoe-bold".to_string());
    }
    // Fall back to the regular stack so the family always resolves.
    bold_family.extend(fonts.families[&FontFamily::Proportional].iter().cloned());
    fonts.families.insert(bold(), bold_family);
    ctx.set_fonts(fonts);
}
