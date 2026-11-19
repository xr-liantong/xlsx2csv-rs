//! `-c/--outputencoding`: upstream opens the output file with
//! `open(path, 'w+', encoding=<name>)`, and leaves `sys.stdout` alone — so the
//! flag only ever affects file output. Only the encodings we can transcode
//! without a dependency are implemented; anything else fails loudly instead of
//! silently writing UTF-8 under a different name.

use std::io::{self, Write};

use crate::errors::PortError;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Encoding {
    Utf8,
    Ascii,
    Latin1,
    Cp1252,
    Utf16Le,
    Utf16Be,
    /// `utf-16`: BOM chosen by the writer, little endian like CPython.
    Utf16,
}

impl Encoding {
    pub fn parse(name: &str) -> Result<Encoding, PortError> {
        let key: String = name.trim().to_ascii_lowercase().replace(['_', ' '], "-");
        let enc = match key.as_str() {
            "utf-8" | "utf8" | "u8" | "utf" | "cp65001" => Encoding::Utf8,
            "ascii" | "us-ascii" | "646" | "ansi-x3.4-1968" => Encoding::Ascii,
            "latin-1" | "latin1" | "latin" | "iso-8859-1" | "iso8859-1" | "8859" | "cp819"
            | "iso-latin-1" => Encoding::Latin1,
            "cp1252" | "windows-1252" | "1252" => Encoding::Cp1252,
            "utf-16" | "utf16" | "u16" | "unicode" => Encoding::Utf16,
            "utf-16-le" | "utf-16le" | "utf16-le" | "utf-16-little" => Encoding::Utf16Le,
            "utf-16-be" | "utf-16be" | "utf16-be" | "utf-16-big" => Encoding::Utf16Be,
            _ => return Err(PortError::Encoding(format!("unknown encoding: {name}"))),
        };
        Ok(enc)
    }

    /// CPython's name for this codec in `UnicodeEncodeError` messages.
    fn codec_name(self) -> &'static str {
        match self {
            Encoding::Utf8 => "utf-8",
            Encoding::Ascii => "ascii",
            Encoding::Latin1 => "latin-1",
            Encoding::Cp1252 => "charmap",
            Encoding::Utf16 => "utf-16",
            Encoding::Utf16Le => "utf-16-le",
            Encoding::Utf16Be => "utf-16-be",
        }
    }
}

/// CPython 3.12 / Windows-1252: the five undefined bytes have no character.
const CP1252_HIGH: [u32; 32] = [
    0x20AC, 0x0081, 0x201A, 0x0192, 0x201E, 0x2026, 0x2020, 0x2021, 0x02C6, 0x2030, 0x0160, 0x2039,
    0x0152, 0x008D, 0x017D, 0x008F, 0x0090, 0x2018, 0x2019, 0x201C, 0x201D, 0x2022, 0x2013, 0x2014,
    0x02DC, 0x2122, 0x0161, 0x203A, 0x0153, 0x009D, 0x017E, 0x0178,
];
const CP1252_UNDEFINED: [u8; 5] = [0x81, 0x8D, 0x8F, 0x90, 0x9D];

fn cp1252_byte(c: char) -> Option<u8> {
    let v = c as u32;
    if v < 0x80 || (0xA0..=0xFF).contains(&v) {
        return Some(v as u8);
    }
    CP1252_HIGH
        .iter()
        .position(|&u| u == v)
        .map(|i| 0x80 + i as u8)
        .filter(|b| !CP1252_UNDEFINED.contains(b))
}

fn escape_char(c: char) -> String {
    let v = c as u32;
    if v <= 0xFF {
        format!("\\x{v:02x}")
    } else if v <= 0xFFFF {
        format!("\\u{v:04x}")
    } else {
        format!("\\U{v:08x}")
    }
}

pub struct EncodedWriter<W: Write> {
    inner: W,
    enc: Encoding,
    pending: Vec<u8>,
    wrote_bom: bool,
    /// Character offset inside the current `write` call, for error messages.
    base: usize,
}

impl<W: Write> EncodedWriter<W> {
    pub fn new(inner: W, enc: Encoding) -> EncodedWriter<W> {
        EncodedWriter {
            inner,
            enc,
            pending: Vec::new(),
            wrote_bom: false,
            base: 0,
        }
    }

