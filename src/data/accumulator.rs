//! Aggregation of parsed records into per-(country, day) sums.
//!
//! Storage is a grid: one row per country, one column per distinct date, both
//! assigned in order of first appearance through small lookup maps (a game has
//! ~100 countries and a few thousand days, so the maps stay cache-resident).
//! Log lines arrive grouped by date, so a one-entry cache resolves the column
//! for almost every line.
//!
//! Large inputs are split at line boundaries and parsed on all cores; worker
//! grids are merged by remapping rows/columns. The same type backs incremental
//! (live) parsing, where only newly appended bytes are fed in.

use rayon::prelude::*;
use rustc_hash::FxHashMap;

use super::{Dataset, Day, Metric, Tag, parser};

/// Below this size, splitting work across threads costs more than it saves.
const PARALLEL_THRESHOLD: usize = 1 << 20;

/// One daily sum per [`Metric`].
type Cell = [f64; Metric::COUNT];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ParseStats {
    /// Non-blank lines seen.
    pub lines: u64,
    /// Lines matching the record format.
    pub valid: u64,
}

impl ParseStats {
    pub fn errors(&self) -> u64 {
        self.lines - self.valid
    }
}

impl std::ops::AddAssign for ParseStats {
    fn add_assign(&mut self, rhs: Self) {
        self.lines += rhs.lines;
        self.valid += rhs.valid;
    }
}

#[derive(Clone, Debug)]
pub struct Accumulator {
    /// Country of each row, in order of first appearance.
    tags: Vec<Tag>,
    row_of: FxHashMap<Tag, u32>,
    /// Date of each column, in order of first appearance.
    days: Vec<Day>,
    col_of: FxHashMap<Day, u32>,
    last_col: (Day, u32),
    /// `cells[row][col]`, indexed by [`Metric`]; rows grow lazily up to the last column written.
    cells: Vec<Vec<Cell>>,
    stats: ParseStats,
}

impl Default for Accumulator {
    fn default() -> Self {
        Self {
            tags: Vec::new(),
            row_of: FxHashMap::default(),
            days: Vec::new(),
            col_of: FxHashMap::default(),
            last_col: (Day(i32::MIN), u32::MAX),
            cells: Vec::new(),
            stats: ParseStats::default(),
        }
    }
}

impl Accumulator {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn stats(&self) -> ParseStats {
        self.stats
    }

    /// Parses `bytes`, which must consist of whole lines (a trailing line
    /// without newline is treated as complete).
    pub fn ingest(&mut self, bytes: &[u8]) {
        if bytes.len() < PARALLEL_THRESHOLD {
            self.ingest_sequential(bytes);
        } else {
            let other = Self::parse_parallel(bytes);
            self.merge(other);
        }
    }

    fn ingest_sequential(&mut self, bytes: &[u8]) {
        let mut start = 0;
        for end in memchr::memchr_iter(b'\n', bytes) {
            self.line(&bytes[start..end]);
            start = end + 1;
        }
        if start < bytes.len() {
            self.line(&bytes[start..]);
        }
    }

    #[inline]
    fn line(&mut self, line: &[u8]) {
        match parser::parse_line(line) {
            Some(record) => {
                self.stats.lines += 1;
                self.stats.valid += 1;
                let row = self.row(record.tag);
                let col = self.col(record.day);
                if let Some(metric) = record.metric {
                    self.cell(row, col)[metric as usize] += record.value;
                }
            }
            None => {
                if !parser::is_blank(line) {
                    self.stats.lines += 1;
                }
            }
        }
    }

    #[inline]
    fn row(&mut self, tag: Tag) -> usize {
        let next = self.tags.len() as u32;
        let row = *self.row_of.entry(tag).or_insert(next);
        if row == next {
            self.tags.push(tag);
            self.cells.push(Vec::new());
        }
        row as usize
    }

    #[inline]
    fn col(&mut self, day: Day) -> usize {
        if self.last_col.0 == day {
            return self.last_col.1 as usize;
        }
        let next = self.days.len() as u32;
        let col = *self.col_of.entry(day).or_insert(next);
        if col == next {
            self.days.push(day);
        }
        self.last_col = (day, col);
        col as usize
    }

    #[inline]
    fn cell(&mut self, row: usize, col: usize) -> &mut Cell {
        let cells = &mut self.cells[row];
        if cells.len() <= col {
            cells.resize(col + 1, [0.0; Metric::COUNT]);
        }
        &mut cells[col]
    }

    fn parse_parallel(bytes: &[u8]) -> Self {
        let chunk_target =
            (bytes.len() / (rayon::current_num_threads() * 4)).max(PARALLEL_THRESHOLD / 4);
        split_lines(bytes, chunk_target)
            .into_par_iter()
            .map(|chunk| {
                let mut acc = Self::new();
                acc.ingest_sequential(chunk);
                acc
            })
            .reduce(Self::new, |mut a, b| {
                a.merge(b);
                a
            })
    }

    pub fn merge(&mut self, other: Self) {
        self.stats += other.stats;
        let col_map: Vec<usize> = other.days.iter().map(|&day| self.col(day)).collect();
        for (tag, cells) in other.tags.into_iter().zip(other.cells) {
            let row = self.row(tag);
            for (col, values) in cells.into_iter().enumerate() {
                if values.iter().any(|&v| v != 0.0) {
                    let cell = self.cell(row, col_map[col]);
                    for (sum, value) in cell.iter_mut().zip(values) {
                        *sum += value;
                    }
                }
            }
        }
    }

