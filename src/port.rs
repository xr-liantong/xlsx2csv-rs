//! Driver: package → workbook → sheet → CSV, mirroring upstream `Xlsx2csv`.
//!
//! Upstream reference: `Xlsx2csv.__init__`, `convert`, `_convert`,
//! `_filehandle` (xlsx2csv.py 0.8.6, commit ec1883bb). Each branch below has a
//! line in the upstream source; branches not ported yet return
//! `PortError::Unsupported` instead of guessing.

use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::path::Path;

use crate::csvw::{CsvWriter, Quoting};
use crate::encoding::{EncodedWriter, Encoding};
use crate::errors::PortError;
use crate::ooxml::{ContentTypes, Package, Relationships, Workbook};
use crate::pattern::{any_matches, none_matches};
use crate::sheet::{collect_hyperlinks, collect_merges, convert_sheet, SheetContext, SheetOptions};
use crate::sst::SharedStrings;
use crate::styles::Styles;

/// Everything `Xlsx2csv.__init__` puts in `options`, plus the CLI-only
/// `sheet_id` / `sheet_name` / `out_path` that upstream keeps in `main`.
#[derive(Debug, Clone)]
pub struct ConvertOptions {
    pub delimiter: char,
    pub lineterminator: String,
    pub quoting: Quoting,
    pub sheet_delimiter: String,
    pub dateformat: Option<String>,
    pub timeformat: String,
    pub floatformat: Option<String>,
    pub scifloat: bool,
    pub include_empty: bool,
    pub skip_empty_columns: bool,
    pub escape_strings: bool,
    pub skip_hidden_rows: bool,
    pub ignore_percentage: bool,
    pub ignore_formats: Vec<String>,
    pub exclude_hidden_sheets: bool,
    /// `-I`: keep only sheets whose name matches one of these (empty = all).
    pub include_patterns: Vec<String>,
    /// `-E`: drop sheets whose name matches one of these.
    pub exclude_patterns: Vec<String>,
    pub hyperlinks: bool,
    /// `-m`: fill merged followers with the top-left cell's value.
    pub merge_cells: bool,
    /// `--no-line-breaks`: line breaks and tabs inside a cell become spaces.
    pub no_line_breaks: bool,
    /// `--continue-on-error`: directory mode keeps going after a bad file.
    pub continue_on_error: bool,
    /// `-c/--outputencoding`: only applies to file output, like upstream,
    /// which opens the file with `encoding=` and leaves `sys.stdout` alone.
    pub output_encoding: Option<String>,
    pub sheet_id: u32,
    pub sheet_name: Option<String>,
    pub out_path: Option<String>,
}

impl Default for ConvertOptions {
    fn default() -> Self {
        ConvertOptions {
            delimiter: ',',
            lineterminator: "\n".to_string(),
            quoting: Quoting::Minimal,
            sheet_delimiter: "--------".to_string(),
            dateformat: None,
            timeformat: "%H:%M".to_string(),
            floatformat: None,
            scifloat: false,
            include_empty: false,
            skip_empty_columns: false,
            escape_strings: false,
            skip_hidden_rows: true,
            ignore_percentage: false,
            ignore_formats: vec![],
            exclude_hidden_sheets: false,
            include_patterns: vec!["^.*$".to_string()],
            exclude_patterns: vec![],
            hyperlinks: false,
            merge_cells: false,
            no_line_breaks: false,
            continue_on_error: false,
            output_encoding: None,
            sheet_id: 1,
            sheet_name: None,
            out_path: None,
        }
    }
}

pub struct Xlsx2csv {
    pkg: Package,
    content_types: ContentTypes,
    workbook: Workbook,
    shared_strings: Vec<String>,
    styles: Styles,
}

