//! Main window.

use std::{
    path::{Path, PathBuf},
    sync::{
        Arc,
        mpsc::{self, Receiver, Sender},
    },
    time::{Duration, Instant, SystemTime},
};

use eframe::egui::{self, Button, RichText, ScrollArea, Ui};
use rustc_hash::{FxHashMap, FxHashSet};

use super::{
    chart::ViewOptions,
    dialogs,
    graph_window::{GraphWindow, SharedGraphWindow},
    library_window::{LibraryAction, LibraryWindow},
    live_dashboard::{LiveDashboard, LiveInputs},
    rankings_window::RankingsWindow,
    settings_window::{GameStatus, SettingsAction, SettingsWindow},
};
use crate::{
    analysis::{
        self, Align, Group, Source,
        factions::{self, CountryLists, Grouping, Labels},
    },
    data::{self, Dataset, Tag},
    game::{self, CountryNames, GameEvent, GameWatcher, RunningGame},
    inject::{self, DllKind},
    library::{Library, SaveOutcome},
    live::LibrarySink,
    logging,
    settings::Settings,
    update::{self, Update},
};

/// Results delivered from worker threads.
enum TaskResult {
    Loaded {
        slot: usize,
        path: PathBuf,
        stamp: Option<FileStamp>,
        result: Result<Arc<Dataset>, String>,
    },
    Injected(Result<&'static str, String>),
    Saved {
        slot: usize,
        stamp: Option<FileStamp>,
        result: Result<SaveOutcome, String>,
    },
    UpdateChecked(Result<Option<Update>, String>),
}

/// Size and modification time, to detect a file that changed since it was parsed.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct FileStamp {
    len: u64,
    modified: Option<SystemTime>,
}

impl FileStamp {
    fn of(path: &Path) -> Option<Self> {
        let meta = std::fs::metadata(path).ok()?;
        Some(Self {
            len: meta.len(),
            modified: meta.modified().ok(),
        })
    }
}

#[derive(Default)]
enum LoadState {
    #[default]
    Empty,
    Loading,
    Ready {
        data: Arc<Dataset>,
        stamp: Option<FileStamp>,
    },
    Failed(String),
}

/// Where a data slot's content comes from.
enum SlotSource {
    File(PathBuf),
    Library { id: i64, name: String },
}

#[derive(Default)]
struct FileSlot {
    source: Option<SlotSource>,
    state: LoadState,
    /// File state last stored in the library, to skip redundant saves.
    saved: Option<FileStamp>,
    /// A library save of this slot is running.
    saving: bool,
}

impl FileSlot {
    fn path(&self) -> Option<&Path> {
        match &self.source {
            Some(SlotSource::File(path)) => Some(path),
            _ => None,
        }
    }

    fn label(&self) -> String {
        match &self.source {
            None => String::new(),
            Some(SlotSource::File(path)) => path.display().to_string(),
            Some(SlotSource::Library { id, name }) => format!("Game library #{id}: {name}"),
        }
    }

