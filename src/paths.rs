//! Per-user folders on each platform.

use std::{env, path::PathBuf};

const APP_DIR: &str = "HOI4_Analyzer";

/// The app's data folder (settings, game library, added DLLs):
/// * Windows: `%LOCALAPPDATA%\HOI4_Analyzer`
/// * macOS: `~/Library/Application Support/HOI4_Analyzer`
/// * Linux: `$XDG_DATA_HOME/HOI4_Analyzer` (default `~/.local/share/HOI4_Analyzer`)
///
/// Falls back to the temp folder if none of these can be determined.
pub fn data_dir() -> PathBuf {
    platform_data_dir()
        .unwrap_or_else(env::temp_dir)
        .join(APP_DIR)
}

/// Steam installation folders that may contain HOI4 (each also lists further
/// library folders in `steamapps/libraryfolders.vdf`).
pub fn steam_roots() -> Vec<PathBuf> {
    #[cfg(windows)]
    {
        vec![
            var("ProgramFiles(x86)")
                .unwrap_or_else(|| PathBuf::from(r"C:\Program Files (x86)"))
                .join("Steam"),
        ]
    }
    #[cfg(target_os = "macos")]
    {
        home()
            .map(|home| {
                home.join("Library")
                    .join("Application Support")
                    .join("Steam")
            })
            .into_iter()
            .collect()
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let mut roots: Vec<PathBuf> = xdg_data_home()
            .map(|d| d.join("Steam"))
            .into_iter()
            .collect();
        if let Some(home) = home() {
            roots.push(home.join(".steam").join("steam"));
            // Flatpak Steam.
            roots.push(
                home.join(".var")
                    .join("app")
                    .join("com.valvesoftware.Steam")
                    .join(".local")
                    .join("share")
                    .join("Steam"),
            );
        }
        roots
    }
    #[cfg(not(any(windows, unix)))]
    {
        Vec::new()
    }
}

/// A non-empty environment variable as a path.
fn var(name: &str) -> Option<PathBuf> {
    env::var_os(name)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

#[cfg(unix)]
fn home() -> Option<PathBuf> {
    var("HOME")
}

#[cfg(all(unix, not(target_os = "macos")))]
fn xdg_data_home() -> Option<PathBuf> {
    var("XDG_DATA_HOME")
        .filter(|p| p.is_absolute())
        .or_else(|| Some(home()?.join(".local").join("share")))
}

#[cfg(windows)]
fn platform_data_dir() -> Option<PathBuf> {
    var("LOCALAPPDATA")
}

#[cfg(target_os = "macos")]
fn platform_data_dir() -> Option<PathBuf> {
    Some(home()?.join("Library").join("Application Support"))
}

#[cfg(all(unix, not(target_os = "macos")))]
fn platform_data_dir() -> Option<PathBuf> {
    xdg_data_home()
}

#[cfg(not(any(windows, unix)))]
fn platform_data_dir() -> Option<PathBuf> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_dir_ends_with_app_folder() {
        assert!(data_dir().ends_with(APP_DIR));
    }
}
