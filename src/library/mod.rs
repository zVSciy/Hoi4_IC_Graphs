//! Game library: every loaded or live-monitored output file is kept, already parsed, in a
//! SQLite database so past campaigns can be compared without the original
//! `output.txt` files.
//!
//! A game is identified by its content, not its path:
//! * re-saving an identical file changes nothing,
//! * saving a file that *extends* a stored game (the same campaign played
//!   further, `output.txt` only ever grows) updates that game in place,
//! * anything else becomes a new game.
//!
//! The database lives at `library.sqlite3` in the app data folder and can
//! be opened with any SQLite tool; the parsed data is a compact blob (see
//! [`codec`]).

pub mod codec;

use std::{
    fmt, io,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};

use crate::data::{self, Dataset, Day, Metric};

pub use codec::CodecError;

/// Bump with a migration step in [`Library::open`] when the schema changes.
const SCHEMA_VERSION: i64 = 2;

/// First bytes of an exported game file.
const EXPORT_MAGIC: &[u8; 8] = b"HOI4GAME";
const EXPORT_FORMAT: u8 = 1;
/// File extension for exported games.
pub const EXPORT_EXTENSION: &str = "hoi4game";

#[derive(Debug)]
pub enum LibraryError {
    Sqlite(rusqlite::Error),
    Io(io::Error),
    Codec(CodecError),
    NotFound(i64),
    /// An import file that is not an exported game.
    Format(String),
}

impl fmt::Display for LibraryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Sqlite(err) => write!(f, "library database error: {err}"),
            Self::Io(err) => write!(f, "{err}"),
            Self::Codec(err) => write!(f, "{err}"),
            Self::NotFound(id) => write!(f, "game #{id} is no longer in the library"),
            Self::Format(why) => write!(f, "not an exported game file: {why}"),
        }
    }
}

impl std::error::Error for LibraryError {}

impl From<rusqlite::Error> for LibraryError {
    fn from(err: rusqlite::Error) -> Self {
        Self::Sqlite(err)
    }
}

impl From<io::Error> for LibraryError {
    fn from(err: io::Error) -> Self {
        Self::Io(err)
    }
}

impl From<CodecError> for LibraryError {
    fn from(err: CodecError) -> Self {
        Self::Codec(err)
    }
}

pub type Result<T> = std::result::Result<T, LibraryError>;

/// Everything about a stored game except its data.
#[derive(Clone, Debug, PartialEq)]
pub struct GameMeta {
    pub id: i64,
    pub name: String,
    /// HOI4 / DataCollection version the game was recorded with.
    pub version: String,
    pub notes: String,
    pub source_path: String,
    /// Unix seconds.
    pub saved_at: i64,
    pub updated_at: i64,
    pub first_day: Option<Day>,
    pub last_day: Option<Day>,
    pub days: u32,
    pub countries: u32,
    pub has_civ: bool,
    /// Size of the stored data in bytes.
    pub stored_bytes: u64,
    pub summary: Summary,
}

/// Headline numbers of a game, computed when it is stored.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Summary {
    /// Everything produced by all countries over the whole game, per metric.
    pub totals: [f64; Metric::COUNT],
    /// Country with the most military output and its share of the total (%).
    pub top_producer: Option<(String, f64)>,
    /// Highest military output of all countries together on one day.
    pub peak_daily_mil: f64,
}

impl Summary {
    pub fn of(dataset: &Dataset) -> Self {
        let width = dataset.dates().len();
        let mut totals = [0.0; Metric::COUNT];
        for metric in Metric::ALL {
            totals[metric as usize] = dataset.matrix(metric).iter().sum();
        }
        let top_producer = (0..dataset.tags().len())
            .map(|i| (i, dataset.row(Metric::Mil, i).iter().sum::<f64>()))
            .max_by(|a, b| a.1.total_cmp(&b.1))
            .filter(|&(_, total)| total > 0.0)
            .map(|(i, total)| {
                let share = total / totals[Metric::Mil as usize] * 100.0;
                (dataset.tags()[i].to_string(), share)
            });
        let mil = dataset.matrix(Metric::Mil);
        let peak_daily_mil = (0..width)
            .map(|day| {
                (0..dataset.tags().len())
                    .map(|row| mil[row * width + day])
                    .sum::<f64>()
            })
            .fold(0.0, f64::max);
        Self {
            totals,
            top_producer,
            peak_daily_mil,
        }
    }
}

