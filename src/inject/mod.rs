//! Locating the DataCollection / AdminMenu DLLs and injecting them into HOI4.
//!
//! DLLs come from three places, in priority order:
//! 1. DLLs added in the app's settings, kept in
//!    `%LOCALAPPDATA%\HOI4_Analyzer\DataCollection` (new game versions without a
//!    new app build),
//! 2. a `DataCollection/` (or `AdminMenu/`) folder next to the executable,
//! 3. zstd-compressed copies embedded at build time, extracted on demand to the
//!    local cache directory.

mod embedded;
#[cfg(windows)]
mod win;

use std::{
    fmt, fs, io,
    path::{Path, PathBuf},
};

pub const PROCESS_NAME: &str = "hoi4.exe";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DllKind {
    DataCollection,
    AdminMenu,
}

impl DllKind {
    pub fn dir_name(self) -> &'static str {
        match self {
            Self::DataCollection => "DataCollection",
            Self::AdminMenu => "AdminMenu",
        }
    }

    pub fn file_name(self, version: &str) -> String {
        format!("{}_{version}.dll", self.dir_name())
    }

    fn already_loaded_message(self) -> &'static str {
        match self {
            Self::DataCollection => "Data collection already running.",
            Self::AdminMenu => "Admin Menu already attached.",
        }
    }

    fn success_message(self) -> &'static str {
        match self {
            Self::DataCollection => "Data collection enabled.",
            Self::AdminMenu => "Admin Menu attached.",
        }
    }
}

#[derive(Debug)]
pub enum InjectError {
    ProcessNotFound,
    DllMissing {
        file_name: String,
    },
    Io {
        context: &'static str,
        source: io::Error,
    },
    LoadLibraryFailed,
    Unsupported,
}

impl fmt::Display for InjectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ProcessNotFound => write!(f, "Process '{PROCESS_NAME}' not found."),
            Self::DllMissing { file_name } => {
                write!(f, "Files incomplete: {file_name} is not available.")
            }
            Self::Io { context, source } => write!(f, "{context}: {source}"),
            Self::LoadLibraryFailed => {
                write!(f, "DLL injection failed (LoadLibrary returned NULL).")
            }
            Self::Unsupported => write!(f, "DLL injection is only supported on Windows."),
        }
    }
}

impl std::error::Error for InjectError {}

/// Where a DLL version comes from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DllSource {
    /// Added in the settings (removable there).
    Added(PathBuf),
    /// In a folder next to the executable.
    NextToExe(PathBuf),
    /// Built into the executable.
    BuiltIn,
}

