//! Rankings: every country on one date, ordered by output, with world share.

use std::sync::Arc;

use eframe::egui::{self, RichText, ScrollArea, Ui, ViewportBuilder, ViewportId};

use super::style;
use crate::{
    analysis::factions::{self, Grouping, Labels},
    data::{Dataset, Metric},
};

const VIEWPORT: &str = "rankings";

/// Which values are ranked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Measure {
    One(Metric),
    /// Military + naval + civilian.
    All,
}

impl Measure {
    fn label(self) -> &'static str {
        match self {
            Self::One(metric) => metric.label(),
            Self::All => "All (MIL + NAV + CIV)",
        }
    }
}

pub struct RankingsWindow {
    open: bool,
    source: usize,
    measure: Measure,
    /// Running total up to the date instead of that day's value.
    cumulative: bool,
    /// Index into the dataset's dates; `None` follows the latest date.
    day: Option<usize>,
    smoothing: u32,
}

/// One ranked country.
#[derive(Debug, PartialEq)]
pub struct Ranked {
    pub row: usize,
    pub value: f64,
    pub share: f64,
}

impl RankingsWindow {
    pub fn new(smoothing: u32) -> Self {
        Self {
            open: true,
            source: 0,
            measure: Measure::One(Metric::Mil),
            cumulative: false,
            day: None,
            smoothing,
        }
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    /// `sources`: loaded datasets with display names ("Data File 1", ...).
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        sources: &[(String, Arc<Dataset>)],
        labels: Labels<'_>,
        grouping: &Grouping,
        icon: Option<&Arc<egui::IconData>>,
    ) {
        let mut builder = ViewportBuilder::default()
            .with_title("HOI4 IC Graphs - Rankings")
            .with_inner_size([760.0, 820.0])
            .with_min_inner_size([520.0, 400.0]);
        if let Some(icon) = icon {
            builder = builder.with_icon(icon.clone());
        }
        ctx.show_viewport_immediate(ViewportId::from_hash_of(VIEWPORT), builder, |ui, _class| {
            egui::CentralPanel::default().show(ui, |ui| {
                if sources.is_empty() {
                    ui.centered_and_justified(|ui| {
                        ui.weak("Load a data file (or a library game) to see rankings.");
                    });
                    return;
                }
                self.source = self.source.min(sources.len() - 1);
                let dataset = &sources[self.source].1;
                self.controls(ui, sources, dataset);
                ui.separator();
                self.table(ui, dataset, labels, grouping);
            });
            if ui.input(|i| i.viewport().close_requested()) {
                self.open = false;
            }
        });
    }

    fn controls(&mut self, ui: &mut Ui, sources: &[(String, Arc<Dataset>)], dataset: &Dataset) {
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt("rank_source")
                .selected_text(&sources[self.source].0)
                .show_ui(ui, |ui| {
                    for (i, (name, _)) in sources.iter().enumerate() {
                        if ui.selectable_value(&mut self.source, i, name).changed() {
                            self.day = None;
                        }
                    }
                });
            egui::ComboBox::from_id_salt("rank_measure")
                .selected_text(self.measure.label())
                .show_ui(ui, |ui| {
                    for metric in Metric::ALL.into_iter().filter(|&m| dataset.has_metric(m)) {
                        ui.selectable_value(
                            &mut self.measure,
                            Measure::One(metric),
                            metric.label(),
                        );
                    }
                    ui.selectable_value(&mut self.measure, Measure::All, Measure::All.label());
                });
            ui.selectable_value(&mut self.cumulative, false, "On that day");
            ui.selectable_value(&mut self.cumulative, true, "Total so far");
            ui.add_enabled_ui(!self.cumulative, |ui| {
                ui.label("Average over");
                ui.add(
                    egui::DragValue::new(&mut self.smoothing)
                        .range(1..=365)
                        .suffix(" days"),
                );
            });
        });
        let last = dataset.dates().len().saturating_sub(1);
        let mut day = self.day.unwrap_or(last).min(last);
        ui.horizontal(|ui| {
            ui.label("Date:");
            let date = dataset
                .dates()
                .get(day)
                .map_or("-".into(), ToString::to_string);
            ui.style_mut().spacing.slider_width = (ui.available_width() - 220.0).max(120.0);
            if ui
                .add(egui::Slider::new(&mut day, 0..=last).show_value(false))
                .changed()
            {
                self.day = (day != last).then_some(day);
            }
            ui.strong(date);
            if ui
                .add_enabled(self.day.is_some(), egui::Button::new("Latest"))
                .clicked()
            {
                self.day = None;
            }
        });
    }

    fn table(&self, ui: &mut Ui, dataset: &Dataset, labels: Labels<'_>, grouping: &Grouping) {
        let last = dataset.dates().len().saturating_sub(1);
        let day = self.day.unwrap_or(last).min(last);
        let window = if self.cumulative {
            0
        } else {
            self.smoothing.max(1) as usize
        };
        let ranked = rank(dataset, self.measure_metrics(), day, window);
        let world: f64 = ranked.iter().map(|r| r.value).sum();
        ui.label(format!(
            "World total: {}  ·  {} countries",
            format_value(world),
            ranked.len()
        ));
        let top = ranked
            .first()
            .map_or(1.0, |r| r.value)
            .max(f64::MIN_POSITIVE);
        ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            egui::Grid::new("rankings")
                .striped(true)
                .num_columns(6)
                .spacing([14.0, 4.0])
                .show(ui, |ui| {
                    for header in ["#", "Country", "Group", "Value", "Share", ""] {
                        ui.strong(header);
                    }
                    ui.end_row();
                    for (i, r) in ranked.iter().enumerate() {
                        let tag = dataset.tags()[r.row];
                        ui.label((i + 1).to_string());
                        let name = labels.name(tag);
                        if name == tag.to_string() {
                            ui.label(name);
                        } else {
                            ui.label(format!("{name} ({tag})"));
                        }
                        ui.label(grouping.group_name(tag));
                        ui.label(RichText::new(format_value(r.value)).monospace());
                        ui.label(RichText::new(format!("{:5.1}%", r.share)).monospace());
                        let (rect, _) =
                            ui.allocate_exact_size(egui::vec2(160.0, 12.0), egui::Sense::hover());
                        let width = (r.value / top) as f32 * rect.width();
                        let color = style::series_color(if labels.stable_colors {
                            factions::stable_color(tag)
                        } else {
                            0
                        });
                        ui.painter().rect_filled(
                            egui::Rect::from_min_size(rect.min, egui::vec2(width, rect.height())),
                            2.0,
                            color,
                        );
                        ui.end_row();
                    }
                });
        });
    }

    fn measure_metrics(&self) -> &'static [Metric] {
        match self.measure {
            Measure::One(Metric::Mil) => &[Metric::Mil],
            Measure::One(Metric::Nav) => &[Metric::Nav],
            Measure::One(Metric::Civ) => &[Metric::Civ],
            Measure::All => &Metric::ALL,
        }
    }
}

