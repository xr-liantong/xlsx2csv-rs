//! Cell value formatting — the port of upstream `Sheet.handleCharData`.
//!
//! Order of decisions, the `FORMATS`/`STANDARD_FORMATS` lookups, the date
//! heuristic, `float()` formatting and `Decimal` percentage rounding all
//! follow upstream 0.8.6 line by line; the odd cases are documented in
//! `docs/UPSTREAM-QUIRKS.md`.

use crate::dates::{excel_format_to_strftime, serial_to_datetime, strftime, DateTime};
use crate::errors::PortError;
use crate::styles::{known_format_type, Styles};

pub const EXCEL_ERROR_VALUES: [&str; 9] = [
    "#N/A", "#REF!", "#DIV/0!", "#CALC!", "#NAME?", "#NULL!", "#NUM!", "#SPILL!", "#VALUE!",
];

pub struct FmtOptions<'a> {
    pub dateformat: Option<&'a str>,
    pub timeformat: &'a str,
    pub floatformat: Option<&'a str>,
    pub ignore_formats: Vec<&'a str>,
    pub ignore_percentage: bool,
    pub scifloat: bool,
}

pub fn format_cell(
    col_type: Option<&str>,
    s_attr: Option<&str>,
    raw: &str,
    styles: &Styles,
    shared_strings: &[String],
    date1904: bool,
    opts: &FmtOptions<'_>,
) -> Result<String, PortError> {
    let mut data = raw.to_string();
    let mut format_type: Option<&'static str> = None;
    let mut format_str = "general".to_string();

    match col_type {
        Some("s") => {
            format_type = Some("string");
            data = match py_int(raw) {
                Some(i) if i >= 0 && (i as usize) < shared_strings.len() => {
                    shared_strings[i as usize].clone()
                }
                _ => String::new(),
            };
            if data.contains("_x000D_\n") {
                data = data.replace("_x000D_\n", "\n");
            }
        }
        Some("b") => {
            format_type = Some("boolean");
            data = match py_int(raw) {
                Some(1) => "TRUE".to_string(),
                Some(0) => "FALSE".to_string(),
                _ => raw.to_string(),
            };
        }
        Some("str") | Some("inlineStr") => {
            format_type = Some("string");
            if data.contains("_x000D_\n") {
                data = data.replace("_x000D_\n", "\n");
            }
        }
        _ => {
            if let Some(s) = s_attr {
                let style_index = py_int(s).unwrap_or(0);
                let xfs_numfmt = styles
                    .cell_xfs
                    .get(style_index.max(0) as usize)
                    .copied()
                    .flatten();
                let looked_up = xfs_numfmt.and_then(|id| styles.lookup(id));
                match looked_up {
                    Some(s) if !s.is_empty() => format_str = s.to_string(),
                    _ => {
                        let shown = match looked_up {
                            Some(s) => s.to_string(),
                            None => "None".to_string(),
                        };
                        let id_shown = match xfs_numfmt {
                            Some(id) => id.to_string(),
                            None => "None".to_string(),
                        };
                        return Err(PortError::UnknownFormat(format!(
                            "unknown format {shown} at {id_shown}"
                        )));
                    }
                }

                if let Some(t) = known_format_type(&format_str) {
                    format_type = Some(t);
                } else {
                    let literals_removed = strip_format_literals(&format_str);
                    if is_plain_number(&data)
                        && has_date_letter(&literals_removed)
                        && !has_bracketed_date(&format_str)
                    {
                        let v = parse_f64(&data).map_err(|_| PortError::Value)?;
                        format_type = Some(if v < 1.0 { "time" } else { "date" });
                    } else if is_loose_number(&data)
                        || (opts.scifloat && is_loose_scientific(&data))
                    {
                        format_type = Some("float");
                    }
                }
                if format_type == Some("date") && opts.dateformat == Some("float") {
                    format_type = Some("float");
                }
            } else if col_type == Some("n")
                || (col_type.is_none() && !data.is_empty() && data.as_bytes()[0].is_ascii_digit())
            {
                format_type = Some("float");
            }
        }
    }

    if let Some(ft) = format_type {
        if !opts.ignore_formats.contains(&ft) && !EXCEL_ERROR_VALUES.contains(&data.as_str()) {
            data = match ft {
                "date" => format_date(&data, &format_str, date1904, opts)?,
                "time" => format_time(&data, opts)?,
                "float" => format_float(&data, &format_str, opts)?,
                "percentage" => format_percentage(&data, &format_str, opts)?,
                _ => data,
            };
        }
    }
    Ok(data)
}