impl Xlsx2csv {
    /// upstream `__init__`: open the zip, then parse content types, shared
    /// strings, styles, workbook and the workbook relationships, in that order.
    pub fn open(path: &str) -> Result<Self, PortError> {
        let mut pkg = Package::open(path)?;

        let content_types = match pkg.read_part("/[Content_Types].xml")? {
            Some(xml) => ContentTypes::parse(&xml)?,
            None => ContentTypes::default(),
        };

        let shared_strings = match content_types.shared_strings.as_deref() {
            Some(p) => match pkg.read_part(p)? {
                Some(xml) => SharedStrings::parse(&xml)?.strings,
                None => Vec::new(),
            },
            None => Vec::new(),
        };

        let styles = match content_types.styles.as_deref() {
            Some(p) => match pkg.read_part(p)? {
                Some(xml) => Styles::parse(&xml)?,
                None => Styles::default(),
            },
            None => Styles::default(),
        };

        let workbook_xml = pkg.require_part(&content_types.workbook)?;
        let mut workbook = Workbook::parse(&workbook_xml)?;

        if let Some(rels_path) = content_types
            .relationships
            .iter()
            .find(|r| r.contains("book"))
            .cloned()
        {
            if let Some(xml) = pkg.read_part(&rels_path)? {
                workbook.relationships = Relationships::parse(&xml)?;
            }
        }

        Ok(Xlsx2csv {
            pkg,
            content_types,
            workbook,
            shared_strings,
            styles,
        })
    }

    pub fn sheets(&self) -> &[crate::ooxml::SheetMeta] {
        &self.workbook.sheets
    }

    pub fn sheet_id_by_name(&self, name: &str) -> Option<u32> {
        self.workbook.sheet_id_by_name(name)
    }

    /// upstream `_convert` sheet-path resolution, same candidate order.
    fn sheet_path(&mut self, sheet_index: u32) -> Result<String, PortError> {
        let meta = self
            .workbook
            .sheets
            .iter()
            .find(|s| s.index == sheet_index)
            .ok_or(PortError::SheetIndexUnhandled(sheet_index))?;

        if let Some(rel_id) = meta.relation_id.as_deref() {
            if let Some(rel) = self.workbook.relationships.map.get(rel_id) {
                if let Some(target) = rel.target.as_deref() {
                    let path = if target.starts_with("/xl/") || target.starts_with("xl/") {
                        target.to_string()
                    } else {
                        format!("/xl/{target}")
                    };
                    if self.pkg.find(&path).is_some() {
                        return Ok(path);
                    }
                    return Err(PortError::SheetIndexNotFound(sheet_index));
                }
            }
        }

        let mut candidates = vec![
            format!("/xl/worksheets/sheet{sheet_index}.xml"),
            format!("/xl/worksheets/worksheet{sheet_index}.xml"),
        ];
        if sheet_index == 1 {
            if let Some(w) = self.content_types.worksheet.clone() {
                candidates.push(w);
            }
        }
        for cand in candidates {
            if self.pkg.find(&cand).is_some() {
                return Ok(cand);
            }
        }
        Err(PortError::SheetIndexNotFound(sheet_index))
    }

    fn write_sheet<W: Write>(
        &mut self,
        sheet_index: u32,
        opts: &ConvertOptions,
        writer: &mut CsvWriter<W>,
    ) -> Result<(), PortError> {
        let path = self.sheet_path(sheet_index)?;
        let xml = self.pkg.require_part(&path)?;
        // upstream reads the worksheet's own relationships part and only then
        // looks for <hyperlinks>; a missing part means no hyperlinks at all.
        let mut hyperlinks = std::collections::HashMap::new();
        if opts.hyperlinks {
            let rels_path = sheet_rels_path(&path);
            if let Some(rels_xml) = self.pkg.read_part(&rels_path)? {
                let rels = Relationships::parse(&rels_xml)?;
                hyperlinks = collect_hyperlinks(&xml, &rels)?;
            }
        }
        let merged = if opts.merge_cells {
            collect_merges(&xml)
        } else {
            std::collections::HashMap::new()
        };
        // `-e`: upstream escapes the shared-string table once, before any sheet
        // is written, and rewrites `<v>` entities as literal \\r \\n \\t.
        let escaped_strings: Option<Vec<String>> = if opts.escape_strings {
            Some(
                self.shared_strings
                    .iter()
                    .map(|s| escape_line_breaks(s))
                    .collect(),
            )
        } else {
            None
        };
        let sheet_opts = SheetOptions {
            dateformat: opts.dateformat.as_deref(),
            timeformat: &opts.timeformat,
            floatformat: opts.floatformat.as_deref(),
            ignore_formats: opts.ignore_formats.iter().map(String::as_str).collect(),
            ignore_percentage: opts.ignore_percentage,
            scifloat: opts.scifloat,
            skip_hidden_rows: opts.skip_hidden_rows,
            include_empty: opts.include_empty,
            skip_empty_columns: opts.skip_empty_columns,
            hyperlinks,
            merged,
            no_line_breaks: opts.no_line_breaks,
            escape_strings: opts.escape_strings,
        };
        let ctx = SheetContext {
            styles: &self.styles,
            shared_strings: escaped_strings.as_deref().unwrap_or(&self.shared_strings),
            date1904: self.workbook.date1904,
            options: sheet_opts,
        };
        convert_sheet(&xml, &ctx, writer)
    }

