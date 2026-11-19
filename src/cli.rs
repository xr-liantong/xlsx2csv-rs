//! Hand-rolled argument parser that mirrors upstream argparse 0.8.6.
//!
//! Usage errors exit 2 (argparse behaviour); value errors that upstream
//! raises with `sys.exit("error: ...")` exit 1.

use crate::csvw::Quoting;
use crate::port::ConvertOptions;

pub struct Args {
    pub input: String,
    pub opts: ConvertOptions,
}

pub enum Outcome {
    Run(Box<Args>),
    Version,
    Help(String),
    /// Exit code 2 with the argparse-style message on stderr.
    Usage(String),
    /// Exit code 1 with an `error: ...` message.
    Fatal(String),
}

const USAGE: &str = "usage: xlsx2csv-rs [-h] [-v] [-a] [-c OUTPUTENCODING] [-d DELIMITER] \
[--hyperlinks] [-e] [--no-line-breaks] [-f DATEFORMAT] [-t TIMEFORMAT] \
[--floatformat FLOATFORMAT] [--sci-float] [--exclude_hidden_sheets] \
[--ignore-formats IGNORE_FORMATS] [-l LINETERMINATOR] [-m] [-n SHEETNAME] \
[-i] [--skipemptycolumns] [-p SHEETDELIMITER] [-q QUOTING] [-s SHEETID] \
[--include-hidden-rows] [--ignore-percentage] xlsxfile [outfile]
";

struct Tok {
    name: String,
    value: Option<String>,
    positional: bool,
}

