//! Live dashboard: graphs that follow the data file while the game writes it.

use std::{path::Path, sync::Arc, time::Duration};

use eframe::egui::{self, RichText, Ui, ViewportBuilder, ViewportId};

use super::{chart::ViewOptions, chart_panel::ChartPanel};
use crate::{
    analysis::{
        self, Source,
        factions::{self, Labels},
    },
    data::{Dataset, Metric, Tag},
    library::SaveOutcome,
    live::{LibrarySink, LiveEvent, LiveMonitor},
    logging,
};

/// Countries graphed when nothing is selected in the main window.
const DEFAULT_COUNTRY_LIMIT: usize = 10;

pub struct LiveDashboard {
    open: bool,
    monitor: Option<LiveMonitor>,
    interval_secs: u32,
    data: Option<Arc<Dataset>>,
    panel: Option<ChartPanel>,
    /// Selection generation the current graph was built from.
    built_for_selection: Option<u64>,
    stats: String,
    status: String,
    /// Library saves done by the monitor, collected by the main window.
    saved: Vec<SaveOutcome>,
}

/// What the main window hands the dashboard every frame.
pub struct LiveInputs<'a> {
    /// Data File 1, monitored when "Start Live Monitoring" is clicked.
    pub file: Option<&'a Path>,
    pub sink: Option<LibrarySink>,
    /// Countries ticked in the main window.
    pub selected: &'a [Tag],
    /// Changes whenever the selection or labelling changes.
    pub selection_gen: u64,
    pub labels: Labels<'a>,
    pub view: ViewOptions,
}

impl LiveDashboard {
    pub fn new(interval_secs: u32) -> Self {
        Self {
            open: true,
            monitor: None,
            interval_secs: interval_secs.max(1),
            data: None,
            panel: None,
            built_for_selection: None,
            stats: "No data loaded".into(),
            status: "Live dashboard ready".into(),
            saved: Vec::new(),
        }
    }
}

pub const VIEWPORT: &str = "live_dashboard";

impl LiveDashboard {
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Library saves since the last call.
    pub fn take_saved(&mut self) -> Vec<SaveOutcome> {
        std::mem::take(&mut self.saved)
    }

    /// The latest live data, if monitoring has produced any.
    pub fn data(&self) -> Option<&Arc<Dataset>> {
        self.data.as_ref()
    }

    pub fn is_monitoring(&self) -> bool {
        self.monitor.is_some()
    }

    /// Starts monitoring `file` (used by the automatic game mode).
    pub fn start_monitoring(
        &mut self,
        ctx: &egui::Context,
        file: &Path,
        sink: Option<LibrarySink>,
    ) {
        self.start(ctx, Some(file), sink);
    }

    /// Stops monitoring; the monitor stores the last data before it ends.
    pub fn stop_monitoring(&mut self) {
        if self.monitor.take().is_some() {
            self.set_status("Live monitoring stopped");
        }
    }

