//! A tabbed (Military / Naval / Civilian) chart with export actions.
//! Shared by graph windows and the live dashboard.

use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

use eframe::egui::{self, Id, Rect, Ui};

use super::{
    chart::{self, PreparedChart, ViewOptions},
    dialogs,
};
use crate::{analysis::GraphData, data::Metric, export};

pub struct ChartPanel {
    id: Id,
    graph: Arc<GraphData>,
    /// One prepared chart per metric that has data (older DataCollection
    /// versions write no `CIV` lines, so that tab is left out for them).
    charts: Vec<(Metric, PreparedChart)>,
    tab: Metric,
    hidden: Vec<bool>,
    view: ViewOptions,
    /// View and visibility the prepared charts were built for.
    built_for: (ViewOptions, Vec<bool>),
    plot_rect: Option<Rect>,
    temp_files: Vec<PathBuf>,
    /// Result of the last background export, shown in the toolbar.
    notice: Arc<Mutex<Option<String>>>,
}

impl ChartPanel {
    pub fn new(id: Id, graph: Arc<GraphData>, view: ViewOptions) -> Self {
        let mut panel = Self {
            id,
            hidden: vec![false; graph.len()],
            view,
            built_for: (view, Vec::new()),
            charts: Vec::new(),
            graph: Arc::default(),
            tab: Metric::Mil,
            plot_rect: None,
            temp_files: Vec::new(),
            notice: Arc::default(),
        };
        panel.set_graph(graph);
        panel
    }

    /// Swaps in new data, keeping the tab and per-series visibility (matched by name).
    pub fn replace_graph(&mut self, graph: Arc<GraphData>) {
        let hidden_names: Vec<&str> = self
            .graph
            .names()
            .zip(&self.hidden)
            .filter_map(|(name, &hidden)| hidden.then_some(name))
            .collect();
        let hidden = graph
            .names()
            .map(|name| hidden_names.contains(&name))
            .collect();
        self.hidden = hidden;
        self.set_graph(graph);
    }

    fn set_graph(&mut self, graph: Arc<GraphData>) {
        let metrics = match graph.metrics() {
            metrics if metrics.is_empty() => vec![Metric::Mil],
            metrics => metrics,
        };
        if !metrics.contains(&self.tab) {
            self.tab = metrics[0];
        }
        self.graph = graph;
        self.rebuild(&metrics);
    }

    fn rebuild(&mut self, metrics: &[Metric]) {
        self.hidden.resize(self.graph.len(), false);
        self.charts = metrics
            .iter()
            .map(|&metric| {
                let chart = PreparedChart::new(&self.graph, metric, self.view, &self.hidden);
                (metric, chart)
            })
            .collect();
        self.built_for = (self.view, self.hidden.clone());
    }

    /// Rebuilds the charts if the view changed (or, for share view, which
    /// series are visible, since shares are among the visible ones).
    fn refresh_view(&mut self) {
        let stale =
            self.built_for.0 != self.view || (self.view.share && self.built_for.1 != self.hidden);
        if stale {
            self.rebuild(&self.metrics());
        }
    }

    fn metrics(&self) -> Vec<Metric> {
        self.charts.iter().map(|(metric, _)| *metric).collect()
    }