fn format_date(
    data: &str,
    format_str: &str,
    date1904: bool,
    opts: &FmtOptions<'_>,
) -> Result<String, PortError> {
    let v = parse_f64(data).map_err(|_| PortError::Value)?;
    let dt = serial_to_datetime(v, date1904);
    match opts.dateformat {
        Some(f) => Ok(strftime(&dt, f)),
        None => Ok(strftime(&dt, &excel_format_to_strftime(format_str))
            .trim()
            .to_string()),
    }
}

fn format_time(data: &str, opts: &FmtOptions<'_>) -> Result<String, PortError> {
    let v = parse_f64(data).map_err(|_| PortError::Value)?;
    let frac = v % 1.0;
    let micros = (frac * 86_400_000_000.0).round_ties_even() as i64;
    let t = micros / 1_000_000;
    let h = ((t / 3600) % 24) as u32;
    let m = ((t / 60) % 60) as u32;
    let s = (t % 60) as u32;
    Ok(strftime(&DateTime::from_hms(h, m, s), opts.timeformat))
}

fn format_float(data: &str, format_str: &str, opts: &FmtOptions<'_>) -> Result<String, PortError> {
    let d = parse_f64(data).map_err(|_| PortError::Value)?;
    if opts.floatformat.is_none() && d.fract() == 0.0 {
        return Ok(integer_repr(d));
    }
    if let Some(ff) = opts.floatformat {
        if ff != "%f" {
            return apply_percent_format(ff, d).map_err(|_| PortError::Value);
        }
    }
    if data.contains('E') || data.contains('e') {
        return Ok(strip_zeros(&format!("{d:.6}")));
    }
    if format_str == "general" {
        return Ok(strip_zeros(&format!("{d:.6}")));
    }
    if format_str.starts_with("0.0") {
        if opts.floatformat.is_some() {
            return Ok(strip_zeros(&format!("{d:.6}")));
        }
        let fmt = strip_format_literals(format_str);
        if let Some((_, frac)) = fmt.split_once('.') {
            let scientific = frac.contains("e+") || frac.contains("e-");
            let mut places = if scientific {
                frac.chars().count()
            } else {
                frac.chars()
                    .take_while(|c| matches!(c, '0' | '#' | '?'))
                    .count()
            };
            if fmt.contains('%') {
                places += if scientific { 1 } else { 2 };
            }
            let places = places.min(17);
            return Ok(format!("{d:.places$}"));
        }
    }
    Ok(strip_zeros(&format!("{d:.6}")))
}

fn format_percentage(
    data: &str,
    format_str: &str,
    opts: &FmtOptions<'_>,
) -> Result<String, PortError> {
    if opts.ignore_percentage {
        let d = parse_f64(data).map_err(|_| PortError::Value)?;
        return Ok(strip_zeros(&format!("{d:.6}")));
    }
    let quantize_places = if format_str == "0.00%" { 2 } else { 0 };
    let dec = Decimal::parse(data).ok_or(PortError::Value)?;
    Ok(format!("{}%", dec.times_100().quantize(quantize_places)))
}

/// `"%i" % Decimal(repr(float))`: shortest round-trip digits, expanded.
fn integer_repr(d: f64) -> String {
    if d == 0.0 {
        return "0".to_string();
    }
    let s = format!("{d}");
    match s.find(['e', 'E']) {
        None => s,
        Some(_) => format!("{d:.0}"),
    }
}