impl DllSource {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Added(_) => "added",
            Self::NextToExe(_) => "next to app",
            Self::BuiltIn => "built in",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DllEntry {
    pub version: String,
    /// The copy that is used, by priority (see the module docs).
    pub source: DllSource,
}

/// Every DataCollection version, newest first, with the copy that would be used.
pub fn catalog(kind: DllKind) -> Vec<DllEntry> {
    let mut entries: Vec<DllEntry> = Vec::new();
    let mut add = |version: String, source: DllSource| {
        if !entries.iter().any(|e| e.version == version) {
            entries.push(DllEntry { version, source });
        }
    };
    for (version, path) in disk_versions(&user_dir(kind), kind) {
        add(version, DllSource::Added(path));
    }
    if let Some(dir) = exe_dir() {
        for (version, path) in disk_versions(&dir.join(kind.dir_name()), kind) {
            add(version, DllSource::NextToExe(path));
        }
    }
    for version in embedded::versions(kind) {
        add(version.to_owned(), DllSource::BuiltIn);
    }
    entries.sort_by_cached_key(|e| std::cmp::Reverse(version_key(&e.version)));
    entries
}

/// All game versions with a DataCollection DLL, newest first.
pub fn available_versions() -> Vec<String> {
    let versions: Vec<String> = catalog(DllKind::DataCollection)
        .into_iter()
        .map(|e| e.version)
        .collect();
    log::info!("Found {} supported HOI4 versions", versions.len());
    versions
}

/// Whether the Admin Menu button should be offered.
pub fn admin_menu_available() -> bool {
    !catalog(DllKind::AdminMenu).is_empty()
}

/// Folder for DLLs added in the settings.
pub fn user_dir(kind: DllKind) -> PathBuf {
    local_data_dir().join(kind.dir_name())
}

/// The DLL version made for `game_version` (as reported by the game, e.g.
/// `1.19.5.0`). Trailing zero components are ignored, so `1.18.1` matches a
/// `1.18.1.0` DLL; a DLL for a hotfix of that exact version (`1.19.0` ->
/// `1.19.0.1`) is used only if it is the only one. Never guesses across
/// versions: a DLL built for another version can crash the game.
pub fn match_game_version<'a>(game_version: &str, versions: &'a [String]) -> Option<&'a str> {
    let wanted = trimmed_key(game_version);
    if wanted.is_empty() {
        return None;
    }
    if let Some(exact) = versions.iter().find(|v| trimmed_key(v) == wanted) {
        return Some(exact);
    }
    // Hotfix fallback compares against the version exactly as reported.
    let reported = version_key(game_version.trim().trim_start_matches(['v', 'V']));
    let mut hotfixes = versions
        .iter()
        .filter(|v| version_key(v).len() > reported.len() && version_key(v).starts_with(&reported));
    match (hotfixes.next(), hotfixes.next()) {
        (Some(only), None) => Some(only),
        _ => None,
    }
}

fn trimmed_key(version: &str) -> Vec<u32> {
    let mut key = version_key(version.trim().trim_start_matches(['v', 'V']));
    while key.len() > 2 && key.last() == Some(&0) {
        key.pop();
    }
    key
}

/// Copies a DataCollection DLL into the user folder so it becomes available
/// for `version`. Checks that the file is a 64-bit Windows DLL first.
pub fn add_dll(source: &Path, version: &str) -> Result<PathBuf, String> {
    let version = version.trim().trim_start_matches(['v', 'V']);
    if version_key(version).len() < 2
        || version.contains(|c: char| !(c.is_ascii_digit() || c == '.'))
    {
        return Err(format!("\"{version}\" is not a version number like 1.19.5"));
    }
    let bytes =
        fs::read(source).map_err(|e| format!("Could not read {}: {e}", source.display()))?;
    check_x64_dll(&bytes)?;
    let target = user_dir(DllKind::DataCollection).join(DllKind::DataCollection.file_name(version));
    fs::create_dir_all(target.parent().unwrap()).map_err(|e| e.to_string())?;
    let partial = target.with_extension("dll.partial");
    fs::write(&partial, &bytes)
        .and_then(|()| fs::rename(&partial, &target))
        .map_err(|e| format!("Could not store the DLL: {e}"))?;
    log::info!("Added DataCollection {version} from {}", source.display());
    Ok(target)
}

/// Removes a DLL that was added in the settings.
pub fn remove_dll(version: &str) -> Result<(), String> {
    let path = user_dir(DllKind::DataCollection).join(DllKind::DataCollection.file_name(version));
    fs::remove_file(&path).map_err(|e| format!("Could not remove {}: {e}", path.display()))?;
    log::info!("Removed added DataCollection {version}");
    Ok(())
}

/// Version number in a DLL file name, e.g. `DataCollection_1.19.5.dll` -> `1.19.5`.
pub fn version_from_file_name(path: &Path) -> Option<String> {
    let name = path.file_stem()?.to_string_lossy();
    let start = name.find(|c: char| c.is_ascii_digit())?;
    let version: String = name[start..]
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    let version = version.trim_end_matches('.').to_owned();
    (version_key(&version).len() >= 2).then_some(version)
}

