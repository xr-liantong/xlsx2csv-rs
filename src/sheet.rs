//! Streaming worksheet reader → CSV rows.
//!
//! Mirrors upstream `Sheet.handleStartElement` / `handleEndElement`:
//! cells are keyed `letters(r) - 1 + colIndex`, every `<c>` stores a value
//! (empty cells included), rows are emitted with
//! `max(columns_count, maxkey + 1)` columns, gaps are back-filled unless
//! `-i`, and `--skipemptycolumns` truncates to the first row's width.

use std::collections::{BTreeMap, HashMap};
use std::io::Write;

use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;

use crate::csvw::CsvWriter;
use crate::errors::PortError;
use crate::fmtval::{format_cell, FmtOptions};
use crate::ooxml::Relationships;
use crate::styles::Styles;

pub struct SheetOptions<'a> {
    pub dateformat: Option<&'a str>,
    pub timeformat: &'a str,
    pub floatformat: Option<&'a str>,
    pub ignore_formats: Vec<&'a str>,
    pub ignore_percentage: bool,
    pub scifloat: bool,
    pub skip_hidden_rows: bool,
    pub include_empty: bool,
    pub skip_empty_columns: bool,
    /// `cell id -> target`, from `<hyperlinks>` plus the sheet relationships.
    pub hyperlinks: HashMap<String, String>,
}

pub struct SheetContext<'a> {
    pub styles: &'a Styles,
    pub shared_strings: &'a [String],
    pub date1904: bool,
    pub options: SheetOptions<'a>,
}

struct RowState {
    columns: BTreeMap<i64, String>,
    columns_count: i64,
    max_columns: i64,
    last_row_num: i64,
}

impl RowState {
    fn new() -> RowState {
        RowState {
            columns: BTreeMap::new(),
            columns_count: -1,
            max_columns: -1,
            last_row_num: 0,
        }
    }
}

pub fn convert_sheet<W: Write>(
    xml: &[u8],
    ctx: &SheetContext<'_>,
    out: &mut CsvWriter<W>,
) -> Result<(), PortError> {
    let mut reader = Reader::from_reader(xml);
    reader.config_mut().trim_text(false);
    let mut buf = Vec::new();

    let mut st = RowState::new();
    let mut in_sheet = false;
    let mut in_row = false;
    let mut in_cell = false;
    let mut in_value = false;
    let mut col_type: Option<String> = None;
    let mut col_style: Option<String> = None;
    let mut cell_letters: i64 = 0;
    let mut cell_id = String::new();
    let mut col_index: i64 = 0;
    let mut cell_text = String::new();
    let mut row_num: i64 = 0;
    let mut row_index: i64 = 0;
    let mut spans: Option<(i64, i64)> = None;

    loop {
        let ev = reader
            .read_event_into(&mut buf)
            .map_err(|e| PortError::Xml(e.to_string()))?;
        match ev {
            Event::Eof => break,
            Event::Start(ref e) | Event::Empty(ref e) => {
                let empty = matches!(ev, Event::Empty(_));
                match local_name(e.name().as_ref()).as_str() {
                    "dimension" => {
                        if let Some(r) = attr(e, b"ref")? {
                            if let Some((a, b)) = r.split_once(':') {
                                let left = leading_letters(a);
                                let right = leading_letters(b);
                                if left > 0 && right >= left {
                                    st.columns_count = right - left + 1;
                                }
                            }
                        }
                    }
                    "sheetdata" => {
                        in_sheet = true;
                        row_index = 0;
                    }
                    "row" if in_sheet => {
                        let hidden = attr(e, b"hidden")?.unwrap_or_default();
                        let skip =
                            ctx.options.skip_hidden_rows && (hidden == "1" || hidden == "true");
                        if skip {
                            st.last_row_num += 1;
                            in_row = false;
                            if empty {
                                continue;
                            }
                        } else {
                            row_index += 1;
                            row_num = match attr(e, b"r")? {
                                Some(r) => r.parse::<i64>().unwrap_or(row_index),
                                None => row_index,
                            };
                            in_row = true;
                            col_index = 0;
                            cell_letters = 0;
                            st.columns.clear();
                            spans = match attr(e, b"spans")? {
                                Some(s) => parse_spans(&s),
                                None => None,
                            };
                            if empty {
                                emit_row(&mut st, row_num, spans, &ctx.options, out)?;
                                in_row = false;
                            }
                        }
                    }
                    "c" if in_row => {
                        col_type = attr(e, b"t")?;
                        col_style = attr(e, b"s")?;
                        cell_text.clear();
                        match attr(e, b"r")? {
                            Some(r) => {
                                cell_letters = leading_letters(&r);
                                cell_id = r;
                                col_index = 0;
                            }
                            None => {
                                col_index += 1;
                                cell_letters = 0;
                                cell_id.clear();
                            }
                        }
                        in_cell = true;
                        in_value = false;
                        if empty {
                            let mut value = String::new();
                            if let Some(target) = ctx.options.hyperlinks.get(&cell_id) {
                                value = format!("<a href='{target}'></a>");
                            }
                            store_cell(&mut st, cell_letters, col_index, value);
                            in_cell = false;
                        }
                    }
                    "v" | "t" if in_cell => in_value = !empty,
                    _ => {}
                }
            }
            Event::End(ref e) => match local_name(e.name().as_ref()).as_str() {
                "sheetdata" => in_sheet = false,
                "row" => {
                    if in_row {
                        emit_row(&mut st, row_num, spans, &ctx.options, out)?;
                    }
                    in_row = false;
                }
                "c" => {
                    if in_cell {
                        let opts = fmt_options(&ctx.options);
                        let mut value = if cell_text.is_empty() {
                            String::new()
                        } else {
                            format_cell(
                                col_type.as_deref(),
                                col_style.as_deref(),
                                &cell_text,
                                ctx.styles,
                                ctx.shared_strings,
                                ctx.date1904,
                                &opts,
                            )?
                        };
                        // upstream wraps the formatted value, even an empty one.
                        if let Some(target) = ctx.options.hyperlinks.get(&cell_id) {
                            value = format!("<a href='{target}'>{value}</a>");
                        }
                        store_cell(&mut st, cell_letters, col_index, value);
                    }
                    in_cell = false;
                    in_value = false;
                    cell_text.clear();
                }
                "v" | "t" => in_value = false,
                _ => {}
            },
            Event::Text(ref e) if in_cell && in_value => {
                let text = e
                    .unescape()
                    .map_err(|err| PortError::Xml(err.to_string()))?;
                cell_text.push_str(&text);
            }
            Event::CData(ref e) if in_cell && in_value => {
                cell_text.push_str(&String::from_utf8_lossy(e.as_ref()));
            }
            _ => {}
        }
        buf.clear();
    }
    Ok(())
}

