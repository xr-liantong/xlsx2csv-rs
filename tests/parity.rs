//! Golden parity: run the library on every tracked fixture and compare the
//! produced bytes with `fixtures/<case>.golden.csv`.
//!
//! The `.xlsx` inputs are git-ignored (see KNOWN-GAPS.md, issue #1), so each
//! case is skipped when the input is absent — the gate is meaningful on the
//! box, where the fixtures exist.

use xlsx2csv_rs::port::{ConvertOptions, Xlsx2csv};

fn read_fixture(path: &str) -> Option<Vec<u8>> {
    std::fs::read(path).ok()
}

fn run_case(
    fixture: &str,
    golden: &str,
    f: impl FnOnce(&mut Xlsx2csv, &ConvertOptions) -> Vec<u8>,
) {
    let Some(_) = read_fixture(&format!("fixtures/{fixture}.xlsx")) else {
        eprintln!("skip {fixture}: fixtures/{fixture}.xlsx missing");
        return;
    };
    let want = std::fs::read(format!("fixtures/{golden}")).expect("golden readable");
    let opts = ConvertOptions::default();
    let mut book = Xlsx2csv::open(&format!("fixtures/{fixture}.xlsx")).expect("open fixture");
    let got = f(&mut book, &opts);
    assert_eq!(
        got.len(),
        want.len(),
        "{fixture}: length differs (got {} want {})",
        got.len(),
        want.len()
    );
    if let Some(i) = got.iter().zip(want.iter()).position(|(a, b)| a != b) {
        panic!("{fixture}: first difference at byte {i}");
    }
}

#[test]
fn golden_bytes_match() {
    run_case("f1-basic", "f1-basic.golden.csv", |book, opts| {
        let mut buf = Vec::new();
        book.convert_one(1, opts, &mut buf).expect("convert");
        buf
    });
    run_case("f2-dates", "f2-dates.golden.csv", |book, opts| {
        let mut buf = Vec::new();
        book.convert_one(1, opts, &mut buf).expect("convert");
        buf
    });
    run_case("f3-floats", "f3-floats.golden.csv", |book, opts| {
        let mut buf = Vec::new();
        book.convert_one(1, opts, &mut buf).expect("convert");
        buf
    });
    run_case("f7-hyperlinks", "f7-hyperlinks.golden.csv", |book, opts| {
        let mut opts = opts.clone();
        opts.hyperlinks = true;
        let mut buf = Vec::new();
        book.convert_one(1, &opts, &mut buf).expect("convert");
        buf
    });
    run_case("f5-quirks", "f5-quirks.golden.csv", |book, opts| {
        let mut buf = Vec::new();
        book.convert_one(1, opts, &mut buf).expect("convert");
        buf
    });
    run_case("f4-sheets", "f4-sheets.golden.csv", |book, opts| {
        let mut buf = Vec::new();
        book.convert_one(1, opts, &mut buf).expect("convert");
        buf
    });
    run_case("f4-sheets", "f4-sheets.s2.golden.csv", |book, opts| {
        let mut buf = Vec::new();
        book.convert_one(2, opts, &mut buf).expect("convert");
        buf
    });
    run_case("f4-sheets", "f4-sheets.a.golden.csv", |book, opts| {
        let mut buf = Vec::new();
        book.convert_all(opts, &mut buf).expect("convert");
        buf
    });
}