/// Rejects anything that is not a 64-bit Windows PE image (HOI4 is 64-bit).
fn check_x64_dll(bytes: &[u8]) -> Result<(), String> {
    const NOT_DLL: &str = "This is not a Windows DLL.";
    if bytes.get(..2) != Some(b"MZ") {
        return Err(NOT_DLL.into());
    }
    let pe = bytes
        .get(0x3c..0x40)
        .map(|b| u32::from_le_bytes(b.try_into().unwrap()) as usize)
        .ok_or(NOT_DLL)?;
    if bytes.get(pe..pe + 4) != Some(b"PE\0\0") {
        return Err(NOT_DLL.into());
    }
    match bytes.get(pe + 4..pe + 6) {
        Some([0x64, 0x86]) => Ok(()),
        Some(_) => Err("This DLL is not 64-bit, so it cannot work with HOI4.".into()),
        None => Err(NOT_DLL.into()),
    }
}

/// The running game: process id and full path of `hoi4.exe`.
pub fn running_game() -> Option<(u32, PathBuf)> {
    #[cfg(windows)]
    {
        let pid = win::find_process(PROCESS_NAME)?;
        Some((pid, win::process_image_path(pid)?))
    }
    #[cfg(not(windows))]
    {
        None
    }
}

/// Injects the DLL for `kind`/`version` into the running game.
/// Returns the user-facing status message on success.
pub fn inject(kind: DllKind, version: &str) -> Result<&'static str, InjectError> {
    let file_name = kind.file_name(version);
    log::info!("Injecting {file_name} into {PROCESS_NAME}");

    #[cfg(windows)]
    {
        let pid = win::find_process(PROCESS_NAME).ok_or(InjectError::ProcessNotFound)?;
        log::info!("Found {PROCESS_NAME} with PID {pid}");

        match win::is_module_loaded(pid, &file_name) {
            Ok(true) => {
                log::info!("{file_name} is already loaded, skipping injection");
                return Ok(kind.already_loaded_message());
            }
            Ok(false) => {}
            Err(err) => log::warn!("Could not enumerate modules of PID {pid}: {err}"),
        }

        let path = resolve_dll(kind, version)?;
        log::info!("Using DLL at {}", path.display());
        win::inject_library(pid, &path)?;
        log::info!("{file_name} injected successfully");
        Ok(kind.success_message())
    }
    #[cfg(not(windows))]
    {
        let _ = file_name;
        Err(InjectError::Unsupported)
    }
}

/// Finds the DLL on disk or extracts the embedded copy.
pub fn resolve_dll(kind: DllKind, version: &str) -> Result<PathBuf, InjectError> {
    let file_name = kind.file_name(version);
    let dirs =
        std::iter::once(user_dir(kind)).chain(exe_dir().map(|dir| dir.join(kind.dir_name())));
    for dir in dirs {
        let candidate = dir.join(&file_name);
        if candidate.is_file() {
            return Ok(candidate);
        }
    }
    let dll = embedded::find(kind, version).ok_or_else(|| InjectError::DllMissing {
        file_name: file_name.clone(),
    })?;
    let target = cache_dir().join(&file_name);
    extract(dll, &target).map_err(|source| InjectError::Io {
        context: "Failed to extract DLL",
        source,
    })?;
    Ok(target)
}

fn extract(dll: &embedded::EmbeddedDll, target: &Path) -> io::Result<()> {
    let bytes = zstd::bulk::decompress(dll.data, dll.size as usize)?;
    if fs::read(target).is_ok_and(|existing| existing == bytes) {
        return Ok(());
    }
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    // Write-then-rename so a crash never leaves a truncated DLL behind.
    let partial = target.with_extension("dll.partial");
    fs::write(&partial, &bytes)?;
    fs::rename(&partial, target)
}

fn exe_dir() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()?
        .parent()
        .map(Path::to_path_buf)
}

