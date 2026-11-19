//! Logger that writes to stderr and keeps recent lines for the in-app log viewer.

use std::{
    collections::VecDeque,
    io::Write,
    sync::{Mutex, MutexGuard, OnceLock},
};

use log::{LevelFilter, Log, Metadata, Record};

const CAPACITY: usize = 10_000;

pub struct AppLogger {
    lines: Mutex<VecDeque<String>>,
}

static LOGGER: OnceLock<AppLogger> = OnceLock::new();

/// Installs the global logger. Safe to call more than once.
pub fn init() {
    let logger = LOGGER.get_or_init(|| AppLogger {
        lines: Mutex::new(VecDeque::with_capacity(1024)),
    });
    if log::set_logger(logger).is_ok() {
        log::set_max_level(LevelFilter::Info);
    }
}

/// Locks and returns the buffered log lines, oldest first.
pub fn lines() -> Option<MutexGuard<'static, VecDeque<String>>> {
    LOGGER
        .get()
        .map(|l| l.lines.lock().unwrap_or_else(|e| e.into_inner()))
}

impl Log for AppLogger {
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        // Our own messages at Info, dependencies only when something is wrong.
        // (Matches both the `hoi4_analyzer` library and the `HOI4_Analyzer` binary.)
        const OURS: &str = "hoi4_analyzer";
        let target = metadata.target().as_bytes();
        let ours = target.len() >= OURS.len()
            && target[..OURS.len()].eq_ignore_ascii_case(OURS.as_bytes());
        metadata.level()
            <= if ours {
                LevelFilter::Info
            } else {
                LevelFilter::Warn
            }
    }

    fn log(&self, record: &Record<'_>) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let line = format!("{} - {} - {}", clock(), record.level(), record.args());
        let _ = writeln!(std::io::stderr(), "{line}");
        let mut lines = self.lines.lock().unwrap_or_else(|e| e.into_inner());
        if lines.len() == CAPACITY {
            lines.pop_front();
        }
        lines.push_back(line);
    }

    fn flush(&self) {}
}

/// Local wall-clock time as `HH:MM:SS`.
pub fn clock() -> String {
    #[cfg(windows)]
    {
        // SAFETY: GetLocalTime only writes to the provided struct.
        let t = unsafe {
            let mut t = std::mem::zeroed();
            windows_sys::Win32::System::SystemInformation::GetLocalTime(&mut t);
            t
        };
        format!("{:02}:{:02}:{:02}", t.wHour, t.wMinute, t.wSecond)
    }
    #[cfg(not(windows))]
    {
        let secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        format!(
            "{:02}:{:02}:{:02} UTC",
            secs / 3600 % 24,
            secs / 60 % 60,
            secs % 60
        )
    }
}

/// Current offset of local time from UTC in seconds (0 if unknown).
pub fn utc_offset_secs() -> i64 {
    #[cfg(windows)]
    {
        use windows_sys::Win32::{
            Foundation::SYSTEMTIME,
            System::SystemInformation::{GetLocalTime, GetSystemTime},
        };
        let seconds = |t: &SYSTEMTIME| {
            let day =
                crate::data::Day::from_ymd(i32::from(t.wYear), t.wMonth.into(), t.wDay.into())
                    .map_or(0, |d| i64::from(d.0));
            day * 86_400 + i64::from(t.wHour) * 3600 + i64::from(t.wMinute) * 60
        };
        // SAFETY: both calls only write to the provided structs.
        let (local, utc) = unsafe {
            let (mut local, mut utc) = (std::mem::zeroed(), std::mem::zeroed());
            GetLocalTime(&mut local);
            GetSystemTime(&mut utc);
            (local, utc)
        };
        // Round to whole minutes so a minute rollover between the calls is harmless.
        (seconds(&local) - seconds(&utc) + 30).div_euclid(60) * 60
    }
    #[cfg(not(windows))]
    {
        0
    }
}
