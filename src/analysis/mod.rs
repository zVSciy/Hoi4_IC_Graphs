//! Turning parsed datasets into plottable daily / cumulative series.

pub mod factions;

use crate::data::{Dataset, Day, Metric, Tag};

/// A named set of countries whose values are summed into one series.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Group {
    pub name: String,
    pub tags: Vec<Tag>,
    /// Fixed palette index (used when charting a single source).
    pub color: Option<usize>,
}

impl Group {
    pub fn new(name: String, tags: Vec<Tag>) -> Self {
        Self {
            name,
            tags,
            color: None,
        }
    }
}

/// How the date axes of several sources are combined.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Align {
    /// By calendar date (all games share the HOI4 calendar).
    #[default]
    Calendar,
    /// By days since each source's first date, for games started at
    /// different bookmarks.
    FromStart,
}

/// One data source plus the suffix appended to its series names (`" (F1)"`).
#[derive(Clone, Copy)]
pub struct Source<'a> {
    pub dataset: &'a Dataset,
    pub suffix: &'a str,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Series {
    pub name: String,
    pub daily: Vec<f64>,
    pub cumulative: Vec<f64>,
    /// Palette index.
    pub color: usize,
}

/// Everything a graph window needs. Series `i` of every metric describes the
/// same group, so one legend toggle applies to all tabs.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GraphData {
    pub title_suffix: String,
    /// Calendar dates, or day offsets (`Day(0)` = start) if `relative`.
    pub dates: Vec<Day>,
    pub series: [Vec<Series>; Metric::COUNT],
    /// The x axis counts days since each game's start ([`Align::FromStart`]).
    pub relative: bool,
}

impl GraphData {
    pub fn series(&self, metric: Metric) -> &[Series] {
        &self.series[metric as usize]
    }

    /// Series names, shared by all metrics.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.series[0].iter().map(|s| s.name.as_str())
    }

    /// Metrics with any positive value (files from older DataCollection
    /// versions have no `CIV` data, so that tab is hidden for them).
    pub fn metrics(&self) -> Vec<Metric> {
        Metric::ALL
            .into_iter()
            .filter(|&m| {
                self.series(m)
                    .iter()
                    .any(|s| s.daily.iter().any(|&v| v > 0.0))
            })
            .collect()
    }

    pub fn len(&self) -> usize {
        self.series[0].len()
    }

    pub fn is_empty(&self) -> bool {
        self.series[0].is_empty()
    }
}

