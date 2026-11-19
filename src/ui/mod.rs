//! egui front end.

mod app;
mod chart;
mod chart_panel;
mod dialogs;
mod graph_window;
mod library_window;
mod live_dashboard;
mod rankings_window;
mod settings_window;
mod style;

use eframe::egui;

/// Starts the GUI. `initial_files` are preloaded as Data File 1 / 2.
pub fn run(initial_files: Vec<std::path::PathBuf>) -> eframe::Result {
    let icon = style::load_icon();
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("HOI4 IC Graphs")
        .with_inner_size([950.0, 850.0])
        .with_min_inner_size([720.0, 560.0]);
    if let Some(icon) = &icon {
        viewport = viewport.with_icon(icon.clone());
    }
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    eframe::run_native(
        "HOI4 IC Analyzer",
        options,
        Box::new(|cc| {
            style::apply(&cc.egui_ctx);
            Ok(Box::new(app::App::new(&cc.egui_ctx, icon, initial_files)))
        }),
    )
}