    /// One sheet into any sink (`_convert` for `sheetid > 0`).
    pub fn convert_one<W: Write>(
        &mut self,
        sheet_index: u32,
        opts: &ConvertOptions,
        out: W,
    ) -> Result<W, PortError> {
        let mut writer = CsvWriter::new(out, opts.delimiter, &opts.lineterminator, opts.quoting);
        self.write_sheet(sheet_index, opts, &mut writer)?;
        writer.flush()?;
        Ok(writer.into_inner())
    }

    /// Every selected sheet into one sink (upstream `convert` for `sheetid == 0`
    /// writing to a stream: a delimiter line precedes each sheet).
    pub fn convert_all<W: Write>(&mut self, opts: &ConvertOptions, out: W) -> Result<W, PortError> {
        let mut writer = CsvWriter::new(out, opts.delimiter, &opts.lineterminator, opts.quoting);
        let sheets: Vec<(u32, String, Option<String>)> = self
            .workbook
            .sheets
            .iter()
            .map(|s| (s.index, s.name.clone(), s.state.clone()))
            .collect();
        for (index, name, state) in sheets {
            if opts.exclude_hidden_sheets
                && matches!(state.as_deref(), Some("hidden") | Some("veryHidden"))
            {
                continue;
            }
            if !any_matches(&opts.include_patterns, &name)? {
                continue;
            }
            if !none_matches(&opts.exclude_patterns, &name)? {
                continue;
            }
            if !opts.sheet_delimiter.is_empty() {
                writer.write_raw(&format!(
                    "{} {} - {}{}",
                    opts.sheet_delimiter, index, name, opts.lineterminator
                ))?;
            }
            self.write_sheet(index, opts, &mut writer)?;
        }
        writer.flush()?;
        Ok(writer.into_inner())
    }
}

/// upstream `SharedStrings.escape_strings()`.
fn escape_line_breaks(value: &str) -> String {
    value
        .replace('\r', "\\r")
        .replace('\n', "\\n")
        .replace('\t', "\\t")
}

fn open_out_file(path: &str, encoding: Option<&str>) -> Result<Box<dyn Write>, PortError> {
    let file = BufWriter::new(File::create(path)?);
    match encoding {
        None => Ok(Box::new(file)),
        Some(name) => Ok(Box::new(EncodedWriter::new(file, Encoding::parse(name)?))),
    }
}

/// Bench helper: convert `input` into a throwaway sink.
/// `dirname(path)/_rels/basename(path).rels`, the worksheet's own rels part.
fn sheet_rels_path(sheet_path: &str) -> String {
    let (dir, file) = match sheet_path.rfind('/') {
        Some(i) => (&sheet_path[..i], &sheet_path[i + 1..]),
        None => ("", sheet_path),
    };
    format!("{dir}/_rels/{file}.rels")
}

