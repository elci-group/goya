//! UTC timestamps without a calendar dependency.

use std::time::{SystemTime, UNIX_EPOCH};

/// Civil date (year, month, day) for a count of days since 1970-01-01.
/// Howard Hinnant's `civil_from_days`, valid for the proleptic Gregorian
/// calendar.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// RFC 3339 UTC timestamp with second precision, e.g. `2026-11-19T08:15:00Z`.
pub fn utc_timestamp(at: SystemTime) -> String {
    let secs = at
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let (y, m, d) = civil_from_days(secs.div_euclid(86_400));
    let rem = secs.rem_euclid(86_400);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        (rem % 3600) / 60,
        rem % 60
    )
}

/// UTC calendar date, e.g. `2026-11-19`.
pub fn utc_date(at: SystemTime) -> String {
    utc_timestamp(at)[..10].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn epoch_and_known_instants() {
        assert_eq!(utc_timestamp(UNIX_EPOCH), "1970-01-01T00:00:00Z");
        // 2026-11-19T08:15:30Z = 1795076130
        let t = UNIX_EPOCH + Duration::from_secs(1_795_076_130);
        assert_eq!(utc_timestamp(t), "2026-11-19T08:15:30Z");
        assert_eq!(utc_date(t), "2026-11-19");
        // Leap day 2024-02-29T23:59:59Z = 1709251199
        let t = UNIX_EPOCH + Duration::from_secs(1_709_251_199);
        assert_eq!(utc_timestamp(t), "2024-02-29T23:59:59Z");
    }
}
