//! Native rendering of daily + cumulative IC charts.

use eframe::egui::{self, Color32, Id, Rect, RichText, Ui};
use egui_plot::{GridInput, GridMark, HoverPosition, Line, Plot, PlotPoint, PlotPoints};

use super::style;
use crate::{
    analysis::{self, GraphData},
    data::{Day, Metric, date},
};

/// How a chart presents its data.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ViewOptions {
    /// Rolling average over this many days for the daily plot (0/1 = off).
    pub smoothing: u32,
    /// Show each series as a percentage of all visible series.
    pub share: bool,
}

/// Plot points converted once per graph and view so frames only borrow them.
pub struct PreparedChart {
    metric: Metric,
    title_suffix: String,
    names: Vec<String>,
    colors: Vec<usize>,
    relative: bool,
    view: ViewOptions,
    daily: Vec<Vec<PlotPoint>>,
    cumulative: Vec<Vec<PlotPoint>>,
}

impl PreparedChart {
    /// `hidden` matters only for share view (shares are among visible series).
    pub fn new(graph: &GraphData, metric: Metric, view: ViewOptions, hidden: &[bool]) -> Self {
        let xs: Vec<f64> = graph.dates.iter().map(|d| d.as_f64()).collect();
        let points = |ys: &[f64]| {
            xs.iter()
                .zip(ys)
                .map(|(&x, &y)| PlotPoint::new(x, y))
                .collect()
        };
        let series = graph.series(metric);
        let daily: Vec<Vec<f64>> = series
            .iter()
            .map(|s| analysis::rolling_mean(&s.daily, view.smoothing as usize))
            .collect();
        let cumulative: Vec<Vec<f64>> = series.iter().map(|s| s.cumulative.clone()).collect();
        let (daily, cumulative) = if view.share {
            let visible: Vec<bool> = (0..series.len())
                .map(|i| !hidden.get(i).copied().unwrap_or(false))
                .collect();
            let rows = |values: &[Vec<f64>]| {
                let refs: Vec<&[f64]> = values.iter().map(Vec::as_slice).collect();
                analysis::shares(&refs, &visible)
            };
            (rows(&daily), rows(&cumulative))
        } else {
            (daily, cumulative)
        };
        Self {
            metric,
            title_suffix: graph.title_suffix.clone(),
            names: series.iter().map(|s| s.name.clone()).collect(),
            colors: series.iter().map(|s| s.color).collect(),
            relative: graph.relative,
            view,
            daily: daily.iter().map(|v| points(v)).collect(),
            cumulative: cumulative.iter().map(|v| points(v)).collect(),
        }
    }
}