/// Builds series for every `(group, source)` pair on a shared date axis.
///
/// With several sources the axis is the union of their dates; days missing from
/// a source contribute zero and the cumulative line carries forward. Series
/// without a positive value in any metric are dropped. Returns `None` if
/// nothing remains.
pub fn build(sources: &[Source<'_>], groups: &[Group], title_suffix: &str) -> Option<GraphData> {
    build_aligned(sources, groups, title_suffix, Align::Calendar)
}

/// [`build`] with a choice of how several sources' dates line up.
pub fn build_aligned(
    sources: &[Source<'_>],
    groups: &[Group],
    title_suffix: &str,
    align: Align,
) -> Option<GraphData> {
    // Each source's axis: its dates, or offsets from its first date.
    let axes: Vec<Vec<Day>> = sources
        .iter()
        .map(|s| match (align, s.dataset.dates().first()) {
            (Align::FromStart, Some(first)) => s
                .dataset
                .dates()
                .iter()
                .map(|d| Day(d.0 - first.0))
                .collect(),
            _ => s.dataset.dates().to_vec(),
        })
        .collect();
    let dates = merged_dates(&axes);
    if dates.is_empty() {
        return None;
    }
    // Column mapping from each source onto the shared axis (None = identity).
    let mappings: Vec<Option<Vec<usize>>> = axes
        .iter()
        .map(|axis| (axis != &dates).then(|| column_map(axis, &dates)))
        .collect();

    let mut out = GraphData {
        title_suffix: title_suffix.to_owned(),
        dates,
        relative: align == Align::FromStart,
        ..GraphData::default()
    };
    let n = out.dates.len();
    for group in groups {
        for (source, mapping) in sources.iter().zip(&mappings) {
            let mut daily: [Vec<f64>; Metric::COUNT] = std::array::from_fn(|_| vec![0.0; n]);
            for &tag in &group.tags {
                let Some(row) = source.dataset.tag_index(tag) else {
                    continue;
                };
                for metric in Metric::ALL {
                    let values = source.dataset.row(metric, row);
                    let target = &mut daily[metric as usize];
                    match mapping {
                        None => target.iter_mut().zip(values).for_each(|(t, v)| *t += v),
                        Some(map) => map
                            .iter()
                            .zip(values)
                            .for_each(|(&col, v)| target[col] += v),
                    }
                }
            }
            if !daily.iter().flatten().any(|&v| v > 0.0) {
                continue;
            }
            let name = format!("{}{}", group.name, source.suffix);
            // A fixed group colour would repeat across sources, so it is only
            // used when charting one source.
            let color = group
                .color
                .filter(|_| sources.len() == 1)
                .unwrap_or(out.series[0].len());
            for (list, values) in out.series.iter_mut().zip(daily) {
                list.push(series(name.clone(), values, color));
            }
        }
    }
    (!out.is_empty()).then_some(out)
}

fn series(name: String, daily: Vec<f64>, color: usize) -> Series {
    let cumulative = daily
        .iter()
        .scan(0.0, |acc, &v| {
            *acc += v;
            Some(*acc)
        })
        .collect();
    Series {
        name,
        daily,
        cumulative,
        color,
    }
}

fn merged_dates(axes: &[Vec<Day>]) -> Vec<Day> {
    match axes {
        [] => Vec::new(),
        [only] => only.clone(),
        _ => {
            let mut all: Vec<Day> = axes.iter().flatten().copied().collect();
            all.sort_unstable();
            all.dedup();
            all
        }
    }
}

/// Trailing average over `window` values (the first values average what is
/// available). `window` <= 1 returns the input unchanged.
pub fn rolling_mean(values: &[f64], window: usize) -> Vec<f64> {
    if window <= 1 {
        return values.to_vec();
    }
    let mut out = Vec::with_capacity(values.len());
    let mut sum = 0.0;
    for (i, &v) in values.iter().enumerate() {
        sum += v;
        if i >= window {
            sum -= values[i - window];
        }
        out.push(sum / (i + 1).min(window) as f64);
    }
    out
}

/// Each row as a percentage of the sum of the `included` rows, day by day
/// (0 where that sum is 0). Rows that are not included are returned as zeros.
pub fn shares(rows: &[&[f64]], included: &[bool]) -> Vec<Vec<f64>> {
    let len = rows.first().map_or(0, |r| r.len());
    let mut totals = vec![0.0; len];
    for (row, _) in rows.iter().zip(included).filter(|(_, inc)| **inc) {
        for (total, v) in totals.iter_mut().zip(*row) {
            *total += v;
        }
    }
    rows.iter()
        .zip(included)
        .map(|(row, &inc)| {
            row.iter()
                .zip(&totals)
                .map(|(v, total)| {
                    if inc && *total > 0.0 {
                        v / total * 100.0
                    } else {
                        0.0
                    }
                })
                .collect()
        })
        .collect()
}

/// For each date of `subset`, its index in `superset` (both sorted).
fn column_map(subset: &[Day], superset: &[Day]) -> Vec<usize> {
    let mut j = 0;
    subset
        .iter()
        .map(|day| {
            while superset[j] < *day {
                j += 1;
            }
            j
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::Accumulator;

    fn dataset(text: &str) -> Dataset {
        let mut acc = Accumulator::new();
        acc.ingest(text.as_bytes());
        acc.to_dataset()
    }

    #[test]
    fn single_source_cumulates() {
        let ds = dataset(
            "1:00, 1 Jan, 1936 GER MIL 1 1.5\n\
             1:00, 1 Jan, 1936 GER MIL 1 0.5\n\
             1:00, 2 Jan, 1936 GER NAV 1 3.0\n\
             1:00, 3 Jan, 1936 GER MIL 1 1.0\n\
             1:00, 3 Jan, 1936 ITA MIL 1 0.0\n",
        );
        let groups = factions::individual_groups(
            &[Tag::from_static("GER"), Tag::from_static("ITA")],
            factions::Labels::default(),
        );
        let g = build(
            &[Source {
                dataset: &ds,
                suffix: "",
            }],
            &groups,
            "",
        )
        .unwrap();
        assert_eq!(g.len(), 1, "ITA has no positive IC and is dropped");
        assert_eq!(g.series(Metric::Mil)[0].name, "GER");
        assert_eq!(g.series(Metric::Mil)[0].daily, [2.0, 0.0, 1.0]);
        assert_eq!(g.series(Metric::Mil)[0].cumulative, [2.0, 2.0, 3.0]);
        assert_eq!(g.series(Metric::Nav)[0].cumulative, [0.0, 3.0, 3.0]);
        assert_eq!(g.metrics(), [Metric::Mil, Metric::Nav], "no CIV lines");
    }

    #[test]
    fn civilian_series_and_civ_only_countries() {
        let ds = dataset(
            "1:00, 1 Jan, 1936 GER MIL 1 1.0\n\
             1:00, 1 Jan, 1936 GER CIV 1 10.5\n\
             1:00, 2 Jan, 1936 GER CIV 1 12.0\n\
             1:00, 2 Jan, 1936 LUX CIV 1 8.0\n",
        );
        let t = Tag::from_static;
        let groups =
            factions::individual_groups(&[t("GER"), t("LUX")], factions::Labels::default());
        let g = build(
            &[Source {
                dataset: &ds,
                suffix: "",
            }],
            &groups,
            "",
        )
        .unwrap();
        assert_eq!(g.names().collect::<Vec<_>>(), ["GER", "LUX"]);
        assert_eq!(g.series(Metric::Civ)[0].daily, [10.5, 12.0]);
        assert_eq!(g.series(Metric::Civ)[0].cumulative, [10.5, 22.5]);
        assert_eq!(
            g.series(Metric::Mil)[1].daily,
            [0.0, 0.0],
            "LUX kept for its CIV data"
        );
        assert_eq!(g.metrics(), [Metric::Mil, Metric::Civ]);
    }

    #[test]
    fn aligns_games_by_day_since_start() {
        let a = dataset("1:00, 1 Jan, 1936 GER MIL 1 1.0\n1:00, 2 Jan, 1936 GER MIL 1 2.0\n");
        let b = dataset("1:00, 1 Jan, 1939 GER MIL 1 5.0\n1:00, 3 Jan, 1939 GER MIL 1 6.0\n");
        let groups =
            factions::individual_groups(&[Tag::from_static("GER")], factions::Labels::default());
        let sources = [
            Source {
                dataset: &a,
                suffix: " (A)",
            },
            Source {
                dataset: &b,
                suffix: " (B)",
            },
        ];
        let g = build_aligned(&sources, &groups, "", Align::FromStart).unwrap();
        assert!(g.relative);
        assert_eq!(g.dates, [Day(0), Day(1), Day(2)]);
        assert_eq!(g.series(Metric::Mil)[0].daily, [1.0, 2.0, 0.0]);
        assert_eq!(g.series(Metric::Mil)[1].daily, [5.0, 0.0, 6.0]);
        assert_eq!(
            (
                g.series(Metric::Mil)[0].color,
                g.series(Metric::Mil)[1].color
            ),
            (0, 1)
        );
    }

    #[test]
    fn rolling_mean_and_shares() {
        assert_eq!(rolling_mean(&[2.0, 4.0, 6.0, 8.0], 2), [2.0, 3.0, 5.0, 7.0]);
        assert_eq!(rolling_mean(&[1.0, 2.0], 1), [1.0, 2.0]);
        let axis = [3.0, 0.0, 1.0];
        let allies = [1.0, 0.0, 3.0];
        let other = [4.0, 4.0, 4.0];
        let s = shares(&[&axis, &allies, &other], &[true, true, false]);
        assert_eq!(s[0], [75.0, 0.0, 25.0]);
        assert_eq!(s[1], [25.0, 0.0, 75.0]);
        assert_eq!(s[2], [0.0, 0.0, 0.0], "hidden rows do not count");
    }

    #[test]
    fn comparison_aligns_dates() {
        let a = dataset("1:00, 1 Jan, 1936 GER MIL 1 1.0\n1:00, 3 Jan, 1936 GER MIL 1 1.0\n");
        let b = dataset("1:00, 2 Jan, 1936 GER MIL 1 5.0\n");
        let groups =
            factions::individual_groups(&[Tag::from_static("GER")], factions::Labels::default());
        let g = build(
            &[
                Source {
                    dataset: &a,
                    suffix: " (F1)",
                },
                Source {
                    dataset: &b,
                    suffix: " (F2)",
                },
            ],
            &groups,
            "",
        )
        .unwrap();
        assert_eq!(g.dates.len(), 3);
        let names: Vec<_> = g
            .series(Metric::Mil)
            .iter()
            .map(|s| s.name.as_str())
            .collect();
        assert_eq!(names, ["GER (F1)", "GER (F2)"]);
        assert_eq!(g.series(Metric::Mil)[0].daily, [1.0, 0.0, 1.0]);
        assert_eq!(g.series(Metric::Mil)[1].daily, [0.0, 5.0, 0.0]);
        assert_eq!(g.series(Metric::Mil)[1].cumulative, [0.0, 5.0, 5.0]);
    }

    #[test]
    fn factions_sum_members() {
        let ds = dataset(
            "1:00, 1 Jan, 1936 GER MIL 1 1.0\n1:00, 1 Jan, 1936 ITA MIL 1 2.0\n1:00, 1 Jan, 1936 SOV MIL 1 4.0\n",
        );
        let t = Tag::from_static;
        let groups = factions::Grouping::builtin().groups_for(&[t("GER"), t("ITA"), t("SOV")]);
        let g = build(
            &[Source {
                dataset: &ds,
                suffix: "",
            }],
            &groups,
            "",
        )
        .unwrap();
        assert_eq!(g.series(Metric::Mil)[0].name, "Axis");
        assert_eq!(g.series(Metric::Mil)[0].daily, [3.0]);
        assert_eq!(g.series(Metric::Mil)[1].name, "Allies");
        assert_eq!(g.series(Metric::Mil)[1].daily, [4.0]);
    }
}
