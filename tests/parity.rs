//! Golden parity: run the library on every tracked fixture and compare the
//! produced bytes with `fixtures/<case>.golden.csv`.
//!
//! The `.xlsx` inputs are git-ignored (see KNOWN-GAPS.md, issue #1), so each
//! case is skipped when the input is absent — the gate is meaningful on the
//! box, where the fixtures exist.

use xlsx2csv_rs::csvw::Quoting;
use xlsx2csv_rs::encoding::{EncodedWriter, Encoding};
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
    run_case("f4-sheets", "f4-sheets.i-s2.golden.csv", |book, opts| {
        let mut opts = opts.clone();
        opts.include_patterns = vec!["^S2".to_string()];
        let mut buf = Vec::new();
        book.convert_all(&opts, &mut buf).expect("convert");
        buf
    });
    run_case("f4-sheets", "f4-sheets.e-s.golden.csv", |book, opts| {
        let mut opts = opts.clone();
        opts.exclude_patterns = vec!["^S".to_string()];
        let mut buf = Vec::new();
        book.convert_all(&opts, &mut buf).expect("convert");
        buf
    });
    run_case("f8-merged", "f8-merged.golden.csv", |book, opts| {
        let mut opts = opts.clone();
        opts.merge_cells = true;
        let mut buf = Vec::new();
        book.convert_one(1, &opts, &mut buf).expect("convert");
        buf
    });
    run_case("f5-quirks", "f5-quirks.nlb.golden.csv", |book, opts| {
        let mut opts = opts.clone();
        opts.no_line_breaks = true;
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

/// Directory input (`convert_recursive`): every `.xlsx` becomes a sibling
/// `<name>csv`, and `--continue-on-error` walks past a file that is not a zip.
/// `-q nonnumeric` quotes every field: CPython only leaves `int`/`float`
/// objects unquoted, and xlsx2csv hands the writer strings.
#[test]
fn nonnumeric_quoting_matches_upstream() {
    run_case(
        "f10-edges",
        "f10-edges.qnonnumeric.golden.csv",
        |book, opts| {
            let mut opts = opts.clone();
            opts.quoting = Quoting::NonNumeric;
            let mut buf = Vec::new();
            book.convert_one(1, &opts, &mut buf).expect("convert");
            buf
        },
    );
}

/// `-c/--outputencoding` writes the file through the named codec. Upstream
/// only applies it to file output, so the goldens are whole files.
#[test]
fn output_encoding_matches_goldens() {
    for (golden, enc) in [
        ("f3-floats.utf16le.golden.csv", "utf-16-le"),
        ("f3-floats.cp1252.golden.csv", "cp1252"),
        ("f3-floats.utf16.golden.csv", "utf-16"),
    ] {
        let Some(_) = read_fixture("fixtures/f3-floats.xlsx") else {
            eprintln!("skip {enc}: fixture missing");
            continue;
        };
        let want = std::fs::read(format!("fixtures/{golden}")).expect("golden readable");
        let mut sink = EncodedWriter::new(Vec::new(), Encoding::parse(enc).expect("codec"));
        let mut book = Xlsx2csv::open("fixtures/f3-floats.xlsx").expect("open fixture");
        let opts = ConvertOptions::default();
        book.convert_one(1, &opts, &mut sink).expect("convert");
        assert_eq!(sink.into_inner(), want, "-c {enc}");
    }
}

/// A character the codec cannot represent fails like CPython, with the codec
/// name and the character in the message.
#[test]
fn output_encoding_reports_unencodable() {
    let Some(_) = read_fixture("fixtures/f1-basic.xlsx") else {
        eprintln!("skip encoding error case: fixture missing");
        return;
    };
    let mut sink = EncodedWriter::new(Vec::new(), Encoding::parse("latin-1").expect("codec"));
    let mut book = Xlsx2csv::open("fixtures/f1-basic.xlsx").expect("open fixture");
    let opts = ConvertOptions::default();
    let err = book
        .convert_one(1, &opts, &mut sink)
        .err()
        .expect("latin-1 cannot encode CJK");
    let text = err.to_string();
    assert!(
        text.starts_with("'latin-1' codec can't encode character '\\u"),
        "unexpected message: {text}"
    );
}

/// `--floatformat` is Python's `%` operator on a float, not printf: `%e`
/// always prints two exponent digits, `%g` strips mantissa zeros only.
#[test]
fn floatformat_conversions_match_upstream() {
    for (golden, fmt) in [
        ("f3-floats.e.golden.csv", "%e"),
        ("f3-floats.g.golden.csv", "%g"),
        ("f3-floats.f0.golden.csv", "%.0f"),
    ] {
        run_case("f3-floats", golden, |book, opts| {
            let mut opts = opts.clone();
            opts.floatformat = Some(fmt.to_string());
            let mut buf = Vec::new();
            book.convert_one(1, &opts, &mut buf).expect("convert");
            buf
        });
    }
}

/// Negative serial fractions used to wrap the hour/minute fields through u32,
/// and `--floatformat %.3f` used to ignore the precision.
#[test]
fn edge_values_match_upstream() {
    run_case("f10-edges", "f10-edges.golden.csv", |book, opts| {
        let mut buf = Vec::new();
        book.convert_one(1, opts, &mut buf).expect("convert");
        buf
    });
    run_case("f10-edges", "f10-edges.ff3.golden.csv", |book, opts| {
        let mut opts = opts.clone();
        opts.floatformat = Some("%.3f".to_string());
        let mut buf = Vec::new();
        book.convert_one(1, &opts, &mut buf).expect("convert");
        buf
    });
}

#[test]
fn directory_input_recurses_and_continues() {
    if read_fixture("fixtures/f1-basic.xlsx").is_none()
        || read_fixture("fixtures/f3-floats.xlsx").is_none()
    {
        eprintln!("skip directory case: fixtures missing");
        return;
    }
    let root = std::env::temp_dir().join("xlsx2csv-rs-dirtest");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("sub")).expect("temp dir");
    std::fs::copy("fixtures/f1-basic.xlsx", root.join("a.xlsx")).expect("copy a");
    std::fs::copy("fixtures/f3-floats.xlsx", root.join("sub/b.xlsx")).expect("copy b");

    let opts = ConvertOptions::default();
    xlsx2csv_rs::port::run(&root.to_string_lossy(), &opts).expect("directory convert");

    for (produced, golden) in [
        (root.join("a.csv"), "fixtures/f1-basic.golden.csv"),
        (root.join("sub/b.csv"), "fixtures/f3-floats.golden.csv"),
    ] {
        let got = std::fs::read(&produced).expect("converted file");
        let want = std::fs::read(golden).expect("golden readable");
        assert_eq!(
            got.len(),
            want.len(),
            "{}: length differs",
            produced.display()
        );
        assert!(
            got == want,
            "{}: bytes differ from {golden}",
            produced.display()
        );
    }

    // A non-zip file stops the walk unless --continue-on-error is set.
    std::fs::write(root.join("bad.xlsx"), b"not a zip").expect("write bad");
    let strict = xlsx2csv_rs::port::run(&root.to_string_lossy(), &opts);
    assert!(strict.is_err(), "a bad file must fail the walk");
    let lenient = ConvertOptions {
        continue_on_error: true,
        ..ConvertOptions::default()
    };
    xlsx2csv_rs::port::run(&root.to_string_lossy(), &lenient).expect("lenient walk");

    std::fs::remove_dir_all(&root).ok();
}

/// The usage/error text is what a user sees first, and argparse's exact shape
/// (wrapped block, `prog: error:` prefix, positional group on its own line) is
/// part of the port's promise. Each assertion below is one byte-level property
/// that used to be wrong before #5.
#[test]
fn stderr_matches_argparse_shape() {
    let bin = env!("CARGO_BIN_EXE_xlsx2csv-rs");
    let prog = "xlsx2csv-rs";

    let out = std::process::Command::new(bin).output().expect("run");
    assert_eq!(out.status.code(), Some(2));
    let err = String::from_utf8(out.stderr).expect("utf8");
    assert!(
        err.starts_with(&format!("usage: {prog} [-h] [-v] [-a]")),
        "{err}"
    );
    assert!(err.ends_with("the following arguments are required: xlsxfile\n"));
    let indent = " ".repeat("usage: ".len() + prog.len() + 1);
    for line in err.lines().filter(|l| !l.starts_with("usage: ")) {
        if line.starts_with(&indent) {
            assert!(line.len() <= 78, "usage line too long: {line:?}");
        }
    }
    // argparse starts the positional group on its own line.
    assert!(
        err.contains(&format!("\n{indent}xlsxfile [outfile]\n")),
        "{err}"
    );
    // ...and prefixes the error with the program name.
    assert!(err.contains(&format!("{prog}: error: ")), "{err}");

    let out = std::process::Command::new(bin)
        .arg("-s")
        .output()
        .expect("run");
    assert_eq!(out.status.code(), Some(2));
    let err = String::from_utf8(out.stderr).expect("utf8");
    assert!(
        err.contains("argument -s/--sheet: expected one argument"),
        "argparse names every spelling of the option: {err}"
    );

    // NB: without a positional argparse refuses first, so give it a filename;
    // quoting is validated before the file is opened, so it need not exist.
    let out = std::process::Command::new(bin)
        .arg("-q")
        .arg("bogus")
        .arg("does-not-exist.xlsx")
        .output()
        .expect("run");
    assert_eq!(out.status.code(), Some(1));
    // sys.exit(str(e) + "\n") leaves two newlines, and that is the whole message.
    assert_eq!(out.stderr, b"error: invalid quoting\n\n");
}