fn strip_zeros(s: &str) -> String {
    let t = s.trim_end_matches('0').trim_end_matches('.');
    if t.is_empty() || t == "-" {
        "0".to_string()
    } else {
        t.to_string()
    }
}

/// Python `%.Nf` subset: `%.15f`, `%f`, `%0.2f`.
fn apply_percent_format(fmt: &str, d: f64) -> Result<String, String> {
    let body = fmt.strip_prefix('%').ok_or("format")?;
    let digits: String = body.chars().take_while(|c| c.is_ascii_digit()).collect();
    let places: usize = if digits.is_empty() {
        6
    } else {
        digits.parse().map_err(|_| "format")?
    };
    if !body.ends_with('f') {
        return Err("format".to_string());
    }
    Ok(format!("{d:.places$}"))
}

fn parse_f64(s: &str) -> Result<f64, ()> {
    let t = s.trim();
    lexical_core::parse::<f64>(t.as_bytes()).map_err(|_| ())
}

fn py_int(s: &str) -> Option<i64> {
    let t = s.trim();
    if t.is_empty() {
        return None;
    }
    let (neg, digits) = match t.strip_prefix('-') {
        Some(r) => (true, r),
        None => (false, t.strip_prefix('+').unwrap_or(t)),
    };
    if digits.is_empty() || !digits.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let v: i64 = digits.parse().ok()?;
    Some(if neg { -v } else { v })
}

/// upstream `strip_format_literals`: drop `"..."` and `_x` / `*x` pairs.
pub fn strip_format_literals(s: &str) -> String {
    let mut out = String::new();
    let mut it = s.chars();
    while let Some(c) = it.next() {
        match c {
            '"' => {
                for x in it.by_ref() {
                    if x == '"' {
                        break;
                    }
                }
            }
            '_' | '*' => {
                it.next();
            }
            _ => out.push(c),
        }
    }
    out
}

fn has_date_letter(s: &str) -> bool {
    s.chars().any(|c| "hsmdyY".contains(c))
}

fn has_bracketed_date(s: &str) -> bool {
    match (s.find('['), s.rfind(']')) {
        (Some(a), Some(b)) if b > a => s[a + 1..b].chars().any(|c| "dmhys".contains(c)),
        _ => false,
    }
}

/// `^\d+(\.\d+)?$`
fn is_plain_number(s: &str) -> bool {
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
    }
    if i == 0 {
        return false;
    }
    if i == b.len() {
        return true;
    }
    if b[i] != b'.' {
        return false;
    }
    i += 1;
    let start = i;
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
    }
    i == b.len() && i > start
}

/// `^-?\d+(.\d+)?$` — note the unescaped dot: any single character.
fn is_loose_number(s: &str) -> bool {
    let b = s.as_bytes();
    let mut i = usize::from(b.first() == Some(&b'-'));
    let start = i;
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
    }
    if i == start {
        return false;
    }
    if i == b.len() {
        return true;
    }
    i += 1; // the "any character"
    let digits = i;
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
    }
    i == b.len() && i > digits
}

/// `^-?\d+(.\d+)?([eE]-?\d+)?$`
fn is_loose_scientific(s: &str) -> bool {
    let b = s.as_bytes();
    let epos = b.iter().position(|&c| c == b'e' || c == b'E');
    let head = match epos {
        Some(i) => &s[..i],
        None => s,
    };
    if !is_loose_number(head) {
        return false;
    }
    match epos {
        None => true,
        Some(i) => {
            let tail = &s[i + 1..];
            let tail = tail.strip_prefix('-').unwrap_or(tail);
            !tail.is_empty() && tail.bytes().all(|c| c.is_ascii_digit())
        }
    }
}

