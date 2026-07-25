//! RFC 3339 UTC timestamps under the frozen wire derivation (ops schema
//! `timestamp` def, gap note G3 / RT-17): `YYYY-MM-DDThh:mm:ss[.f{1,9}]Z`,
//! semantically valid in the proleptic Gregorian calendar — impossible
//! calendar instants and leap seconds are rejected.

/// Seconds since the Unix epoch, from the system clock.
pub fn unix_now() -> i64 {
    match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
        Ok(d) => d.as_secs() as i64,
        Err(_) => 0,
    }
}

fn is_leap(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn days_in_month(year: i64, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if is_leap(year) {
                29
            } else {
                28
            }
        }
        _ => 0,
    }
}

/// Renders a Unix timestamp as `YYYY-MM-DDThh:mm:ssZ` (proleptic
/// Gregorian, civil-from-days algorithm).
pub fn rfc3339_utc(unix: i64) -> String {
    let days = unix.div_euclid(86_400);
    let secs = unix.rem_euclid(86_400);
    let (h, m, s) = (secs / 3600, (secs % 3600) / 60, secs % 60);
    // Howard Hinnant's civil_from_days.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let mth = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if mth <= 2 { y + 1 } else { y };
    format!("{year:04}-{mth:02}-{d:02}T{h:02}:{m:02}:{s:02}Z")
}

/// Parses one wire timestamp back to Unix seconds (fractions dropped),
/// enforcing lexical shape AND calendar validity.
pub fn parse_rfc3339_utc(s: &str) -> Option<i64> {
    let bytes = s.as_bytes();
    if bytes.len() < 20 || *bytes.last()? != b'Z' {
        return None;
    }
    let digits = |range: std::ops::Range<usize>| -> Option<i64> {
        let part = s.get(range)?;
        if !part.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        part.parse().ok()
    };
    let sep = |i: usize, c: u8| bytes.get(i) == Some(&c);
    if !(sep(4, b'-') && sep(7, b'-') && sep(10, b'T') && sep(13, b':') && sep(16, b':')) {
        return None;
    }
    let (year, month, day) = (digits(0..4)?, digits(5..7)?, digits(8..10)?);
    let (hour, minute, second) = (digits(11..13)?, digits(14..16)?, digits(17..19)?);
    // Fractional part: `.` plus 1..=9 digits, then the final Z.
    match bytes.get(19) {
        Some(b'Z') if bytes.len() == 20 => {}
        Some(b'.') => {
            let frac = s.get(20..bytes.len() - 1)?;
            if frac.is_empty() || frac.len() > 9 || !frac.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
        }
        _ => return None,
    }
    if !(1..=12).contains(&month)
        || day < 1
        || day > days_in_month(year, month as u32) as i64
        || hour > 23
        || minute > 59
        || second > 59
    {
        // Impossible instants and leap seconds fail closed (RT-17).
        return None;
    }
    // days_from_civil.
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = if month > 2 { month - 3 } else { month + 9 };
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some(days * 86_400 + hour * 3600 + minute * 60 + second)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_rejects_impossible_instants() {
        let now = 1_785_000_000;
        let text = rfc3339_utc(now);
        assert_eq!(parse_rfc3339_utc(&text), Some(now));
        assert_eq!(parse_rfc3339_utc("2026-02-30T00:00:00Z"), None);
        assert_eq!(
            parse_rfc3339_utc("2026-06-30T23:59:60Z"),
            None,
            "leap seconds rejected"
        );
        assert!(parse_rfc3339_utc("2026-08-01T00:00:00.123Z").is_some());
        assert_eq!(parse_rfc3339_utc("2026-08-01T00:00:00+00:00"), None);
        assert!(parse_rfc3339_utc("2024-02-29T12:00:00Z").is_some());
    }
}