/// Metadata stored in an exported game file.
#[derive(Serialize, Deserialize)]
struct ExportHeader {
    name: String,
    version: String,
    notes: String,
    source_path: String,
    content_len: i64,
    content_hash: String,
    saved_at: i64,
    updated_at: i64,
}

impl GameMeta {
    /// Case-insensitive match against name, version, notes and source path.
    pub fn matches(&self, query: &str) -> bool {
        let query = query.trim().to_lowercase();
        query.is_empty()
            || [&self.name, &self.version, &self.notes, &self.source_path]
                .iter()
                .any(|field| field.to_lowercase().contains(&query))
    }
}

/// What [`Library::save_file`] did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SaveOutcome {
    /// A new game was added.
    Added(i64),
    /// An existing game was extended with newer data.
    Updated(i64),
    /// The file was already stored.
    Unchanged(i64),
}

impl SaveOutcome {
    pub fn id(self) -> i64 {
        match self {
            Self::Added(id) | Self::Updated(id) | Self::Unchanged(id) => id,
        }
    }
}

pub struct Library {
    conn: Connection,
    path: PathBuf,
}

impl Library {
    /// `library.sqlite3` in the app's data folder ([`crate::paths::data_dir`]),
    /// or the file named by `HOI4_ANALYZER_LIBRARY` (e.g. in a synced folder).
    pub fn default_path() -> PathBuf {
        if let Some(path) = std::env::var_os("HOI4_ANALYZER_LIBRARY").filter(|p| !p.is_empty()) {
            return PathBuf::from(path);
        }
        crate::paths::data_dir().join("library.sqlite3")
    }

