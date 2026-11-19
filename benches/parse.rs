//! Parsing / series-building throughput on a synthetic game log.
//!
//! Run with `cargo bench`.

use std::{fmt::Write, hint::black_box};

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use hoi4_analyzer::{
    analysis::{self, Source, factions},
    data::{Accumulator, Tag},
};

const TAGS: [&str; 24] = [
    "GER", "ITA", "JAP", "ROM", "HUN", "SPR", "BUL", "YUG", "FIN", "MAN", "SIA", "SOV", "USA",
    "ENG", "FRA", "CAN", "UAC", "AST", "RAJ", "SAF", "MON", "NZL", "BRA", "MEX",
];
const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

/// Roughly a full 1936-1946 campaign: every country logs MIL, NAV and CIV once per day.
fn synthetic_log(years: i32) -> String {
    let mut out = String::new();
    let mut n = 0u32;
    for year in 1936..1936 + years {
        for (m, month) in MONTHS.iter().enumerate() {
            let days = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31][m];
            for day in 1..=days {
                for tag in TAGS {
                    // Same shape as DataCollection 1.19.x: leading and trailing spaces.
                    for kind in ["MIL", "NAV", "CIV"] {
                        n = n.wrapping_mul(1_103_515_245).wrapping_add(12_345);
                        let _ = writeln!(
                            out,
                            " 1:00, {day} {month}, {year} {tag} {kind} 1 {}.{:06}     ",
                            n % 600,
                            n % 1_000_000
                        );
                    }
                }
            }
        }
    }
    out
}

fn bench(c: &mut Criterion) {
    let log = synthetic_log(10);
    let mut group = c.benchmark_group("parse");
    group.throughput(Throughput::Bytes(log.len() as u64));
    group.sample_size(20);
    group.bench_function("full_file", |b| {
        b.iter(|| {
            let mut acc = Accumulator::new();
            acc.ingest(black_box(log.as_bytes()));
            acc.to_dataset()
        });
    });
    group.finish();

    let mut acc = Accumulator::new();
    acc.ingest(log.as_bytes());
    let dataset = acc.to_dataset();
    let all: Vec<Tag> = TAGS.iter().map(|t| Tag::from_static(t)).collect();
    let individual = factions::individual_groups(&all, factions::Labels::default());
    let by_faction = factions::Grouping::builtin().groups_for(&all);
    c.bench_function("build/individual_24_countries", |b| {
        b.iter(|| {
            analysis::build(
                &[Source {
                    dataset: &dataset,
                    suffix: "",
                }],
                black_box(&individual),
                "",
            )
        });
    });
    c.bench_function("build/factions_compare", |b| {
        let sources = [
            Source {
                dataset: &dataset,
                suffix: " (F1)",
            },
            Source {
                dataset: &dataset,
                suffix: " (F2)",
            },
        ];
        b.iter(|| analysis::build(&sources, black_box(&by_faction), ""));
    });
}

criterion_group!(benches, bench);
criterion_main!(benches);