    fn data(&self) -> Option<&Arc<Dataset>> {
        match &self.state {
            LoadState::Ready { data, .. } => Some(data),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GraphRequest {
    Individual,
    Factions,
    CompareIndividual,
    CompareFactions,
}

impl GraphRequest {
    fn needs_both_files(self) -> bool {
        matches!(self, Self::CompareIndividual | Self::CompareFactions)
    }

    fn by_group(self) -> bool {
        matches!(self, Self::Factions | Self::CompareFactions)
    }

    fn title_suffix(self) -> &'static str {
        match self {
            Self::Individual => " (File 1)",
            Self::Factions => " - Combined Groups (File 1)",
            Self::CompareIndividual => " - Country Comparison",
            Self::CompareFactions => " - Group Comparison",
        }
    }
}

/// The automatic game mode's progress for the running game.
#[derive(Default)]
struct Automation {
    game: Option<RunningGame>,
    /// DataCollection version made for the running game.
    dll: Option<String>,
    /// When to inject automatically.
    inject_at: Option<Instant>,
    /// Live monitoring was started for this game.
    live_started: bool,
}

pub struct App {
    files: [FileSlot; 2],
    grouping: Grouping,
    countries: CountryLists,
    names: CountryNames,
    selected: FxHashSet<Tag>,
    /// Bumped whenever the selection or labels change (lets the live
    /// dashboard react, and the selection be remembered).
    selection_gen: u64,
    persisted_gen: u64,
    versions: Vec<String>,
    version: usize,
    admin_menu: bool,
    injecting: bool,
    status: String,
    show_log: bool,
    graph_windows: Vec<SharedGraphWindow>,
    window_serial: u64,
    live: Option<LiveDashboard>,
    tasks_tx: Sender<TaskResult>,
    tasks_rx: Receiver<TaskResult>,
    icon: Option<Arc<egui::IconData>>,
    library: Option<Library>,
    library_error: Option<String>,
    library_window: Option<LibraryWindow>,
    /// Decoded library games, by id.
    library_data: FxHashMap<i64, Arc<Dataset>>,
    /// Games ticked in the library window; their countries are selectable.
    library_selection: Vec<i64>,
    settings: Settings,
    settings_path: PathBuf,
    /// What is on disk, to save only real changes.
    saved_settings: Settings,
    settings_window: Option<SettingsWindow>,
    rankings_window: Option<RankingsWindow>,
    watcher: Option<GameWatcher>,
    automation: Automation,
    update: Option<Update>,
    update_status: String,
}

impl App {
    /// `initial_files` (e.g. from the command line) are loaded into File 1 / File 2.
    pub fn new(
        ctx: &egui::Context,
        icon: Option<Arc<egui::IconData>>,
        initial_files: Vec<PathBuf>,
    ) -> Self {
        let (tasks_tx, tasks_rx) = mpsc::channel();
        let settings_path = Settings::default_path();
        let settings = Settings::load(&settings_path);
        let versions = inject::available_versions();
        let version = versions
            .iter()
            .position(|v| *v == settings.version)
            .unwrap_or(0);
        let (library, library_error) = match Library::open(&Library::default_path()) {
            Ok(library) => {
                log::info!("Game library: {}", library.path().display());
                (Some(library), None)
            }
            Err(err) => {
                log::error!("Game library unavailable: {err}");
                (None, Some(err.to_string()))
            }
        };
        let names = game::install_dir()
            .map(|dir| CountryNames::load(&dir))
            .unwrap_or_default();
        let selected = settings
            .selected_countries
            .iter()
            .filter_map(|t| Tag::from_bytes(t.as_bytes()))
            .collect();
        let mut app = Self {
            files: Default::default(),
            grouping: grouping_from(&settings),
            countries: CountryLists::default(),
            names,
            selected,
            selection_gen: 0,
            persisted_gen: 0,
            versions,
            version,
            admin_menu: inject::admin_menu_available(),
            injecting: false,
            status: String::new(),
            show_log: false,
            graph_windows: Vec::new(),
            window_serial: 0,
            live: None,
            tasks_tx,
            tasks_rx,
            icon,
            library,
            library_error,
            library_window: None,
            library_data: FxHashMap::default(),
            library_selection: Vec::new(),
            saved_settings: settings.clone(),
            settings,
            settings_path,
            settings_window: None,
            rankings_window: None,
            watcher: None,
            automation: Automation::default(),
            update: None,
            update_status: String::new(),
        };
        app.set_status("Application started. Select data file.");
        if app.settings.watch_game {
            app.watcher = Some(start_watcher(ctx));
        }
        if app.settings.check_updates && update::current_release().is_some() {
            app.check_for_updates(ctx);
        }
        for (slot, path) in initial_files.into_iter().take(2).enumerate() {
            app.load(ctx, slot, path);
        }
        app
    }

    fn set_status(&mut self, message: impl Into<String>) {
        self.status = message.into();
        log::info!("Status: {}", self.status);
    }

    fn labels(&self) -> Labels<'_> {
        Labels {
            names: self.settings.country_names.then_some(&self.names),
            stable_colors: self.settings.stable_colors,
        }
    }

    fn view(&self) -> ViewOptions {
        ViewOptions {
            smoothing: self.settings.smoothing_days,
            share: false,
        }
    }

    fn current_version(&self) -> Option<String> {
        self.versions.get(self.version).cloned()
    }

    fn library_sink(&self) -> Option<LibrarySink> {
        let library = self.library.as_ref().filter(|_| self.settings.auto_save)?;
        Some(LibrarySink {
            database: library.path().to_path_buf(),
            version: self.current_version().unwrap_or_default(),
            every: Duration::from_secs(u64::from(self.settings.live_save_every_secs.max(5))),
        })
    }

    /// Where DataCollection writes: the configured file, or `output.txt` in
    /// the game folder.
    fn output_path(&self) -> Option<PathBuf> {
        if let Some(path) = &self.settings.output_file {
            return Some(path.clone());
        }
        self.automation
            .game
            .as_ref()
            .map(|g| g.dir.clone())
            .or_else(game::install_dir)
            .map(|dir| dir.join("output.txt"))
    }

    // ----- background work -------------------------------------------------

    fn spawn<F>(&self, ctx: &egui::Context, work: F)
    where
        F: FnOnce() -> TaskResult + Send + 'static,
    {
        let tx = self.tasks_tx.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(work());
            ctx.request_repaint();
        });
    }

    fn poll_tasks(&mut self, ctx: &egui::Context) {
        while let Ok(result) = self.tasks_rx.try_recv() {
            match result {
                TaskResult::Loaded {
                    slot,
                    path,
                    stamp,
                    result,
                } => {
                    // Ignore results for a file that has since been replaced.
                    if self.files[slot].path() != Some(path.as_path()) {
                        continue;
                    }
                    let name = file_name(&path);
                    match result {
                        Ok(data) => {
                            self.set_status(format!(
                                "Loaded File {}: {name} ({} dates, {} countries)",
                                slot + 1,
                                data.dates().len(),
                                data.tags().len()
                            ));
                            self.files[slot].state = LoadState::Ready { data, stamp };
                            self.refresh_country_lists();
                            if self.settings.auto_save {
                                self.save_slot(ctx, slot);
                            }
                        }
                        Err(err) => {
                            self.set_status(format!("Error loading File {}: {err}", slot + 1));
                            self.files[slot].state = LoadState::Failed(err);
                        }
                    }
                }
                TaskResult::Injected(result) => {
                    self.injecting = false;
                    match result {
                        Ok(message) => {
                            self.set_status(message);
                            if self.settings.auto_live {
                                self.start_auto_live(ctx);
                            }
                        }
                        Err(err) => self.set_status(err),
                    }
                }
                TaskResult::Saved {
                    slot,
                    stamp,
                    result,
                } => self.on_saved(slot, stamp, result),
                TaskResult::UpdateChecked(result) => {
                    self.update_status = match &result {
                        Ok(Some(update)) => format!("{} is available.", update.tag),
                        Ok(None) => "You have the latest release.".to_owned(),
                        Err(err) => format!("Update check failed: {err}"),
                    };
                    if let Ok(Some(update)) = result {
                        self.set_status(format!("A new release is available: {}", update.tag));
                        self.update = Some(update);
                    }
                }
            }
        }
    }

    fn check_for_updates(&mut self, ctx: &egui::Context) {
        self.update_status = "Checking…".to_owned();
        self.spawn(ctx, || TaskResult::UpdateChecked(update::check()));
    }

    fn select_file(&mut self, ctx: &egui::Context, slot: usize) {
        let Some(path) = dialogs::open_data_file(&format!("Select Data File {}", slot + 1)) else {
            self.set_status("File selection cancelled.");
            return;
        };
        self.load(ctx, slot, path);
    }

    /// Starts parsing `path` into `slot` on a worker thread.
    fn load(&mut self, ctx: &egui::Context, slot: usize, path: PathBuf) {
        self.set_status(format!(
            "Loading File {}: {}...",
            slot + 1,
            file_name(&path)
        ));
        self.files[slot] = FileSlot {
            source: Some(SlotSource::File(path.clone())),
            state: LoadState::Loading,
            saved: None,
            saving: false,
        };
        self.spawn(ctx, move || {
            let stamp = FileStamp::of(&path);
            let result = data::load_file(&path)
                .map(Arc::new)
                .map_err(|e| e.to_string());
            TaskResult::Loaded {
                slot,
                path,
                stamp,
                result,
            }
        });
    }

    /// Files dropped onto the window: two fill both slots, one goes to the
    /// first empty slot (or replaces File 1).
    fn handle_dropped_files(&mut self, ctx: &egui::Context) {
        let dropped: Vec<PathBuf> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .map(|f| f.path().to_path_buf())
                .filter(|p| !p.as_os_str().is_empty())
                .collect()
        });
        match dropped.as_slice() {
            [] => {}
            [one] => {
                let slot = self
                    .files
                    .iter()
                    .position(|f| f.source.is_none())
                    .unwrap_or(0);
                self.load(ctx, slot, one.clone());
            }
            [first, second, ..] => {
                self.load(ctx, 0, first.clone());
                self.load(ctx, 1, second.clone());
            }
        }
    }

    fn inject(&mut self, ctx: &egui::Context, kind: DllKind, version: Option<String>) {
        let Some(version) = version.or_else(|| self.current_version()) else {
            dialogs::error(
                "No DataCollection version is available. Add the DLL for your HOI4 version in Settings.",
            );
            return;
        };
        self.injecting = true;
        self.set_status(match kind {
            DllKind::DataCollection => format!("Starting data collection ({version})..."),
            DllKind::AdminMenu => "Attempting to attach AdminMenu...".to_owned(),
        });
        self.spawn(ctx, move || {
            TaskResult::Injected(inject::inject(kind, &version).map_err(|e| e.to_string()))
        });
    }

    // ----- automatic game mode ---------------------------------------------

    fn pump_game_events(&mut self, ctx: &egui::Context) {
        let events: Vec<GameEvent> = match &self.watcher {
            Some(watcher) => watcher.events().collect(),
            None => return,
        };
        for event in events {
            match event {
                GameEvent::Started(game) => self.on_game_started(game),
                GameEvent::Exited => self.on_game_exited(),
            }
        }
        if let Some(at) = self.automation.inject_at {
            let now = Instant::now();
            if now >= at {
                self.automation.inject_at = None;
                let dll = self.automation.dll.clone();
                if dll.is_some() && !self.injecting {
                    self.inject(ctx, DllKind::DataCollection, dll);
                }
            } else {
                ctx.request_repaint_after(at - now);
            }
        }
    }

    fn on_game_started(&mut self, game: RunningGame) {
        let version = game
            .version
            .clone()
            .unwrap_or_else(|| "(unknown version)".into());
        let dll = game
            .version
            .as_deref()
            .and_then(|v| inject::match_game_version(v, &self.versions))
            .map(str::to_owned);
        if self.names.is_empty() {
            self.names = CountryNames::load(&game.dir);
            self.selection_gen += 1;
        }
        match &dll {
            Some(dll) => {
                if self.settings.auto_select_version
                    && let Some(i) = self.versions.iter().position(|v| v == dll)
                {
                    self.version = i;
                }
                let next = if self.settings.auto_inject {
                    let delay = if game.was_running {
                        0
                    } else {
                        self.settings.inject_delay_secs
                    };
                    self.automation.inject_at =
                        Some(Instant::now() + Duration::from_secs(u64::from(delay)));
                    if delay > 0 {
                        format!(" Data collection starts in {delay} s.")
                    } else {
                        " Starting data collection.".to_owned()
                    }
                } else {
                    String::new()
                };
                self.set_status(format!(
                    "HOI4 {version} detected (DataCollection {dll}).{next}"
                ));
            }
            None => self.set_status(format!(
                "HOI4 {version} detected, but there is no DataCollection DLL for it. \
                 Add one in Settings."
            )),
        }
        self.automation.game = Some(game);
        self.automation.dll = dll;
        self.automation.live_started = false;
    }

    fn on_game_exited(&mut self) {
        if self.automation.live_started
            && let Some(live) = &mut self.live
        {
            // Stopping stores the last data in the library.
            live.stop_monitoring();
        }
        self.automation = Automation::default();
        self.set_status("HOI4 closed.");
    }

    /// Opens the live dashboard on the game's output file.
    fn start_auto_live(&mut self, ctx: &egui::Context) {
        let Some(path) = self.output_path() else {
            return;
        };
        let sink = self.library_sink();
        let live = self
            .live
            .get_or_insert_with(|| LiveDashboard::new(self.settings.live_interval_secs));
        if !live.is_monitoring() {
            live.start_monitoring(ctx, &path, sink);
        }
        self.automation.live_started = true;
        if self.files[0].source.is_none() && path.is_file() {
            self.load(ctx, 0, path);
        }
    }

    // ----- countries -------------------------------------------------------

    /// Countries come from both data files and the games ticked in the library.
    fn refresh_country_lists(&mut self) {
        let datasets: Vec<Arc<Dataset>> = self
            .files
            .iter()
            .filter_map(|f| f.data().cloned())
            .chain(
                self.library_selection
                    .iter()
                    .filter_map(|id| self.library_data.get(id).cloned()),
            )
            .collect();
        let lists = self
            .grouping
            .lists(datasets.iter().flat_map(|d| d.active_tags()));
        for (name, tags) in lists.columns() {
            let tags: Vec<String> = tags.iter().map(Tag::to_string).collect();
            log::info!("Countries - {name}: [{}]", tags.join(", "));
        }
        self.countries = lists;
        self.selection_gen += 1;
    }

    /// Selected tags that are available, in display order.
    fn selected_tags(&self) -> Vec<Tag> {
        self.countries
            .iter()
            .filter(|t| self.selected.contains(t))
            .collect()
    }

    fn toggle_column(&mut self, column: usize) {
        let Some((name, tags)) = self.countries.columns().get(column).cloned() else {
            return;
        };
        let select = tags.iter().any(|t| !self.selected.contains(t));
        for tag in &tags {
            if select {
                self.selected.insert(*tag);
            } else {
                self.selected.remove(tag);
            }
        }
        self.selection_gen += 1;
        let action = if select { "Selected" } else { "Deselected" };
        self.set_status(format!("{action} {} countries ({name})", tags.len()));
    }

    // ----- graphs ----------------------------------------------------------

    /// Returns the parsed file, re-reading it first if it changed on disk
    /// (the game may still be appending to it).
    fn fresh_dataset(&mut self, slot: usize) -> Result<Arc<Dataset>, String> {
        let path = match &self.files[slot].source {
            None => return Err(format!("Data File {} is not selected.", slot + 1)),
            // Library games never change on disk.
            Some(SlotSource::Library { .. }) => {
                return self.files[slot]
                    .data()
                    .cloned()
                    .ok_or_else(|| format!("Data File {} has no data.", slot + 1));
            }
            Some(SlotSource::File(path)) => path.clone(),
        };
        let stamp = FileStamp::of(&path);
        if let LoadState::Ready {
            data,
            stamp: loaded,
        } = &self.files[slot].state
            && *loaded == stamp
        {
            return Ok(data.clone());
        }
        let data = data::load_file(&path)
            .map(Arc::new)
            .map_err(|e| format!("Failed to read File {}: {e}", slot + 1))?;
        self.files[slot].state = LoadState::Ready {
            data: data.clone(),
            stamp,
        };
        self.refresh_country_lists();
        Ok(data)
    }

    fn groups(&self, selected: &[Tag], by_group: bool) -> Vec<Group> {
        if by_group {
            self.grouping.groups_for(selected)
        } else {
            factions::individual_groups(selected, self.labels())
        }
    }

    fn generate(&mut self, ctx: &egui::Context, request: GraphRequest) {
        let started = Instant::now();
        if request.needs_both_files() {
            if self.files.iter().any(|f| f.source.is_none()) {
                dialogs::error("Please select both Data File 1 and Data File 2.");
                return self.set_status("Error: Both data files must be selected for comparison.");
            }
        } else if self.files[0].source.is_none() {
            dialogs::error("Please select Data File 1 first.");
            return self.set_status("Error: No Data File 1 selected for graph generation.");
        }
        let selected = self.selected_tags();
        if selected.is_empty() {
            dialogs::error("Please select at least one country.");
            return self.set_status("Error: No countries selected for graph generation.");
        }

        let slots: &[usize] = if request.needs_both_files() {
            &[0, 1]
        } else {
            &[0]
        };
        let mut datasets = Vec::with_capacity(slots.len());
        for &slot in slots {
            match self.fresh_dataset(slot) {
                Ok(data) => datasets.push(data),
                Err(err) => {
                    dialogs::error(&err);
                    return self.set_status(err);
                }
            }
        }

        let suffixes: &[&str] = if request.needs_both_files() {
            &[" (F1)", " (F2)"]
        } else {
            &[""]
        };
        let sources: Vec<Source<'_>> = datasets
            .iter()
            .zip(suffixes)
            .map(|(dataset, suffix)| Source { dataset, suffix })
            .collect();
        let groups = self.groups(&selected, request.by_group());

        let Some(graph) = analysis::build(&sources, &groups, request.title_suffix()) else {
            dialogs::info("No data found for the selected countries.");
            return self.set_status("No data found for selected countries.");
        };
        log::info!(
            "Built {:?} graph: {} series x {} dates in {:.2} ms",
            request,
            graph.len(),
            graph.dates.len(),
            started.elapsed().as_secs_f64() * 1000.0
        );
        self.open_graph(graph);
        if self.settings.auto_save {
            for &slot in slots {
                self.save_slot(ctx, slot);
            }
        }
    }

    fn open_graph(&mut self, graph: analysis::GraphData) {
        self.window_serial += 1;
        self.graph_windows.push(GraphWindow::new(
            self.window_serial,
            Arc::new(graph),
            self.view(),
        ));
        self.set_status("Graph window displayed.");
    }

    // ----- game library ----------------------------------------------------

    /// Stores a data file in the library on a worker thread, unless it has not
    /// changed since it was last stored.
    fn save_slot(&mut self, ctx: &egui::Context, slot: usize) {
        let (Some(library), Some(path)) = (&self.library, self.files[slot].path()) else {
            return;
        };
        let stamp = FileStamp::of(path);
        if self.files[slot].saving || (stamp.is_some() && self.files[slot].saved == stamp) {
            return;
        }
        let db = library.path().to_path_buf();
        let path = path.to_path_buf();
        let version = self.current_version().unwrap_or_default();
        self.files[slot].saving = true;
        self.spawn(ctx, move || {
            let result = Library::open(&db)
                .and_then(|mut library| library.save_file(&path, &version))
                .map(|(outcome, _)| outcome)
                .map_err(|e| e.to_string());
            TaskResult::Saved {
                slot,
                stamp,
                result,
            }
        });
    }

    fn on_saved(
        &mut self,
        slot: usize,
        stamp: Option<FileStamp>,
        result: Result<SaveOutcome, String>,
    ) {
        self.files[slot].saving = false;
        let outcome = match result {
            Ok(outcome) => outcome,
            Err(err) => {
                return self.set_status(format!(
                    "Could not save File {} to the game library: {err}",
                    slot + 1
                ));
            }
        };
        self.files[slot].saved = stamp;
        match outcome {
            SaveOutcome::Added(id) => {
                self.set_status(format!(
                    "File {} saved to the game library as game #{id}.",
                    slot + 1
                ));
            }
            SaveOutcome::Updated(id) => self.set_status(format!(
                "Game library: game #{id} updated with the latest data from File {}.",
                slot + 1
            )),
            SaveOutcome::Unchanged(id) => {
                log::info!("File {} is already library game #{id}", slot + 1)
            }
        }
        self.on_library_changed(outcome);
    }

    /// Keeps caches and the library window in step after a save.
    fn on_library_changed(&mut self, outcome: SaveOutcome) {
        if matches!(outcome, SaveOutcome::Unchanged(_)) {
            return;
        }
        // Cached decodes of a game that just grew are stale.
        self.library_data.remove(&outcome.id());
        if let (Some(window), Some(library)) = (&mut self.library_window, &self.library) {
            window.refresh(library);
            window.set_status(self.status.clone());
        }
    }

    fn open_library(&mut self) {
        match (&self.library, &self.library_window) {
            (Some(_), Some(_)) => {}
            (Some(library), None) => {
                self.library_window = Some(LibraryWindow::new(library));
                self.set_status("Game library opened");
            }
            (None, _) => dialogs::error(&format!(
                "The game library could not be opened: {}",
                self.library_error.as_deref().unwrap_or("unknown error")
            )),
        }
    }

    fn library_dataset(&mut self, id: i64) -> Result<Arc<Dataset>, String> {
        if let Some(data) = self.library_data.get(&id) {
            return Ok(data.clone());
        }
        let library = self
            .library
            .as_ref()
            .ok_or("The game library is not available.")?;
        let data = Arc::new(library.load(id).map_err(|e| e.to_string())?);
        self.library_data.insert(id, data.clone());
        Ok(data)
    }

    fn library_name(&self, id: i64) -> String {
        self.library_window
            .as_ref()
            .and_then(|w| w.name_of(id))
            .map_or_else(|| format!("game #{id}"), str::to_owned)
    }

    fn handle_library_action(&mut self, ctx: &egui::Context, action: LibraryAction) {
        match action {
            LibraryAction::SelectionChanged(ids) => {
                for &id in &ids {
                    if let Err(err) = self.library_dataset(id) {
                        self.set_status(format!("Could not load library game #{id}: {err}"));
                    }
                }
                self.library_selection = ids;
                self.refresh_country_lists();
            }
            LibraryAction::Compare {
                ids,
                by_faction,
                from_start,
            } => self.compare_library(&ids, by_faction, from_start),
            LibraryAction::LoadIntoSlot { id, slot } => match self.library_dataset(id) {
                Ok(data) => {
                    let name = self.library_name(id);
                    self.set_status(format!(
                        "Data File {} is now library game #{id} ({name}).",
                        slot + 1
                    ));
                    self.files[slot] = FileSlot {
                        source: Some(SlotSource::Library { id, name }),
                        state: LoadState::Ready { data, stamp: None },
                        saved: None,
                        saving: false,
                    };
                    self.refresh_country_lists();
                }
                Err(err) => dialogs::error(&format!("Could not load library game #{id}: {err}")),
            },
            LibraryAction::SaveCurrentFiles => {
                let slots: Vec<usize> =
                    (0..2).filter(|&s| self.files[s].path().is_some()).collect();
                if slots.is_empty() {
                    dialogs::info(
                        "Load a data file first; only files (not library games) can be saved.",
                    );
                }
                for slot in slots {
                    self.files[slot].saved = None;
                    self.save_slot(ctx, slot);
                }
            }
        }
    }

    /// Charts the ticked countries of several library games against each other.
    fn compare_library(&mut self, ids: &[i64], by_group: bool, from_start: bool) {
        let started = Instant::now();
        let selected = self.selected_tags();
        if selected.is_empty() {
            dialogs::error("Tick the countries to compare in the main window first.");
            return self.set_status("Error: No countries selected for the library comparison.");
        }
        let mut games = Vec::with_capacity(ids.len());
        for &id in ids {
            match self.library_dataset(id) {
                Ok(data) => games.push((id, self.library_name(id), data)),
                Err(err) => {
                    return dialogs::error(&format!("Could not load library game #{id}: {err}"));
                }
            }
        }
        // Series are named after their game; the id disambiguates equal names.
        let suffixes: Vec<String> = games
            .iter()
            .map(|(id, name, _)| {
                if games.iter().filter(|(_, other, _)| other == name).count() > 1 {
                    format!(" ({name} #{id})")
                } else {
                    format!(" ({name})")
                }
            })
            .collect();
        let sources: Vec<Source<'_>> = games
            .iter()
            .zip(&suffixes)
            .map(|((_, _, dataset), suffix)| Source { dataset, suffix })
            .collect();
        let groups = self.groups(&selected, by_group);
        let align = if from_start {
            Align::FromStart
        } else {
            Align::Calendar
        };
        let title = format!(" - Library Comparison ({} games)", games.len());
        let Some(graph) = analysis::build_aligned(&sources, &groups, &title, align) else {
            dialogs::info("None of the selected games has data for the selected countries.");
            return self.set_status("No data found for the selected countries.");
        };
        log::info!(
            "Built library comparison: {} games, {} series x {} dates in {:.2} ms",
            games.len(),
            graph.len(),
            graph.dates.len(),
            started.elapsed().as_secs_f64() * 1000.0
        );
        self.open_graph(graph);
    }

    // ----- settings ----------------------------------------------------------

    /// Reacts to settings changes and writes them to disk.
    fn apply_settings(&mut self, ctx: &egui::Context, before: &Settings) {
        let now = &self.settings;
        let watch_changed = now.watch_game != before.watch_game;
        let groups_changed = now.use_custom_groups != before.use_custom_groups
            || now.custom_groups != before.custom_groups;
        let labels_changed =
            now.country_names != before.country_names || now.stable_colors != before.stable_colors;
        if watch_changed {
            if self.settings.watch_game {
                self.watcher = Some(start_watcher(ctx));
            } else {
                self.watcher = None;
                self.automation = Automation::default();
            }
        }
        if groups_changed {
            self.grouping = grouping_from(&self.settings);
            self.refresh_country_lists();
        }
        if labels_changed {
            self.selection_gen += 1;
        }
        if self.settings != self.saved_settings {
            match self.settings.save(&self.settings_path) {
                Ok(()) => self.saved_settings = self.settings.clone(),
                Err(err) => log::error!("Could not save settings: {err}"),
            }
        }
    }

    fn reload_versions(&mut self) {
        let current = self.current_version();
        self.versions = inject::available_versions();
        self.version = current
            .and_then(|v| self.versions.iter().position(|x| *x == v))
            .unwrap_or(0);
        if let Some(game) = &self.automation.game {
            self.automation.dll = game
                .version
                .as_deref()
                .and_then(|v| inject::match_game_version(v, &self.versions))
                .map(str::to_owned);
            // A DLL for the running game just became available.
            if let Some(dll) = &self.automation.dll
                && self.settings.auto_select_version
                && let Some(i) = self.versions.iter().position(|v| v == dll)
            {
                self.version = i;
            }
        }
    }

    /// Remembers version and selection between sessions.
    fn remember_state(&mut self) {
        if let Some(version) = self.current_version() {
            self.settings.version = version;
        }
        if self.persisted_gen != self.selection_gen {
            self.persisted_gen = self.selection_gen;
            let mut selected: Vec<String> = self.selected.iter().map(Tag::to_string).collect();
            selected.sort();
            self.settings.selected_countries = selected;
        }
    }

    // ----- layout ----------------------------------------------------------

    fn file_rows(&mut self, ui: &mut Ui) {
        for slot in 0..2 {
            ui.horizontal(|ui| {
                ui.add_sized(
                    [80.0, 22.0],
                    egui::Label::new(format!("Data File {}:", slot + 1)),
                );
                // Right-to-left: status (fixed width), button, then the path takes what is left.
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    match &self.files[slot].state {
                        LoadState::Empty => ui.add_sized([110.0, 22.0], egui::Label::new("")),
                        LoadState::Loading => ui.add_sized([110.0, 22.0], egui::Spinner::new()),
                        LoadState::Ready { data, .. } => ui
                            .add_sized(
                                [110.0, 22.0],
                                egui::Label::new(
                                    RichText::new(format!("{} dates", data.dates().len())).weak(),
                                ),
                            )
                            .on_hover_text(format!(
                                "{} lines, {} valid, {} errors, {} countries",
                                data.stats().lines,
                                data.stats().valid,
                                data.stats().errors(),
                                data.tags().len()
                            )),
                        LoadState::Failed(err) => ui
                            .add_sized(
                                [110.0, 22.0],
                                egui::Label::new(
                                    RichText::new("Failed to load")
                                        .color(ui.visuals().error_fg_color),
                                ),
                            )
                            .on_hover_text(err),
                    };
                    if ui.button(format!("Browse File {}...", slot + 1)).clicked() {
                        self.select_file(ui.ctx(), slot);
                    }
                    let shown = self.files[slot].label();
                    ui.add_sized(
                        [ui.available_width(), 22.0],
                        egui::TextEdit::singleline(&mut shown.as_str()),
                    );
                });
            });
        }
    }

    fn country_columns(&mut self, ui: &mut Ui) {
        if self.countries.is_empty() {
            ui.centered_and_justified(|ui| {
                ui.weak(
                    "Select a data file (or drop it onto this window), or tick games in the \
                     Game Library, to list their countries.",
                );
            });
            return;
        }
        let mut toggle = None;
        let mut changed = false;
        let columns = self.countries.columns().to_vec();
        let names = self.settings.country_names.then_some(&self.names);
        let mut selected = std::mem::take(&mut self.selected);
        ui.columns(columns.len(), |uis| {
            for (index, (ui, (name, tags))) in uis.iter_mut().zip(&columns).enumerate() {
                egui::Frame::group(ui.style()).show(ui, |ui| {
                    ui.set_min_size(ui.available_size());
                    ui.strong(name);
                    if !tags.is_empty() && ui.button(format!("Toggle All {name}")).clicked() {
                        toggle = Some(index);
                    }
                    ScrollArea::vertical()
                        .id_salt(("countries", index))
                        .auto_shrink(false)
                        .show(ui, |ui| {
                            for &tag in tags {
                                let mut checked = selected.contains(&tag);
                                let text = match names.and_then(|n| n.get(tag)) {
                                    Some(name) => format!("{tag}  {name}"),
                                    None => tag.to_string(),
                                };
                                if ui.checkbox(&mut checked, text).changed() {
                                    changed = true;
                                    if checked {
                                        selected.insert(tag);
                                    } else {
                                        selected.remove(&tag);
                                    }
                                }
                            }
                        });
                });
            }
        });
        self.selected = selected;
        if changed {
            self.selection_gen += 1;
        }
        if let Some(column) = toggle {
            self.toggle_column(column);
        }
    }

    fn actions(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        ui.add_space(4.0);
        ui.columns(4, |columns| {
            let full = |ui: &mut Ui, enabled: bool, text: &str| {
                ui.add_enabled(
                    enabled,
                    Button::new(text).min_size(egui::vec2(ui.available_width(), 28.0)),
                )
                .clicked()
            };

            group(&mut columns[0], "HOI4 Integration", |ui| {
                ui.horizontal(|ui| {
                    ui.label("Version:");
                    let current = self
                        .versions
                        .get(self.version)
                        .map_or("none", String::as_str);
                    egui::ComboBox::from_id_salt("version")
                        .width(ui.available_width())
                        .selected_text(current)
                        .show_ui(ui, |ui| {
                            for (i, v) in self.versions.iter().enumerate() {
                                ui.selectable_value(&mut self.version, i, v);
                            }
                        });
                });
                if full(ui, !self.injecting, "Start Data Collection") {
                    self.inject(&ctx, DllKind::DataCollection, None);
                }
                if self.admin_menu && full(ui, !self.injecting, "Attach Admin Menu") {
                    self.inject(&ctx, DllKind::AdminMenu, None);
                }
                self.game_line(ui);
            });
            let groups = if self.settings.use_custom_groups {
                "Group"
            } else {
                "Faction"
            };
            group(&mut columns[1], "Individual IC Analysis", |ui| {
                if full(ui, true, "Individual IC (File 1)") {
                    self.generate(&ctx, GraphRequest::Individual);
                }
                if full(ui, true, "Compare Individual IC (Files 1 & 2)") {
                    self.generate(&ctx, GraphRequest::CompareIndividual);
                }
            });
            group(&mut columns[2], &format!("{groups} IC Analysis"), |ui| {
                if full(ui, true, &format!("{groups}s IC (File 1)")) {
                    self.generate(&ctx, GraphRequest::Factions);
                }
                if full(ui, true, &format!("Compare {groups} IC (Files 1 & 2)")) {
                    self.generate(&ctx, GraphRequest::CompareFactions);
                }
            });
            group(&mut columns[3], "Tools", |ui| {
                ui.columns(2, |halves| {
                    if full(&mut halves[0], true, "Live Dashboard") {
                        self.open_live_dashboard(&ctx);
                    }
                    if full(&mut halves[1], true, "Rankings") {
                        self.rankings_window.get_or_insert_with(|| {
                            RankingsWindow::new(self.settings.smoothing_days.max(1))
                        });
                    }
                    let library_ok = self.library.is_some();
                    let response = halves[0].add_enabled(
                        library_ok,
                        Button::new("Game Library")
                            .min_size(egui::vec2(halves[0].available_width(), 28.0)),
                    );
                    if let Some(err) = &self.library_error {
                        response.on_disabled_hover_text(format!("Unavailable: {err}"));
                    } else if response.clicked() {
                        self.open_library();
                    }
                    if full(&mut halves[1], true, "Settings") {
                        self.settings_window.get_or_insert_with(SettingsWindow::new);
                    }
                });
                let log_label = if self.show_log {
                    "Hide Log Viewer"
                } else {
                    "Show Log Viewer"
                };
                if full(ui, true, log_label) {
                    self.show_log = !self.show_log;
                }
            });
        });
        ui.add_space(4.0);
    }

    /// One line about the running game under the HOI4 buttons.
    fn game_line(&self, ui: &mut Ui) {
        if !self.settings.watch_game {
            return;
        }
        match (&self.automation.game, &self.automation.dll) {
            (None, _) => {
                ui.weak("HOI4 is not running.");
            }
            (Some(game), Some(_)) => {
                let version = game.version.as_deref().unwrap_or("?");
                let text = match self.automation.inject_at {
                    Some(at) => format!(
                        "HOI4 {version}: collecting in {} s",
                        at.saturating_duration_since(Instant::now()).as_secs()
                    ),
                    None => format!("HOI4 {version} is running."),
                };
                ui.weak(text);
            }
            (Some(game), None) => {
                ui.colored_label(
                    ui.visuals().warn_fg_color,
                    format!(
                        "No DLL for HOI4 {}: add it in Settings.",
                        game.version.as_deref().unwrap_or("?")
                    ),
                );
            }
        }
    }

    fn open_live_dashboard(&mut self, ctx: &egui::Context) {
        if self.live.is_none() {
            self.live = Some(LiveDashboard::new(self.settings.live_interval_secs));
        }
        ctx.request_repaint();
        self.set_status("Live dashboard opened");
    }

    fn log_viewer(ui: &mut Ui) {
        ui.strong("Log Viewer");
        let row_height = ui.text_style_height(&egui::TextStyle::Monospace);
        let total = logging::lines().map_or(0, |l| l.len());
        ScrollArea::both()
            .stick_to_bottom(true)
            .auto_shrink(false)
            .show_rows(ui, row_height, total, |ui, range| {
                // Copy the visible rows out so the lock is not held while drawing
                // (drawing may log, which would re-enter the logger).
                let rows: Vec<String> = logging::lines()
                    .map(|lines| {
                        lines
                            .range(range.start..range.end.min(lines.len()))
                            .cloned()
                            .collect()
                    })
                    .unwrap_or_default();
                for row in rows {
                    ui.add(egui::Label::new(RichText::new(row).monospace()).extend());
                }
            });
    }

    // ----- secondary windows -------------------------------------------------

    fn show_secondary_windows(&mut self, ctx: &egui::Context) {
        self.graph_windows.retain(|w| w.lock().unwrap().is_open());
        for window in &self.graph_windows {
            GraphWindow::show(window, ctx, self.icon.as_ref());
        }

        if self.live.is_some() {
            let selected = self.selected_tags();
            let sink = self.library_sink();
            let file = self.files[0]
                .path()
                .map(Path::to_path_buf)
                .or_else(|| self.output_path());
            let view = self.view();
            let labels = Labels {
                names: self.settings.country_names.then_some(&self.names),
                stable_colors: self.settings.stable_colors,
            };
            let live = self.live.as_mut().unwrap();
            live.show(
                ctx,
                LiveInputs {
                    file: file.as_deref(),
                    sink,
                    selected: &selected,
                    selection_gen: self.selection_gen,
                    labels,
                    view,
                },
                self.icon.as_ref(),
            );
            let saved = live.take_saved();
            if !live.is_open() {
                self.live = None;
            }
            for outcome in saved {
                self.on_library_changed(outcome);
            }
        }

        let mut actions = Vec::new();
        if let (Some(window), Some(library)) = (&mut self.library_window, &mut self.library) {
            actions = window.show(ctx, library, self.icon.as_ref());
            if !window.is_open() {
                self.library_window = None;
                // Countries of games that are no longer ticked anywhere go away.
                actions.push(LibraryAction::SelectionChanged(Vec::new()));
            }
        }
        for action in actions {
            self.handle_library_action(ctx, action);
        }

        if self.rankings_window.is_some() {
            let mut sources: Vec<(String, Arc<Dataset>)> = Vec::new();
            for (i, slot) in self.files.iter().enumerate() {
                if let Some(data) = slot.data() {
                    sources.push((format!("Data File {}", i + 1), data.clone()));
                }
            }
            if let Some(data) = self.live.as_ref().and_then(LiveDashboard::data) {
                sources.push(("Live game".to_owned(), data.clone()));
            }
            for &id in &self.library_selection {
                if let Some(data) = self.library_data.get(&id) {
                    sources.push((format!("Library: {}", self.library_name(id)), data.clone()));
                }
            }
            let labels = Labels {
                names: self.settings.country_names.then_some(&self.names),
                stable_colors: self.settings.stable_colors,
            };
            let window = self.rankings_window.as_mut().unwrap();
            window.show(ctx, &sources, labels, &self.grouping, self.icon.as_ref());
            if !window.is_open() {
                self.rankings_window = None;
            }
        }

        if let Some(window) = &mut self.settings_window {
            let before = self.settings.clone();
            let game_version = self
                .automation
                .game
                .as_ref()
                .and_then(|g| g.version.clone());
            let status = GameStatus {
                running_version: game_version.as_deref(),
                matched_dll: self.automation.dll.as_deref(),
                country_names: self.names.len(),
            };
            let actions = window.show(
                ctx,
                &mut self.settings,
                &status,
                &self.update_status,
                self.icon.as_ref(),
            );
            if !window.is_open() {
                self.settings_window = None;
            }
            self.apply_settings(ctx, &before);
            for action in actions {
                match action {
                    SettingsAction::DllsChanged => self.reload_versions(),
                    SettingsAction::CheckForUpdates => self.check_for_updates(ctx),
                }
            }
        }
    }
}

