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
    Help,
    /// Exit code 2 with the argparse-style message on stderr.
    Usage(String),
    /// Exit code 1 with an `error: ...` message.
    Fatal(String),
}

/// The usage line, assembled the way argparse does it: one part per option,
/// wrapped at 80 columns with the continuation aligned under the first part.
const USAGE_PARTS: &[&str] = &[
    "[-h]",
    "[-v]",
    "[-a]",
    "[-c OUTPUTENCODING]",
    "[-d DELIMITER]",
    "[--hyperlinks]",
    "[-e]",
    "[--no-line-breaks]",
    "[-E EXCLUDE_SHEET_PATTERN [EXCLUDE_SHEET_PATTERN ...]]",
    "[-f DATEFORMAT]",
    "[-t TIMEFORMAT]",
    "[--floatformat FLOATFORMAT]",
    "[--sci-float]",
    "[-I INCLUDE_SHEET_PATTERN [INCLUDE_SHEET_PATTERN ...]]",
    "[--exclude_hidden_sheets]",
    "[--ignore-formats IGNORE_FORMATS [IGNORE_FORMATS ...]]",
    "[-l LINETERMINATOR]",
    "[-m]",
    "[-n SHEETNAME]",
    "[-i]",
    "[--skipemptycolumns]",
    "[-p SHEETDELIMITER]",
    "[-q QUOTING]",
    "[-s SHEETID]",
    "[--include-hidden-rows]",
    "[--continue-on-error]",
    "[--ignore-percentage]",
    "xlsxfile",
    "[outfile]",
];

/// argparse asks the terminal for its width and keeps two columns spare; with
/// no terminal it gets 80, so the default usable width is 78.
fn usage_width() -> usize {
    std::env::var("COLUMNS")
        .ok()
        .and_then(|v| v.trim().parse::<usize>().ok())
        .filter(|w| *w > 20)
        .map(|w| w - 2)
        .unwrap_or(78)
}

pub fn usage(prog: &str) -> String {
    let prefix = format!("usage: {prog} ");
    let indent = " ".repeat(prefix.len());
    let width_limit = usage_width();
    let mut out = prefix.clone();
    let mut width = prefix.len();
    for (i, part) in USAGE_PARTS.iter().enumerate() {
        if i == 0 {
            out.push_str(part);
            width += part.len();
            continue;
        }
        // argparse starts the positional group on a line of its own.
        if part.starts_with("xlsxfile") {
            out.push('\n');
            out.push_str(&indent);
            out.push_str(part);
            width = indent.len() + part.len();
            continue;
        }
        if width + 1 + part.len() > width_limit {
            out.push('\n');
            out.push_str(&indent);
            out.push_str(part);
            width = indent.len() + part.len();
        } else {
            out.push(' ');
            out.push_str(part);
            width += 1 + part.len();
        }
    }
    out.push('\n');
    out
}

/// `xlsx2csv.py: error: ...` -- argparse prefixes the offending program name.
pub fn error_line(prog: &str, msg: &str) -> String {
    format!("{prog}: error: {msg}\n")
}

/// argparse reports an option by every spelling it has: `-s/--sheet`.
fn option_label(name: &str) -> &'static str {
    match name {
        "-a" | "--all" => "-a/--all",
        "-e" | "--escape" => "-e/--escape",
        "-i" | "--ignoreempty" => "-i/--ignoreempty",
        "-E" | "--exclude_sheet_pattern" => "-E/--exclude_sheet_pattern",
        "-I" | "--include_sheet_pattern" => "-I/--include_sheet_pattern",
        "-m" | "--merge-cells" => "-m/--merge-cells",
        "-c" | "--outputencoding" => "-c/--outputencoding",
        "-d" | "--delimiter" => "-d/--delimiter",
        "-f" | "--dateformat" => "-f/--dateformat",
        "-t" | "--timeformat" => "-t/--timeformat",
        "-l" | "--lineterminator" => "-l/--lineterminator",
        "-n" | "--sheetname" => "-n/--sheetname",
        "-p" | "--sheetdelimiter" => "-p/--sheetdelimiter",
        "-q" | "--quoting" => "-q/--quoting",
        "-s" | "--sheet" => "-s/--sheet",
        "--floatformat" => "--floatformat",
        "--ignore-formats" => "--ignore-formats",
        _ => "--help",
    }
}

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
            return Outcome::Help;
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
            "--continue-on-error" => opts.continue_on_error = true,
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
            "--no-line-breaks" => opts.no_line_breaks = true,
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
                return Outcome::Usage(format!("unrecognized arguments: {other}"));
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
        return Outcome::Usage("the following arguments are required: xlsxfile".to_string());
    }
    if positionals.len() > 2 {
        return Outcome::Usage(format!(
            "unrecognized arguments: {}",
            positionals[2..].join(" ")
        ));
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
    // Upstream only passes the codec to `open(...)` for the output file;
    // `sys.stdout` keeps its own encoding, so the flag is a no-op there.
    if outputencoding != "utf-8" {
        opts.output_encoding = Some(outputencoding);
    }
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
        "argument {}: expected one argument",
        option_label(name)
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
///
/// The program name reaches stderr exactly as argparse would print it, so a
/// byte comparison with upstream only differs in that name (and in `argv[0]`
/// under a different binary name it legitimately does).
pub fn run() -> i32 {
    let mut raw = std::env::args_os();
    let prog = raw
        .next()
        .map(|p| {
            let s = p.to_string_lossy().into_owned();
            s.rsplit(['/', '\\']).next().unwrap_or(&s).to_string()
        })
        .unwrap_or_else(|| "xlsx2csv-rs".to_string());
    let argv: Vec<String> = raw.map(|a| a.to_string_lossy().into_owned()).collect();
    match parse(&argv) {
        Outcome::Help => {
            print!("{}", usage(&prog));
            0
        }
        Outcome::Version => {
            println!("{}", crate::UPSTREAM_VERSION);
            0
        }
        Outcome::Usage(msg) => {
            // argparse prints the usage block first, then `prog: error: ...`.
            eprint!("{}", usage(&prog));
            eprint!("{}", error_line(&prog, &msg));
            2
        }
        Outcome::Fatal(msg) => {
            // Upstream raises these with `sys.exit(str(e) + "\n")`, and
            // `sys.exit` adds a newline of its own: two blank lines of slack.
            eprintln!("{msg}\n");
            1
        }
        Outcome::Run(args) => match crate::port::run(&args.input, &args.opts) {
            Ok(()) => 0,
            Err(e) => {
                eprintln!("{e}\n");
                e.exit_code()
            }
        },
    }
}
