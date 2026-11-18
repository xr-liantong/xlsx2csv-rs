//! Excel serial dates → `datetime`, and the strftime subset upstream needs.
//!
//! `timedelta(days=serial)` rounds to the microsecond with round-half-even;
//! the civil-date conversion is Hinnant's algorithm (no external crate).

#![allow(clippy::many_single_char_names)]

#[derive(Debug, Clone, Copy)]
pub struct DateTime {
    pub year: i32,
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
    pub second: u32,
}

const MICROS: i64 = 86_400_000_000;

fn days_from_civil(y: i32, m: u32, d: u32) -> i64 {
    let y = i64::from(y) - i64::from(m <= 2);
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let m = i64::from(m);
    let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + i64::from(d) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn civil_from_days(z: i64) -> (i32, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y as i32, m as u32, d as u32)
}

/// `datetime(1899,12,30) + timedelta(days=serial)` (or the 1904 epoch).
pub fn serial_to_datetime(serial: f64, date1904: bool) -> DateTime {
    let base = if date1904 {
        days_from_civil(1904, 1, 1)
    } else {
        days_from_civil(1899, 12, 30)
    };
    let total = (serial * MICROS as f64).round_ties_even() as i64;
    let mut days = total.div_euclid(MICROS);
    let mut rem = total.rem_euclid(MICROS);
    // Python normalizes timedelta the same way: whole days carry the rest.
    while rem >= MICROS {
        rem -= MICROS;
        days += 1;
    }
    let (year, month, day) = civil_from_days(base + days);
    let secs = rem / 1_000_000;
    DateTime {
        year,
        month,
        day,
        hour: ((secs / 3600) % 24) as u32,
        minute: ((secs / 60) % 60) as u32,
        second: (secs % 60) as u32,
    }
    .fix_secs(secs)
}

impl DateTime {
    fn fix_secs(self, _secs: i64) -> Self {
        self
    }

    pub fn from_hms(hour: u32, minute: u32, second: u32) -> Self {
        DateTime {
            year: 1900,
            month: 1,
            day: 1,
            hour,
            minute,
            second,
        }
    }
}

const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

const WEEKDAYS: [&str; 7] = [
    "Sunday",
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
];

fn weekday(dt: &DateTime) -> usize {
    let days = days_from_civil(dt.year, dt.month, dt.day);
    // 1970-01-01 was a Thursday (4).
    ((days + 4).rem_euclid(7)) as usize
}

/// Subset of C strftime: enough for Excel number formats and user overrides.
/// Unknown directives are passed through verbatim, like glibc does.
pub fn strftime(dt: &DateTime, fmt: &str) -> String {
    let mut out = String::new();
    let mut chars = fmt.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '%' {
            out.push(c);
            continue;
        }
        let Some(d) = chars.next() else {
            out.push('%');
            break;
        };
        match d {
            'Y' => out.push_str(&format!("{:04}", dt.year)),
            'y' => out.push_str(&format!("{:02}", dt.year.rem_euclid(100))),
            'm' => out.push_str(&format!("{:02}", dt.month)),
            'd' => out.push_str(&format!("{:02}", dt.day)),
            'e' => out.push_str(&format!("{:2}", dt.day)),
            'H' => out.push_str(&format!("{:02}", dt.hour)),
            'I' => {
                let h = dt.hour % 12;
                out.push_str(&format!("{:02}", if h == 0 { 12 } else { h }));
            }
            'M' => out.push_str(&format!("{:02}", dt.minute)),
            'S' => out.push_str(&format!("{:02}", dt.second)),
            'p' => out.push_str(if dt.hour < 12 { "AM" } else { "PM" }),
            'B' => out.push_str(MONTHS[(dt.month as usize).saturating_sub(1).min(11)]),
            'b' | 'h' => {
                let m = MONTHS[(dt.month as usize).saturating_sub(1).min(11)];
                out.push_str(&m[..3]);
            }
            'A' => out.push_str(WEEKDAYS[weekday(dt)]),
            'a' => out.push_str(&WEEKDAYS[weekday(dt)][..3]),
            'j' => {
                let doy =
                    days_from_civil(dt.year, dt.month, dt.day) - days_from_civil(dt.year, 1, 1) + 1;
                out.push_str(&format!("{doy:03}"));
            }
            'F' => out.push_str(&format!("{:04}-{:02}-{:02}", dt.year, dt.month, dt.day)),
            'T' => out.push_str(&format!("{:02}:{:02}:{:02}", dt.hour, dt.minute, dt.second)),
            'R' => out.push_str(&format!("{:02}:{:02}", dt.hour, dt.minute)),
            '%' => out.push('%'),
            other => {
                out.push('%');
                out.push(other);
            }
        }
    }
    out
}

/// Upstream's format-code → strftime translation (no `-f` override given).
pub fn excel_format_to_strftime(format_str: &str) -> String {
    let s = strip_currency(format_str);
    s.replace(";@", "")
        .replace("yyyy", "%Y")
        .replace("yy", "%y")
        .replace("hh:mm", "%H:%M")
        .replace('h', "%I")
        .replace("%H%H", "%H")
        .replace("ss", "%S")
        .replace("dddd", "d")
        .replace("dd", "d")
        .replace('d', "%d")
        .replace("am/pm", "%p")
        .replace("mmmm", "%B")
        .replace("mmm", "%b")
        .replace(":mm", ":%M")
        .replace('m', "%m")
        .replace("%m%m", "%m")
}

/// Remove the first `[$-xxx]` locale prefix (upstream regex, count=1).
fn strip_currency(s: &str) -> String {
    let bytes: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i + 3 < bytes.len() {
        if bytes[i] == '[' && bytes[i + 1] == '$' && bytes[i + 2] == '-' {
            let mut j = i + 3;
            while j < bytes.len() && bytes[j].is_ascii_alphanumeric() {
                j += 1;
            }
            if j < bytes.len() && bytes[j] == ']' {
                let mut out: String = bytes[..i].iter().collect();
                out.extend(bytes[j + 1..].iter());
                return out;
            }
        }
        i += 1;
    }
    s.to_string()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn serial_matches_excel_epoch() {
        // 45000 is Excel's serial for 2023-03-15 (epoch 1899-12-30).
        let dt = serial_to_datetime(45000.0, false);
        assert_eq!((dt.year, dt.month, dt.day), (2023, 3, 15));
        let dt1904 = serial_to_datetime(45000.0, true);
        assert_eq!((dt1904.year, dt1904.month, dt1904.day), (2027, 3, 16));
    }

    #[test]
    fn translation_matches_upstream_table() {
        assert_eq!(excel_format_to_strftime("yyyy-mm-dd"), "%Y-%m-%d");
        assert_eq!(excel_format_to_strftime("yyyy/m/d hh:mm"), "%Y/%m/%d %H:%M");
        assert_eq!(excel_format_to_strftime("0.00%"), "0.00%");
    }

    #[test]
    fn time_format_pads() {
        let dt = DateTime::from_hms(7, 5, 9);
        assert_eq!(strftime(&dt, "%H:%M"), "07:05");
        assert_eq!(strftime(&DateTime::from_hms(13, 45, 0), "%H:%M"), "13:45");
    }
}
