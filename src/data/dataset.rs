use super::{Day, Metric, ParseStats, Tag};

/// Parsed contents of one output file in dense, row-major form:
/// one row per country, one column per date, one matrix per [`Metric`].
/// Series building becomes contiguous slice arithmetic.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Dataset {
    dates: Vec<Day>,
    tags: Vec<Tag>,
    values: [Vec<f64>; Metric::COUNT],
    stats: ParseStats,
}

impl Dataset {
    /// Assembles a dataset from sorted, unique `dates` and `tags` and one
    /// `tags.len() * dates.len()` row-major matrix per metric. Returns `None`
    /// if the axes are not strictly ascending or the matrices do not fit them.
    pub fn from_parts(
        dates: Vec<Day>,
        tags: Vec<Tag>,
        values: [Vec<f64>; Metric::COUNT],
        stats: ParseStats,
    ) -> Option<Self> {
        let cells = dates.len().checked_mul(tags.len())?;
        let ascending =
            dates.windows(2).all(|w| w[0] < w[1]) && tags.windows(2).all(|w| w[0] < w[1]);
        (ascending && values.iter().all(|m| m.len() == cells)).then_some(Self {
            dates,
            tags,
            values,
            stats,
        })
    }

    /// Sorted, unique dates.
    pub fn dates(&self) -> &[Day] {
        &self.dates
    }

    /// Sorted, unique country tags.
    pub fn tags(&self) -> &[Tag] {
        &self.tags
    }

    pub fn stats(&self) -> ParseStats {
        self.stats
    }

    pub fn is_empty(&self) -> bool {
        self.dates.is_empty()
    }

    pub fn tag_index(&self, tag: Tag) -> Option<usize> {
        self.tags.binary_search(&tag).ok()
    }

    /// The whole row-major matrix of one metric.
    pub fn matrix(&self, metric: Metric) -> &[f64] {
        &self.values[metric as usize]
    }

    /// Daily values of one country, aligned with [`Self::dates`].
    pub fn row(&self, metric: Metric, index: usize) -> &[f64] {
        let width = self.dates.len();
        &self.values[metric as usize][index * width..(index + 1) * width]
    }

    pub fn has_positive(&self, metric: Metric, index: usize) -> bool {
        self.row(metric, index).iter().any(|&v| v > 0.0)
    }

    /// Whether any country logged a positive value for `metric`
    /// (older DataCollection versions write no `CIV` lines).
    pub fn has_metric(&self, metric: Metric) -> bool {
        self.values[metric as usize].iter().any(|&v| v > 0.0)
    }

    /// Countries with any positive value in any metric.
    pub fn active_tags(&self) -> impl Iterator<Item = Tag> + '_ {
        (0..self.tags.len())
            .filter(|&i| Metric::ALL.iter().any(|&m| self.has_positive(m, i)))
            .map(|i| self.tags[i])
    }

    /// Sum of all countries' values on the latest date.
    pub fn latest_total(&self, metric: Metric) -> f64 {
        let Some(last) = self.dates.len().checked_sub(1) else {
            return 0.0;
        };
        (0..self.tags.len())
            .map(|i| self.row(metric, i)[last])
            .sum()
    }
}