    /// Opens (creating if needed) the library at `path`. Every thread that
    /// writes should open its own `Library`; SQLite serialises the writers.
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let conn = Connection::open(path)?;
        conn.busy_timeout(std::time::Duration::from_secs(10))?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        let version: i64 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
        if version == 0 {
            conn.execute_batch(
                "BEGIN;
                 CREATE TABLE IF NOT EXISTS games (
                     id           INTEGER PRIMARY KEY,
                     name         TEXT    NOT NULL,
                     version      TEXT    NOT NULL DEFAULT '',
                     notes        TEXT    NOT NULL DEFAULT '',
                     source_path  TEXT    NOT NULL,
                     content_len  INTEGER NOT NULL,
                     content_hash BLOB    NOT NULL,
                     saved_at     INTEGER NOT NULL,
                     updated_at   INTEGER NOT NULL,
                     first_day    INTEGER,
                     last_day     INTEGER,
                     days         INTEGER NOT NULL,
                     countries    INTEGER NOT NULL,
                     has_civ      INTEGER NOT NULL,
                     data         BLOB    NOT NULL,
                     summary      TEXT    NOT NULL DEFAULT ''
                 );
                 CREATE INDEX IF NOT EXISTS games_by_source ON games(source_path);
                 CREATE UNIQUE INDEX IF NOT EXISTS games_by_content ON games(content_hash, content_len);
                 PRAGMA user_version = 2;
                 COMMIT;",
            )?;
        } else if version == 1 {
            conn.execute_batch(
                "BEGIN;
                 ALTER TABLE games ADD COLUMN summary TEXT NOT NULL DEFAULT '';
                 PRAGMA user_version = 2;
                 COMMIT;",
            )?;
        } else if version > SCHEMA_VERSION {
            log::warn!(
                "Library {} was written by a newer version of the app (schema {version})",
                path.display()
            );
        }
        let library = Self {
            conn,
            path: path.to_path_buf(),
        };
        library.backfill_summaries()?;
        Ok(library)
    }

    /// Computes summaries for games stored before summaries existed.
    fn backfill_summaries(&self) -> Result<()> {
        let ids: Vec<i64> = {
            let mut stmt = self
                .conn
                .prepare("SELECT id FROM games WHERE summary = ''")?;
            stmt.query_map([], |row| row.get(0))?
                .collect::<rusqlite::Result<_>>()?
        };
        for id in ids {
            match self.load(id) {
                Ok(dataset) => {
                    let summary = serde_json::to_string(&Summary::of(&dataset)).unwrap_or_default();
                    self.conn.execute(
                        "UPDATE games SET summary = ?2 WHERE id = ?1",
                        params![id, summary],
                    )?;
                }
                Err(err) => log::warn!("Library game #{id} could not be summarised: {err}"),
            }
        }
        Ok(())
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// All games, most recently updated first.
    pub fn games(&self) -> Result<Vec<GameMeta>> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT id, name, version, notes, source_path, saved_at, updated_at,
                    first_day, last_day, days, countries, has_civ, length(data), summary
             FROM games ORDER BY updated_at DESC, id DESC",
        )?;
        let games = stmt
            .query_map([], |row| {
                Ok(GameMeta {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    version: row.get(2)?,
                    notes: row.get(3)?,
                    source_path: row.get(4)?,
                    saved_at: row.get(5)?,
                    updated_at: row.get(6)?,
                    first_day: row.get::<_, Option<i32>>(7)?.map(Day),
                    last_day: row.get::<_, Option<i32>>(8)?.map(Day),
                    days: row.get(9)?,
                    countries: row.get(10)?,
                    has_civ: row.get(11)?,
                    stored_bytes: row.get::<_, i64>(12)?.max(0) as u64,
                    summary: serde_json::from_str(&row.get::<_, String>(13)?).unwrap_or_default(),
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(games)
    }

    /// Distinct non-empty versions, newest first.
    pub fn versions(&self) -> Result<Vec<String>> {
        let mut stmt = self
            .conn
            .prepare_cached("SELECT DISTINCT version FROM games WHERE version != ''")?;
        let mut versions: Vec<String> = stmt
            .query_map([], |row| row.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        versions.sort_by_cached_key(|v| std::cmp::Reverse(crate::inject::version_key(v)));
        Ok(versions)
    }

    /// Decodes a stored game.
    pub fn load(&self, id: i64) -> Result<Dataset> {
        let blob: Vec<u8> = self
            .conn
            .query_row("SELECT data FROM games WHERE id = ?1", [id], |row| {
                row.get(0)
            })
            .optional()?
            .ok_or(LibraryError::NotFound(id))?;
        Ok(codec::decode(&blob)?)
    }

    /// Updates the user-editable fields of a game.
    pub fn update_details(&self, id: i64, name: &str, version: &str, notes: &str) -> Result<()> {
        let changed = self.conn.execute(
            "UPDATE games SET name = ?2, version = ?3, notes = ?4 WHERE id = ?1",
            params![id, name.trim(), version.trim(), notes],
        )?;
        if changed == 0 {
            return Err(LibraryError::NotFound(id));
        }
        Ok(())
    }

    pub fn delete(&self, id: i64) -> Result<()> {
        self.conn.execute("DELETE FROM games WHERE id = ?1", [id])?;
        Ok(())
    }

    /// Parses `path` and stores it, deduplicating by content (see module docs).
    /// `version` is recorded for new games; existing games keep theirs.
    pub fn save_file(&mut self, path: &Path, version: &str) -> Result<(SaveOutcome, Dataset)> {
        let source_path = path.to_string_lossy().into_owned();
        // Games from this path that the file could extend: their stored content
        // must be a prefix of the file, which is checked while parsing.
        let candidates: Vec<(i64, i64, Vec<u8>)> = {
            let mut stmt = self.conn.prepare_cached(
                "SELECT id, content_len, content_hash FROM games
                 WHERE source_path = ?1 ORDER BY content_len DESC",
            )?;
            stmt.query_map([&source_path], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })?
            .collect::<rusqlite::Result<_>>()?
        };

        let (dataset, (len, hash, extends)) = data::load_file_with(path, |bytes| {
            let extends = candidates
                .iter()
                .find(|(_, len, hash)| {
                    usize::try_from(*len).is_ok_and(|len| {
                        len <= bytes.len()
                            && fingerprint(&bytes[..len]).as_slice() == hash.as_slice()
                    })
                })
                .map(|(id, ..)| *id);
            (bytes.len() as i64, fingerprint(bytes), extends)
        })?;

        // Exclusive from here on: saves from other threads or app instances
        // cannot slip in between the duplicate check and the write.
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let existing: Option<i64> = tx
            .query_row(
                "SELECT id FROM games WHERE content_hash = ?1 AND content_len = ?2",
                params![hash.as_slice(), len],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(id) = existing {
            return Ok((SaveOutcome::Unchanged(id), dataset));
        }
        // The candidate may have been deleted or extended in the meantime.
        let extends = match extends {
            Some(id) => tx
                .query_row(
                    "SELECT id FROM games WHERE id = ?1 AND content_len <= ?2",
                    params![id, len],
                    |row| row.get::<_, i64>(0),
                )
                .optional()?,
            None => None,
        };

        let blob = codec::encode(&dataset);
        let now = unix_now();
        let first_day = dataset.dates().first().map(|d| d.0);
        let last_day = dataset.dates().last().map(|d| d.0);
        let days = dataset.dates().len() as u32;
        let countries = dataset.active_tags().count() as u32;
        let has_civ = dataset.has_metric(Metric::Civ);
        let summary = serde_json::to_string(&Summary::of(&dataset)).unwrap_or_default();

        let outcome = match extends {
            Some(id) => {
                tx.execute(
                    "UPDATE games SET content_len = ?2, content_hash = ?3, updated_at = ?4,
                         first_day = ?5, last_day = ?6, days = ?7, countries = ?8,
                         has_civ = ?9, data = ?10, summary = ?11
                     WHERE id = ?1",
                    params![
                        id,
                        len,
                        hash.as_slice(),
                        now,
                        first_day,
                        last_day,
                        days,
                        countries,
                        has_civ,
                        blob,
                        summary
                    ],
                )?;
                SaveOutcome::Updated(id)
            }
            None => {
                tx.execute(
                    "INSERT INTO games (name, version, source_path, content_len, content_hash,
                         saved_at, updated_at, first_day, last_day, days, countries, has_civ, data,
                         summary)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                    params![
                        default_name(path),
                        version.trim(),
                        source_path,
                        len,
                        hash.as_slice(),
                        now,
                        first_day,
                        last_day,
                        days,
                        countries,
                        has_civ,
                        blob,
                        summary
                    ],
                )?;
                let id = tx.last_insert_rowid();
                // Generic file names get the game number: "Game #7".
                tx.execute(
                    "UPDATE games SET name = 'Game #' || id WHERE id = ?1 AND name = ''",
                    [id],
                )?;
                SaveOutcome::Added(id)
            }
        };
        tx.commit()?;
        log::info!(
            "Library: {:?} for {} ({} KiB stored)",
            outcome,
            path.display(),
            blob.len() / 1024
        );
        Ok((outcome, dataset))
    }
}

impl Library {
    /// Writes one game to a self-contained file another user can import.
    pub fn export_game(&self, id: i64, target: &Path) -> Result<()> {
        let (header, blob) = self
            .conn
            .query_row(
                "SELECT name, version, notes, source_path, content_len, content_hash,
                        saved_at, updated_at, data
                 FROM games WHERE id = ?1",
                [id],
                |row| {
                    let hash: Vec<u8> = row.get(5)?;
                    Ok((
                        ExportHeader {
                            name: row.get(0)?,
                            version: row.get(1)?,
                            notes: row.get(2)?,
                            source_path: row.get(3)?,
                            content_len: row.get(4)?,
                            content_hash: hash.iter().map(|b| format!("{b:02x}")).collect(),
                            saved_at: row.get(6)?,
                            updated_at: row.get(7)?,
                        },
                        row.get::<_, Vec<u8>>(8)?,
                    ))
                },
            )
            .optional()?
            .ok_or(LibraryError::NotFound(id))?;
        let json = serde_json::to_vec(&header).map_err(io::Error::other)?;
        let mut file = Vec::with_capacity(EXPORT_MAGIC.len() + 5 + json.len() + blob.len());
        file.extend_from_slice(EXPORT_MAGIC);
        file.push(EXPORT_FORMAT);
        file.extend_from_slice(&(json.len() as u32).to_le_bytes());
        file.extend_from_slice(&json);
        file.extend_from_slice(&blob);
        std::fs::write(target, file)?;
        log::info!("Exported library game #{id} to {}", target.display());
        Ok(())
    }

    /// Adds a game exported with [`Self::export_game`]; a game that is
    /// already in the library is left alone.
    pub fn import_game(&mut self, source: &Path) -> Result<SaveOutcome> {
        let bytes = std::fs::read(source)?;
        let format_err = |why: &str| LibraryError::Format(why.to_owned());
        if bytes.get(..EXPORT_MAGIC.len()) != Some(EXPORT_MAGIC) {
            return Err(format_err("missing header"));
        }
        let rest = &bytes[EXPORT_MAGIC.len()..];
        if rest.first() != Some(&EXPORT_FORMAT) {
            return Err(format_err("unsupported format, update the app"));
        }
        let json_len = rest
            .get(1..5)
            .map(|b| u32::from_le_bytes(b.try_into().unwrap()) as usize)
            .ok_or_else(|| format_err("truncated"))?;
        let json = rest
            .get(5..5 + json_len)
            .ok_or_else(|| format_err("truncated"))?;
        let blob = &rest[5 + json_len..];
        let header: ExportHeader =
            serde_json::from_slice(json).map_err(|e| LibraryError::Format(e.to_string()))?;
        let hash: Vec<u8> = (0..header.content_hash.len() / 2)
            .filter_map(|i| u8::from_str_radix(header.content_hash.get(i * 2..i * 2 + 2)?, 16).ok())
            .collect();
        if hash.len() != 16 {
            return Err(format_err("bad fingerprint"));
        }
        // Decoding validates the data and yields the listing columns.
        let dataset = codec::decode(blob)?;

        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let existing: Option<i64> = tx
            .query_row(
                "SELECT id FROM games WHERE content_hash = ?1 AND content_len = ?2",
                params![hash, header.content_len],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(id) = existing {
            return Ok(SaveOutcome::Unchanged(id));
        }
        tx.execute(
            "INSERT INTO games (name, version, notes, source_path, content_len, content_hash,
                 saved_at, updated_at, first_day, last_day, days, countries, has_civ, data, summary)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
            params![
                header.name,
                header.version,
                header.notes,
                header.source_path,
                header.content_len,
                hash,
                header.saved_at,
                header.updated_at,
                dataset.dates().first().map(|d| d.0),
                dataset.dates().last().map(|d| d.0),
                dataset.dates().len() as u32,
                dataset.active_tags().count() as u32,
                dataset.has_metric(Metric::Civ),
                blob,
                serde_json::to_string(&Summary::of(&dataset)).unwrap_or_default(),
            ],
        )?;
        let id = tx.last_insert_rowid();
        tx.commit()?;
        log::info!("Imported {} as library game #{id}", source.display());
        Ok(SaveOutcome::Added(id))
    }
}

/// 128-bit content fingerprint.
fn fingerprint(bytes: &[u8]) -> [u8; 16] {
    xxhash_rust::xxh3::xxh3_128(bytes).to_le_bytes()
}

/// The file stem, or empty for the usual `output.txt` (its name says nothing
/// about the game; such games are named "Game #<id>" once inserted).
fn default_name(path: &Path) -> String {
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    if stem.eq_ignore_ascii_case("output") {
        String::new()
    } else {
        stem
    }
}

pub fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

/// Formats unix seconds as local `YYYY-MM-DD HH:MM`.
pub fn format_local(unix: i64) -> String {
    let local = unix + crate::logging::utc_offset_secs();
    let day = Day(local.div_euclid(86_400) as i32);
    let secs = local.rem_euclid(86_400);
    format!("{day} {:02}:{:02}", secs / 3600, secs / 60 % 60)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("hoi4_lib_{name}_{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn append(path: &Path, lines: &str) {
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .unwrap();
        file.write_all(lines.as_bytes()).unwrap();
    }

    #[test]
    fn saves_extends_and_deduplicates() {
        let dir = TempDir::new("dedupe");
        let mut library = Library::open(&dir.0.join("library.sqlite3")).unwrap();
        let file = dir.0.join("output.txt");

        append(
            &file,
            " 1:00, 1 Jan, 1936 GER MIL 1 1.5\n 1:00, 1 Jan, 1936 GER CIV 1 20.0\n",
        );
        let (first, _) = library.save_file(&file, "1.19.4").unwrap();
        assert!(matches!(first, SaveOutcome::Added(_)));

        let (same, _) = library.save_file(&file, "1.19.4").unwrap();
        assert_eq!(same, SaveOutcome::Unchanged(first.id()));

        // The campaign continues: same game, more data.
        append(&file, " 1:00, 2 Jan, 1936 GER MIL 1 2.5\n");
        let (grown, dataset) = library.save_file(&file, "ignored").unwrap();
        assert_eq!(grown, SaveOutcome::Updated(first.id()));
        assert_eq!(dataset.dates().len(), 2);

        // A new campaign written to the same path is a different game.
        std::fs::write(&file, " 1:00, 1 Jan, 1936 SOV MIL 1 9.0\n").unwrap();
        let (other, _) = library.save_file(&file, "1.19.4").unwrap();
        assert!(matches!(other, SaveOutcome::Added(id) if id != first.id()));

        let games = library.games().unwrap();
        assert_eq!(games.len(), 2);
        let game = games.iter().find(|g| g.id == first.id()).unwrap();
        assert_eq!(game.version, "1.19.4", "updates keep the recorded version");
        assert_eq!(game.days, 2);
        assert_eq!(game.countries, 1);
        assert!(game.has_civ);
        assert_eq!(game.first_day, Day::from_ymd(1936, 1, 1));
        assert_eq!(game.name, format!("Game #{}", first.id()));

        let stored = library.load(first.id()).unwrap();
        assert_eq!(stored.row(Metric::Mil, 0), [1.5, 2.5]);
        assert_eq!(stored.row(Metric::Civ, 0), [20.0, 0.0]);
    }

    #[test]
    fn concurrent_saves_store_one_game() {
        let dir = TempDir::new("race");
        let db = dir.0.join("library.sqlite3");
        Library::open(&db).unwrap();
        let file = dir.0.join("output.txt");
        let mut text = String::new();
        for day in 1..=28 {
            text.push_str(&format!(" 1:00, {day} Feb, 1936 GER MIL 1 {day}.5\n"));
        }
        append(&file, &text);

        let outcomes: Vec<SaveOutcome> = std::thread::scope(|scope| {
            let handles: Vec<_> = (0..4)
                .map(|_| {
                    scope.spawn(|| {
                        Library::open(&db)
                            .unwrap()
                            .save_file(&file, "1.19.4")
                            .unwrap()
                            .0
                    })
                })
                .collect();
            handles.into_iter().map(|h| h.join().unwrap()).collect()
        });
        let added = outcomes
            .iter()
            .filter(|o| matches!(o, SaveOutcome::Added(_)))
            .count();
        assert_eq!(added, 1, "{outcomes:?}");
        assert_eq!(Library::open(&db).unwrap().games().unwrap().len(), 1);
    }

    #[test]
    fn edits_versions_and_deletes() {
        let dir = TempDir::new("edit");
        let mut library = Library::open(&dir.0.join("library.sqlite3")).unwrap();
        for (i, version) in ["1.16.9", "1.19.4", ""].iter().enumerate() {
            let file = dir.0.join(format!("game{i}.txt"));
            append(&file, &format!(" 1:00, 1 Jan, 1936 GER MIL 1 {i}.5\n"));
            library.save_file(&file, version).unwrap();
        }
        assert_eq!(library.versions().unwrap(), ["1.19.4", "1.16.9"]);

        let id = library.games().unwrap()[0].id;
        library
            .update_details(id, "  My Germany run ", "1.17.0", "won 1941")
            .unwrap();
        let game = library
            .games()
            .unwrap()
            .into_iter()
            .find(|g| g.id == id)
            .unwrap();
        assert_eq!(game.name, "My Germany run");
        assert!(game.matches("germany") && game.matches("WON") && !game.matches("soviet"));

        library.delete(id).unwrap();
        assert!(matches!(library.load(id), Err(LibraryError::NotFound(_))));
        assert_eq!(library.games().unwrap().len(), 2);

        // Reopening keeps everything (schema is not recreated).
        drop(library);
        let library = Library::open(&dir.0.join("library.sqlite3")).unwrap();
        assert_eq!(library.games().unwrap().len(), 2);
    }

    #[test]
    fn summaries_export_and_import() {
        let dir = TempDir::new("export");
        let mut library = Library::open(&dir.0.join("a.sqlite3")).unwrap();
        let file = dir.0.join("output.txt");
        append(
            &file,
            " 1:00, 1 Jan, 1936 GER MIL 1 30.0\n 1:00, 1 Jan, 1936 ITA MIL 1 10.0\n\
             1:00, 2 Jan, 1936 GER MIL 1 20.0\n 1:00, 2 Jan, 1936 GER CIV 1 5.0\n",
        );
        let (outcome, _) = library.save_file(&file, "1.19.4").unwrap();
        let id = outcome.id();
        library
            .update_details(id, "Shared run", "1.19.4", "with friends")
            .unwrap();
        let game = &library.games().unwrap()[0];
        assert_eq!(game.summary.totals, [60.0, 0.0, 5.0]);
        let (top, share) = game.summary.top_producer.clone().unwrap();
        assert_eq!((top.as_str(), share.round()), ("GER", 83.0));
        assert_eq!(game.summary.peak_daily_mil, 40.0);

        let exported = dir.0.join("run.hoi4game");
        library.export_game(id, &exported).unwrap();

        let mut other = Library::open(&dir.0.join("b.sqlite3")).unwrap();
        let imported = other.import_game(&exported).unwrap();
        assert!(matches!(imported, SaveOutcome::Added(_)));
        assert_eq!(
            other.import_game(&exported).unwrap(),
            SaveOutcome::Unchanged(imported.id())
        );
        let game = &other.games().unwrap()[0];
        assert_eq!(
            (game.name.as_str(), game.notes.as_str()),
            ("Shared run", "with friends")
        );
        assert_eq!(game.days, 2);
        assert_eq!(
            other.load(imported.id()).unwrap(),
            library.load(id).unwrap()
        );

        assert!(matches!(
            other.import_game(&file),
            Err(LibraryError::Format(_))
        ));
    }

    #[test]
    fn migrates_version_1_libraries() {
        let dir = TempDir::new("migrate");
        let db = dir.0.join("library.sqlite3");
        {
            let mut library = Library::open(&db).unwrap();
            let file = dir.0.join("g.txt");
            append(&file, " 1:00, 1 Jan, 1936 GER MIL 1 3.0\n");
            library.save_file(&file, "").unwrap();
            // Turn it back into a version-1 library: no summary column.
            library
                .conn
                .execute_batch("ALTER TABLE games DROP COLUMN summary; PRAGMA user_version = 1;")
                .unwrap();
        }
        let library = Library::open(&db).unwrap();
        let game = &library.games().unwrap()[0];
        assert_eq!(game.summary.totals[0], 3.0, "summary backfilled");
    }

    #[test]
    fn formats_local_time() {
        let text = format_local(0);
        assert_eq!(text.len(), "1970-01-01 00:00".len());
    }
}