/// Draws the legend and both plots; returns the screen area they cover.
pub fn show(ui: &mut Ui, id: Id, chart: &PreparedChart, hidden: &mut Vec<bool>) -> Rect {
    hidden.resize(chart.names.len(), false);
    let top = ui.cursor().min;

    let hovered = legend(ui, &chart.names, &chart.colors, hidden);
    ui.add_space(4.0);

    let label = chart.metric.label();
    let link = id.with("link");
    let title_height = ui.text_style_height(&egui::TextStyle::Body) + ui.spacing().item_spacing.y;
    let plot_height = ((ui.available_height() - 2.0 * title_height - ui.spacing().item_spacing.y)
        / 2.0)
        .max(120.0);

    let share = chart.view.share;
    let smoothing = if chart.view.smoothing > 1 {
        format!(", {}-day average", chart.view.smoothing)
    } else {
        String::new()
    };
    let what = if share { " share (%)" } else { "" };
    let rows = [
        (
            format!("Daily {label} IC{what}{smoothing}{}", chart.title_suffix),
            &chart.daily,
            if share { "Share" } else { "IC" },
        ),
        (
            format!("Cumulative {label} IC{what}{}", chart.title_suffix),
            &chart.cumulative,
            if share { "Share of total" } else { "Total IC" },
        ),
    ];
    let relative = chart.relative;
    for (row, (title, data, value_label)) in rows.into_iter().enumerate() {
        ui.vertical_centered(|ui| ui.label(RichText::new(title).strong()));
        let mut plot = Plot::new(id.with(row))
            .height(plot_height)
            .link_axis(link, [true, false])
            .link_cursor(link, [true, false])
            // Same axis width for both rows keeps their x axes aligned.
            .y_axis_min_width(64.0)
            .y_axis_label(if share { "%" } else { "IC" })
            .include_y(0.0)
            .label_formatter(move |pos| hover_label(pos, value_label, share, relative));
        plot = if share {
            plot.include_y(100.0)
                .y_axis_formatter(|mark, _| format!("{:.0}%", mark.value))
        } else {
            plot.y_axis_formatter(|mark, _| format_si(mark.value))
        };
        plot = if relative {
            plot.x_axis_formatter(|mark, _| format!("Day {:.0}", mark.value))
        } else {
            plot.x_grid_spacer(date_grid)
                .x_axis_formatter(|mark, _| format_date_mark(mark))
        };
        plot.show(ui, |plot| {
            for (i, points) in data.iter().enumerate() {
                if hidden[i] {
                    continue;
                }
                plot.line(
                    Line::new(chart.names[i].as_str(), PlotPoints::Borrowed(points))
                        .color(style::series_color(chart.colors[i]))
                        .width(if hovered == Some(i) { 3.0 } else { 1.5 })
                        .highlight(hovered == Some(i)),
                );
            }
        });
    }
    Rect::from_min_max(top, ui.min_rect().max)
}

/// Clickable series toggles shared by both plots. Returns the hovered entry.
fn legend(ui: &mut Ui, names: &[String], colors: &[usize], hidden: &mut [bool]) -> Option<usize> {
    let mut hovered = None;
    ui.horizontal_wrapped(|ui| {
        if ui.small_button("Show all").clicked() {
            hidden.fill(false);
        }
        if ui.small_button("Hide all").clicked() {
            hidden.fill(true);
        }
        ui.separator();
        for (i, name) in names.iter().enumerate() {
            let color = if hidden[i] {
                Color32::from_gray(90)
            } else {
                style::series_color(colors[i])
            };
            let response =
                legend_entry(ui, name, color, hidden[i]).on_hover_text("Click to show / hide");
            if response.clicked() {
                hidden[i] = !hidden[i];
            }
            if response.hovered() && !hidden[i] {
                hovered = Some(i);
            }
        }
    });
    hovered
}

/// Colour swatch plus name, laid out as one widget so the legend wraps cleanly.
fn legend_entry(ui: &mut Ui, name: &str, color: Color32, hidden: bool) -> egui::Response {
    const SWATCH: f32 = 14.0;
    const GAP: f32 = 4.0;
    let font = egui::TextStyle::Body.resolve(ui.style());
    let galley = ui.painter().layout_no_wrap(name.to_owned(), font, color);
    let text_size = galley.size();
    let size = egui::vec2(
        SWATCH + GAP + text_size.x,
        text_size.y.max(ui.spacing().interact_size.y),
    );
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        let y = rect.center().y;
        painter.line_segment(
            [
                egui::pos2(rect.left(), y),
                egui::pos2(rect.left() + SWATCH, y),
            ],
            egui::Stroke::new(3.0, color),
        );
        let text_pos = egui::pos2(rect.left() + SWATCH + GAP, y - text_size.y / 2.0);
        painter.galley(text_pos, galley, color);
        if hidden {
            painter.line_segment(
                [
                    egui::pos2(text_pos.x, y),
                    egui::pos2(text_pos.x + text_size.x, y),
                ],
                egui::Stroke::new(1.0, color),
            );
        }
    }
    response
}

