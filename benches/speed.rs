//! Throughput benchmark on three input sizes (the PLAN acceptance harness).
//!
//! Run on box:  cargo bench --bench speed
//! The median of `f6-big` is the number compared against the Python baseline
//! (hot 4.46 s on the 100k x 8 fixture) — see docs/QUALITY-GATES.md.

use std::io::Write;

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use xlsx2csv_rs::csvw::{CsvWriter, Quoting};
use xlsx2csv_rs::port::{ConvertOptions, Xlsx2csv};

fn fixture(name: &str) -> String {
    format!("fixtures/{name}.xlsx")
}

fn bench_convert(c: &mut Criterion) {
    let mut group = c.benchmark_group("convert");
    for name in ["f1-basic", "f3-floats", "f6-big"] {
        let path = fixture(name);
        if !std::path::Path::new(&path).exists() {
            continue;
        }
        let meta = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        group.throughput(Throughput::Bytes(meta));
        group.bench_with_input(BenchmarkId::from_parameter(name), &path, |b, path| {
            b.iter(|| {
                let opts = ConvertOptions::default();
                let mut book = Xlsx2csv::open(path).expect("open fixture");
                let mut buf: Vec<u8> = Vec::with_capacity(1 << 20);
                book.convert_one(1, &opts, &mut buf)
                    .expect("convert fixture");
                buf.len()
            })
        });
    }
    group.finish();
}

fn bench_csv_writer(c: &mut Criterion) {
    let row: Vec<String> = (0..8).map(|i| format!("value-{i}")).collect();
    let mut group = c.benchmark_group("csv_row");
    group.bench_function("8_columns", |b| {
        b.iter(|| {
            let mut buf = Vec::with_capacity(256);
            {
                let mut w = CsvWriter::new(&mut buf, ',', "\n", Quoting::Minimal);
                w.write_row(&row).expect("row");
                w.flush().expect("flush");
            }
            buf.write_all(b"").expect("noop");
            buf.len()
        })
    });
    group.finish();
}

criterion_group!(benches, bench_convert, bench_csv_writer);
criterion_main!(benches);