    pub fn ui(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            for (metric, _) in &self.charts {
                ui.selectable_value(&mut self.tab, *metric, format!("{} IC", metric.label()));
            }
            ui.separator();
            let smoothing = |days: u32| match days {
                0 | 1 => "No smoothing".to_owned(),
                days => format!("{days}-day average"),
            };
            egui::ComboBox::from_id_salt(self.id.with("smoothing"))
                .selected_text(smoothing(self.view.smoothing))
                .show_ui(ui, |ui| {
                    for days in [0, 7, 14, 30, 90] {
                        ui.selectable_value(&mut self.view.smoothing, days, smoothing(days));
                    }
                })
                .response
                .on_hover_text("Rolling average of the daily values, hides day-to-day noise");
            ui.selectable_value(&mut self.view.share, false, "Values");
            ui.selectable_value(&mut self.view.share, true, "Share %")
                .on_hover_text(
                    "Each series as a percentage of all visible series, e.g. the Axis vs \
                     Allies balance (hide series in the legend to compare fewer)",
                );
            ui.separator();
            if ui
                .button("Export HTML…")
                .on_hover_text("Interactive page with every tab (raw values), opens in any browser")
                .clicked()
            {
                self.save_html();
            }
            if ui
                .button("Export CSV…")
                .on_hover_text("Raw daily and cumulative values for spreadsheets")
                .clicked()
            {
                self.save_csv();
            }
            if ui
                .button("Save PNG…")
                .on_hover_text("Image of the current tab")
                .clicked()
            {
                self.save_png(ui.ctx());
            }
            if ui.button("Open in Browser").clicked() {
                self.open_in_browser();
            }
            if let Some(notice) = self.notice.lock().unwrap().as_deref() {
                ui.weak(notice);
            }
        });
        ui.separator();
        self.refresh_view();
        let Some((_, chart)) = self.charts.iter().find(|(metric, _)| *metric == self.tab) else {
            return;
        };
        self.plot_rect = Some(chart::show(
            ui,
            self.id.with(self.tab as usize),
            chart,
            &mut self.hidden,
        ));
    }

    fn html(&self) -> String {
        export::html::render(&self.graph, &self.metrics(), self.tab, &self.hidden)
    }

    fn save_html(&self) {
        let Some(path) = dialogs::save_file("HTML page", "html", "hoi4_ic_graphs") else {
            return;
        };
        match std::fs::write(&path, self.html()) {
            Ok(()) => self.set_notice(format!("Saved {}", path.display())),
            Err(err) => dialogs::error(&format!("Failed to save graph: {err}")),
        }
    }

    fn save_csv(&self) {
        let Some(path) = dialogs::save_file("CSV table", "csv", "hoi4_ic_data") else {
            return;
        };
        match std::fs::write(&path, export::csv::render(&self.graph, &self.metrics())) {
            Ok(()) => self.set_notice(format!("Saved {}", path.display())),
            Err(err) => dialogs::error(&format!("Failed to save data: {err}")),
        }
    }

    fn save_png(&self, ctx: &egui::Context) {
        let Some(rect) = self.plot_rect else { return };
        let Some(path) = dialogs::save_file("PNG image", "png", self.tab.file_stem()) else {
            return;
        };
        let ppp = ctx.pixels_per_point();
        let notice = self.notice.clone();
        let repaint = ctx.clone();
        // The screenshot arrives after the next frame has been rendered.
        ctx.request_screenshot(move |image| {
            std::thread::spawn(move || {
                let message = match export::save_png(&image, rect, ppp, &path) {
                    Ok(()) => {
                        log::info!("Graph saved to {}", path.display());
                        format!("Saved {}", path.display())
                    }
                    Err(err) => {
                        log::error!("Failed to save PNG: {err}");
                        format!("PNG export failed: {err}")
                    }
                };
                *notice.lock().unwrap() = Some(message);
                repaint.request_repaint();
            });
        });
        ctx.request_repaint();
    }

    fn open_in_browser(&mut self) {
        let stem = format!("{:x}_graphs", self.id.value());
        match export::open_in_browser(&self.html(), &stem) {
            Ok(path) => {
                if !self.temp_files.contains(&path) {
                    self.temp_files.push(path);
                }
            }
            Err(err) => dialogs::error(&format!("Failed to open graph in browser: {err}")),
        }
    }

    fn set_notice(&self, message: String) {
        log::info!("{message}");
        *self.notice.lock().unwrap() = Some(message);
    }
}

impl Drop for ChartPanel {
    fn drop(&mut self) {
        for path in &self.temp_files {
            let _ = std::fs::remove_file(path);
        }
    }
}