/// Countries with a positive value on `day`, largest first. `window` = 0 ranks
/// the running total up to `day`; otherwise the average of the last `window`
/// days (1 = that day alone). `metrics` are added together.
pub fn rank(dataset: &Dataset, metrics: &[Metric], day: usize, window: usize) -> Vec<Ranked> {
    if dataset.dates().is_empty() {
        return Vec::new();
    }
    let day = day.min(dataset.dates().len() - 1);
    let start = if window == 0 {
        0
    } else {
        (day + 1).saturating_sub(window)
    };
    let days = (day + 1 - start) as f64;
    let mut ranked: Vec<Ranked> = (0..dataset.tags().len())
        .map(|row| {
            let sum: f64 = metrics
                .iter()
                .map(|&m| dataset.row(m, row)[start..=day].iter().sum::<f64>())
                .sum();
            let value = if window == 0 { sum } else { sum / days };
            Ranked {
                row,
                value,
                share: 0.0,
            }
        })
        .filter(|r| r.value > 0.0)
        .collect();
    let world: f64 = ranked.iter().map(|r| r.value).sum();
    for r in &mut ranked {
        r.share = r.value / world * 100.0;
    }
    ranked.sort_by(|a, b| b.value.total_cmp(&a.value));
    ranked
}

fn format_value(value: f64) -> String {
    if value >= 100_000.0 {
        format!("{:.0}", value)
    } else {
        format!("{value:.2}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::Accumulator;

    #[test]
    fn ranks_by_day_average_and_total() {
        let mut acc = Accumulator::new();
        acc.ingest(
            b"1:00, 1 Jan, 1936 GER MIL 1 10.0\n1:00, 1 Jan, 1936 SOV MIL 1 30.0\n\
              1:00, 2 Jan, 1936 GER MIL 1 50.0\n1:00, 2 Jan, 1936 SOV MIL 1 10.0\n\
              1:00, 2 Jan, 1936 SOV CIV 1 100.0\n",
        );
        let ds = acc.to_dataset();
        let tag = |r: &Ranked| ds.tags()[r.row].to_string();

        let day2 = rank(&ds, &[Metric::Mil], 1, 1);
        assert_eq!(day2.iter().map(tag).collect::<Vec<_>>(), ["GER", "SOV"]);
        assert_eq!((day2[0].value, day2[0].share.round()), (50.0, 83.0));

        let avg = rank(&ds, &[Metric::Mil], 1, 2);
        assert_eq!((tag(&avg[0]), avg[0].value), ("GER".into(), 30.0));
        assert_eq!(avg[1].value, 20.0);

        let total = rank(&ds, &[Metric::Mil], 1, 0);
        assert_eq!((total[0].value, total[1].value), (60.0, 40.0));

        let all = rank(&ds, &Metric::ALL, 1, 1);
        assert_eq!((tag(&all[0]), all[0].value), ("SOV".into(), 110.0));
        assert!(rank(&ds, &[Metric::Nav], 1, 1).is_empty());
    }
}
