//! Per-user folders.

use std::{env, path::PathBuf};

/// The app's data folder (settings, game library, added DLLs):
/// `%LOCALAPPDATA%\HOI4_Analyzer`, or the temp folder if that is not set.
pub fn data_dir() -> PathBuf {
    env::var_os("LOCALAPPDATA")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(env::temp_dir)
        .join("HOI4_Analyzer")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_dir_ends_with_app_folder() {
        assert!(data_dir().ends_with("HOI4_Analyzer"));
    }
}
