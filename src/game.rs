//! Knowledge about the Hearts of Iron IV installation: where it is, which
//! version it is, what the countries are called, and whether it is running.

use std::{
    fs,
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver, RecvTimeoutError, Sender},
    thread::JoinHandle,
    time::Duration,
};

use rustc_hash::FxHashMap;

use crate::{countries::COUNTRY_NAMES, data::Tag, inject, paths};

/// The HOI4 installation folder: from the running game if there is one,
/// otherwise from Steam's library folders.
pub fn install_dir() -> Option<PathBuf> {
    if let Some((_, exe)) = inject::running_game()
        && let Some(dir) = exe.parent()
    {
        return Some(dir.to_path_buf());
    }
    paths::steam_roots()
        .into_iter()
        .flat_map(|steam| {
            let vdf = fs::read_to_string(steam.join("steamapps").join("libraryfolders.vdf"))
                .unwrap_or_default();
            let libraries: Vec<PathBuf> = steam_library_paths(&vdf).collect();
            std::iter::once(steam).chain(libraries)
        })
        .map(|library| {
            library
                .join("steamapps")
                .join("common")
                .join("Hearts of Iron IV")
        })
        .find(|dir| dir.is_dir())
}

/// `"path"  "D:\\SteamLibrary"` entries of Steam's `libraryfolders.vdf`.
fn steam_library_paths(vdf: &str) -> impl Iterator<Item = PathBuf> + '_ {
    vdf.lines().filter_map(|line| {
        let rest = line.trim().strip_prefix("\"path\"")?.trim();
        let value = rest.strip_prefix('"')?.strip_suffix('"')?;
        Some(PathBuf::from(value.replace("\\\\", "\\")))
    })
}

/// The game version from the Paradox launcher's `launcher-settings.json`
/// (`"rawVersion": "1.19.5.0"`).
pub fn version(dir: &Path) -> Option<String> {
    let text = fs::read_to_string(dir.join("launcher-settings.json")).ok()?;
    let json: serde_json::Value = serde_json::from_str(&text).ok()?;
    let raw = json.get("rawVersion")?.as_str()?.trim();
    (!raw.is_empty()).then(|| raw.trim_start_matches(['v', 'V']).to_owned())
}

/// Country names: the built-in table ([`crate::countries`]) first, then the
/// game's English localisation (` GER: "Germany"`) for tags the table lacks
/// (mod countries, dynamic tags).
#[derive(Clone, Debug)]
pub struct CountryNames {
    names: FxHashMap<Tag, String>,
    /// How many names came from the game's localisation.
    from_game: usize,
}

impl Default for CountryNames {
    fn default() -> Self {
        Self::builtin()
    }
}

impl CountryNames {
    /// Only the built-in names.
    pub fn builtin() -> Self {
        let names = COUNTRY_NAMES
            .iter()
            .filter_map(|&(tag, name)| Some((Tag::from_bytes(tag.as_bytes())?, name.to_owned())))
            .collect();
        Self {
            names,
            from_game: 0,
        }
    }

    /// Built-in names plus the game's names for tags without a built-in one.
    pub fn load(dir: &Path) -> Self {
        let path = dir
            .join("localisation")
            .join("english")
            .join("countries_l_english.yml");
        match fs::read_to_string(&path) {
            Ok(text) => {
                let names = Self::builtin().with_game_names(Self::parse(&text));
                log::info!(
                    "Using {} country names ({} from the game)",
                    names.len(),
                    names.from_game
                );
                names
            }
            Err(err) => {
                log::warn!("Game country names unavailable ({}): {err}", path.display());
                Self::builtin()
            }
        }
    }

    fn with_game_names(mut self, game: FxHashMap<Tag, String>) -> Self {
        for (tag, name) in game {
            if let std::collections::hash_map::Entry::Vacant(slot) = self.names.entry(tag) {
                slot.insert(name);
                self.from_game += 1;
            }
        }
        self
    }

    /// Plain `TAG: "Name"` entries of a localisation file.
    fn parse(text: &str) -> FxHashMap<Tag, String> {
        let mut names = FxHashMap::default();
        for line in text.lines() {
            // Only plain `TAG:` keys; `GER_fascism:` etc. are ideology variants.
            let Some((key, rest)) = line.trim().split_once(':') else {
                continue;
            };
            if key.len() != 3 || !key.bytes().all(|b| b.is_ascii_uppercase()) {
                continue;
            }
            let rest = rest.trim_start_matches(|c: char| c.is_ascii_digit()).trim();
            let Some(name) = rest.strip_prefix('"').and_then(|r| r.split('"').next()) else {
                continue;
            };
            if let (Some(tag), false) = (Tag::from_bytes(key.as_bytes()), name.is_empty()) {
                names.insert(tag, name.to_owned());
            }
        }
        names
    }

    pub fn get(&self, tag: Tag) -> Option<&str> {
        self.names.get(&tag).map(String::as_str)
    }

    pub fn is_empty(&self) -> bool {
        self.names.is_empty()
    }

    pub fn len(&self) -> usize {
        self.names.len()
    }

    /// Whether the game's localisation has been merged in.
    pub fn has_game_names(&self) -> bool {
        self.from_game > 0
    }

    /// How many names came from the game's localisation.
    pub fn from_game(&self) -> usize {
        self.from_game
    }
}

