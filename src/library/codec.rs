//! Compact binary form of a [`Dataset`] for the game library.
//!
//! Layout (little endian), zstd-compressed as a whole:
//!
//! ```text
//! magic "HOI4DS\0" + format byte
//! u32 dates, u32 tags, u32 metrics
//! u64 lines, u64 valid
//! i32 × dates     first date, then day-to-day deltas (almost all 1)
//! u64 × tags      packed country tags
//! f64 × metrics × tags × dates, byte-planar
//! ```
//!
//! Before compression every value is XORed with the previous day's value of
//! the same country (identical days become zero) and the bytes are split into
//! eight planes (sign/exponent bytes of neighbouring values end up adjacent).
//! Both steps are exactly reversible, so decoding is bit-identical, and zstd
//! then shrinks a typical campaign several-fold.

use std::fmt;

use crate::data::{Dataset, Day, Metric, ParseStats, Tag};

const MAGIC: &[u8; 7] = b"HOI4DS\0";
const FORMAT: u8 = 1;
const HEADER_LEN: usize = 8 + 3 * 4 + 2 * 8;
const ZSTD_LEVEL: i32 = 3;

#[derive(Debug)]
pub enum CodecError {
    Corrupt(&'static str),
    UnsupportedFormat(u8),
    Zstd(std::io::Error),
}

impl fmt::Display for CodecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Corrupt(what) => write!(f, "stored game data is corrupt ({what})"),
            Self::UnsupportedFormat(v) => {
                write!(
                    f,
                    "stored game data uses format {v}, update the app to read it"
                )
            }
            Self::Zstd(err) => write!(f, "stored game data could not be decompressed: {err}"),
        }
    }
}

impl std::error::Error for CodecError {}

pub fn encode(dataset: &Dataset) -> Vec<u8> {
    let dates = dataset.dates();
    let tags = dataset.tags();
    let width = dates.len();
    let values_len = Metric::COUNT * tags.len() * width;
    let mut raw =
        Vec::with_capacity(HEADER_LEN + dates.len() * 4 + tags.len() * 8 + values_len * 8);

    raw.extend_from_slice(MAGIC);
    raw.push(FORMAT);
    for n in [dates.len(), tags.len(), Metric::COUNT] {
        raw.extend_from_slice(&(n as u32).to_le_bytes());
    }
    let stats = dataset.stats();
    raw.extend_from_slice(&stats.lines.to_le_bytes());
    raw.extend_from_slice(&stats.valid.to_le_bytes());

    let mut previous = 0i32;
    for day in dates {
        raw.extend_from_slice(&day.0.wrapping_sub(previous).to_le_bytes());
        previous = day.0;
    }
    for tag in tags {
        raw.extend_from_slice(&tag.to_bits().to_le_bytes());
    }

    // XOR against the previous day of the same row, then split into byte planes.
    let mut words = Vec::with_capacity(values_len);
    for metric in Metric::ALL {
        for row in dataset.matrix(metric).chunks(width.max(1)) {
            let mut previous = 0u64;
            for value in row {
                let bits = value.to_bits();
                words.push(bits ^ previous);
                previous = bits;
            }
        }
    }
    let planes_start = raw.len();
    raw.resize(planes_start + words.len() * 8, 0);
    let planes = &mut raw[planes_start..];
    for (i, word) in words.iter().enumerate() {
        for (plane, byte) in word.to_le_bytes().into_iter().enumerate() {
            planes[plane * words.len() + i] = byte;
        }
    }

    zstd::bulk::compress(&raw, ZSTD_LEVEL).expect("compressing to memory cannot fail")
}

