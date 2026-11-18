//! CSV writer that reproduces Python's `csv.writer` with
//! `lineterminator=<term>`, `delimiter=<delim>` and QUOTE_* modes.
//!
//! Python's QUOTE_MINIMAL quotes a field when it contains the delimiter, the
//! quotechar or *any character of the line terminator*, and doubles embedded
//! quotes. An empty record writes only the line terminator.

use crate::errors::PortError;
use std::io::{self, Write};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Quoting {
    None,
    Minimal,
    NonNumeric,
    All,
}

pub struct CsvWriter<W: Write> {
    out: W,
    delimiter: char,
    terminator: String,
    quoting: Quoting,
}

impl<W: Write> CsvWriter<W> {
    pub fn new(out: W, delimiter: char, terminator: &str, quoting: Quoting) -> Self {
        CsvWriter {
            out,
            delimiter,
            terminator: terminator.to_string(),
            quoting,
        }
    }

    pub fn write_row(&mut self, row: &[String]) -> Result<(), PortError> {
        let mut line = String::new();
        for (i, field) in row.iter().enumerate() {
            if i > 0 {
                line.push(self.delimiter);
            }
            match self.quoting {
                Quoting::All => push_quoted(&mut line, field),
                Quoting::Minimal => {
                    // Python's csv module quotes an empty field when it is the
                    // whole record, so that it is not read back as a blank line.
                    let lone_empty = row.len() == 1 && field.is_empty();
                    if lone_empty || self.minimal_needs_quote(field) {
                        push_quoted(&mut line, field);
                    } else {
                        line.push_str(field);
                    }
                }
                Quoting::NonNumeric => {
                    if is_numeric(field) {
                        line.push_str(field);
                    } else {
                        push_quoted(&mut line, field);
                    }
                }
                Quoting::None => {
                    if field.contains(self.delimiter)
                        || field.contains('"')
                        || field.chars().any(|c| self.terminator.contains(c))
                    {
                        return Err(PortError::Io(io::Error::new(
                            io::ErrorKind::InvalidData,
                            "need to escape, but no escapechar set",
                        )));
                    }
                    line.push_str(field);
                }
            }
        }
        line.push_str(&self.terminator);
        self.out.write_all(line.as_bytes())?;
        Ok(())
    }

    /// Raw write, used for the `-a` sheet header line.
    pub fn write_raw(&mut self, s: &str) -> Result<(), PortError> {
        self.out.write_all(s.as_bytes())?;
        Ok(())
    }

    pub fn into_inner(self) -> W {
        self.out
    }

    pub fn flush(&mut self) -> Result<(), PortError> {
        self.out.flush()?;
        Ok(())
    }

    fn minimal_needs_quote(&self, field: &str) -> bool {
        field.contains(self.delimiter)
            || field.contains('"')
            || field.chars().any(|c| self.terminator.contains(c))
    }
}

fn push_quoted(line: &mut String, field: &str) {
    line.push('"');
    for c in field.chars() {
        if c == '"' {
            line.push('"');
        }
        line.push(c);
    }
    line.push('"');
}

fn is_numeric(field: &str) -> bool {
    let t = field.trim();
    !t.is_empty() && lexical_core::parse::<f64>(t.as_bytes()).is_ok()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    fn row(fields: &[&str]) -> Vec<String> {
        fields.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn minimal_quotes_delimiter_quote_and_terminator_chars() {
        let mut out = Vec::new();
        {
            let mut w = CsvWriter::new(&mut out, ',', "\n", Quoting::Minimal);
            w.write_row(&row(&["a,b", "say \"hi\"", "line\nbreak", "plain"]))
                .unwrap();
        }
        assert_eq!(
            String::from_utf8(out).unwrap(),
            "\"a,b\",\"say \"\"hi\"\"\",\"line\nbreak\",plain\n"
        );
    }

    #[test]
    fn empty_record_writes_only_terminator() {
        let mut out = Vec::new();
        {
            let mut w = CsvWriter::new(&mut out, ',', "\n", Quoting::Minimal);
            w.write_row(&[]).unwrap();
        }
        assert_eq!(String::from_utf8(out).unwrap(), "\n");
    }

    #[test]
    fn single_empty_field_is_quoted() {
        let mut out = Vec::new();
        {
            let mut w = CsvWriter::new(&mut out, ',', "\n", Quoting::Minimal);
            w.write_row(&row(&[""])).unwrap();
        }
        assert_eq!(String::from_utf8(out).unwrap(), "\"\"\n");
    }
}
