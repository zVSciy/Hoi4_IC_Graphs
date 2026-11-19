//! Standalone OS window showing one generated graph.
//!
//! Rendered as a deferred viewport: it repaints independently of the main
//! window, so hovering a chart never redraws anything else.

use std::sync::{Arc, Mutex};

use eframe::egui::{self, ViewportBuilder, ViewportId};

use super::{chart::ViewOptions, chart_panel::ChartPanel};
use crate::analysis::GraphData;

pub struct GraphWindow {
    viewport: ViewportId,
    title: String,
    panel: ChartPanel,
    open: bool,
}

pub type SharedGraphWindow = Arc<Mutex<GraphWindow>>;

impl GraphWindow {
    pub fn new(serial: u64, graph: Arc<GraphData>, view: ViewOptions) -> SharedGraphWindow {
        let viewport = ViewportId::from_hash_of(("graph", serial));
        let title = format!("IC Graphs{}", graph.title_suffix);
        Arc::new(Mutex::new(Self {
            viewport,
            title,
            panel: ChartPanel::new(egui::Id::unique(viewport), graph, view),
            open: true,
        }))
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Registers the viewport for this frame; must be called every frame while open.
    pub fn show(
        window: &SharedGraphWindow,
        ctx: &egui::Context,
        icon: Option<&Arc<egui::IconData>>,
    ) {
        let (viewport, mut builder) = {
            let w = window.lock().unwrap();
            let builder = ViewportBuilder::default()
                .with_title(w.title.clone())
                .with_inner_size([1200.0, 900.0])
                .with_min_inner_size([800.0, 600.0]);
            (w.viewport, builder)
        };
        if let Some(icon) = icon {
            builder = builder.with_icon(icon.clone());
        }
        let window = window.clone();
        ctx.show_viewport_deferred(viewport, builder, move |ui, _class| {
            let mut w = window.lock().unwrap();
            egui::CentralPanel::default().show(ui, |ui| w.panel.ui(ui));
            if ui.input(|i| i.viewport().close_requested()) {
                w.open = false;
                ui.ctx().request_repaint_of(ViewportId::ROOT);
            }
        });
    }
}