/// `%LOCALAPPDATA%\HOI4_Analyzer` (temp dir as fallback).
fn local_data_dir() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("HOI4_Analyzer")
}

fn cache_dir() -> PathBuf {
    local_data_dir().join("dll")
}

/// `(version, path)` of every `<Kind>_<version>.dll` in `dir`.
fn disk_versions(dir: &Path, kind: DllKind) -> Vec<(String, PathBuf)> {
    let prefix = format!("{}_", kind.dir_name().to_ascii_lowercase());
    fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
            let version = name.strip_prefix(&prefix)?.strip_suffix(".dll")?;
            (version_key(version).len() >= 2).then(|| (version.to_owned(), entry.path()))
        })
        .collect()
}

/// Numeric sort key: "1.17.4.1" -> [1, 17, 4, 1]. Non-numeric parts end the key.
pub fn version_key(version: &str) -> Vec<u32> {
    version
        .split('.')
        .map_while(|part| part.parse().ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_game_versions_conservatively() {
        let versions: Vec<String> = [
            "1.19.4", "1.19.0.1", "1.18.1.0", "1.17.4.1", "1.17.4", "1.16.10",
        ]
        .map(String::from)
        .to_vec();
        let m = |game: &str| match_game_version(game, &versions);
        assert_eq!(m("1.19.4.0"), Some("1.19.4"));
        assert_eq!(m("v1.18.1"), Some("1.18.1.0"));
        assert_eq!(m("1.17.4.1"), Some("1.17.4.1"));
        assert_eq!(m("1.17.4"), Some("1.17.4"));
        assert_eq!(m("1.19.0"), Some("1.19.0.1"), "only hotfix of that version");
        assert_eq!(m("1.16.10.0"), Some("1.16.10"));
        assert_eq!(m("1.19.5.0"), None, "never use another version's DLL");
        assert_eq!(m("1.16.1"), None, "1.16.1 is not 1.16.10");
        assert_eq!(m(""), None);
    }

    #[test]
    fn validates_dlls_and_file_names() {
        let mut dll = vec![0u8; 0x200];
        dll[..2].copy_from_slice(b"MZ");
        dll[0x3c..0x40].copy_from_slice(&0x80u32.to_le_bytes());
        dll[0x80..0x84].copy_from_slice(b"PE\0\0");
        dll[0x84..0x86].copy_from_slice(&[0x64, 0x86]);
        assert!(check_x64_dll(&dll).is_ok());
        dll[0x84..0x86].copy_from_slice(&[0x4c, 0x01]); // i386
        assert!(check_x64_dll(&dll).unwrap_err().contains("64-bit"));
        assert!(check_x64_dll(b"not a dll").is_err());

        let v = |name: &str| version_from_file_name(Path::new(name));
        assert_eq!(v("DataCollection_1.19.5.dll").as_deref(), Some("1.19.5"));
        assert_eq!(
            v("C:/x/DataCollection_1.17.4.1.dll").as_deref(),
            Some("1.17.4.1")
        );
        assert_eq!(v("my_build.dll"), None);
    }

    #[test]
    fn versions_sort_numerically() {
        let mut v = vec!["1.16.9", "1.16.10", "1.17.4.1", "1.17.4", "1.9"];
        v.sort_by_key(|s| std::cmp::Reverse(version_key(s)));
        assert_eq!(v, ["1.17.4.1", "1.17.4", "1.16.10", "1.16.9", "1.9"]);
    }

    #[test]
    #[cfg(feature = "embed-dlls")]
    fn embedded_dlls_decompress() {
        let dll = embedded::find(DllKind::DataCollection, "1.16.4").expect("1.16.4 is embedded");
        let bytes = zstd::bulk::decompress(dll.data, dll.size as usize).unwrap();
        assert_eq!(bytes.len() as u64, dll.size);
        assert_eq!(&bytes[..2], b"MZ");
    }
}