fn hover_label(
    pos: &HoverPosition<'_>,
    value_label: &str,
    share: bool,
    relative: bool,
) -> Option<String> {
    let x = |x: f64| {
        if relative {
            format!("Day {}", x.round())
        } else {
            Day::from_f64(x).to_string()
        }
    };
    let unit = if share { "%" } else { "" };
    Some(match pos {
        HoverPosition::NearDataPoint {
            plot_name,
            position,
            ..
        } => format!(
            "{plot_name}\n{}\n{value_label}: {:.2}{unit}",
            x(position.x),
            position.y
        ),
        HoverPosition::Elsewhere { position } => {
            format!("{}\n{:.2}{unit}", x(position.x), position.y)
        }
    })
}

/// Grid steps for a date axis, in approximate days.
#[derive(Clone, Copy)]
enum Step {
    Days(i32),
    Months(u32),
    Years(i32),
}

impl Step {
    const LADDER: [Self; 12] = [
        Self::Days(1),
        Self::Days(2),
        Self::Days(7),
        Self::Days(14),
        Self::Months(1),
        Self::Months(2),
        Self::Months(3),
        Self::Months(6),
        Self::Years(1),
        Self::Years(2),
        Self::Years(5),
        Self::Years(10),
    ];

    fn approx_days(self) -> f64 {
        match self {
            Self::Days(d) => f64::from(d),
            Self::Months(m) => f64::from(m) * 30.44,
            Self::Years(y) => f64::from(y) * 365.25,
        }
    }
}

const MAX_X_LABELS: f64 = 8.0;

/// Places grid lines on calendar boundaries (month starts, year starts, ...).
fn date_grid(input: GridInput) -> Vec<GridMark> {
    let (min, max) = input.bounds;
    let span = max - min;
    let Some(step) = Step::LADDER.into_iter().find(|s| {
        s.approx_days() >= input.base_step_size && span / s.approx_days() <= MAX_X_LABELS
    }) else {
        // Zoomed out beyond decades: plain numeric spacing is fine.
        return egui_plot::log_grid_spacer(10)(input);
    };
    let step_size = step.approx_days();
    let (min_day, max_day) = (Day::from_f64(min.floor()), Day::from_f64(max.ceil()));
    let (mut y, mut m, _) = min_day.ymd();
    let mut marks = Vec::new();
    let mut push = |day: Option<Day>| {
        if let Some(day) = day.filter(|d| *d >= min_day && *d <= max_day) {
            marks.push(GridMark {
                value: day.as_f64(),
                step_size,
            });
        }
    };
    match step {
        Step::Days(n) => {
            let first = min_day.0 - min_day.0.rem_euclid(n);
            (first..=max_day.0)
                .step_by(n as usize)
                .for_each(|d| push(Some(Day(d))));
        }
        Step::Months(n) => {
            m = ((m - 1) / n) * n + 1;
            while Day::from_ymd(y, m, 1).is_some_and(|d| d <= max_day) {
                push(Day::from_ymd(y, m, 1));
                m += n;
                if m > 12 {
                    m -= 12;
                    y += 1;
                }
            }
        }
        Step::Years(n) => {
            let mut year = y - y.rem_euclid(n);
            while Day::from_ymd(year, 1, 1).is_some_and(|d| d <= max_day) {
                push(Day::from_ymd(year, 1, 1));
                year += n;
            }
        }
    }
    marks
}

/// Compact axis numbers: 950, 25k, 1.5M.
fn format_si(value: f64) -> String {
    let magnitude = value.abs();
    let (scaled, suffix) = if magnitude >= 1e9 {
        (value / 1e9, "G")
    } else if magnitude >= 1e6 {
        (value / 1e6, "M")
    } else if magnitude >= 1e4 {
        (value / 1e3, "k")
    } else {
        (value, "")
    };
    let mut text = format!("{scaled:.2}");
    while text.ends_with('0') {
        text.pop();
    }
    if text.ends_with('.') {
        text.pop();
    }
    text + suffix
}

fn format_date_mark(mark: GridMark) -> String {
    let (y, m, d) = Day::from_f64(mark.value).ymd();
    if mark.step_size >= 365.0 {
        format!("{y}")
    } else if mark.step_size >= 28.0 {
        format!("{} {y}", date::month_abbrev(m))
    } else {
        format!("{y:04}-{m:02}-{d:02}")
    }
}
