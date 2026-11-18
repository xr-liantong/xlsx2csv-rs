//! `xl/styles.xml` → number formats per cell style, exactly as upstream
//! `Styles.parse` reads them (including the `applyNumberFormat` fallback).

use crate::errors::PortError;
use crate::ooxml::{attr, local_name};
use quick_xml::events::Event;
use quick_xml::Reader;
use std::collections::HashMap;

pub fn standard_format(num_fmt_id: i32) -> Option<&'static str> {
    Some(match num_fmt_id {
        0 => "general",
        1 => "0",
        2 => "0.00",
        3 => "#,##0",
        4 => "#,##0.00",
        9 => "0%",
        10 => "0.00%",
        11 => "0.00e+00",
        12 => "# ?/?",
        13 => "# ??/??",
        14 => "mm-dd-yy",
        15 => "d-mmm-yy",
        16 => "d-mmm",
        17 => "mmm-yy",
        18 => "h:mm am/pm",
        19 => "h:mm:ss am/pm",
        20 => "h:mm",
        21 => "h:mm:ss",
        22 => "m/d/yy h:mm",
        37 => "#,##0 ;(#,##0)",
        38 => "#,##0 ;[red](#,##0)",
        39 => "#,##0.00;(#,##0.00)",
        40 => "#,##0.00;[red](#,##0.00)",
        45 => "mm:ss",
        46 => "[h]:mm:ss",
        47 => "mmss.0",
        48 => "##0.0e+0",
        49 => "@",
        _ => return None,
    })
}

/// Format code → format type, mirroring upstream `FORMATS`.
pub fn known_format_type(format_str: &str) -> Option<&'static str> {
    Some(match format_str {
        "general"
        | "0"
        | "0.00"
        | "#,##0"
        | "#,##0.00"
        | "0.00e+00"
        | "#,##0 ;(#,##0)"
        | "#,##0 ;[red](#,##0)"
        | "#,##0.00;(#,##0.00)"
        | "#,##0.00;[red](#,##0.00)"
        | "##0.0e+0"
        | "@"
        | "#,##0;(#,##0)"
        | "_(* #,##0_);_(* (#,##0);_(* \"-\"??_);_(@_)"
        | "_(* #,##0.00_);_(* (#,##0.00);_(* \"-\"??_);_(@_)" => "float",
        "0%" | "0.00%" => "percentage",
        "mm-dd-yy"
        | "d-mmm-yy"
        | "d-mmm"
        | "mmm-yy"
        | "h:mm am/pm"
        | "h:mm:ss am/pm"
        | "m/d/yy h:mm"
        | "yyyy\\-mm\\-dd"
        | "dd/mm/yy"
        | "dd/mm/yy\\ hh:mm"
        | "dd/mm/yyyy hh:mm:ss"
        | "yy-mm-dd"
        | "d-mmm-yyyy"
        | "m/d/yy"
        | "m/d/yyyy"
        | "dd-mmm-yyyy"
        | "dd/mm/yyyy"
        | "mm/dd/yy h:mm am/pm"
        | "mm/dd/yy hh:mm"
        | "mm/dd/yyyy h:mm am/pm"
        | "mm/dd/yyyy hh:mm:ss"
        | "yyyy-mm-dd hh:mm:ss" => "date",
        "h:mm" | "h:mm:ss" | "mm:ss" | "[h]:mm:ss" | "mmss.0" | "hh:mm:ss" => "time",
        _ => return None,
    })
}

#[derive(Default)]
pub struct Styles {
    pub num_fmts: HashMap<i32, String>,
    pub cell_xfs: Vec<Option<i32>>,
}

impl Styles {
    pub fn parse(xml: &[u8]) -> Result<Self, PortError> {
        let mut st = Styles::default();
        let mut r = Reader::from_reader(xml);
        let mut buf = Vec::new();
        let mut in_cell_xfs = false;
        loop {
            match r.read_event_into(&mut buf) {
                Ok(Event::Start(e)) => {
                    let name = local_name(e.name().as_ref());
                    match name.as_str() {
                        "numFmt" => {
                            if let (Some(id), Some(code)) =
                                (attr(&e, "numFmtId"), attr(&e, "formatCode"))
                            {
                                if let Ok(id) = id.trim().parse::<i32>() {
                                    st.num_fmts
                                        .insert(id, code.to_lowercase().replace('\\', ""));
                                }
                            }
                        }
                        "cellXfs" => in_cell_xfs = true,
                        _ => {}
                    }
                }
                Ok(Event::Empty(e)) => {
                    let name = local_name(e.name().as_ref());
                    if name == "numFmt" {
                        if let (Some(id), Some(code)) =
                            (attr(&e, "numFmtId"), attr(&e, "formatCode"))
                        {
                            if let Ok(id) = id.trim().parse::<i32>() {
                                st.num_fmts
                                    .insert(id, code.to_lowercase().replace('\\', ""));
                            }
                        }
                    } else if name == "xf" && in_cell_xfs {
                        st.push_xf(&e);
                    }
                }
                Ok(Event::End(e)) => {
                    if local_name(e.name().as_ref()) == "cellXfs" {
                        in_cell_xfs = false;
                    }
                }
                Ok(Event::Eof) => break,
                Ok(_) => {}
                Err(e) => return Err(PortError::Xml(format!("{e}"))),
            }
            buf.clear();
        }
        Ok(st)
    }

    fn push_xf(&mut self, e: &quick_xml::events::BytesStart<'_>) {
        match attr(e, "numFmtId") {
            Some(raw) => {
                let mut id = raw.trim().parse::<i32>().unwrap_or(0);
                if self.lookup(id).is_none() {
                    id = attr(e, "applyNumberFormat")
                        .and_then(|v| v.trim().parse::<i32>().ok())
                        .unwrap_or(0);
                }
                self.cell_xfs.push(Some(id));
            }
            None => self.cell_xfs.push(None),
        }
    }

    /// upstream `chk_exists`: custom numFmts first, then the standard table.
    pub fn lookup(&self, id: i32) -> Option<&str> {
        if let Some(s) = self.num_fmts.get(&id) {
            return Some(s.as_str());
        }
        standard_format(id)
    }
}