    fn encode_char(&self, c: char, pos: usize, out: &mut Vec<u8>) -> Result<(), PortError> {
        let err = |limit: &str, reason: &str| {
            PortError::Encoding(format!(
                "'{}' codec can't encode character '{}' in position {pos}: {reason}",
                self.enc.codec_name(),
                escape_char(c),
                reason = if reason.is_empty() { limit } else { reason },
            ))
        };
        match self.enc {
            Encoding::Utf8 => {
                let mut buf = [0u8; 4];
                out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
            }
            Encoding::Ascii => {
                if (c as u32) < 0x80 {
                    out.push(c as u8);
                } else {
                    return Err(err("", "ordinal not in range(128)"));
                }
            }
            Encoding::Latin1 => {
                if (c as u32) < 0x100 {
                    out.push(c as u8);
                } else {
                    return Err(err("", "ordinal not in range(256)"));
                }
            }
            Encoding::Cp1252 => match cp1252_byte(c) {
                Some(b) => out.push(b),
                None => return Err(err("", "character maps to <undefined>")),
            },
            Encoding::Utf16 | Encoding::Utf16Le | Encoding::Utf16Be => {
                let big = matches!(self.enc, Encoding::Utf16Be);
                let mut buf = [0u16; 2];
                for unit in c.encode_utf16(&mut buf) {
                    let bytes = if big {
                        unit.to_be_bytes()
                    } else {
                        unit.to_le_bytes()
                    };
                    out.extend_from_slice(&bytes);
                }
            }
        }
        Ok(())
    }

    pub fn into_inner(self) -> W {
        self.inner
    }

    fn bom(&self) -> &'static [u8] {
        match self.enc {
            Encoding::Utf16 => &[0xFF, 0xFE],
            _ => &[],
        }
    }
}

impl<W: Write> Write for EncodedWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.pending.extend_from_slice(buf);
        let mut out = Vec::with_capacity(buf.len());
        if !self.wrote_bom && matches!(self.enc, Encoding::Utf16) {
            out.extend_from_slice(self.bom());
            self.wrote_bom = true;
        }
        let mut pos = self.base;
        let mut consumed = 0usize;
        let chunk: &[u8] = &self.pending;
        let (text, complete) = match std::str::from_utf8(chunk) {
            Ok(text) => (text, true),
            Err(e) => {
                let good = e.valid_up_to();
                if e.error_len().is_some() {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "invalid utf-8 in output",
                    ));
                }
                // incomplete tail: hand over what is complete, keep the rest
                (
                    std::str::from_utf8(&chunk[..good]).map_err(|_| {
                        io::Error::new(io::ErrorKind::InvalidData, "invalid utf-8 in output")
                    })?,
                    false,
                )
            }
        };
        for c in text.chars() {
            self.encode_char(c, pos, &mut out)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
            pos += 1;
        }
        if complete {
            consumed = chunk.len();
        }
        self.pending.drain(..consumed);
        self.base = pos;
        if !out.is_empty() {
            self.inner.write_all(&out)?;
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    fn encode(enc: &str, text: &str) -> Result<Vec<u8>, PortError> {
        let mut w = EncodedWriter::new(Vec::new(), Encoding::parse(enc)?);
        w.write_all(text.as_bytes()).map_err(PortError::Io)?;
        Ok(w.inner)
    }

    #[test]
    fn latin1_and_cp1252_map_single_bytes() {
        assert_eq!(encode("latin-1", "aé").unwrap(), vec![b'a', 0xE9]);
        assert_eq!(encode("cp1252", "€").unwrap(), vec![0x80]);
        assert!(encode("latin-1", "€").is_err());
    }

    #[test]
    fn utf16_adds_one_bom() {
        let bytes = encode("utf-16", "ab").unwrap();
        assert_eq!(bytes, vec![0xFF, 0xFE, b'a', 0x00, b'b', 0x00]);
        assert_eq!(encode("utf-16-be", "a").unwrap(), vec![0x00, b'a']);
    }

    #[test]
    fn error_message_matches_cpython() {
        let err = encode("latin-1", "a中").unwrap_err().to_string();
        assert_eq!(
            err,
            "'latin-1' codec can't encode character '\\u4e2d' in position 1: ordinal not in range(256)"
        );
        let err = encode("cp1252", "\u{81}").unwrap_err().to_string();
        assert!(err.contains("character maps to <undefined>"));
    }

    #[test]
    fn split_utf8_sequences_survive() {
        let mut w = EncodedWriter::new(Vec::new(), Encoding::Utf8);
        let emoji = "😀".as_bytes();
        w.write_all(&emoji[..2]).unwrap();
        w.write_all(&emoji[2..]).unwrap();
        assert_eq!(w.inner, emoji);
    }
}
