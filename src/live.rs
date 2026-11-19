//! Background monitoring of a file the game is still writing.
//!
//! Instead of re-parsing the whole file on every change, the monitor remembers
//! how far it has read and only parses newly appended bytes. A partially
//! written last line is buffered until its newline arrives. If the file shrinks
//! (new game session), parsing restarts from scratch.
//!
//! With a [`LibrarySink`] the monitor also keeps the game library current:
//! while new data arrives it stores the file every [`LibrarySink::every`], and
//! once more when monitoring stops, so nothing has to be saved by hand.

use std::{
    fs::File,
    io::{self, Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    sync::{
        Arc,
        mpsc::{self, Receiver, RecvTimeoutError, Sender},
    },
    thread::JoinHandle,
    time::{Duration, Instant, SystemTime},
};

use crate::{
    data::{Accumulator, Dataset},
    library::{Library, SaveOutcome},
};

pub enum LiveEvent {
    Updated(Arc<Dataset>),
    Status(String),
    /// The monitored file was stored in the game library.
    Saved(Result<SaveOutcome, String>),
}

/// Where and how often the monitor stores the file in the game library.
#[derive(Clone, Debug)]
pub struct LibrarySink {
    pub database: PathBuf,
    /// HOI4 version recorded for a new game.
    pub version: String,
    pub every: Duration,
}

enum Command {
    PollNow,
    Stop,
}

/// Handle to the monitoring thread; stops and joins it on drop.
pub struct LiveMonitor {
    commands: Sender<Command>,
    events: Receiver<LiveEvent>,
    thread: Option<JoinHandle<()>>,
}

impl LiveMonitor {
    /// Starts polling `path` every `interval`, saving to the library through
    /// `sink` if given. `on_event` is called from the worker after each event
    /// is queued (used to wake the UI).
    pub fn start(
        path: PathBuf,
        interval: Duration,
        sink: Option<LibrarySink>,
        on_event: impl Fn() + Send + 'static,
    ) -> Self {
        let (command_tx, command_rx) = mpsc::channel();
        let (event_tx, event_rx) = mpsc::channel();
        let thread = std::thread::Builder::new()
            .name("live-monitor".into())
            .spawn(move || run(path, interval, sink, &command_rx, &event_tx, &on_event))
            .expect("failed to spawn live monitor thread");
        Self {
            commands: command_tx,
            events: event_rx,
            thread: Some(thread),
        }
    }

    /// Requests an immediate poll instead of waiting for the next interval.
    pub fn poll_now(&self) {
        let _ = self.commands.send(Command::PollNow);
    }

    /// Drains all pending events.
    pub fn events(&self) -> impl Iterator<Item = LiveEvent> + '_ {
        self.events.try_iter()
    }
}

