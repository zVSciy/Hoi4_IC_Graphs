//! Settings window, including the DataCollection DLL manager.

use std::{path::PathBuf, sync::Arc};

use eframe::egui::{self, RichText, ScrollArea, Ui, ViewportBuilder, ViewportId};

use super::dialogs;
use crate::{
    inject::{self, DllEntry, DllKind, DllSource},
    settings::{GroupSetting, Settings},
    update,
};

const VIEWPORT: &str = "settings";

/// What the main window knows about the game, for display.
pub struct GameStatus<'a> {
    pub running_version: Option<&'a str>,
    pub matched_dll: Option<&'a str>,
    pub country_names: usize,
    /// How many of [`Self::country_names`] came from the HOI4 installation.
    pub game_country_names: usize,
}

/// Requests for the main window.
#[derive(Debug, PartialEq, Eq)]
pub enum SettingsAction {
    /// A DLL was added or removed: re-read the version list.
    DllsChanged,
    CheckForUpdates,
}

pub struct SettingsWindow {
    open: bool,
    dlls: Vec<DllEntry>,
    /// A DLL picked with "Add DLL…" that still needs its version confirmed.
    pending_dll: Option<(PathBuf, String)>,
    message: Option<(bool, String)>,
}

impl SettingsWindow {
    pub fn new() -> Self {
        Self {
            open: true,
            dlls: inject::catalog(DllKind::DataCollection),
            pending_dll: None,
            message: None,
        }
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Shows a result message in the DLL section (`ok` = success colour).
    pub fn set_message(&mut self, ok: bool, message: impl Into<String>) {
        self.message = Some((ok, message.into()));
    }

    pub fn show(
        &mut self,
        ctx: &egui::Context,
        settings: &mut Settings,
        game: &GameStatus<'_>,
        update_status: &str,
        icon: Option<&Arc<egui::IconData>>,
    ) -> Vec<SettingsAction> {
        let mut actions = Vec::new();
        let mut builder = ViewportBuilder::default()
            .with_title("HOI4 IC Graphs - Settings")
            .with_inner_size([720.0, 820.0])
            .with_min_inner_size([560.0, 420.0]);
        if let Some(icon) = icon {
            builder = builder.with_icon(icon.clone());
        }
        ctx.show_viewport_immediate(ViewportId::from_hash_of(VIEWPORT), builder, |ui, _class| {
            egui::CentralPanel::default().show(ui, |ui| {
                ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                    self.game_section(ui, settings, game);
                    self.dll_section(ui, game, &mut actions);
                    library_section(ui, settings);
                    graphs_section(ui, settings, game);
                    groups_section(ui, settings);
                    updates_section(ui, settings, update_status, &mut actions);
                });
            });
            if ui.input(|i| i.viewport().close_requested()) {
                self.open = false;
            }
        });
        actions
    }

    fn game_section(&mut self, ui: &mut Ui, s: &mut Settings, game: &GameStatus<'_>) {
        heading(ui, "HOI4 & Data Collection");
        ui.label(match game.running_version {
            Some(v) => format!("Game: HOI4 {v} is running."),
            None if s.watch_game => "Game: not running.".to_owned(),
            None => "Game: not watched.".to_owned(),
        });
        ui.checkbox(&mut s.watch_game, "Watch for HOI4 starting and stopping");
        ui.add_enabled_ui(s.watch_game, |ui| {
            ui.checkbox(
                &mut s.auto_select_version,
                "Select the DataCollection version that matches the game",
            );
            ui.horizontal(|ui| {
                ui.checkbox(&mut s.auto_inject, "Start data collection automatically,");
                ui.add_enabled(
                    s.auto_inject,
                    egui::DragValue::new(&mut s.inject_delay_secs)
                        .range(0..=600)
                        .suffix(" s"),
                );
                ui.label("after the game starts");
            })
            .response
            .on_hover_text(
                "The game needs time to load before data collection can attach. If the game \
                 was already running when the app started, there is no wait. Only a DLL made \
                 for the exact game version is ever used.",
            );
            ui.checkbox(
                &mut s.auto_live,
                "Open the live dashboard when data collection is running",
            );
        });
        ui.horizontal(|ui| {
            ui.label("Output file:");
            let shown = s.output_file.as_ref().map_or_else(
                || "output.txt in the HOI4 folder".to_owned(),
                |p| p.display().to_string(),
            );
            ui.monospace(shown);
        });
        ui.horizontal(|ui| {
            if ui
                .button("Choose…")
                .on_hover_text("Only needed if your DataCollection writes somewhere else")
                .clicked()
                && let Some(path) = dialogs::open_data_file("Where DataCollection writes its data")
            {
                s.output_file = Some(path);
            }
            if ui
                .add_enabled(s.output_file.is_some(), egui::Button::new("Use Default"))
                .clicked()
            {
                s.output_file = None;
            }
        });
    }

    fn dll_section(
        &mut self,
        ui: &mut Ui,
        game: &GameStatus<'_>,
        actions: &mut Vec<SettingsAction>,
    ) {
        heading(ui, "DataCollection DLLs");
        ui.label(
            "When a new HOI4 version comes out, add the matching DataCollection DLL here; \
             no new app build is needed.",
        );
        if let Some(version) = game.running_version {
            match game.matched_dll {
                Some(dll) => ui.label(format!("HOI4 {version} uses DataCollection {dll}.")),
                None => ui.colored_label(
                    ui.visuals().warn_fg_color,
                    format!(
                        "There is no DataCollection DLL for HOI4 {version} yet. Add one below."
                    ),
                ),
            };
        }
        ui.add_space(4.0);

        let mut remove = None;
        ScrollArea::vertical()
            .id_salt("dll_list")
            .max_height(190.0)
            .show(ui, |ui| {
                egui::Grid::new("dlls")
                    .striped(true)
                    .spacing([16.0, 4.0])
                    .show(ui, |ui| {
                        ui.strong("Version");
                        ui.strong("Source");
                        ui.label("");
                        ui.end_row();
                        for entry in &self.dlls {
                            ui.label(&entry.version);
                            let source = ui.label(entry.source.label());
                            if let DllSource::Added(path) | DllSource::NextToExe(path) =
                                &entry.source
                            {
                                source.on_hover_text(path.display().to_string());
                            }
                            if matches!(entry.source, DllSource::Added(_)) {
                                if ui.small_button("Remove").clicked() {
                                    remove = Some(entry.version.clone());
                                }
                            } else {
                                ui.label("");
                            }
                            ui.end_row();
                        }
                    });
            });
        if let Some(version) = remove {
            match inject::remove_dll(&version) {
                Ok(()) => self.set_message(true, format!("Removed DataCollection {version}.")),
                Err(err) => self.set_message(false, err),
            }
            self.dlls = inject::catalog(DllKind::DataCollection);
            actions.push(SettingsAction::DllsChanged);
        }

        ui.add_space(4.0);
        match &mut self.pending_dll {
            None => {
                ui.horizontal(|ui| {
                    if ui.button("Add DLL…").clicked()
                        && let Some(path) = rfd::FileDialog::new()
                            .set_title("Select a DataCollection DLL")
                            .add_filter("DLL", &["dll"])
                            .pick_file()
                    {
                        let version = inject::version_from_file_name(&path)
                            .or_else(|| game.running_version.map(str::to_owned))
                            .unwrap_or_default();
                        self.pending_dll = Some((path, version));
                    }
                    if ui.button("Open DLL Folder").clicked() {
                        let dir = inject::user_dir(DllKind::DataCollection);
                        let _ = std::fs::create_dir_all(&dir);
                        if let Err(err) = open::that_detached(&dir) {
                            dialogs::error(&format!("Could not open {}: {err}", dir.display()));
                        }
                    }
                });
            }
            Some((path, version)) => {
                let mut done = None;
                ui.group(|ui| {
                    ui.label(format!("Add {}", path.display()));
                    ui.horizontal(|ui| {
                        ui.label("for HOI4 version");
                        ui.add(
                            egui::TextEdit::singleline(version)
                                .desired_width(90.0)
                                .hint_text("1.19.5"),
                        );
                        if ui.button("Add").clicked() {
                            done = Some(inject::add_dll(path, version).map(|_| version.clone()));
                        }
                        if ui.button("Cancel").clicked() {
                            done = Some(Err(String::new()));
                        }
                    });
                });
                match done {
                    Some(Ok(version)) => {
                        self.pending_dll = None;
                        self.set_message(true, format!("Added DataCollection {version}."));
                        self.dlls = inject::catalog(DllKind::DataCollection);
                        actions.push(SettingsAction::DllsChanged);
                    }
                    Some(Err(err)) if err.is_empty() => self.pending_dll = None,
                    Some(Err(err)) => self.set_message(false, err),
                    None => {}
                }
            }
        }
        if let Some((ok, message)) = &self.message {
            let color = if *ok {
                ui.visuals().text_color()
            } else {
                ui.visuals().error_fg_color
            };
            ui.colored_label(color, message);
        }
    }
}

