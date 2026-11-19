//! Parses a DataCollection output file and prints timing and a summary.
//!
//! `cargo run --release --example parse_file -- path/to/output.txt`

use std::{path::PathBuf, time::Instant};

use hoi4_analyzer::{
    analysis::{self, Source, factions},
    data::{self, Metric},
};

fn main() {
    let path: PathBuf = std::env::args_os()
        .nth(1)
        .expect("usage: parse_file <output.txt>")
        .into();
    // Repeat to separate cold (first read from disk) from warm runs.
    for run in 1..=3 {
        let started = Instant::now();
        let dataset = data::load_file(&path).expect("failed to read file");
        let parsed = started.elapsed();

        let started = Instant::now();
        let tags: Vec<_> = dataset.active_tags().collect();
        let graph = analysis::build(
            &[Source {
                dataset: &dataset,
                suffix: "",
            }],
            &factions::individual_groups(&tags, factions::Labels::default()),
            "",
        );
        let built = started.elapsed();

        let top = graph.as_ref().and_then(|g| {
            g.series(Metric::Mil)
                .iter()
                .map(|s| (s.name.as_str(), *s.cumulative.last().unwrap_or(&0.0)))
                .max_by(|a, b| a.1.total_cmp(&b.1))
        });
        let stats = dataset.stats();
        if let Some((name, total)) = top {
            println!("        top cumulative MIL: {name} {total:.4}");
        }
        println!(
            "run {run}: parse {:>8.2} ms | build {} series {:>6.2} ms | {} lines, {} errors, {} dates, {} countries, latest MIL total {:.2}",
            parsed.as_secs_f64() * 1e3,
            graph.as_ref().map_or(0, |g| g.len()),
            built.as_secs_f64() * 1e3,
            stats.lines,
            stats.errors(),
            dataset.dates().len(),
            dataset.tags().len(),
            dataset.latest_total(Metric::Mil),
        );
    }
}