/// upstream `Sheet._range`: one cell is returned verbatim, a rectangle yields
/// every cell in it (column-major, as upstream does).
fn expand_range(reference: &str) -> Vec<String> {
    let Some((start, end)) = reference.split_once(':') else {
        return vec![reference.to_string()];
    };
    let Some((start_col, start_row)) = split_cell(start) else {
        return Vec::new();
    };
    let Some((end_col, end_row)) = split_cell(end) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut col = start_col;
    loop {
        for row in start_row..=end_row {
            out.push(format!("{}{row}", letters_of(col)));
        }
        if col == end_col {
            break;
        }
        col += 1;
    }
    out
}

/// `^([A-Z]+)(\d+)$` → (letters indicator, row number).
fn split_cell(cell: &str) -> Option<(i64, i64)> {
    let letters_len = cell.bytes().take_while(|b| b.is_ascii_uppercase()).count();
    if letters_len == 0 || letters_len == cell.len() {
        return None;
    }
    let (letters, digits) = cell.split_at(letters_len);
    if !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let row = digits.parse::<i64>().ok()?;
    Some((letters_value(letters), row))
}

fn letters_value(letters: &str) -> i64 {
    let mut value: i64 = 0;
    for c in letters.chars() {
        value = value * 26 + (c as i64) - 64;
    }
    value
}

fn letters_of(mut value: i64) -> String {
    let mut out = Vec::new();
    while value > 0 {
        value -= 1;
        out.push((b'A' + (value % 26) as u8) as char);
        value /= 26;
    }
    out.iter().rev().collect()
}