pub fn convert(input: &str, opts: &ConvertOptions) -> Result<(), PortError> {
    let mut book = Xlsx2csv::open(input)?;
    let sheet_id = match opts.sheet_name.as_deref() {
        Some(name) => book
            .sheet_id_by_name(name)
            .ok_or_else(|| PortError::SheetNameNotFound(name.to_string()))?,
        None => opts.sheet_id,
    };
    if sheet_id > 0 {
        book.convert_one(sheet_id, opts, io::sink())?;
    } else {
        book.convert_all(opts, io::sink())?;
    }
    Ok(())
}

/// CLI driver: upstream `main` after argument decoding.
pub fn run(input: &str, opts: &ConvertOptions) -> Result<(), PortError> {
    if Path::new(input).is_dir() {
        return run_dir(input, opts);
    }
    if input != "-" && !Path::new(input).exists() {
        return Err(PortError::InputNotFound);
    }
    run_one(input, opts)
}

/// upstream `convert_recursive`: every file in the tree becomes `<name>csv`
/// next to itself, unless an outfile (or an outfile directory) was given.
fn run_dir(dir: &str, opts: &ConvertOptions) -> Result<(), PortError> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let full = entry.path();
        let full = full.to_string_lossy().to_string();
        if Path::new(&full).is_dir() {
            run_dir(&full, opts)?;
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        let out = match opts.out_path.as_deref() {
            None => format!("{}csv", drop_tail(&full, 4)),
            Some(path) if Path::new(path).is_dir() => Path::new(path)
                .join(format!("{}csv", drop_tail(&name, 4)))
                .to_string_lossy()
                .to_string(),
            Some(path) => path.to_string(),
        };
        println!("Converting {full} to {out}");
        let mut sub = opts.clone();
        sub.out_path = Some(out.clone());
        if let Err(e) = run_one(&full, &sub) {
            if opts.continue_on_error {
                eprintln!("ERROR processing file '{full}': {e}");
                continue;
            }
            return Err(e);
        }
    }
    Ok(())
}

/// Python `s[:-n]` for `str` (character based, not bytes).
fn drop_tail(s: &str, n: usize) -> &str {
    let chars = s.chars().count();
    if chars <= n {
        return "";
    }
    let cut = s.char_indices().nth(chars - n).map(|(i, _)| i).unwrap_or(0);
    &s[..cut]
}

fn run_one(input: &str, opts: &ConvertOptions) -> Result<(), PortError> {
    let mut book = Xlsx2csv::open(input)?;

    // upstream main: `if options.sheetname: sheetid = getSheetIdByName(...)`
    let sheet_id = match opts.sheet_name.as_deref() {
        Some(name) => match book.sheet_id_by_name(name) {
            Some(id) => id,
            None => return Err(PortError::SheetNameNotFound(name.to_string())),
        },
        None => opts.sheet_id,
    };

    if sheet_id > 0 {
        match opts.out_path.as_deref() {
            Some(path) => {
                book.convert_one(
                    sheet_id,
                    opts,
                    open_out_file(path, opts.output_encoding.as_deref())?,
                )?;
            }
            None => {
                book.convert_one(sheet_id, opts, io::stdout())?;
            }
        }
        return Ok(());
    }

    // sheetid == 0: all sheets. A path means a directory of `<name>.csv`.
    match opts.out_path.as_deref() {
        Some(dir) => {
            let p = Path::new(dir);
            if p.is_file() {
                return Err(PortError::OutFileExists(dir.to_string()));
            }
            if !p.exists() {
                std::fs::create_dir_all(p)?;
            }
            let sheets: Vec<(u32, String, Option<String>)> = book
                .sheets()
                .iter()
                .map(|s| (s.index, s.name.clone(), s.state.clone()))
                .collect();
            for (index, name, state) in sheets {
                if opts.exclude_hidden_sheets
                    && matches!(state.as_deref(), Some("hidden") | Some("veryHidden"))
                {
                    continue;
                }
                let target = p.join(format!("{name}.csv"));
                book.convert_one(
                    index,
                    opts,
                    open_out_file(&target.to_string_lossy(), opts.output_encoding.as_deref())?,
                )?;
            }
            Ok(())
        }
        None => {
            book.convert_all(opts, io::stdout())?;
            Ok(())
        }
    }
}