fn heading(ui: &mut Ui, text: &str) {
    ui.add_space(10.0);
    ui.label(RichText::new(text).heading());
    ui.separator();
}

fn library_section(ui: &mut Ui, s: &mut Settings) {
    heading(ui, "Game Library & Live Dashboard");
    ui.checkbox(
        &mut s.auto_save,
        "Save loaded files and live games in the game library automatically",
    );
    ui.horizontal(|ui| {
        ui.label("Check the live file every");
        ui.add(
            egui::DragValue::new(&mut s.live_interval_secs)
                .range(1..=3600)
                .suffix(" s"),
        );
    });
    ui.add_enabled_ui(s.auto_save, |ui| {
        ui.horizontal(|ui| {
            ui.label("Save a running game every");
            ui.add(
                egui::DragValue::new(&mut s.live_save_every_secs)
                    .range(5..=3600)
                    .suffix(" s"),
            );
        });
    });
}

fn graphs_section(ui: &mut Ui, s: &mut Settings, game: &GameStatus<'_>) {
    heading(ui, "Graphs");
    let names = ui.checkbox(&mut s.country_names, "Show country names instead of tags");
    names.on_hover_text(match game.game_country_names {
        0 => format!("{} built-in country names", game.country_names),
        from_game => format!(
            "{} country names: built-in, plus {from_game} from your HOI4 installation              (mod and other countries without a built-in name)",
            game.country_names
        ),
    });
    ui.checkbox(
        &mut s.stable_colors,
        "Give each country the same colour in every graph",
    );
    ui.horizontal(|ui| {
        ui.label("Smoothing for new graphs:");
        let label = |days: u32| match days {
            0 | 1 => "off".to_owned(),
            days => format!("{days}-day average"),
        };
        egui::ComboBox::from_id_salt("default_smoothing")
            .selected_text(label(s.smoothing_days))
            .show_ui(ui, |ui| {
                for days in [0, 7, 14, 30, 90] {
                    ui.selectable_value(&mut s.smoothing_days, days, label(days));
                }
            });
    });
}