pub fn parse(argv: &[String]) -> Outcome {
    let mut toks: Vec<Tok> = Vec::new();
    let mut literal = false;
    for arg in argv {
        if literal {
            toks.push(Tok {
                name: arg.clone(),
                value: None,
                positional: true,
            });
            continue;
        }
        if arg == "--" {
            literal = true;
            continue;
        }
        // argparse treats negative numbers as positional values when no option
        // looks like a negative number.
        let negative_number =
            arg.len() > 1 && arg.starts_with('-') && arg[1..].chars().all(|c| c.is_ascii_digit());
        if arg == "-" || !arg.starts_with('-') || negative_number {
            toks.push(Tok {
                name: arg.clone(),
                value: None,
                positional: true,
            });
            continue;
        }
        let (name, value) = match arg.split_once('=') {
            Some((n, v)) if n.starts_with("--") => (n.to_string(), Some(v.to_string())),
            _ => (arg.clone(), None),
        };
        if name == "-h" || name == "--help" {
            return Outcome::Help(USAGE.to_string());
        }
        if name == "-v" || name == "--version" {
            return Outcome::Version;
        }
        toks.push(Tok {
            name,
            value,
            positional: false,
        });
    }

    let mut opts = ConvertOptions::default();
    let mut all = false;
    let mut sheetid: i64 = 1;
    let mut delimiter = ",".to_string();
    let mut quoting = "minimal".to_string();
    let mut lineterminator = "\n".to_string();
    let mut sheetdelimiter = "--------".to_string();
    let mut include_hidden_rows = false;
    let mut outputencoding = "utf-8".to_string();

    let mut used = vec![false; toks.len()];
    let mut j = 0;
    while j < toks.len() {
        let name = toks[j].name.clone();
        let inline = toks[j].value.clone();
        let positional = toks[j].positional;
        j += 1;
        if positional {
            continue;
        }
        macro_rules! need {
            () => {
                match take_value(&toks, &mut used, &mut j, inline.clone(), &name) {
                    Ok(v) => v,
                    Err(e) => return e,
                }
            };
        }
        match name.as_str() {
            "-a" | "--all" => all = true,
            "-e" | "--escape" => opts.escape_strings = true,
            "-i" | "--ignoreempty" => opts.include_empty = true,
            "--skipemptycolumns" => opts.skip_empty_columns = true,
            "--sci-float" => opts.scifloat = true,
            "--ignore-percentage" => opts.ignore_percentage = true,
            "--exclude_hidden_sheets" => opts.exclude_hidden_sheets = true,
            "--include-hidden-rows" => include_hidden_rows = true,
            "--hyperlinks" => opts.hyperlinks = true,
            "--continue-on-error" => {}
            "-E" | "--exclude_sheet_pattern" => {
                opts.exclude_patterns =
                    match take_many(&toks, &mut used, &mut j, inline.clone(), &name) {
                        Ok(v) => v,
                        Err(e) => return e,
                    };
            }
            "-I" | "--include_sheet_pattern" => {
                opts.include_patterns =
                    match take_many(&toks, &mut used, &mut j, inline.clone(), &name) {
                        Ok(v) => v,
                        Err(e) => return e,
                    };
            }
            "-m" | "--merge-cells" => opts.merge_cells = true,
            "--no-line-breaks" => {
                return Outcome::Fatal(format!("error: option {name} not implemented yet"));
            }
            "-c" | "--outputencoding" => outputencoding = need!(),
            "-d" | "--delimiter" => delimiter = need!(),
            "-f" | "--dateformat" => opts.dateformat = Some(need!()),
            "-t" | "--timeformat" => opts.timeformat = need!(),
            "--floatformat" => opts.floatformat = Some(need!()),
            "--ignore-formats" => opts.ignore_formats = vec![need!()],
            "-l" | "--lineterminator" => lineterminator = need!(),
            "-n" | "--sheetname" => opts.sheet_name = Some(need!()),
            "-p" | "--sheetdelimiter" => sheetdelimiter = need!(),
            "-q" | "--quoting" => quoting = need!(),
            "-s" | "--sheet" => {
                let v = need!();
                match v.trim().parse::<i64>() {
                    Ok(n) => sheetid = n,
                    Err(_) => {
                        return Outcome::Usage(format!(
                            "argument -s/--sheet: invalid int value: '{v}'"
                        ))
                    }
                }
            }
            other => {
                if !other.starts_with("--") && other.len() > 2 {
                    let (flag, rest) = other.split_at(2);
                    toks.insert(
                        j,
                        Tok {
                            name: flag.to_string(),
                            value: Some(rest.to_string()),
                            positional: false,
                        },
                    );
                    used.insert(j, false);
                    continue;
                }
                return Outcome::Usage(format!("unrecognized arguments: {other}\n{USAGE}"));
            }
        }
    }

    let mut positionals: Vec<String> = Vec::new();
    for (idx, tok) in toks.iter().enumerate() {
        if tok.positional && !used[idx] {
            positionals.push(tok.name.clone());
        }
    }

    if positionals.is_empty() {
        return Outcome::Usage(format!(
            "the following arguments are required: xlsxfile\n{USAGE}"
        ));
    }
    if positionals.len() > 2 {
        return Outcome::Usage(format!(
            "unrecognized arguments: {}\n{USAGE}",
            positionals[2..].join(" ")
        ));
    }
    if outputencoding != "utf-8" {
        return Outcome::Fatal("error: only utf-8 output encoding is supported".to_string());
    }
    match lineterminator.as_str() {
        "\n" => {}
        "\\n" => lineterminator = "\n".to_string(),
        "\\r" => lineterminator = "\r".to_string(),
        "\\r\\n" => lineterminator = "\r\n".to_string(),
        _ => return Outcome::Fatal("error: invalid line terminator".to_string()),
    }
    let delimiter = match parse_delimiter(&delimiter) {
        Ok(c) => c,
        Err(e) => return e,
    };
    let quoting = match quoting.as_str() {
        "none" => Quoting::None,
        "minimal" => Quoting::Minimal,
        "nonnumeric" => Quoting::NonNumeric,
        "all" => Quoting::All,
        _ => return Outcome::Fatal("error: invalid quoting".to_string()),
    };
    let sheetdelimiter = match parse_sheet_delimiter(&sheetdelimiter) {
        Ok(s) => s,
        Err(e) => return e,
    };

    opts.sheet_id = if all { 0 } else { sheetid.max(0) as u32 };
    opts.out_path = positionals.get(1).cloned();
    opts.delimiter = delimiter;
    opts.lineterminator = lineterminator;
    opts.quoting = quoting;
    opts.sheet_delimiter = sheetdelimiter;
    opts.skip_hidden_rows = !include_hidden_rows;
    Outcome::Run(Box::new(Args {
        input: positionals[0].clone(),
        opts,
    }))
}

