//! "Game Library" window: browse, filter, annotate and compare stored games.

use std::sync::Arc;

use eframe::egui::{self, RichText, ScrollArea, Ui, ViewportBuilder, ViewportId};
use rustc_hash::FxHashSet;

use super::dialogs;
use crate::library::{self, GameMeta, Library};

/// Requests for the main window, which owns the datasets and graph windows.
pub enum LibraryAction {
    /// Chart the selected games against each other.
    Compare {
        ids: Vec<i64>,
        by_faction: bool,
        /// Line games up by day since start instead of calendar date.
        from_start: bool,
    },
    /// Put a stored game into Data File 1 / 2.
    LoadIntoSlot { id: i64, slot: usize },
    /// The checked games changed (their countries become selectable).
    SelectionChanged(Vec<i64>),
    /// Store the files currently in Data File 1 / 2.
    SaveCurrentFiles,
}

const VIEWPORT: &str = "game_library";

pub struct LibraryWindow {
    open: bool,
    games: Vec<GameMeta>,
    versions: Vec<String>,
    search: String,
    /// `None`: all versions, `Some("")`: games without a version.
    version_filter: Option<String>,
    selected: Vec<i64>,
    /// Games whose editable fields changed and still need writing.
    dirty: FxHashSet<i64>,
    confirm_delete: Option<i64>,
    /// Compare by day since start instead of calendar date.
    from_start: bool,
    status: String,
}

