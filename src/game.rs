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

use crate::{data::Tag, inject};

/// The HOI4 installation folder: from the running game if there is one,
/// otherwise from Steam's library folders.
pub fn install_dir() -> Option<PathBuf> {
    if let Some((_, exe)) = inject::running_game()
        && let Some(dir) = exe.parent()
    {
        return Some(dir.to_path_buf());
    }
    let steam = std::env::var_os("ProgramFiles(x86)")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Program Files (x86)"))
        .join("Steam");
    let vdf =
        fs::read_to_string(steam.join("steamapps").join("libraryfolders.vdf")).unwrap_or_default();
    std::iter::once(steam)
        .chain(steam_library_paths(&vdf))
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

/// Country names from the game's English localisation (` GER: "Germany"`).
#[derive(Clone, Debug, Default)]
pub struct CountryNames(FxHashMap<Tag, String>);

impl CountryNames {
    pub fn load(dir: &Path) -> Self {
        let path = dir
            .join("localisation")
            .join("english")
            .join("countries_l_english.yml");
        match fs::read_to_string(&path) {
            Ok(text) => {
                let names = Self::parse(&text);
                log::info!("Loaded {} country names from the game", names.0.len());
                names
            }
            Err(err) => {
                log::warn!("Country names unavailable ({}): {err}", path.display());
                Self::default()
            }
        }
    }

    fn parse(text: &str) -> Self {
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
        Self(names)
    }

    pub fn get(&self, tag: Tag) -> Option<&str> {
        self.0.get(&tag).map(String::as_str)
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn len(&self) -> usize {
        self.0.len()
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
        assert_eq!(names.get(Tag::from_static("GER")), Some("Germany"));
        assert_eq!(names.get(Tag::from_static("ENG")), Some("Britain"));
        assert_eq!(names.0.len(), 2);
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