/// `nargs='+'`: one or more values, stopping at the next option token.
fn take_many(
    toks: &[Tok],
    used: &mut [bool],
    j: &mut usize,
    inline: Option<String>,
    name: &str,
) -> Result<Vec<String>, Outcome> {
    if let Some(v) = inline {
        return Ok(vec![v]);
    }
    let mut out = Vec::new();
    while *j < toks.len() && toks[*j].positional && !used[*j] {
        out.push(toks[*j].name.clone());
        used[*j] = true;
        *j += 1;
    }
    if out.is_empty() {
        return Err(Outcome::Usage(format!(
            "argument {name}: expected at least one argument"
        )));
    }
    Ok(out)
}

fn take_value(
    toks: &[Tok],
    used: &mut [bool],
    j: &mut usize,
    inline: Option<String>,
    name: &str,
) -> Result<String, Outcome> {
    if let Some(v) = inline {
        return Ok(v);
    }
    if *j < toks.len() && toks[*j].positional && !used[*j] {
        let v = toks[*j].name.clone();
        used[*j] = true;
        *j += 1;
        return Ok(v);
    }
    Err(Outcome::Usage(format!(
        "argument {name}: expected one argument"
    )))
}

fn parse_delimiter(spec: &str) -> Result<char, Outcome> {
    let chars: Vec<char> = spec.chars().collect();
    if chars.len() == 1 {
        return Ok(chars[0]);
    }
    if spec == "tab" || spec == "\\t" {
        return Ok('\t');
    }
    if spec == "comma" {
        return Ok(',');
    }
    if let Some(rest) = spec.strip_prefix('x') {
        if let Some(c) = code_point(rest) {
            return Ok(c);
        }
    }
    if spec == "fs" || spec == "FS" {
        return Ok('\u{1c}');
    }
    Err(Outcome::Fatal("error: invalid delimiter".to_string()))
}

fn parse_sheet_delimiter(spec: &str) -> Result<String, Outcome> {
    if spec == "--------" || spec.is_empty() {
        return Ok(spec.to_string());
    }
    if spec == "\\f" {
        return Ok("\u{0c}".to_string());
    }
    if let Some(rest) = spec.strip_prefix('x') {
        if let Some(c) = code_point(rest) {
            return Ok(c.to_string());
        }
    }
    Err(Outcome::Fatal("error: invalid sheet delimiter".to_string()))
}

fn code_point(s: &str) -> Option<char> {
    let n: u32 = s.parse().ok()?;
    char::from_u32(n)
}

/// Entry point used by `src/bin/main.rs`: parse, dispatch, return the exit code.
pub fn run<I: Iterator<Item = String>>(argv: I) -> i32 {
    let argv: Vec<String> = argv.collect();
    match parse(&argv) {
        Outcome::Help(text) => {
            print!("{text}");
            0
        }
        Outcome::Version => {
            println!("{}", crate::UPSTREAM_VERSION);
            0
        }
        Outcome::Usage(msg) => {
            eprintln!("{}", msg.trim_end());
            2
        }
        Outcome::Fatal(msg) => {
            eprintln!("{msg}");
            1
        }
        Outcome::Run(args) => match crate::port::run(&args.input, &args.opts) {
            Ok(()) => 0,
            Err(e) => {
                eprintln!("{e}");
                e.exit_code()
            }
        },
    }
}