impl Drop for LiveMonitor {
    fn drop(&mut self) {
        let _ = self.commands.send(Command::Stop);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn run(
    path: PathBuf,
    interval: Duration,
    sink: Option<LibrarySink>,
    commands: &Receiver<Command>,
    events: &Sender<LiveEvent>,
    notify: &dyn Fn(),
) {
    let send = |event: LiveEvent| {
        let _ = events.send(event);
        notify();
    };
    send(LiveEvent::Status("Live monitoring started".into()));
    log::info!(
        "Live monitoring of {} started (interval {interval:?})",
        path.display()
    );

    // Data that has not reached the library yet, and when it last did.
    let mut unsaved = false;
    let mut last_save: Option<Instant> = None;
    let save = |path: &Path| {
        let Some(sink) = &sink else { return };
        let result = Library::open(&sink.database)
            .and_then(|mut library| library.save_file(path, &sink.version))
            .map(|(outcome, _)| outcome)
            .map_err(|e| e.to_string());
        if let Err(err) = &result {
            log::error!("Live: saving to the game library failed: {err}");
        }
        send(LiveEvent::Saved(result));
    };

    let mut tail = Tail::new(path.clone());
    let mut missing_reported = false;
    loop {
        match tail.poll() {
            Ok(Some(dataset)) => {
                missing_reported = false;
                unsaved = true;
                send(LiveEvent::Status(format!(
                    "Live data updated: {} dates",
                    dataset.dates().len()
                )));
                send(LiveEvent::Updated(Arc::new(dataset)));
            }
            Ok(None) => missing_reported = false,
            Err(err) if err.kind() == io::ErrorKind::NotFound => {
                if !missing_reported {
                    send(LiveEvent::Status("Monitoring file not found".into()));
                    missing_reported = true;
                }
            }
            Err(err) => {
                log::error!("Live monitoring error: {err}");
                send(LiveEvent::Status(format!("Live monitoring error: {err}")));
            }
        }
        if unsaved
            && let Some(sink) = &sink
            && last_save.is_none_or(|t| t.elapsed() >= sink.every)
        {
            save(&path);
            unsaved = false;
            last_save = Some(Instant::now());
        }
        match commands.recv_timeout(interval) {
            Ok(Command::PollNow) | Err(RecvTimeoutError::Timeout) => {}
            Ok(Command::Stop) | Err(RecvTimeoutError::Disconnected) => break,
        }
    }
    if unsaved {
        save(&path);
    }
    log::info!("Live monitoring stopped");
}

/// Incremental reader state.
struct Tail {
    path: PathBuf,
    acc: Accumulator,
    /// Bytes consumed so far (including `pending`).
    offset: u64,
    /// Trailing bytes after the last newline, waiting to be completed.
    pending: Vec<u8>,
    modified: Option<SystemTime>,
}

impl Tail {
    fn new(path: PathBuf) -> Self {
        Self {
            path,
            acc: Accumulator::new(),
            offset: 0,
            pending: Vec::new(),
            modified: None,
        }
    }

    /// Returns a fresh dataset if the file changed since the last call.
    fn poll(&mut self) -> io::Result<Option<Dataset>> {
        let meta = std::fs::metadata(&self.path)?;
        let len = meta.len();
        let modified = meta.modified().ok();

        if len < self.offset {
            log::info!("{} shrank, re-reading from the start", self.path.display());
            *self = Self::new(std::mem::take(&mut self.path));
        } else if len == self.offset && modified == self.modified {
            return Ok(None);
        }
        self.modified = modified;
        if len == self.offset {
            return Ok(None);
        }

        let started = Instant::now();
        let new_bytes = read_from(&self.path, self.offset, len - self.offset)?;
        self.offset += new_bytes.len() as u64;
        self.pending.extend_from_slice(&new_bytes);

        // Parse only up to the last complete line.
        let Some(last_newline) = memchr::memrchr(b'\n', &self.pending) else {
            return Ok(None);
        };
        self.acc.ingest(&self.pending[..=last_newline]);
        self.pending.drain(..=last_newline);

        let dataset = self.acc.to_dataset();
        log::info!(
            "Live: parsed {} new bytes in {:.1} ms ({} dates total)",
            new_bytes.len(),
            started.elapsed().as_secs_f64() * 1000.0,
            dataset.dates().len()
        );
        Ok(Some(dataset))
    }
}

fn read_from(path: &Path, offset: u64, len: u64) -> io::Result<Vec<u8>> {
    let mut file = File::open(path)?;
    file.seek(SeekFrom::Start(offset))?;
    let mut buf = Vec::with_capacity(len as usize);
    file.take(len).read_to_end(&mut buf)?;
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn monitor_keeps_library_current() {
        let dir = std::env::temp_dir().join(format!("hoi4_live_sink_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("output.txt");
        let database = dir.join("library.sqlite3");
        std::fs::write(&file, " 1:00, 1 Jan, 1936 GER MIL 1 1.5\n").unwrap();

        let sink = LibrarySink {
            database: database.clone(),
            version: "1.19.4".into(),
            every: Duration::from_secs(3600),
        };
        let monitor =
            LiveMonitor::start(file.clone(), Duration::from_millis(20), Some(sink), || {});
        // First data is stored right away.
        let first = loop {
            if let Ok(LiveEvent::Saved(result)) = monitor.events.recv() {
                break result.unwrap();
            }
        };
        assert!(matches!(first, SaveOutcome::Added(_)));

        // More data within the save interval: stored when monitoring stops.
        let mut f = std::fs::OpenOptions::new()
            .append(true)
            .open(&file)
            .unwrap();
        writeln!(f, " 1:00, 2 Jan, 1936 GER CIV 1 20.0").unwrap();
        drop(f);
        monitor.poll_now();
        std::thread::sleep(Duration::from_millis(200));
        drop(monitor);

        let library = Library::open(&database).unwrap();
        let games = library.games().unwrap();
        assert_eq!(games.len(), 1);
        assert_eq!(games[0].days, 2, "final save on stop");
        assert!(games[0].has_civ);
        drop(library);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn tail_reads_only_appended_lines() {
        let path = std::env::temp_dir().join(format!("hoi4_tail_test_{}.txt", std::process::id()));
        let mut file = File::create(&path).unwrap();
        let mut tail = Tail::new(path.clone());

        writeln!(file, "DLL: 12:00, 1 January, 1936 GER MIL 1 1.5").unwrap();
        write!(file, "DLL: 12:00, 2 January, 1936 GER MIL 1 2.").unwrap(); // incomplete line
        file.flush().unwrap();
        let ds = tail.poll().unwrap().unwrap();
        assert_eq!(ds.dates().len(), 1);

        writeln!(file, "5").unwrap();
        file.flush().unwrap();
        let ds = tail.poll().unwrap().unwrap();
        assert_eq!(ds.dates().len(), 2);
        assert_eq!(ds.row(crate::data::Metric::Mil, 0), [1.5, 2.5]);
        assert!(tail.poll().unwrap().is_none());

        // Truncation restarts parsing.
        drop(file);
        let mut file = File::create(&path).unwrap();
        writeln!(file, "DLL: 12:00, 5 May, 1940 SOV NAV 1 1.0").unwrap();
        file.flush().unwrap();
        let ds = tail.poll().unwrap().unwrap();
        assert_eq!(ds.dates().len(), 1);
        assert_eq!(&*ds.tags()[0].as_str(), "SOV");

        drop(file);
        let _ = std::fs::remove_file(&path);
    }
}
