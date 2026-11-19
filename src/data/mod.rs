//! Reading and aggregating DataCollection output files.

mod accumulator;
mod dataset;
pub mod date;
mod parser;
mod tag;

use std::{fs::File, io, path::Path, time::Instant};

pub use accumulator::{Accumulator, ParseStats};
pub use dataset::Dataset;
pub use date::Day;
pub use parser::{Record, parse_line};
pub use tag::Tag;

/// What a log line measures. Every metric is a daily value: several lines
/// for one country and date add up, and a running total is meaningful.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Metric {
    /// Military factory output (`MIL`).
    Mil = 0,
    /// Naval dockyard output (`NAV`).
    Nav = 1,
    /// Civilian factory output (`CIV`, newer DataCollection versions).
    Civ = 2,
}

impl Metric {
    pub const COUNT: usize = 3;
    pub const ALL: [Self; Self::COUNT] = [Self::Mil, Self::Nav, Self::Civ];

    /// "Military" / "Naval" / "Civilian"
    pub fn label(self) -> &'static str {
        match self {
            Self::Mil => "Military",
            Self::Nav => "Naval",
            Self::Civ => "Civilian",
        }
    }

    /// The token used in the log file.
    pub fn token(self) -> &'static str {
        match self {
            Self::Mil => "MIL",
            Self::Nav => "NAV",
            Self::Civ => "CIV",
        }
    }

    /// Default file name stem for exports.
    pub fn file_stem(self) -> &'static str {
        match self {
            Self::Mil => "military_ic",
            Self::Nav => "naval_ic",
            Self::Civ => "civilian_ic",
        }
    }
}

/// Loads and parses a whole file using a memory map and all CPU cores.
pub fn load_file(path: &Path) -> io::Result<Dataset> {
    load_file_with(path, |_| ()).map(|(dataset, ())| dataset)
}

/// Like [`load_file`], additionally running `inspect` over the raw bytes while
/// they are mapped, in parallel with parsing (used to fingerprint the file).
pub fn load_file_with<T: Send>(
    path: &Path,
    inspect: impl FnOnce(&[u8]) -> T + Send,
) -> io::Result<(Dataset, T)> {
    let started = Instant::now();
    let file = File::open(path)?;
    let len = file.metadata()?.len();

    let mut acc = Accumulator::new();
    let inspected = if len > 0 {
        // SAFETY: the map is read-only and dropped before returning. If the game
        // appends while we read, we simply see a prefix of the file; a concurrent
        // truncation is the only hazard and the game never truncates its log.
        let map = unsafe { memmap2::Mmap::map(&file)? };
        rayon::join(|| acc.ingest(&map), || inspect(&map)).1
    } else {
        inspect(&[])
    };
    let dataset = acc.to_dataset();

    let stats = dataset.stats();
    log::info!(
        "Parsed {} ({:.1} MiB) in {:.1} ms: {} lines, {} valid, {} errors, {} dates, {} countries",
        path.display(),
        len as f64 / (1024.0 * 1024.0),
        started.elapsed().as_secs_f64() * 1000.0,
        stats.lines,
        stats.valid,
        stats.errors(),
        dataset.dates().len(),
        dataset.tags().len(),
    );
    Ok((dataset, inspected))
}