    /// Shows the dashboard window.
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        inputs: LiveInputs<'_>,
        icon: Option<&Arc<egui::IconData>>,
    ) {
        self.pump_events();
        if self.built_for_selection != Some(inputs.selection_gen) {
            self.rebuild_graph(&inputs);
        }
        let (file, sink) = (inputs.file, inputs.sink);

        let mut builder = ViewportBuilder::default()
            .with_title("HOI4 IC Graphs - Live Dashboard")
            .with_inner_size([1400.0, 1100.0])
            .with_min_inner_size([1000.0, 700.0]);
        if let Some(icon) = icon {
            builder = builder.with_icon(icon.clone());
        }
        ctx.show_viewport_immediate(ViewportId::from_hash_of(VIEWPORT), builder, |ui, _class| {
            egui::Panel::bottom("live_status").show(ui, |ui| {
                ui.label(format!("Live Dashboard: {}", self.status))
            });
            egui::Panel::top("live_controls").show(ui, |ui| self.controls(ui, file, sink.as_ref()));
            egui::CentralPanel::default().show(ui, |ui| match &mut self.panel {
                Some(panel) => panel.ui(ui),
                None => {
                    ui.centered_and_justified(|ui| {
                        ui.label(
                            RichText::new(
                                "No data - Start live monitoring to see real-time graphs",
                            )
                            .size(16.0),
                        );
                    });
                }
            });
            if ui.input(|i| i.viewport().close_requested()) {
                self.open = false;
            }
        });

        if !self.open {
            // Dropping the monitor stops and joins its thread.
            self.monitor = None;
            log::info!("Live dashboard closed");
        }
    }

    fn controls(&mut self, ui: &mut Ui, file: Option<&Path>, sink: Option<&LibrarySink>) {
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.group(|ui| {
                ui.vertical(|ui| {
                    ui.strong("Live Monitoring");
                    ui.horizontal(|ui| {
                        let running = self.monitor.is_some();
                        if ui
                            .add_enabled(!running, egui::Button::new("Start Live Monitoring"))
                            .clicked()
                        {
                            self.start(ui.ctx(), file, sink.cloned());
                        }
                        if ui
                            .add_enabled(running, egui::Button::new("Stop Live Monitoring"))
                            .clicked()
                        {
                            self.monitor = None;
                            self.set_status("Live monitoring stopped");
                        }
                        if ui
                            .add_enabled(running, egui::Button::new("Refresh Now"))
                            .clicked()
                            && let Some(monitor) = &self.monitor
                        {
                            monitor.poll_now();
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label("Check interval (seconds):");
                        ui.add_enabled(
                            self.monitor.is_none(),
                            egui::DragValue::new(&mut self.interval_secs).range(1..=3600),
                        );
                    });
                });
            });
            ui.group(|ui| {
                ui.vertical(|ui| {
                    ui.strong("Live Statistics");
                    ui.label(RichText::new(&self.stats).monospace());
                });
            });
        });
        ui.add_space(4.0);
    }

    fn start(&mut self, ctx: &egui::Context, file: Option<&Path>, sink: Option<LibrarySink>) {
        let Some(file) = file else {
            super::dialogs::error("Please select Data File 1 first for live monitoring.");
            return;
        };
        let repaint = ctx.clone();
        self.monitor = Some(LiveMonitor::start(
            file.to_path_buf(),
            Duration::from_secs(u64::from(self.interval_secs)),
            sink,
            move || repaint.request_repaint(),
        ));
    }

    fn pump_events(&mut self) {
        let Some(monitor) = &self.monitor else { return };
        let mut latest = None;
        let mut status = None;
        for event in monitor.events() {
            match event {
                LiveEvent::Updated(data) => latest = Some(data),
                LiveEvent::Status(message) => status = Some(message),
                LiveEvent::Saved(Ok(outcome)) => {
                    if !matches!(outcome, SaveOutcome::Unchanged(_)) {
                        status = Some(format!(
                            "Saved to the game library (game #{})",
                            outcome.id()
                        ));
                    }
                    self.saved.push(outcome);
                }
                LiveEvent::Saved(Err(err)) => {
                    status = Some(format!("Could not save to the game library: {err}"));
                }
            }
        }
        if let Some(message) = status {
            self.set_status(message);
        }
        if let Some(data) = latest {
            self.stats = stats_text(&data);
            self.data = Some(data);
            self.built_for_selection = None;
        }
    }

    fn rebuild_graph(&mut self, inputs: &LiveInputs<'_>) {
        self.built_for_selection = Some(inputs.selection_gen);
        let Some(data) = &self.data else { return };
        let tags: Vec<Tag> = if inputs.selected.is_empty() {
            // Nothing ticked: the biggest producers of the latest day.
            super::rankings_window::rank(data, &[Metric::Mil], usize::MAX, 1)
                .iter()
                .take(DEFAULT_COUNTRY_LIMIT)
                .map(|r| data.tags()[r.row])
                .collect()
        } else {
            inputs.selected.to_vec()
        };
        let groups = factions::individual_groups(&tags, inputs.labels);
        let Some(graph) = analysis::build(
            &[Source {
                dataset: data,
                suffix: "",
            }],
            &groups,
            " - Live",
        ) else {
            return;
        };
        let graph = Arc::new(graph);
        match &mut self.panel {
            Some(panel) => panel.replace_graph(graph),
            None => {
                self.panel = Some(ChartPanel::new(
                    egui::Id::unique(VIEWPORT),
                    graph,
                    inputs.view,
                ));
            }
        }
    }

    fn set_status(&mut self, message: impl Into<String>) {
        self.status = message.into();
        log::info!("Live Dashboard: {}", self.status);
    }
}

fn stats_text(data: &Dataset) -> String {
    let Some(latest) = data.dates().last() else {
        return "No data available".into();
    };
    let mut text = format!(
        "Total Dates:           {}\n\
         Latest Date:           {latest}\n\
         Total Countries:       {}\n",
        data.dates().len(),
        data.tags().len(),
    );
    for metric in Metric::ALL.into_iter().filter(|&m| data.has_metric(m)) {
        let token = metric.token();
        let countries = (0..data.tags().len())
            .filter(|&i| data.has_positive(metric, i))
            .count();
        text.push_str(&format!(
            "Countries with {token} IC: {countries}\n\
             Latest {token} IC Total:   {:.2}\n",
            data.latest_total(metric)
        ));
    }
    text.push_str(&format!("Last Updated:          {}", logging::clock()));
    text
}