/// Small exact decimal, enough for `Decimal(x) * 100` + quantize.
#[derive(Debug, Clone, Copy)]
struct Decimal {
    negative: bool,
    mantissa: i128,
    scale: i32,
}

impl Decimal {
    fn parse(s: &str) -> Option<Decimal> {
        let t = s.trim();
        let (negative, rest) = match t.strip_prefix('-') {
            Some(r) => (true, r),
            None => (false, t.strip_prefix('+').unwrap_or(t)),
        };
        let (body, exp) = match rest.find(['e', 'E']) {
            Some(i) => (rest[..i].to_string(), rest[i + 1..].parse::<i32>().ok()?),
            None => (rest.to_string(), 0),
        };
        let (int_part, frac_part) = match body.split_once('.') {
            Some((a, b)) => (a.to_string(), b.to_string()),
            None => (body, String::new()),
        };
        if int_part.is_empty() && frac_part.is_empty() {
            return None;
        }
        if !int_part
            .chars()
            .chain(frac_part.chars())
            .all(|c| c.is_ascii_digit())
        {
            return None;
        }
        let digits = format!("{int_part}{frac_part}");
        let mantissa: i128 = digits.parse().ok()?;
        Some(Decimal {
            negative,
            mantissa,
            scale: frac_part.chars().count() as i32 - exp,
        })
    }

    fn times_100(&self) -> Decimal {
        Decimal {
            negative: self.negative,
            mantissa: self.mantissa,
            scale: self.scale - 2,
        }
    }

    /// ROUND_HALF_UP (ties away from zero), `places` decimals in the output.
    fn quantize(&self, places: i32) -> String {
        let mut mantissa = self.mantissa;
        if self.scale > places {
            let mut divisor: i128 = 1;
            for _ in 0..(self.scale - places) {
                divisor = divisor.saturating_mul(10);
            }
            let q = mantissa / divisor;
            let r = mantissa % divisor;
            mantissa = if r.saturating_mul(2) >= divisor {
                q + 1
            } else {
                q
            };
        } else {
            for _ in 0..(places - self.scale) {
                mantissa = mantissa.saturating_mul(10);
            }
        }
        let mut digits = mantissa.to_string();
        let signed = if self.negative && mantissa != 0 {
            "-".to_string()
        } else {
            String::new()
        };
        if places <= 0 {
            return format!("{signed}{digits}");
        }
        let want = places as usize + 1;
        if digits.len() < want {
            digits = format!("{}{}", "0".repeat(want - digits.len()), digits);
        }
        let split = digits.len() - places as usize;
        let (int_part, frac_part) = digits.split_at(split);
        format!("{signed}{int_part}.{frac_part}")
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn plain_numbers_match_upstream_heuristics() {
        assert!(is_plain_number("0.135"));
        assert!(!is_plain_number("1e-7"));
        assert!(!is_loose_number("1e-7"));
        assert!(is_loose_scientific("1e-7"));
        assert!(is_loose_number("-12"));
        assert!(!is_loose_number("-"));
    }

    #[test]
    fn integer_floats_print_without_exponent() {
        assert_eq!(integer_repr(0.0), "0");
        assert_eq!(integer_repr(-0.0), "0");
        assert_eq!(integer_repr(2.5), "2.5");
        assert_eq!(integer_repr(1e20), "100000000000000000000");
    }

    #[test]
    fn percentage_rounds_half_up() {
        let dec = Decimal::parse("0.135").unwrap().times_100();
        assert_eq!(dec.quantize(2), "13.50");
        let dec = Decimal::parse("-0.135").unwrap().times_100();
        assert_eq!(dec.quantize(0), "-14");
        let dec = Decimal::parse("0.9995").unwrap().times_100();
        assert_eq!(dec.quantize(0), "100");
    }

    #[test]
    fn percent_format_code_keeps_places() {
        assert_eq!(strip_format_literals("0.00%"), "0.00%");
        assert_eq!(strip_format_literals("0.0 \"mm\""), "0.0 ");
    }
}
