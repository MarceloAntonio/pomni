//! Translucent monochrome surfaces matching a minimal Hyprland glass desktop.
use super::{ACCENT, MUTED, PANEL};
use eframe::egui::{self, Color32, Stroke, Vec2};

pub fn panel_frame() -> egui::Frame {
    egui::Frame::new()
        .fill(PANEL)
        .stroke(Stroke::new(1.0, Color32::from_white_alpha(22)))
        .corner_radius(14)
}

pub fn apply(ctx: &egui::Context) {
    ctx.set_theme(egui::Theme::Dark);
    ctx.style_mut_of(egui::Theme::Dark, |s| {
        s.spacing.item_spacing = Vec2::new(10.0, 10.0);
        s.spacing.button_padding = Vec2::new(14.0, 8.0);
        s.visuals.override_text_color = Some(Color32::from_gray(235));
        s.visuals.panel_fill = Color32::TRANSPARENT;
        // Floating menus need a stronger tint for contrast over other widgets.
        s.visuals.window_fill = Color32::from_rgba_unmultiplied(36, 38, 42, 240);
        s.visuals.extreme_bg_color = Color32::from_white_alpha(9);
        s.visuals.text_edit_bg_color = Some(Color32::from_white_alpha(12));
        s.visuals.window_stroke = Stroke::new(1.0, Color32::from_white_alpha(32));
        s.visuals.window_corner_radius = 12.into();
        s.visuals.selection.bg_fill = ACCENT;
        s.visuals.selection.stroke = Stroke::new(1.0, Color32::WHITE);
        s.visuals.hyperlink_color = Color32::from_gray(220);
        s.visuals.faint_bg_color = Color32::from_white_alpha(8);
        s.visuals.window_shadow = egui::epaint::Shadow {
            offset: [0, 6],
            blur: 22,
            spread: 0,
            color: Color32::from_black_alpha(55),
        };
        for (widget, opacity) in [
            (&mut s.visuals.widgets.noninteractive, 8),
            (&mut s.visuals.widgets.inactive, 14),
            (&mut s.visuals.widgets.hovered, 30),
            (&mut s.visuals.widgets.active, 42),
            (&mut s.visuals.widgets.open, 32),
        ] {
            widget.bg_fill = Color32::from_white_alpha(opacity);
            widget.weak_bg_fill = widget.bg_fill;
            widget.bg_stroke = Stroke::new(1.0, Color32::from_white_alpha(opacity + 12));
            widget.fg_stroke = Stroke::new(1.0, Color32::from_gray(230));
            widget.corner_radius = 8.into();
            widget.expansion = 0.0;
        }
        s.visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, MUTED);
    });
}

/// Use the user's installed desktop font without bundling proprietary font files.
pub fn load_desktop_font(ctx: &egui::Context) {
    let Ok(output) = std::process::Command::new("fc-match")
        .args(["-f", "%{file}", "SF Pro Display, Inter, sans-serif"])
        .output()
    else {
        return;
    };
    if !output.status.success() {
        return;
    }
    let path = String::from_utf8_lossy(&output.stdout);
    let Ok(bytes) = std::fs::read(path.trim()) else {
        return;
    };
    let mut fonts = egui::FontDefinitions::default();
    fonts
        .font_data
        .insert("desktop".into(), egui::FontData::from_owned(bytes).into());
    fonts
        .families
        .entry(egui::FontFamily::Proportional)
        .or_default()
        .insert(0, "desktop".into());
    ctx.set_fonts(fonts);
}
