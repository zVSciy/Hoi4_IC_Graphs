//! User settings, stored as JSON next to the game library
//! (`%LOCALAPPDATA%\HOI4_Analyzer\settings.json`).
//!
//! Every field has a default, so settings files from older versions load fine
//! and new options simply start with their default value.

use std::{
    io,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

/// A user-defined set of countries, e.g. a multiplayer team.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroupSetting {
    pub name: String,
    /// Country tags separated by spaces or commas: `"GER ITA JAP"`.
    pub tags: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    // --- game & data collection
    /// Watch for `hoi4.exe` starting and stopping.
    pub watch_game: bool,
    /// Pick the DataCollection version matching the running game.
    pub auto_select_version: bool,
    /// Inject DataCollection automatically when the game starts.
    pub auto_inject: bool,
    /// Seconds to wait after the game starts before injecting (it needs to
    /// finish loading first). No wait if the game was already running.
    pub inject_delay_secs: u32,
    /// Start live monitoring after data collection is running.
    pub auto_live: bool,
    /// Where DataCollection writes; `None` = `output.txt` in the game folder.
    pub output_file: Option<PathBuf>,

    // --- live dashboard & library
    pub live_interval_secs: u32,
    /// Store loaded files and live games in the game library.
    pub auto_save: bool,
    pub live_save_every_secs: u32,

    // --- graphs
    /// Show country names from the game's localisation instead of tags.
    pub country_names: bool,
    /// Give each country the same colour in every graph.
    pub stable_colors: bool,
    /// Default rolling average for daily values (0 = off).
    pub smoothing_days: u32,

    // --- country groups
    /// Use `custom_groups` instead of Axis / Allies.
    pub use_custom_groups: bool,
    pub custom_groups: Vec<GroupSetting>,

    // --- updates
    pub check_updates: bool,

    // --- remembered between sessions
    pub version: String,
    pub selected_countries: Vec<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            watch_game: true,
            auto_select_version: true,
            auto_inject: true,
            inject_delay_secs: 30,
            auto_live: true,
            output_file: None,
            live_interval_secs: 2,
            auto_save: true,
            live_save_every_secs: 30,
            country_names: true,
            stable_colors: true,
            smoothing_days: 0,
            use_custom_groups: false,
            custom_groups: vec![
                GroupSetting {
                    name: "Team 1".into(),
                    tags: "GER ITA JAP".into(),
                },
                GroupSetting {
                    name: "Team 2".into(),
                    tags: "ENG SOV USA FRA".into(),
                },
            ],
            check_updates: true,
            version: String::new(),
            selected_countries: Vec::new(),
        }
    }
}

impl Settings {
    /// `%LOCALAPPDATA%\HOI4_Analyzer\settings.json` (temp dir as fallback),
    /// or the file named by `HOI4_ANALYZER_SETTINGS`.
    pub fn default_path() -> PathBuf {
        if let Some(path) = std::env::var_os("HOI4_ANALYZER_SETTINGS").filter(|p| !p.is_empty()) {
            return PathBuf::from(path);
        }
        std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir)
            .join("HOI4_Analyzer")
            .join("settings.json")
    }

    /// Loads settings, falling back to defaults if the file is missing or broken.
    pub fn load(path: &Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(text) => serde_json::from_str(&text).unwrap_or_else(|err| {
                log::warn!(
                    "Ignoring unreadable settings file {}: {err}",
                    path.display()
                );
                Self::default()
            }),
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self, path: &Path) -> io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let json = serde_json::to_string_pretty(self).map_err(io::Error::other)?;
        // Write-then-rename so a crash never leaves a truncated file.
        let partial = path.with_extension("json.partial");
        std::fs::write(&partial, json)?;
        std::fs::rename(&partial, path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_tolerates_old_files() {
        let dir = std::env::temp_dir().join(format!("hoi4_settings_{}", std::process::id()));
        let path = dir.join("settings.json");
        let settings = Settings {
            version: "1.19.5".into(),
            selected_countries: vec!["GER".into(), "SOV".into()],
            ..Settings::default()
        };
        settings.save(&path).unwrap();
        assert_eq!(Settings::load(&path), settings);

        // A file written by an older version lacks most fields.
        std::fs::write(&path, r#"{ "auto_save": false, "unknown_field": 1 }"#).unwrap();
        let loaded = Settings::load(&path);
        assert!(!loaded.auto_save);
        assert_eq!(
            loaded.inject_delay_secs,
            Settings::default().inject_delay_secs
        );

        std::fs::write(&path, "not json").unwrap();
        assert_eq!(Settings::load(&path), Settings::default());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
