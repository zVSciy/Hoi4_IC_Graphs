//! Calendar dates as plain integers.
//!
//! A [`Day`] is the number of days since 1970-01-01 in the proleptic Gregorian
//! calendar. Dates are parsed once and then compared, hashed, sorted and plotted
//! as integers — no string keys, no repeated `strptime`.

use std::fmt;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub struct Day(pub i32);

const MONTHS: [&str; 12] = [
    "january",
    "february",
    "march",
    "april",
    "may",
    "june",
    "july",
    "august",
    "september",
    "october",
    "november",
    "december",
];

impl Day {
    /// Builds a validated date, rejecting impossible days such as 30 February.
    pub fn from_ymd(year: i32, month: u32, day: u32) -> Option<Self> {
        if !(1..=12).contains(&month) || day == 0 || day > days_in_month(year, month) {
            return None;
        }
        Some(Self(days_from_civil(year, month, day)))
    }

    pub fn ymd(self) -> (i32, u32, u32) {
        civil_from_days(self.0)
    }

    /// Value used for the plot x axis.
    #[inline]
    pub fn as_f64(self) -> f64 {
        f64::from(self.0)
    }

    /// Nearest day for a plot x coordinate.
    #[inline]
    pub fn from_f64(x: f64) -> Self {
        Self(x.round().clamp(i32::MIN as f64, i32::MAX as f64) as i32)
    }
}

/// Formats as ISO 8601 (`YYYY-MM-DD`).
impl fmt::Display for Day {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (y, m, d) = self.ymd();
        write!(f, "{y:04}-{m:02}-{d:02}")
    }
}

/// Parses an English month name, either in full or as a 3-letter abbreviation
/// (case-insensitive), mirroring `strptime`'s `%B` / `%b`.
pub fn parse_month(name: &[u8]) -> Option<u32> {
    if name.len() < 3 {
        return None;
    }
    MONTHS
        .iter()
        .position(|full| {
            let full = full.as_bytes();
            (name.len() == 3 || name.len() == full.len())
                && name.eq_ignore_ascii_case(&full[..name.len()])
        })
        .map(|i| i as u32 + 1)
}

/// "Jan" .. "Dec" for months 1..=12.
pub fn month_abbrev(month: u32) -> &'static str {
    const ABBREV: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    ABBREV
        .get(month.wrapping_sub(1) as usize)
        .copied()
        .unwrap_or("?")
}

pub fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

pub fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => 0,
    }
}

// Howard Hinnant's algorithms: http://howardhinnant.github.io/date_algorithms.html
fn days_from_civil(year: i32, month: u32, day: u32) -> i32 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (month as i32 + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day as i32 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn civil_from_days(days: i32) -> (i32, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = yoe + era * 400 + i32::from(month <= 2);
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_civil() {
        for days in -800_000..800_000 {
            let (y, m, d) = civil_from_days(days);
            assert_eq!(Day::from_ymd(y, m, d), Some(Day(days)));
        }
    }

    #[test]
    fn known_dates() {
        assert_eq!(Day::from_ymd(1970, 1, 1), Some(Day(0)));
        assert_eq!(Day::from_ymd(1936, 1, 1).unwrap().to_string(), "1936-01-01");
        assert_eq!(Day::from_ymd(1936, 2, 30), None);
        assert_eq!(Day::from_ymd(1937, 2, 29), None);
        assert!(Day::from_ymd(1936, 2, 29).is_some());
    }

    #[test]
    fn month_names() {
        assert_eq!(parse_month(b"January"), Some(1));
        assert_eq!(parse_month(b"jan"), Some(1));
        assert_eq!(parse_month(b"SEPTEMBER"), Some(9));
        assert_eq!(parse_month(b"Sep"), Some(9));
        assert_eq!(parse_month(b"Sept"), None);
        assert_eq!(parse_month(b"Ja"), None);
        assert_eq!(parse_month(b"Janu"), None);
    }
}
