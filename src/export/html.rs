//! Standalone interactive HTML export using Plotly.js from its CDN.
//!
//! One page with a tab per metric (Military / Naval / Civilian). Each tab shows
//! the figure the original tool drew: daily IC on top, cumulative IC below,
//! shared date axis, dark theme.

use serde_json::{Value, json};

use crate::{
    analysis::GraphData,
    data::Metric,
    theme::{self, ACCENT, BACKGROUND, BORDER, BUTTON_BG, FOREGROUND},
};

const PLOTLY_CDN: &str = "https://cdn.plot.ly/plotly-2.35.2.min.js";

/// Renders `metrics` of `graph` as one complete HTML document, opening on
/// `active`. Series whose index is set in `hidden` start toggled off.
pub fn render(graph: &GraphData, metrics: &[Metric], active: Metric, hidden: &[bool]) -> String {
    let dates: Vec<serde_json::Value> = graph
        .dates
        .iter()
        .map(|d| {
            if graph.relative {
                json!(d.0)
            } else {
                json!(d.to_string())
            }
        })
        .collect();
    let fg = theme::css(FOREGROUND);
    let bg = theme::css(BACKGROUND);

    let mut tabs = String::new();
    let mut panes = String::new();
    let mut scripts = String::new();
    for &metric in metrics {
        let id = metric.token().to_ascii_lowercase();
        let (selected, shown) = if metric == active {
            (" class=\"active\"", "")
        } else {
            ("", " style=\"display:none\"")
        };
        tabs.push_str(&format!(
            "<button data-tab=\"{id}\"{selected}>{} IC</button>",
            metric.label()
        ));
        panes.push_str(&format!("<div id=\"{id}\" class=\"chart\"{shown}></div>\n"));
        scripts.push_str(&format!(
            "Plotly.newPlot(\"{id}\", {}, {}, {{\"responsive\": true}});\n",
            Value::Array(traces(graph, metric, &dates, hidden)),
            layout(metric, &graph.title_suffix, graph.relative),
        ));
    }

    let title = html_escape(&format!("IC Graphs{}", graph.title_suffix));
    let accent = theme::css(ACCENT);
    let button = theme::css(BUTTON_BG);
    format!(
        r#"<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<title>{title}</title>
<style>
html, body {{ background-color: {bg}; color: {fg}; margin: 0; padding: 0; height: 100%; width: 100%; font-family: "Segoe UI", sans-serif; }}
nav {{ display: flex; gap: 4px; padding: 8px; }}
nav button {{ background: {button}; color: {fg}; border: none; padding: 8px 14px; border-radius: 2px; cursor: pointer; font-size: 14px; }}
nav button.active {{ background: {accent}; color: #fff; }}
.chart {{ height: calc(100vh - 52px); width: 100%; }}
</style>
<script src="{PLOTLY_CDN}" charset="utf-8"></script>
</head>
<body>
<nav>{tabs}</nav>
{panes}<script>
{scripts}document.querySelectorAll("nav button").forEach(function (button) {{
  button.addEventListener("click", function () {{
    document.querySelectorAll("nav button").forEach(function (b) {{ b.classList.toggle("active", b === button); }});
    document.querySelectorAll(".chart").forEach(function (pane) {{
      var show = pane.id === button.dataset.tab;
      pane.style.display = show ? "" : "none";
      if (show) {{ Plotly.Plots.resize(pane); }}
    }});
  }});
}});
</script>
</body>
</html>
"#
    )
}

fn traces(graph: &GraphData, metric: Metric, dates: &[Value], hidden: &[bool]) -> Vec<Value> {
    let mut traces = Vec::with_capacity(graph.len() * 2);
    let x_hover = if graph.relative {
        "Day %{x}"
    } else {
        "%{x|%Y-%m-%d}"
    };
    for (i, series) in graph.series(metric).iter().enumerate() {
        let color = theme::css(theme::series_color(series.color));
        let visible: Value = if hidden.get(i).copied().unwrap_or(false) {
            json!("legendonly")
        } else {
            json!(true)
        };
        for (values, axis, prefix, show_legend) in [
            (&series.daily, "", "IC", true),
            (&series.cumulative, "2", "Total IC", false),
        ] {
            traces.push(json!({
                "type": "scatter",
                "mode": "lines",
                "name": series.name,
                "legendgroup": series.name,
                "showlegend": show_legend,
                "visible": visible,
                "x": dates,
                "y": values,
                "xaxis": format!("x{axis}"),
                "yaxis": format!("y{axis}"),
                "line": { "color": color },
                "hovertemplate": format!(
                    "{x_hover}<br>{prefix}: %{{y:.2f}}<extra>{}</extra>",
                    series.name
                ),
            }));
        }
    }
    traces
}

fn layout(metric: Metric, suffix: &str, relative: bool) -> Value {
    let label = metric.label();
    let fg = theme::css(FOREGROUND);
    let bg = theme::css(BACKGROUND);
    let grid = theme::css(BORDER);
    let axis = |extra: Value| {
        let mut base = json!({
            "showgrid": true,
            "gridcolor": grid,
            "linecolor": grid,
            "zeroline": false,
            "color": fg,
        });
        base.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        base
    };
    let title = |text: String, y: f64| {
        json!({
            "text": text, "x": 0.5, "y": y, "xref": "paper", "yref": "paper",
            "xanchor": "center", "yanchor": "bottom", "showarrow": false,
            "font": { "size": 16, "color": fg },
        })
    };
    json!({
        "paper_bgcolor": bg,
        "plot_bgcolor": bg,
        "font": { "color": fg },
        "hovermode": "closest",
        "margin": { "l": 60, "r": 20, "t": 70, "b": 50 },
        "legend": {
            "orientation": "h", "yanchor": "bottom", "y": 1.04, "xanchor": "right", "x": 1,
            "font": { "color": fg },
        },
        "xaxis": axis(if relative {
            json!({ "anchor": "y", "matches": "x2", "showticklabels": false, "type": "linear" })
        } else {
            json!({ "anchor": "y", "matches": "x2", "showticklabels": false, "type": "date" })
        }),
        "xaxis2": axis(if relative {
            json!({ "anchor": "y2", "type": "linear", "title": { "text": "Days since start" } })
        } else {
            json!({ "anchor": "y2", "tickformat": "%Y-%m-%d", "tickangle": 30, "type": "date" })
        }),
        "yaxis": axis(json!({ "domain": [0.575, 1.0], "title": { "text": "IC" } })),
        "yaxis2": axis(json!({ "domain": [0.0, 0.425], "title": { "text": "IC" } })),
        "annotations": [
            title(format!("Daily {label} IC{suffix}"), 1.0),
            title(format!("Cumulative {label} IC{suffix}"), 0.425),
        ],
    })
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{analysis::Series, data::Day};

    fn series(name: &str, daily: Vec<f64>, cumulative: Vec<f64>) -> Series {
        Series {
            name: name.into(),
            daily,
            cumulative,
            color: 0,
        }
    }

    #[test]
    fn renders_one_tab_per_metric() {
        let graph = GraphData {
            title_suffix: " (File 1)".into(),
            dates: vec![
                Day::from_ymd(1936, 1, 1).unwrap(),
                Day::from_ymd(1936, 1, 2).unwrap(),
            ],
            series: [
                vec![series("GER", vec![1.0, 2.0], vec![1.0, 3.0])],
                vec![series("GER", vec![0.0, 1.0], vec![0.0, 1.0])],
                vec![series("GER", vec![5.0, 6.0], vec![5.0, 11.0])],
            ],
            relative: false,
        };
        let html = render(&graph, &Metric::ALL, Metric::Civ, &[true]);
        assert!(html.contains("Daily Military IC (File 1)"));
        assert!(html.contains("Cumulative Civilian IC (File 1)"));
        assert!(html.contains(r#"<button data-tab="civ" class="active">"#));
        assert!(html.contains(r#"<div id="mil" class="chart" style="display:none">"#));
        assert!(html.contains(r#""1936-01-02""#));
        assert!(html.contains(r#""legendonly""#));
        assert_eq!(html.matches("Plotly.newPlot(").count(), 3);
        assert_eq!(html.matches(r#""type":"scatter""#).count(), 6);
    }
}
