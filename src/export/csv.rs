//! CSV export in long format, one row per date, series and metric, ready for
//! pivot tables in Excel / LibreOffice / pandas.

use std::fmt::Write;

use crate::{analysis::GraphData, data::Metric};

/// `date,series,metric,daily,cumulative` for every metric in `metrics`.
pub fn render(graph: &GraphData, metrics: &[Metric]) -> String {
    let rows = graph.dates.len() * graph.len() * metrics.len();
    let mut out = String::with_capacity(32 + rows * 40);
    let dates: Vec<String> = if graph.relative {
        out.push_str("day,series,metric,daily,cumulative\n");
        graph.dates.iter().map(|d| d.0.to_string()).collect()
    } else {
        out.push_str("date,series,metric,daily,cumulative\n");
        graph.dates.iter().map(ToString::to_string).collect()
    };
    for &metric in metrics {
        for series in graph.series(metric) {
            let name = quote(&series.name);
            for ((date, daily), cumulative) in
                dates.iter().zip(&series.daily).zip(&series.cumulative)
            {
                let _ = writeln!(
                    out,
                    "{date},{name},{},{},{}",
                    metric.token(),
                    number(*daily),
                    number(*cumulative)
                );
            }
        }
    }
    out
}

/// Six decimals like the source data, without float noise or trailing zeros
/// (`687.635524` instead of `687.6355239999999`, `2` instead of `2.000000`).
fn number(value: f64) -> String {
    let mut text = format!("{value:.6}");
    if text.contains('.') {
        text.truncate(text.trim_end_matches('0').trim_end_matches('.').len());
    }
    text
}

/// Quotes a field if it contains a delimiter, quote or newline.
fn quote(field: &str) -> String {
    if field.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", field.replace('"', "\"\""))
    } else {
        field.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{analysis::Series, data::Day};

    #[test]
    fn long_format_rows() {
        let s = |daily: Vec<f64>, cumulative: Vec<f64>| Series {
            name: "Axis (Game, 1)".into(),
            daily,
            cumulative,
            color: 0,
        };
        let graph = GraphData {
            title_suffix: String::new(),
            dates: vec![
                Day::from_ymd(1936, 1, 1).unwrap(),
                Day::from_ymd(1936, 1, 2).unwrap(),
            ],
            series: [
                vec![s(vec![1.5, 2.0], vec![1.5, 3.5])],
                vec![s(vec![0.0, 0.0], vec![0.0, 0.0])],
                vec![s(vec![10.0, 12.25], vec![10.0, 22.25])],
            ],
            relative: false,
        };
        let csv = render(&graph, &[Metric::Mil, Metric::Civ]);
        let lines: Vec<&str> = csv.lines().collect();
        assert_eq!(lines.len(), 5);
        assert_eq!(lines[0], "date,series,metric,daily,cumulative");
        assert_eq!(lines[1], "1936-01-01,\"Axis (Game, 1)\",MIL,1.5,1.5");
        assert_eq!(lines[4], "1936-01-02,\"Axis (Game, 1)\",CIV,12.25,22.25");
        assert_eq!(number(687.635_523_999_999_9), "687.635524");
        assert_eq!(number(2.0), "2");
        assert_eq!(number(0.0), "0");
    }
}