pub fn decode(bytes: &[u8]) -> Result<Dataset, CodecError> {
    let raw = zstd::decode_all(bytes).map_err(CodecError::Zstd)?;
    let mut reader = Reader {
        bytes: &raw,
        pos: 0,
    };

    if reader.take(MAGIC.len())? != MAGIC {
        return Err(CodecError::Corrupt("bad magic"));
    }
    let format = reader.take(1)?[0];
    if format != FORMAT {
        return Err(CodecError::UnsupportedFormat(format));
    }
    let n_dates = reader.u32()? as usize;
    let n_tags = reader.u32()? as usize;
    let n_metrics = reader.u32()? as usize;
    let stats = ParseStats {
        lines: reader.u64()?,
        valid: reader.u64()?,
    };

    let mut dates = Vec::with_capacity(n_dates);
    let mut day = 0i32;
    for _ in 0..n_dates {
        day = day.wrapping_add(reader.i32()?);
        dates.push(Day(day));
    }
    let mut tags = Vec::with_capacity(n_tags);
    for _ in 0..n_tags {
        tags.push(Tag::from_bits(reader.u64()?).ok_or(CodecError::Corrupt("bad country tag"))?);
    }

    let cells = n_tags
        .checked_mul(n_dates)
        .ok_or(CodecError::Corrupt("size overflow"))?;
    let count = cells
        .checked_mul(n_metrics)
        .ok_or(CodecError::Corrupt("size overflow"))?;
    let planes = reader.take(
        count
            .checked_mul(8)
            .ok_or(CodecError::Corrupt("size overflow"))?,
    )?;
    if reader.pos != raw.len() {
        return Err(CodecError::Corrupt("trailing bytes"));
    }

    // Metrics this build does not know are skipped; missing ones stay zero
    // (a library written by an older version has no CIV matrix).
    let mut values: [Vec<f64>; Metric::COUNT] = std::array::from_fn(|_| vec![0.0; cells]);
    for (metric, matrix) in values.iter_mut().enumerate().take(n_metrics) {
        let base = metric * cells;
        for (row, out) in matrix.chunks_mut(n_dates.max(1)).enumerate() {
            let mut previous = 0u64;
            for (col, value) in out.iter_mut().enumerate() {
                let i = base + row * n_dates + col;
                let mut word = [0u8; 8];
                for (plane, byte) in word.iter_mut().enumerate() {
                    *byte = planes[plane * count + i];
                }
                let bits = u64::from_le_bytes(word) ^ previous;
                *value = f64::from_bits(bits);
                previous = bits;
            }
        }
    }

    Dataset::from_parts(dates, tags, values, stats).ok_or(CodecError::Corrupt("unsorted axes"))
}

struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], CodecError> {
        let end = self
            .pos
            .checked_add(n)
            .filter(|&end| end <= self.bytes.len())
            .ok_or(CodecError::Corrupt("truncated"))?;
        let slice = &self.bytes[self.pos..end];
        self.pos = end;
        Ok(slice)
    }

    fn u32(&mut self) -> Result<u32, CodecError> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }

    fn i32(&mut self) -> Result<i32, CodecError> {
        Ok(i32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }

    fn u64(&mut self) -> Result<u64, CodecError> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::Accumulator;

    fn campaign() -> Dataset {
        let mut text = String::new();
        let mut state = 0x2545_f491_4f6c_dd1du64;
        for day in 0..400 {
            let date = Day(Day::from_ymd(1936, 1, 1).unwrap().0 + day);
            let (y, m, d) = date.ymd();
            let month = crate::data::date::month_abbrev(m);
            for tag in ["GER", "SOV", "USA", "LUX"] {
                for kind in ["MIL", "NAV", "CIV"] {
                    state ^= state << 13;
                    state ^= state >> 7;
                    state ^= state << 17;
                    // Like real output: MIL changes a little every day, NAV and
                    // CIV only when factories are built or lost.
                    let value = if kind == "MIL" {
                        format!("{}.{:06}", 100 + day / 3, state % 1_000_000)
                    } else {
                        format!("{}.{:06}", 50 + day / 45, (day / 45) * 125_000 % 1_000_000)
                    };
                    text.push_str(&format!(" 1:00, {d} {month}, {y} {tag} {kind} 1 {value}\n"));
                }
            }
        }
        let mut acc = Accumulator::new();
        acc.ingest(text.as_bytes());
        acc.to_dataset()
    }

    #[test]
    fn round_trip_is_bit_exact() {
        let dataset = campaign();
        let encoded = encode(&dataset);
        let decoded = decode(&encoded).unwrap();
        assert_eq!(decoded.dates(), dataset.dates());
        assert_eq!(decoded.tags(), dataset.tags());
        assert_eq!(decoded.stats(), dataset.stats());
        for metric in Metric::ALL {
            let bits = |d: &Dataset| {
                d.matrix(metric)
                    .iter()
                    .map(|v| v.to_bits())
                    .collect::<Vec<_>>()
            };
            assert_eq!(bits(&decoded), bits(&dataset), "{metric:?}");
        }
        let raw_size = dataset.dates().len() * dataset.tags().len() * Metric::COUNT * 8;
        assert!(
            encoded.len() < raw_size / 2,
            "{} vs {raw_size}",
            encoded.len()
        );
    }

    #[test]
    fn empty_dataset_round_trips() {
        let empty = Accumulator::new().to_dataset();
        assert!(decode(&encode(&empty)).unwrap().is_empty());
    }

    #[test]
    fn rejects_corrupt_data() {
        let encoded = encode(&campaign());
        assert!(decode(&encoded[..encoded.len() / 2]).is_err());
        assert!(decode(b"not zstd at all").is_err());
        let mut raw = zstd::decode_all(encoded.as_slice()).unwrap();
        raw.truncate(raw.len() - 1);
        let truncated = zstd::bulk::compress(&raw, 1).unwrap();
        assert!(matches!(decode(&truncated), Err(CodecError::Corrupt(_))));
    }
}