/// upstream `set_include_hyperlinks`: collect `<hyperlink ref r:id>` and resolve
/// each id through the worksheet relationships. Without a relationship part
/// there is nothing to resolve, so the map stays empty.
pub fn collect_hyperlinks(
    xml: &[u8],
    rels: &Relationships,
) -> Result<HashMap<String, String>, PortError> {
    let mut out = HashMap::new();
    if rels.map.is_empty() {
        return Ok(out);
    }
    let mut reader = Reader::from_reader(xml);
    let mut buf = Vec::new();
    let mut in_hyperlinks = false;
    loop {
        match reader
            .read_event_into(&mut buf)
            .map_err(|e| PortError::Xml(e.to_string()))?
        {
            Event::Eof => break,
            Event::Start(ref e) | Event::Empty(ref e) => {
                match local_name(e.name().as_ref()).as_str() {
                    "hyperlinks" => in_hyperlinks = true,
                    "hyperlink" if in_hyperlinks => {
                        if let Some((reference, rel_id)) = hyperlink_refs(e)? {
                            if let Some(rel) = rels.map.get(&rel_id) {
                                if let Some(target) = rel.target.as_deref() {
                                    for cell in expand_range(&reference) {
                                        out.insert(cell, target.to_string());
                                    }
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            Event::End(ref e) if local_name(e.name().as_ref()) == "hyperlinks" => {
                in_hyperlinks = false;
            }
            _ => {}
        }
        buf.clear();
    }
    Ok(out)
}

/// `ref` plus any attribute whose name ends in `:id`; both are required.
fn hyperlink_refs(e: &BytesStart<'_>) -> Result<Option<(String, String)>, PortError> {
    let mut reference: Option<String> = None;
    let mut rel_id: Option<String> = None;
    for a in e.attributes() {
        let a = a.map_err(|err| PortError::Xml(err.to_string()))?;
        let key = a.key.as_ref().to_vec();
        if key == b"ref" {
            reference = Some(String::from_utf8_lossy(&a.value).to_string());
        } else if key.ends_with(b":id") {
            rel_id = Some(String::from_utf8_lossy(&a.value).to_string());
        }
    }
    match (reference, rel_id) {
        (Some(reference), Some(rel_id)) => Ok(Some((reference, rel_id))),
        _ => Ok(None),
    }
}

fn store_cell(st: &mut RowState, letters: i64, col_index: i64, value: String) {
    let key = if letters > 0 {
        letters - 1
    } else {
        col_index - 1
    };
    st.columns.insert(key, value);
}

fn fmt_options<'a>(o: &'a SheetOptions<'a>) -> FmtOptions<'a> {
    FmtOptions {
        dateformat: o.dateformat,
        timeformat: o.timeformat,
        floatformat: o.floatformat,
        ignore_formats: o.ignore_formats.clone(),
        ignore_percentage: o.ignore_percentage,
        scifloat: o.scifloat,
    }
}

fn emit_row<W: Write>(
    st: &mut RowState,
    row_num: i64,
    spans: Option<(i64, i64)>,
    opts: &SheetOptions<'_>,
    out: &mut CsvWriter<W>,
) -> Result<(), PortError> {
    if st.columns.is_empty() {
        return Ok(());
    }
    let mut row: Vec<String>;
    if st.columns.keys().any(|k| *k < 0) {
        row = st.columns.values().cloned().collect();
    } else {
        let max_key = *st.columns.keys().max().unwrap_or(&0);
        let width = std::cmp::max(st.columns_count, max_key + 1).max(0) as usize;
        row = vec![String::new(); width];
        for (k, v) in st.columns.iter() {
            if *k >= 0 && (*k as usize) < row.len() {
                row[*k as usize] = v.clone();
            }
        }
    }
    if let Some((_, end)) = spans {
        while (row.len() as i64) < end {
            row.push(String::new());
        }
    }
    if st.columns_count < 0 {
        st.columns_count = row.len() as i64;
    }
    if !opts.include_empty {
        while st.last_row_num < row_num - 1 {
            out.write_row(&[])?;
            st.last_row_num += 1;
        }
        st.last_row_num = row_num;
    }
    if opts.include_empty && row.iter().all(|x| x.is_empty()) {
        st.columns.clear();
        return Ok(());
    }
    while (row.len() as i64) < st.columns_count {
        row.push(String::new());
    }
    if opts.skip_empty_columns {
        if st.max_columns < 0 {
            st.max_columns = row.len() as i64;
            while row.last().is_some_and(|x| x.is_empty()) {
                row.pop();
                st.max_columns -= 1;
            }
        } else if st.max_columns > 0 {
            row.truncate(st.max_columns as usize);
        }
    }
    out.write_row(&row)?;
    st.columns.clear();
    Ok(())
}

fn parse_spans(s: &str) -> Option<(i64, i64)> {
    let last = s.split(' ').next_back()?;
    let (a, b) = last.split_once(':')?;
    Some((a.trim().parse().ok()?, b.trim().parse().ok()?))
}

fn local_name(raw: &[u8]) -> String {
    let name = match raw.iter().rposition(|&b| b == b':') {
        Some(i) => &raw[i + 1..],
        None => raw,
    };
    String::from_utf8_lossy(name).to_ascii_lowercase()
}

fn attr(e: &BytesStart<'_>, key: &[u8]) -> Result<Option<String>, PortError> {
    for a in e.attributes() {
        let a = a.map_err(|err| PortError::Xml(err.to_string()))?;
        if a.key.as_ref() == key {
            let v = a
                .unescape_value()
                .map_err(|err| PortError::Xml(err.to_string()))?;
            return Ok(Some(v.into_owned()));
        }
    }
    Ok(None)
}

/// `A1` → 1, `AB12` → 28; 0 when the reference has no leading letters.
fn leading_letters(cell: &str) -> i64 {
    let mut n: i64 = 0;
    for c in cell.chars() {
        let c = c.to_ascii_uppercase();
        if c.is_ascii_uppercase() {
            n = n * 26 + (c as i64 - 'A' as i64 + 1);
        } else {
            break;
        }
    }
    n
}
