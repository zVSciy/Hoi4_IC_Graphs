// No console window in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    hoi4_analyzer::logging::init();
    log::info!("HOI4 IC Analyzer {} starting", env!("CARGO_PKG_VERSION"));

    // With `panic = "abort"` and no console, make crashes visible to the user.
    std::panic::set_hook(Box::new(|info| {
        log::error!("Fatal error: {info}");
        rfd::MessageDialog::new()
            .set_level(rfd::MessageLevel::Error)
            .set_title("HOI4 IC Analyzer crashed")
            .set_description(info.to_string())
            .show();
    }));

    // Optional: data files to open right away (also what "Open with…" passes).
    let files = std::env::args_os()
        .skip(1)
        .map(std::path::PathBuf::from)
        .collect();
    if let Err(err) = hoi4_analyzer::ui::run(files) {
        log::error!("Fatal error during startup: {err}");
        rfd::MessageDialog::new()
            .set_level(rfd::MessageLevel::Error)
            .set_title("HOI4 IC Analyzer")
            .set_description(format!("Failed to start: {err}"))
            .show();
        std::process::exit(1);
    }
}
