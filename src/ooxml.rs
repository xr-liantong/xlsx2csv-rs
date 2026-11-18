//! OOXML container: zip access plus the package parts upstream reads with
//! minidom (`[Content_Types].xml`, workbook, workbook relationships).

use crate::errors::PortError;
use quick_xml::events::Event;
use quick_xml::Reader;
use std::collections::HashMap;
use std::io::{Cursor, Read, Seek};

pub trait ReadSeek: Read + Seek {}
impl<T: Read + Seek> ReadSeek for T {}

/// Whole package handle. Parts are located the way upstream `_filehandle`
/// does it: compare the entry name with the requested path lowercased and
/// with the leading `/` stripped.
pub struct Package {
    archive: zip::ZipArchive<Box<dyn ReadSeek>>,
    names: Vec<String>,
}

impl Package {
    pub fn open(path: &str) -> Result<Self, PortError> {
        let src: Box<dyn ReadSeek> = if path == "-" {
            let mut buf = Vec::new();
            std::io::stdin().read_to_end(&mut buf)?;
            Box::new(Cursor::new(buf))
        } else {
            Box::new(std::fs::File::open(path).map_err(|_| PortError::InvalidXlsx(path.into()))?)
        };
        let archive =
            zip::ZipArchive::new(src).map_err(|_| PortError::InvalidXlsx(path.to_string()))?;
        let names = archive.file_names().map(|s| s.to_string()).collect();
        Ok(Package { archive, names })
    }

    /// Index of the first entry whose lowercased name equals `path` without
    /// its leading slash, or None.
    pub fn find(&self, path: &str) -> Option<usize> {
        let want = path.trim_start_matches('/').to_lowercase();
        self.names.iter().position(|n| n.to_lowercase() == want)
    }

    pub fn entry(
        &mut self,
        index: usize,
    ) -> Result<zip::read::ZipFile<'_, Box<dyn ReadSeek>>, PortError> {
        Ok(self.archive.by_index(index)?)
    }

    /// Read a whole part; None when the part is absent (upstream would fail).
    pub fn read_part(&mut self, path: &str) -> Result<Option<Vec<u8>>, PortError> {
        let Some(i) = self.find(path) else {
            return Ok(None);
        };
        let mut f = self.entry(i)?;
        let mut buf = Vec::with_capacity(f.size() as usize);
        f.read_to_end(&mut buf)?;
        Ok(Some(buf))
    }

    pub fn require_part(&mut self, path: &str) -> Result<Vec<u8>, PortError> {
        match self.read_part(path)? {
            Some(b) => Ok(b),
            None => Err(PortError::InvalidXlsx(path.to_string())),
        }
    }
}

pub(crate) fn local_name(qname: &[u8]) -> String {
    let s = String::from_utf8_lossy(qname);
    match s.rsplit_once(':') {
        Some((_, local)) => local.to_string(),
        None => s.to_string(),
    }
}

pub(crate) fn attr(e: &quick_xml::events::BytesStart<'_>, key: &str) -> Option<String> {
    for a in e.attributes().flatten() {
        if a.key.as_ref() == key.as_bytes() {
            return match a.unescape_value() {
                Ok(v) => Some(v.into_owned()),
                Err(_) => Some(String::from_utf8_lossy(&a.value).into_owned()),
            };
        }
    }
    None
}

pub(crate) fn text_of(raw: &[u8]) -> String {
    let s = String::from_utf8_lossy(raw);
    match quick_xml::escape::unescape(&s) {
        Ok(v) => v.into_owned(),
        Err(_) => s.into_owned(),
    }
}

pub struct ContentTypes {
    pub workbook: String,
    pub styles: Option<String>,
    pub worksheet: Option<String>,
    pub shared_strings: Option<String>,
    pub relationships: Vec<String>,
}

const T_WORKBOOK: &str =
    "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml";
const T_STYLES: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml";
const T_WORKSHEET: &str =
    "application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml";
const T_SST: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml.sharedStrings+xml";
const T_RELS: &str = "application/vnd.openxmlformats-package.relationships+xml";

impl Default for ContentTypes {
    /// upstream `ContentTypes()`: only the workbook path and the derived
    /// relationships path are known before `[Content_Types].xml` is read.
    fn default() -> Self {
        ContentTypes {
            workbook: "/xl/workbook.xml".to_string(),
            styles: None,
            worksheet: None,
            shared_strings: None,
            relationships: vec!["/xl/_rels/workbook.xml.rels".to_string()],
        }
    }
}