    /// Builds the dense, sorted, analysis-ready representation.
    pub fn to_dataset(&self) -> Dataset {
        let col_order = sorted_order(&self.days);
        let row_order = sorted_order(&self.tags);
        let mut col_rank = vec![0usize; self.days.len()];
        for (rank, &col) in col_order.iter().enumerate() {
            col_rank[col] = rank;
        }

        let width = self.days.len();
        let mut values: [Vec<f64>; Metric::COUNT] =
            std::array::from_fn(|_| vec![0.0; self.tags.len() * width]);
        for (rank, &row) in row_order.iter().enumerate() {
            let base = rank * width;
            for (col, cell) in self.cells[row].iter().enumerate() {
                for (matrix, &value) in values.iter_mut().zip(cell) {
                    matrix[base + col_rank[col]] = value;
                }
            }
        }
        Dataset::from_parts(
            col_order.iter().map(|&c| self.days[c]).collect(),
            row_order.iter().map(|&r| self.tags[r]).collect(),
            values,
            self.stats,
        )
        .expect("accumulator axes are unique and its matrices match them")
    }
}

/// Indices that would sort `items`.
fn sorted_order<T: Ord>(items: &[T]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..items.len()).collect();
    order.sort_unstable_by(|&a, &b| items[a].cmp(&items[b]));
    order
}

/// Splits `bytes` into chunks of roughly `target` bytes, each ending on a newline.
fn split_lines(bytes: &[u8], target: usize) -> Vec<&[u8]> {
    let mut chunks = Vec::with_capacity(bytes.len() / target + 1);
    let mut rest = bytes;
    while rest.len() > target {
        let cut = match memchr::memchr(b'\n', &rest[target..]) {
            Some(i) => target + i + 1,
            None => rest.len(),
        };
        let (head, tail) = rest.split_at(cut);
        chunks.push(head);
        rest = tail;
    }
    if !rest.is_empty() {
        chunks.push(rest);
    }
    chunks
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(lines: usize) -> String {
        let tags = ["GER", "SOV", "USA", "ENG", "ITA", "JAP"];
        let mut s = String::new();
        for i in 0..lines {
            let day = 1 + (i / tags.len()) % 28;
            let month = ["January", "Feb", "March"][(i / (tags.len() * 28)) % 3];
            let tag = tags[i % tags.len()];
            let kind = ["MIL", "NAV", "CIV"][i % 3];
            s.push_str(&format!(
                "DLL: 12:00, {day} {month}, 1936 {tag} {kind} 1 {}.25\n",
                i % 7
            ));
            if i % 100 == 0 {
                s.push_str("some unrelated log line\n\n");
            }
        }
        s
    }

    fn assert_same(a: &Dataset, b: &Dataset) {
        assert_eq!(a.stats(), b.stats());
        assert_eq!(a.dates(), b.dates());
        assert_eq!(a.tags(), b.tags());
        for metric in Metric::ALL {
            for (x, y) in a.matrix(metric).iter().zip(b.matrix(metric)) {
                assert!((x - y).abs() < 1e-6, "{metric:?}: {x} != {y}");
            }
        }
    }

    #[test]
    fn parallel_matches_sequential() {
        let input = sample(200_000);
        assert!(input.len() > PARALLEL_THRESHOLD);

        let mut seq = Accumulator::new();
        seq.ingest_sequential(input.as_bytes());
        let mut par = Accumulator::new();
        par.ingest(input.as_bytes());

        assert_eq!(seq.stats.errors(), 2000);
        let seq = seq.to_dataset();
        assert!(Metric::ALL.iter().all(|&m| seq.has_metric(m)));
        assert_same(&seq, &par.to_dataset());
    }

    #[test]
    fn incremental_matches_whole() {
        let input = sample(5_000);
        let mut whole = Accumulator::new();
        whole.ingest(input.as_bytes());

        let mut parts = Accumulator::new();
        for chunk in split_lines(input.as_bytes(), 997) {
            parts.ingest(chunk);
        }
        assert_same(&whole.to_dataset(), &parts.to_dataset());
    }

    #[test]
    fn dates_include_non_ic_records() {
        let mut acc = Accumulator::new();
        acc.ingest(b"1:00, 2 Jan, 1936 GER AIR 1 1.0\n1:00, 1 Jan, 1936 ITA MIL 1 2.0\n");
        let ds = acc.to_dataset();
        assert_eq!(ds.dates().len(), 2);
        assert_eq!(ds.tags().len(), 2);
        assert_eq!(
            ds.row(Metric::Mil, ds.tag_index(Tag::from_static("ITA")).unwrap()),
            [2.0, 0.0]
        );
    }

    #[test]
    fn split_lines_respects_boundaries() {
        let input = b"aaaa\nbbbb\ncccc\ndd";
        let chunks = split_lines(input, 3);
        assert_eq!(chunks.concat(), input);
        assert!(
            chunks[..chunks.len() - 1]
                .iter()
                .all(|c| c.ends_with(b"\n"))
        );
    }
}
