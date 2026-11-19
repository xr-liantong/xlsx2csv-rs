//! `xl/sharedStrings.xml` — expat state machine from upstream
//! (`si` / `t` / `rPh`), including the `-e` escape helpers.

use crate::errors::PortError;
use crate::ooxml::{local_name, normalize_eol, text_of};
use quick_xml::events::Event;
use quick_xml::Reader;

#[derive(Default)]
pub struct SharedStrings {
    pub strings: Vec<String>,
}

impl SharedStrings {
    pub fn parse(xml: &[u8]) -> Result<Self, PortError> {
        let mut sst = SharedStrings::default();
        let mut r = Reader::from_reader(xml);
        let mut buf = Vec::new();
        let mut value = String::new();
        let mut in_si = false;
        let mut in_t = false;
        let mut in_rph = false;
        let mut text = Vec::new();
        loop {
            match r.read_event_into(&mut buf) {
                Ok(Event::Start(e)) => {
                    let name = local_name(e.name().as_ref());
                    match name.as_str() {
                        "si" => {
                            in_si = true;
                            value.clear();
                        }
                        "t" => {
                            in_t = !in_rph && in_si;
                            text.clear();
                        }
                        "rPh" => in_rph = true,
                        _ => {}
                    }
                }
                Ok(Event::Text(e)) => {
                    if in_t {
                        text.extend_from_slice(e.as_ref());
                    }
                }
                Ok(Event::End(e)) => {
                    let name = local_name(e.name().as_ref());
                    match name.as_str() {
                        "si" => {
                            in_si = false;
                            sst.strings.push(std::mem::take(&mut value));
                        }
                        "t" => {
                            if in_t {
                                value.push_str(&text_of(&normalize_eol(&text)));
                            }
                            in_t = false;
                        }
                        "rPh" => in_rph = false,
                        _ => {}
                    }
                }
                Ok(Event::Eof) => break,
                Ok(_) => {}
                Err(e) => return Err(PortError::Xml(format!("{e}"))),
            }
            buf.clear();
        }
        Ok(sst)
    }

    /// upstream `escape_strings()` (`-e`).
    pub fn escape_strings(&mut self) {
        for s in &mut self.strings {
            *s = s
                .replace('\r', "\\r")
                .replace('\n', "\\n")
                .replace('\t', "\\t");
        }
    }

    /// upstream `replace_line_breaks()` (`--no-line-breaks`).
    pub fn replace_line_breaks(&mut self) {
        for s in &mut self.strings {
            *s = s.replace(['\r', '\n', '\t'], " ");
        }
    }
}