impl ContentTypes {
    pub fn parse(xml: &[u8]) -> Result<Self, PortError> {
        let mut ct = ContentTypes {
            workbook: "/xl/workbook.xml".to_string(),
            styles: None,
            worksheet: None,
            shared_strings: None,
            relationships: Vec::new(),
        };
        let mut r = Reader::from_reader(xml);
        let mut buf = Vec::new();
        loop {
            match r.read_event_into(&mut buf) {
                Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                    if local_name(e.name().as_ref()) == "Override" {
                        let ty = attr(&e, "ContentType").unwrap_or_default();
                        let part = attr(&e, "PartName").unwrap_or_default();
                        match ty.as_str() {
                            T_WORKBOOK => ct.workbook = part,
                            T_STYLES => ct.styles = Some(part),
                            T_WORKSHEET => ct.worksheet = Some(part),
                            T_SST => ct.shared_strings = Some(part),
                            T_RELS => ct.relationships.push(part),
                            _ => {}
                        }
                    }
                }
                Ok(Event::Eof) => break,
                Ok(_) => {}
                Err(e) => return Err(PortError::Xml(format!("{e}"))),
            }
            buf.clear();
        }
        if ct.relationships.is_empty() {
            let (dir, base) = match ct.workbook.rsplit_once('/') {
                Some((d, b)) => (d.to_string(), b.to_string()),
                None => (String::new(), ct.workbook.clone()),
            };
            ct.relationships.push(format!("{dir}/_rels/{base}.rels"));
        }
        Ok(ct)
    }
}

pub struct Rel {
    pub target: Option<String>,
}

#[derive(Default)]
pub struct Relationships {
    pub map: HashMap<String, Rel>,
}

impl Relationships {
    pub fn parse(xml: &[u8]) -> Result<Self, PortError> {
        let mut rels = Relationships::default();
        let mut r = Reader::from_reader(xml);
        let mut buf = Vec::new();
        loop {
            match r.read_event_into(&mut buf) {
                Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                    if local_name(e.name().as_ref()) == "Relationship" {
                        if let Some(id) = attr(&e, "Id") {
                            rels.map.insert(
                                id,
                                Rel {
                                    target: attr(&e, "Target"),
                                },
                            );
                        }
                    }
                }
                Ok(Event::Eof) => break,
                Ok(_) => {}
                Err(e) => return Err(PortError::Xml(format!("{e}"))),
            }
            buf.clear();
        }
        Ok(rels)
    }
}

pub struct SheetMeta {
    pub name: String,
    pub index: u32,
    pub state: Option<String>,
    pub relation_id: Option<String>,
}

pub struct Workbook {
    pub date1904: bool,
    pub sheets: Vec<SheetMeta>,
    pub relationships: Relationships,
}

impl Workbook {
    pub fn parse(xml: &[u8]) -> Result<Self, PortError> {
        let mut wb = Workbook {
            date1904: false,
            sheets: Vec::new(),
            relationships: Relationships::default(),
        };
        let mut r = Reader::from_reader(xml);
        let mut buf = Vec::new();
        let mut index = 0u32;
        loop {
            match r.read_event_into(&mut buf) {
                Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                    let name = local_name(e.name().as_ref());
                    if name == "workbookPr" {
                        if let Some(v) = attr(&e, "date1904") {
                            let v = v.trim().to_lowercase();
                            wb.date1904 = v == "1" || v == "true";
                        }
                    } else if name == "sheet" {
                        index += 1;
                        wb.sheets.push(SheetMeta {
                            name: attr(&e, "name").unwrap_or_default(),
                            index,
                            state: attr(&e, "state"),
                            relation_id: attr(&e, "r:id"),
                        });
                    }
                }
                Ok(Event::Eof) => break,
                Ok(_) => {}
                Err(e) => return Err(PortError::Xml(format!("{e}"))),
            }
            buf.clear();
        }
        Ok(wb)
    }

    pub fn sheet_id_by_name(&self, name: &str) -> Option<u32> {
        self.sheets.iter().find(|s| s.name == name).map(|s| s.index)
    }
}
