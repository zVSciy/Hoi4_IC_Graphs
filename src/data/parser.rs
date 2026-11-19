//! Hand-written parser for the DataCollection output format.
//!
//! Accepts exactly what the original regular expression accepted:
//!
//! ```text
//! [DLL: ]HH:MM, <day> <Month>, <YYYY> <TAG> <TYPE> <units> <ic.frac>
//! ```
//!
//! e.g. `DLL: 12:00, 1 January, 1936 GER MIL 12 34.50`. `<TYPE>` is `MIL`, `NAV`
//! or (newer DataCollection versions) `CIV`; other types are valid but not
//! charted. Lines without a country tag (`1936  MIL 1 4.74`) are rejected.
//! No allocations, no regex engine; every line is a single forward scan.

use super::{Day, Metric, Tag, date::parse_month};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Record {
    pub day: Day,
    pub tag: Tag,
    /// `None` for well-formed lines of a type this tool does not chart.
    pub metric: Option<Metric>,
    pub value: f64,
}

struct Cursor<'a> {
    bytes: &'a [u8],
    pos: usize,
}

#[inline]
fn is_space(b: u8) -> bool {
    // Python's `\s` for ASCII: space, \t, \n, \v, \f, \r
    matches!(b, b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r')
}

impl<'a> Cursor<'a> {
    #[inline]
    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    #[inline]
    fn skip_spaces(&mut self) -> usize {
        let start = self.pos;
        while self.peek().is_some_and(is_space) {
            self.pos += 1;
        }
        self.pos - start
    }

    /// `\s+`
    #[inline]
    fn spaces1(&mut self) -> Option<()> {
        (self.skip_spaces() > 0).then_some(())
    }

    #[inline]
    fn byte(&mut self, expected: u8) -> Option<()> {
        (self.peek()? == expected).then(|| self.pos += 1)
    }

    #[inline]
    fn take_while(&mut self, pred: impl Fn(u8) -> bool) -> &'a [u8] {
        let start = self.pos;
        while self.peek().is_some_and(&pred) {
            self.pos += 1;
        }
        &self.bytes[start..self.pos]
    }

    /// `\d+` parsed as an integer (saturating, values here are tiny).
    #[inline]
    fn number(&mut self) -> Option<u32> {
        let digits = self.take_while(|b| b.is_ascii_digit());
        if digits.is_empty() {
            return None;
        }
        Some(digits.iter().fold(0u32, |acc, &d| {
            acc.saturating_mul(10).saturating_add(u32::from(d - b'0'))
        }))
    }
}

/// Parses one line (without its trailing newline).
pub fn parse_line(line: &[u8]) -> Option<Record> {
    let mut c = Cursor {
        bytes: line,
        pos: 0,
    };
    c.skip_spaces();
    if line[c.pos..].starts_with(b"DLL: ") {
        c.pos += 5;
        c.skip_spaces();
    }

    // HH:MM,
    c.number()?;
    c.byte(b':')?;
    c.number()?;
    c.byte(b',')?;
    c.spaces1()?;

    // <day> <Month>, <YYYY>
    let day = c.number()?;
    c.spaces1()?;
    let month = parse_month(c.take_while(|b| b.is_ascii_alphabetic()))?;
    c.byte(b',')?;
    c.spaces1()?;
    let year_digits = c.take_while(|b| b.is_ascii_digit());
    if year_digits.len() != 4 {
        return None;
    }
    let year = year_digits
        .iter()
        .fold(0i32, |acc, &d| acc * 10 + i32::from(d - b'0'));
    c.spaces1()?;
    let day = Day::from_ymd(year, month, day)?;

    // <TAG> <TYPE>
    let tag = Tag::from_bytes(c.take_while(|b| b.is_ascii_uppercase()))?;
    c.spaces1()?;
    let metric = match c.take_while(|b| b.is_ascii_uppercase()) {
        b"" => return None,
        b"MIL" => Some(Metric::Mil),
        b"NAV" => Some(Metric::Nav),
        b"CIV" => Some(Metric::Civ),
        _ => None,
    };
    c.spaces1()?;

    // <units> <ic.frac>
    c.number()?;
    c.spaces1()?;
    let ic_start = c.pos;
    let int_digits = c.take_while(|b| b.is_ascii_digit());
    if int_digits.is_empty() {
        return None;
    }
    c.byte(b'.')?;
    let frac_digits = c.take_while(|b| b.is_ascii_digit());
    if frac_digits.is_empty() {
        return None;
    }
    let value = parse_decimal(int_digits, frac_digits, &line[ic_start..c.pos])?;

    Some(Record {
        day,
        tag,
        metric,
        value,
    })
}

/// Exact powers of ten representable in an `f64`.
const POW10: [f64; 23] = [
    1e0, 1e1, 1e2, 1e3, 1e4, 1e5, 1e6, 1e7, 1e8, 1e9, 1e10, 1e11, 1e12, 1e13, 1e14, 1e15, 1e16,
    1e17, 1e18, 1e19, 1e20, 1e21, 1e22,
];

