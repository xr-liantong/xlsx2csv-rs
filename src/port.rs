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
use crate::errors::PortError;
use crate::ooxml::{ContentTypes, Package, Relationships, Workbook};
use crate::sheet::{collect_hyperlinks, convert_sheet, SheetContext, SheetOptions};
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
    pub hyperlinks: bool,
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
            hyperlinks: false,
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
        };
        let ctx = SheetContext {
            styles: &self.styles,
            shared_strings: &self.shared_strings,
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

fn open_out_file(path: &str) -> Result<BufWriter<File>, PortError> {
    Ok(BufWriter::new(File::create(path)?))
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
        return Err(PortError::Unsupported("directory input"));
    }
    if input != "-" && !Path::new(input).exists() {
        return Err(PortError::InputNotFound);
    }
    if opts.escape_strings {
        return Err(PortError::Unsupported("`-e/--escape`"));
    }

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
                book.convert_one(sheet_id, opts, open_out_file(path)?)?;
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
                book.convert_one(index, opts, open_out_file(&target.to_string_lossy())?)?;
            }
            Ok(())
        }
        None => {
            book.convert_all(opts, io::stdout())?;
            Ok(())
        }
    }
}