fn groups_section(ui: &mut Ui, s: &mut Settings) {
    heading(ui, "Country Groups");
    ui.label(
        "Groups decide the columns of the main window and what \"Factions\" graphs sum up. \
         Countries in no group are listed under Other.",
    );
    ui.radio_value(
        &mut s.use_custom_groups,
        false,
        "Axis and Allies (built in)",
    );
    ui.radio_value(&mut s.use_custom_groups, true, "My own groups:");
    ui.add_enabled_ui(s.use_custom_groups, |ui| {
        let mut remove = None;
        egui::Grid::new("groups")
            .spacing([8.0, 4.0])
            .show(ui, |ui| {
                ui.strong("Name");
                ui.strong("Countries (tags)");
                ui.end_row();
                for (i, group) in s.custom_groups.iter_mut().enumerate() {
                    ui.add(egui::TextEdit::singleline(&mut group.name).desired_width(140.0));
                    ui.add(
                        egui::TextEdit::singleline(&mut group.tags)
                            .desired_width(330.0)
                            .hint_text("GER ITA JAP"),
                    );
                    if ui.small_button("Remove").clicked() {
                        remove = Some(i);
                    }
                    ui.end_row();
                }
            });
        if let Some(i) = remove {
            s.custom_groups.remove(i);
        }
        if ui.button("Add Group").clicked() {
            s.custom_groups.push(GroupSetting {
                name: format!("Group {}", s.custom_groups.len() + 1),
                tags: String::new(),
            });
        }
    });
}

fn updates_section(ui: &mut Ui, s: &mut Settings, status: &str, actions: &mut Vec<SettingsAction>) {
    heading(ui, "Updates");
    ui.label(match update::current_release() {
        Some(tag) => format!("This is release {tag} (app {}).", env!("CARGO_PKG_VERSION")),
        None => format!(
            "Development build (app {}); update checks need a release build.",
            env!("CARGO_PKG_VERSION")
        ),
    });
    ui.checkbox(
        &mut s.check_updates,
        "Check for a new release when the app starts",
    );
    ui.horizontal(|ui| {
        if ui
            .add_enabled(
                update::current_release().is_some(),
                egui::Button::new("Check Now"),
            )
            .clicked()
        {
            actions.push(SettingsAction::CheckForUpdates);
        }
        if ui.link("Releases page").clicked() {
            let _ = open::that_detached(update::RELEASES_PAGE);
        }
        if !status.is_empty() {
            ui.label(status);
        }
    });
    ui.add_space(10.0);
}