impl LibraryWindow {
    pub fn new(library: &Library) -> Self {
        let mut window = Self {
            open: true,
            games: Vec::new(),
            versions: Vec::new(),
            search: String::new(),
            version_filter: None,
            selected: Vec::new(),
            dirty: FxHashSet::default(),
            confirm_delete: None,
            from_start: false,
            status: String::new(),
        };
        window.refresh(library);
        window
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Re-reads the game list (after a save or an edit elsewhere).
    pub fn refresh(&mut self, library: &Library) {
        match library.games().and_then(|g| Ok((g, library.versions()?))) {
            Ok((games, versions)) => {
                self.selected.retain(|id| games.iter().any(|g| g.id == *id));
                self.games = games;
                self.versions = versions;
            }
            Err(err) => self.status = format!("Could not read the library: {err}"),
        }
    }

    pub fn set_status(&mut self, message: impl Into<String>) {
        self.status = message.into();
    }

    pub fn show(
        &mut self,
        ctx: &egui::Context,
        library: &mut Library,
        icon: Option<&Arc<egui::IconData>>,
    ) -> Vec<LibraryAction> {
        let mut actions = Vec::new();
        let mut builder = ViewportBuilder::default()
            .with_title("HOI4 IC Graphs - Game Library")
            .with_inner_size([1250.0, 720.0])
            .with_min_inner_size([900.0, 450.0]);
        if let Some(icon) = icon {
            builder = builder.with_icon(icon.clone());
        }
        ctx.show_viewport_immediate(ViewportId::from_hash_of(VIEWPORT), builder, |ui, _class| {
            egui::Panel::top("library_filters").show(ui, |ui| self.filters(ui, library));
            egui::Panel::bottom("library_status").show(ui, |ui| {
                ui.add_space(2.0);
                ui.label(&self.status);
                ui.add_space(2.0);
            });
            egui::Panel::bottom("library_actions").show(ui, |ui| self.bottom_bar(ui, &mut actions));
            egui::CentralPanel::default().show(ui, |ui| self.table(ui, library, &mut actions));
            if ui.input(|i| i.viewport().close_requested()) {
                self.flush_edits(library);
                self.open = false;
            }
        });
        actions
    }

    fn visible(&self) -> impl Iterator<Item = &GameMeta> {
        self.games.iter().filter(|game| {
            game.matches(&self.search)
                && self
                    .version_filter
                    .as_ref()
                    .is_none_or(|v| &game.version == v)
        })
    }

    fn filters(&mut self, ui: &mut Ui, library: &mut Library) {
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.label("Search:");
            ui.add(
                egui::TextEdit::singleline(&mut self.search)
                    .hint_text("name, notes, version, path")
                    .desired_width(260.0),
            );
            ui.separator();
            ui.label("HOI4 version:");
            let shown = match &self.version_filter {
                None => "All versions".to_owned(),
                Some(v) if v.is_empty() => "(no version)".to_owned(),
                Some(v) => v.clone(),
            };
            egui::ComboBox::from_id_salt("version_filter")
                .selected_text(shown)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.version_filter, None, "All versions");
                    for version in &self.versions {
                        ui.selectable_value(
                            &mut self.version_filter,
                            Some(version.clone()),
                            version,
                        );
                    }
                    ui.selectable_value(
                        &mut self.version_filter,
                        Some(String::new()),
                        "(no version)",
                    );
                });
            ui.separator();
            let shown = self.visible().count();
            ui.label(format!(
                "{shown} of {} games, {} selected",
                self.games.len(),
                self.selected.len()
            ));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .button("Open Folder")
                    .on_hover_text(library.path().display().to_string())
                    .clicked()
                    && let Some(dir) = library.path().parent()
                    && let Err(err) = open::that_detached(dir)
                {
                    dialogs::error(&format!("Could not open the library folder: {err}"));
                }
                if ui.button("Refresh").clicked() {
                    self.refresh(library);
                }
                if ui
                    .button("Import…")
                    .on_hover_text("Add games exported from another library")
                    .clicked()
                {
                    self.import(library);
                }
            });
        });
        ui.add_space(4.0);
    }

    fn table(&mut self, ui: &mut Ui, library: &Library, actions: &mut Vec<LibraryAction>) {
        if self.games.is_empty() {
            ui.centered_and_justified(|ui| {
                ui.weak(
                    "No games yet. Every file you load, and every game the live dashboard \
                     watches, is saved here automatically (\"Auto-save to library\").",
                );
            });
            return;
        }
        let visible: Vec<i64> = self.visible().map(|g| g.id).collect();
        let mut selection_changed = false;
        let mut deleted = None;
        let mut export = None;

        ScrollArea::both().auto_shrink(false).show(ui, |ui| {
            egui::Grid::new("games")
                .striped(true)
                .num_columns(13)
                .spacing([10.0, 6.0])
                .show(ui, |ui| {
                    for header in [
                        "",
                        "Name",
                        "Version",
                        "From",
                        "To",
                        "Days",
                        "Countries",
                        "CIV",
                        "Top producer",
                        "Total MIL",
                        "Saved",
                        "Notes",
                        "",
                    ] {
                        ui.strong(header);
                    }
                    ui.end_row();

                    for id in &visible {
                        let Some(game) = self.games.iter_mut().find(|g| g.id == *id) else {
                            continue;
                        };
                        let mut checked = self.selected.contains(id);
                        if ui.checkbox(&mut checked, "").changed() {
                            if checked {
                                self.selected.push(*id);
                            } else {
                                self.selected.retain(|s| s != id);
                            }
                            selection_changed = true;
                        }

                        let mut edited = false;
                        let mut committed = false;
                        for (field, width, hint) in [
                            (&mut game.name, 190.0, "name"),
                            (&mut game.version, 80.0, "e.g. 1.19.4"),
                        ] {
                            let response = ui.add_sized(
                                [width, 20.0],
                                egui::TextEdit::singleline(field).hint_text(hint),
                            );
                            edited |= response.changed();
                            committed |= response.lost_focus();
                        }
                        let date =
                            |d: Option<crate::data::Day>| d.map_or("-".into(), |d| d.to_string());
                        ui.label(date(game.first_day));
                        ui.label(date(game.last_day));
                        ui.label(game.days.to_string());
                        ui.label(game.countries.to_string());
                        ui.label(if game.has_civ { "yes" } else { "-" });
                        match &game.summary.top_producer {
                            Some((tag, share)) => ui.label(format!("{tag} {share:.0}%")),
                            None => ui.label("-"),
                        }
                        .on_hover_text("Country with the most military output over the game");
                        ui.label(compact(game.summary.totals[0]))
                            .on_hover_text(format!(
                                "Produced over the whole game by all countries:\n\
                             Military {}\nNaval {}\nCivilian {}\n\
                             Peak military output on one day: {}",
                                compact(game.summary.totals[0]),
                                compact(game.summary.totals[1]),
                                compact(game.summary.totals[2]),
                                compact(game.summary.peak_daily_mil)
                            ));
                        ui.label(library::format_local(game.updated_at))
                            .on_hover_text(format!(
                                "First saved {}\nFrom {}\n{} KiB stored",
                                library::format_local(game.saved_at),
                                game.source_path,
                                game.stored_bytes / 1024
                            ));
                        let response = ui.add_sized(
                            [170.0, 20.0],
                            egui::TextEdit::singleline(&mut game.notes).hint_text("notes"),
                        );
                        edited |= response.changed();
                        committed |= response.lost_focus();
                        if edited {
                            self.dirty.insert(*id);
                        }
                        if committed && self.dirty.remove(id) {
                            let (name, version, notes) =
                                (game.name.clone(), game.version.clone(), game.notes.clone());
                            match library.update_details(*id, &name, &version, &notes) {
                                Ok(()) => self.status = format!("Saved changes to \"{name}\"."),
                                Err(err) => self.status = format!("Could not save changes: {err}"),
                            }
                        }

                        if self.confirm_delete == Some(*id) {
                            ui.horizontal(|ui| {
                                if ui
                                    .small_button(
                                        RichText::new("Delete!").color(ui.visuals().error_fg_color),
                                    )
                                    .clicked()
                                {
                                    deleted = Some(*id);
                                }
                                if ui.small_button("Keep").clicked() {
                                    self.confirm_delete = None;
                                }
                            });
                        } else {
                            ui.menu_button("Actions", |ui| {
                                for slot in 0..2 {
                                    if ui
                                        .button(format!("Load as Data File {}", slot + 1))
                                        .clicked()
                                    {
                                        actions.push(LibraryAction::LoadIntoSlot { id: *id, slot });
                                        ui.close();
                                    }
                                }
                                if ui.button("Export…").clicked() {
                                    export = Some((*id, game.name.clone()));
                                    ui.close();
                                }
                                ui.separator();
                                if ui.button("Delete…").clicked() {
                                    self.confirm_delete = Some(*id);
                                    ui.close();
                                }
                            });
                        }
                        ui.end_row();
                    }
                });
        });

        if let Some((id, name)) = export {
            self.export(library, id, &name);
        }
        if let Some(id) = deleted {
            self.confirm_delete = None;
            match library.delete(id) {
                Ok(()) => {
                    self.status = format!("Deleted game #{id}.");
                    self.selected.retain(|s| *s != id);
                    selection_changed = true;
                    self.refresh(library);
                }
                Err(err) => self.status = format!("Could not delete game #{id}: {err}"),
            }
        }
        if selection_changed {
            actions.push(LibraryAction::SelectionChanged(self.selected.clone()));
        }
    }

    fn bottom_bar(&mut self, ui: &mut Ui, actions: &mut Vec<LibraryAction>) {
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            let enough = self.selected.len() >= 2;
            let tip = "Uses the countries ticked in the main window";
            ui.label(format!("Compare {} selected games:", self.selected.len()));
            if ui
                .add_enabled(enough, egui::Button::new("Countries"))
                .on_hover_text(tip)
                .on_disabled_hover_text("Tick at least two games")
                .clicked()
            {
                actions.push(LibraryAction::Compare {
                    ids: self.selected.clone(),
                    by_faction: false,
                    from_start: self.from_start,
                });
            }
            if ui
                .add_enabled(enough, egui::Button::new("Factions"))
                .on_hover_text(tip)
                .on_disabled_hover_text("Tick at least two games")
                .clicked()
            {
                actions.push(LibraryAction::Compare {
                    ids: self.selected.clone(),
                    by_faction: true,
                    from_start: self.from_start,
                });
            }
            ui.checkbox(&mut self.from_start, "Align by day since start")
                .on_hover_text(
                    "Compare games that started at different dates (e.g. 1936 and 1939) \
                     day by day from their start",
                );
            ui.separator();
            if ui.button("Select All Shown").clicked() {
                let visible: Vec<i64> = self.visible().map(|g| g.id).collect();
                for id in visible {
                    if !self.selected.contains(&id) {
                        self.selected.push(id);
                    }
                }
                actions.push(LibraryAction::SelectionChanged(self.selected.clone()));
            }
            if ui
                .add_enabled(
                    !self.selected.is_empty(),
                    egui::Button::new("Clear Selection"),
                )
                .clicked()
            {
                self.selected.clear();
                actions.push(LibraryAction::SelectionChanged(Vec::new()));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .button("Save Data Files Now")
                    .on_hover_text("Store the files loaded as Data File 1 / 2")
                    .clicked()
                {
                    actions.push(LibraryAction::SaveCurrentFiles);
                }
            });
        });
        ui.add_space(6.0);
    }

    fn export(&mut self, library: &Library, id: i64, name: &str) {
        let stem: String = name
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || c == '-' || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        let Some(path) = dialogs::save_file("HOI4 game", library::EXPORT_EXTENSION, &stem) else {
            return;
        };
        self.status = match library.export_game(id, &path) {
            Ok(()) => format!("Exported \"{name}\" to {}.", path.display()),
            Err(err) => format!("Could not export \"{name}\": {err}"),
        };
    }

    fn import(&mut self, library: &mut Library) {
        let Some(paths) = rfd::FileDialog::new()
            .set_title("Import games")
            .add_filter("HOI4 game", &[library::EXPORT_EXTENSION])
            .pick_files()
        else {
            return;
        };
        let (mut added, mut known, mut failed) = (0, 0, Vec::new());
        for path in &paths {
            match library.import_game(path) {
                Ok(library::SaveOutcome::Unchanged(_)) => known += 1,
                Ok(_) => added += 1,
                Err(err) => failed.push(format!("{}: {err}", path.display())),
            }
        }
        self.status = format!("Imported {added} game(s); {known} already in the library.");
        if !failed.is_empty() {
            dialogs::error(&format!(
                "Some files could not be imported:\n{}",
                failed.join("\n")
            ));
        }
        self.refresh(library);
    }

    /// Writes edits still pending when the window closes.
    fn flush_edits(&mut self, library: &Library) {
        for id in std::mem::take(&mut self.dirty) {
            if let Some(game) = self.games.iter().find(|g| g.id == id) {
                let _ = library.update_details(id, &game.name, &game.version, &game.notes);
            }
        }
    }

    /// Display name of a stored game (used for series names).
    pub fn name_of(&self, id: i64) -> Option<&str> {
        self.games
            .iter()
            .find(|g| g.id == id)
            .map(|g| g.name.as_str())
    }
}

/// 1234567 -> "1.23M", 45600 -> "45.6k".
fn compact(value: f64) -> String {
    if value >= 1e6 {
        format!("{:.2}M", value / 1e6)
    } else if value >= 1e3 {
        format!("{:.1}k", value / 1e3)
    } else {
        format!("{value:.0}")
    }
}
