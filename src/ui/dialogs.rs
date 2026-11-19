//! Native message boxes and file pickers.

use std::path::PathBuf;

use rfd::{FileDialog, MessageDialog, MessageLevel};

pub fn error(message: &str) {
    log::error!("{message}");
    MessageDialog::new()
        .set_level(MessageLevel::Error)
        .set_title("Error")
        .set_description(message)
        .show();
}

pub fn info(message: &str) {
    log::info!("{message}");
    MessageDialog::new()
        .set_level(MessageLevel::Info)
        .set_title("Info")
        .set_description(message)
        .show();
}

/// Opens in the HOI4 folder (where DataCollection writes `output.txt`) when found.
pub fn open_data_file(title: &str) -> Option<PathBuf> {
    let mut dialog = FileDialog::new();
    if let Some(dir) = crate::game::install_dir() {
        dialog = dialog.set_directory(dir);
    }
    dialog
        .set_title(title)
        .add_filter("Text files", &["txt"])
        .add_filter("Log files", &["log"])
        .add_filter("All files", &["*"])
        .pick_file()
}

pub fn save_file(filter_name: &str, extension: &str, default_stem: &str) -> Option<PathBuf> {
    let path = FileDialog::new()
        .set_title("Save Graph")
        .add_filter(filter_name, &[extension])
        .set_file_name(format!("{default_stem}.{extension}"))
        .save_file()?;
    // Ensure the extension even if the user typed a bare name.
    Some(if path.extension().is_some() {
        path
    } else {
        path.with_extension(extension)
    })
}