/// `int.frac` as `f64`. When the digits fit in a 53-bit mantissa, `mantissa / 10^k`
/// is a single correctly rounded IEEE division of two exact values, so the result
/// is bit-identical to `str::parse` (Clinger's fast path). Otherwise falls back.
#[inline]
fn parse_decimal(int_digits: &[u8], frac_digits: &[u8], text: &[u8]) -> Option<f64> {
    if int_digits.len() + frac_digits.len() <= 15 {
        let mantissa = int_digits
            .iter()
            .chain(frac_digits)
            .fold(0u64, |acc, &d| acc * 10 + u64::from(d - b'0'));
        return Some(mantissa as f64 / POW10[frac_digits.len()]);
    }
    // ASCII digits and a dot: always valid UTF-8.
    std::str::from_utf8(text).ok()?.parse().ok()
}

/// Returns `true` for lines the original tool ignored entirely (whitespace only).
#[inline]
pub fn is_blank(line: &[u8]) -> bool {
    line.iter().all(|&b| is_space(b))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(y: i32, m: u32, d: u32) -> Day {
        Day::from_ymd(y, m, d).unwrap()
    }

    #[test]
    fn parses_dll_prefixed_line() {
        let r = parse_line(b"DLL: 12:00, 1 January, 1936 GER MIL 12 34.50").unwrap();
        assert_eq!(r.day, day(1936, 1, 1));
        assert_eq!(&*r.tag.as_str(), "GER");
        assert_eq!(r.metric, Some(Metric::Mil));
        assert_eq!(r.value, 34.5);
    }

    #[test]
    fn parses_plain_and_abbreviated_lines() {
        let r = parse_line(b"  0:00,  15 Mar, 1940\tSOV NAV 3 0.25  trailing").unwrap();
        assert_eq!(r.day, day(1940, 3, 15));
        assert_eq!(r.metric, Some(Metric::Nav));
        assert_eq!(r.value, 0.25);
    }

    #[test]
    fn other_types_are_valid_but_unclassified() {
        let r = parse_line(b"1:00, 2 May, 1941 ENG AIR 1 2.0").unwrap();
        assert_eq!(r.metric, None);
    }

    #[test]
    fn parses_civilian_lines() {
        // Verbatim from a DataCollection 1.19.x output file, trailing spaces included.
        let r = parse_line(b" 1:00, 17 Sep, 1937 SOV CIV 1 623.700000     ").unwrap();
        assert_eq!(r.day, day(1937, 9, 17));
        assert_eq!(&*r.tag.as_str(), "SOV");
        assert_eq!(r.metric, Some(Metric::Civ));
        assert_eq!(r.value, 623.7);
    }

    #[test]
    fn rejects_lines_without_country_tag() {
        for line in [
            &b" 1:00, 17 Sep, 1937  MIL 1 4.742690     "[..],
            b" 1:00, 17 Sep, 1937  NAV 1 5.174000     ",
            b" 1:00, 17 Sep, 1937  CIV 1 32.200000     ",
        ] {
            assert!(
                parse_line(line).is_none(),
                "{}",
                String::from_utf8_lossy(line)
            );
        }
    }

    #[test]
    fn fast_decimal_is_bit_exact() {
        let mut state = 0x9e37_79b9_7f4a_7c15u64;
        for _ in 0..200_000 {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            let int_len = 1 + (state % 9) as usize;
            let frac_len = 1 + ((state >> 8) % 9) as usize;
            let digits = format!("{:018}", state % 1_000_000_000_000_000_000);
            let text = format!(
                "{}.{}",
                &digits[..int_len],
                &digits[int_len..int_len + frac_len]
            );
            let (int, frac) = text.split_once('.').unwrap();
            let fast = parse_decimal(int.as_bytes(), frac.as_bytes(), text.as_bytes()).unwrap();
            assert_eq!(
                fast.to_bits(),
                text.parse::<f64>().unwrap().to_bits(),
                "{text}"
            );
        }
        // Too many digits for the fast path still parses correctly.
        let long = "123456789.123456789";
        let (int, frac) = long.split_once('.').unwrap();
        assert_eq!(
            parse_decimal(int.as_bytes(), frac.as_bytes(), long.as_bytes()),
            long.parse().ok()
        );
    }

    #[test]
    fn rejects_malformed_lines() {
        for line in [
            &b""[..],
            b"garbage",
            b"12:00, 1 January, 36 GER MIL 12 34.50", // 2-digit year
            b"12:00, 1 January, 19366 GER MIL 12 34.50", // 5-digit year
            b"12:00, 1 January 1936 GER MIL 12 34.50", // missing comma
            b"12:00, 31 February, 1936 GER MIL 12 34.50", // impossible date
            b"12:00, 1 Janvier, 1936 GER MIL 12 34.50", // unknown month
            b"12:00, 1 January, 1936 ger MIL 12 34.50", // lowercase tag
            b"12:00, 1 January, 1936 D01 MIL 12 34.50", // digits in tag
            b"12:00, 1 January, 1936 GER MIL 12 34",  // integer IC
            b"12:00, 1 January, 1936 GER MIL 12 .5",  // no integer part
        ] {
            assert!(
                parse_line(line).is_none(),
                "{:?}",
                String::from_utf8_lossy(line)
            );
        }
    }
}