/// A running game as seen by the [`GameWatcher`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunningGame {
    pub pid: u32,
    pub dir: PathBuf,
    pub version: Option<String>,
    /// The game was already running when watching started (so it has
    /// finished loading and needs no start-up delay).
    pub was_running: bool,
}

pub enum GameEvent {
    Started(RunningGame),
    Exited,
}

/// Polls for `hoi4.exe` in the background and reports starts and exits.
pub struct GameWatcher {
    stop: Sender<()>,
    events: Receiver<GameEvent>,
    thread: Option<JoinHandle<()>>,
}

impl GameWatcher {
    pub fn start(on_event: impl Fn() + Send + 'static) -> Self {
        let (stop_tx, stop_rx) = mpsc::channel::<()>();
        let (event_tx, event_rx) = mpsc::channel();
        let thread = std::thread::Builder::new()
            .name("game-watcher".into())
            .spawn(move || {
                let mut current: Option<u32> = None;
                let mut first = true;
                loop {
                    let running = inject::running_game();
                    match (current, &running) {
                        (None, Some((pid, exe))) => {
                            let dir = exe.parent().map(Path::to_path_buf).unwrap_or_default();
                            let game = RunningGame {
                                pid: *pid,
                                version: version(&dir),
                                dir,
                                was_running: first,
                            };
                            log::info!("HOI4 detected: {game:?}");
                            current = Some(*pid);
                            let _ = event_tx.send(GameEvent::Started(game));
                            on_event();
                        }
                        (Some(old), Some((pid, _))) if old != *pid => {
                            // Restarted between two polls: report exit, the start follows.
                            current = None;
                            let _ = event_tx.send(GameEvent::Exited);
                            on_event();
                            continue;
                        }
                        (Some(_), None) => {
                            log::info!("HOI4 closed");
                            current = None;
                            let _ = event_tx.send(GameEvent::Exited);
                            on_event();
                        }
                        _ => {}
                    }
                    first = false;
                    match stop_rx.recv_timeout(Duration::from_secs(2)) {
                        Err(RecvTimeoutError::Timeout) => {}
                        _ => break,
                    }
                }
            })
            .expect("failed to spawn game watcher");
        Self {
            stop: stop_tx,
            events: event_rx,
            thread: Some(thread),
        }
    }

    pub fn events(&self) -> impl Iterator<Item = GameEvent> + '_ {
        self.events.try_iter()
    }
}

impl Drop for GameWatcher {
    fn drop(&mut self) {
        let _ = self.stop.send(());
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_steam_library_folders() {
        let vdf = r#"
"libraryfolders"
{
	"0"
	{
		"path"		"C:\\Program Files (x86)\\Steam"
		"label"		""
	}
	"1"
	{
		"path"		"D:\\SteamLibrary"
	}
}"#;
        let paths: Vec<PathBuf> = steam_library_paths(vdf).collect();
        assert_eq!(
            paths,
            [
                PathBuf::from(r"C:\Program Files (x86)\Steam"),
                PathBuf::from(r"D:\SteamLibrary")
            ]
        );
    }

    #[test]
    fn parses_country_localisation() {
        let text = "\u{feff}l_english:\n GER_fascism: \"German Reich\"\n GER: \"Germany\"\n \
                    ENG:0 \"Britain\"\n GER_ADJ: \"German\"\n D01: \"Dynamic\"\n";
        let names = CountryNames::parse(text);
        assert_eq!(
            names.get(&Tag::from_static("GER")).map(String::as_str),
            Some("Germany")
        );
        assert_eq!(
            names.get(&Tag::from_static("ENG")).map(String::as_str),
            Some("Britain")
        );
        assert_eq!(names.len(), 2);
    }

    #[test]
    fn builtin_names_win_over_game_names() {
        let game = CountryNames::parse(" GER: \"Germany\"\n SOV: \"Russia\"\n ZZZ: \"Modland\"\n");
        let names = CountryNames::builtin().with_game_names(game);
        assert_eq!(names.get(Tag::from_static("GER")), Some("German Reich"));
        assert_eq!(names.get(Tag::from_static("SOV")), Some("Soviet Union"));
        assert_eq!(names.get(Tag::from_static("ZZZ")), Some("Modland"));
        assert_eq!(names.from_game(), 1);
        assert!(names.has_game_names());
    }

    #[test]
    fn builtin_table_is_sorted_unique_and_valid() {
        assert!(COUNTRY_NAMES.windows(2).all(|w| w[0].0 < w[1].0));
        for (tag, name) in COUNTRY_NAMES {
            assert!(Tag::from_bytes(tag.as_bytes()).is_some(), "bad tag {tag}");
            assert!(!name.trim().is_empty(), "empty name for {tag}");
        }
        let mut names: Vec<_> = COUNTRY_NAMES.iter().map(|(_, n)| n).collect();
        names.sort();
        assert!(names.windows(2).all(|w| w[0] != w[1]), "duplicate name");
        assert_eq!(CountryNames::builtin().len(), COUNTRY_NAMES.len());
    }

    #[test]
    fn reads_launcher_version() {
        let dir = std::env::temp_dir().join(format!("hoi4_game_{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("launcher-settings.json"),
            r#"{"version": "Operation Postern v1.19.5.0.7c6c (c184)", "rawVersion": "1.19.5.0"}"#,
        )
        .unwrap();
        assert_eq!(version(&dir).as_deref(), Some("1.19.5.0"));
        let _ = fs::remove_dir_all(&dir);
    }
}