fn grouping_from(settings: &Settings) -> Grouping {
    if settings.use_custom_groups {
        Grouping::custom(&settings.custom_groups)
    } else {
        Grouping::builtin()
    }
}

fn start_watcher(ctx: &egui::Context) -> GameWatcher {
    let repaint = ctx.clone();
    GameWatcher::start(move || repaint.request_repaint())
}

fn group(ui: &mut Ui, title: &str, add_contents: impl FnOnce(&mut Ui)) {
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.strong(title);
        add_contents(ui);
    });
}

fn file_name(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    )
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.poll_tasks(&ctx);
        self.pump_game_events(&ctx);
        self.handle_dropped_files(&ctx);

        egui::Panel::bottom("status").show(ui, |ui| {
            ui.add_space(2.0);
            ui.horizontal(|ui| {
                ui.label(&self.status);
                if let Some(update) = &self.update {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .link(format!("Download {}", update.tag))
                            .on_hover_text(&update.url)
                            .clicked()
                        {
                            let _ = open::that_detached(&update.url);
                        }
                        ui.colored_label(ui.visuals().warn_fg_color, "New release available:");
                    });
                }
            });
            ui.add_space(2.0);
        });
        if self.show_log {
            egui::Panel::bottom("log")
                .resizable(true)
                .default_size(200.0)
                .min_size(80.0)
                .show(ui, Self::log_viewer);
        }
        egui::Panel::bottom("actions").show(ui, |ui| self.actions(ui));
        egui::Panel::top("files").show(ui, |ui| {
            ui.add_space(6.0);
            self.file_rows(ui);
            ui.add_space(4.0);
        });
        egui::CentralPanel::default().show(ui, |ui| self.country_columns(ui));

        self.show_secondary_windows(&ctx);

        // Remember choices and save settings that changed outside the
        // settings window (version, selection).
        let before = self.settings.clone();
        self.remember_state();
        if self.settings != before {
            self.apply_settings(&ctx, &before);
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.remember_state();
        if self.settings != self.saved_settings
            && let Err(err) = self.settings.save(&self.settings_path)
        {
            log::error!("Could not save settings: {err}");
        }
    }
}
