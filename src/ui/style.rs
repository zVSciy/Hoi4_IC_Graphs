//! Dark theme matching the original application's palette.

use std::sync::Arc;

use eframe::egui::{self, Color32, CornerRadius, Stroke, Theme, ThemePreference, Visuals};

use crate::theme::{self, Rgb};

pub fn color(rgb: Rgb) -> Color32 {
    Color32::from_rgb(rgb[0], rgb[1], rgb[2])
}

pub fn series_color(index: usize) -> Color32 {
    color(theme::series_color(index))
}

pub fn apply(ctx: &egui::Context) {
    let bg = color(theme::BACKGROUND);
    let fg = color(theme::FOREGROUND);
    let widget = color(theme::WIDGET_BG);
    let button = color(theme::BUTTON_BG);
    let hover = color(theme::HOVER_BG);
    let accent = color(theme::ACCENT);
    let border = color(theme::BORDER);

    let mut v = Visuals::dark();
    v.override_text_color = Some(fg);
    v.panel_fill = bg;
    v.window_fill = bg;
    v.window_stroke = Stroke::new(1.0, border);
    v.faint_bg_color = Color32::from_rgb(0x2d, 0x2d, 0x30);
    v.extreme_bg_color = widget;
    v.text_edit_bg_color = Some(widget);
    v.code_bg_color = widget;
    v.selection.bg_fill = accent;
    v.selection.stroke = Stroke::new(1.0, Color32::WHITE);
    v.hyperlink_color = accent;

    let w = &mut v.widgets;
    w.noninteractive.bg_fill = bg;
    w.noninteractive.weak_bg_fill = bg;
    w.noninteractive.bg_stroke = Stroke::new(1.0, border);
    w.noninteractive.fg_stroke = Stroke::new(1.0, fg);
    for (state, fill) in [
        (&mut w.inactive, button),
        (&mut w.hovered, hover),
        (&mut w.active, accent),
        (&mut w.open, widget),
    ] {
        state.bg_fill = fill;
        state.weak_bg_fill = fill;
        state.bg_stroke = Stroke::NONE;
        state.corner_radius = CornerRadius::same(2);
        state.fg_stroke = Stroke::new(1.0, fg);
    }
    w.hovered.bg_stroke = Stroke::new(1.0, border);
    w.active.fg_stroke = Stroke::new(1.0, Color32::WHITE);

    ctx.set_theme(ThemePreference::Dark);
    ctx.set_visuals_of(Theme::Dark, v);
    ctx.all_styles_mut(|style| {
        style.spacing.button_padding = egui::vec2(8.0, 5.0);
        style.spacing.item_spacing = egui::vec2(8.0, 6.0);
    });
}

/// Application icon decoded from the embedded `.ico`.
pub fn load_icon() -> Option<Arc<egui::IconData>> {
    let bytes = include_bytes!("../../assets/cover.thumb256.ico");
    match image::load_from_memory_with_format(bytes, image::ImageFormat::Ico) {
        Ok(img) => {
            let rgba = img.into_rgba8();
            let (width, height) = rgba.dimensions();
            Some(Arc::new(egui::IconData {
                rgba: rgba.into_raw(),
                width,
                height,
            }))
        }
        Err(err) => {
            log::warn!("Failed to decode application icon: {err}");
            None
        }
    }
}
