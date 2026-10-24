//! Unix time to calendar date/time (proleptic Gregorian, no time zones
//! beyond a fixed UTC offset).

/// A calendar date and time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct DateTime {
    /// Full year, e.g. 2026.
    pub year: i32,
    /// 1–12.
    pub month: u8,
    /// 1–31.
    pub day: u8,
    /// 0–23.
    pub hour: u8,
    /// 0–59.
    pub minute: u8,
    /// 0–59.
    pub second: u8,
    /// 0 = Monday … 6 = Sunday (ISO 8601).
    pub weekday: u8,
}

impl DateTime {
    /// Converts seconds since 1970-01-01T00:00:00 (shifted by
    /// `offset_minutes` east of UTC) into a date and time.
    pub const fn from_unix(unix_seconds: i64, offset_minutes: i32) -> Self {
        let t = unix_seconds + offset_minutes as i64 * 60;
        let days = t.div_euclid(86_400);
        let secs = t.rem_euclid(86_400);
        let (year, month, day) = civil_from_days(days);
        Self {
            year,
            month,
            day,
            hour: (secs / 3600) as u8,
            minute: (secs / 60 % 60) as u8,
            second: (secs % 60) as u8,
            // 1970-01-01 was a Thursday (ISO weekday 3).
            weekday: (days + 3).rem_euclid(7) as u8,
        }
    }

    /// Three-letter English weekday name.
    pub const fn weekday_name(&self) -> &'static str {
        match self.weekday {
            0 => "Mon",
            1 => "Tue",
            2 => "Wed",
            3 => "Thu",
            4 => "Fri",
            5 => "Sat",
            _ => "Sun",
        }
    }
}

/// Days since 1970-01-01 → (year, month, day). Howard Hinnant's
/// `civil_from_days`, valid for the whole `i32` year range.
const fn civil_from_days(z: i64) -> (i32, u8, u8) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097); // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11], March-based
    let day = (doy - (153 * mp + 2) / 5 + 1) as u8;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u8;
    let year = (yoe + era * 400) as i32 + (month <= 2) as i32;
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ymd_hms(t: i64, off: i32) -> (i32, u8, u8, u8, u8, u8) {
        let d = DateTime::from_unix(t, off);
        (d.year, d.month, d.day, d.hour, d.minute, d.second)
    }

    #[test]
    fn epoch_and_known_dates() {
        assert_eq!(ymd_hms(0, 0), (1970, 1, 1, 0, 0, 0));
        assert_eq!(DateTime::from_unix(0, 0).weekday_name(), "Thu");
        assert_eq!(ymd_hms(951_782_400, 0), (2000, 2, 29, 0, 0, 0)); // leap day
        assert_eq!(ymd_hms(1_767_225_600, 0), (2026, 1, 1, 0, 0, 0));
        assert_eq!(DateTime::from_unix(1_767_225_600, 0).weekday_name(), "Thu");
        assert_eq!(ymd_hms(4_102_444_799, 0), (2099, 12, 31, 23, 59, 59));
        assert_eq!(ymd_hms(-1, 0), (1969, 12, 31, 23, 59, 59));
    }

    #[test]
    fn utc_offsets_cross_midnight() {
        // 2026-01-01T20:30Z at UTC+07:00 is 03:30 the next day.
        assert_eq!(
            ymd_hms(1_767_225_600 + 20 * 3600 + 1800, 420),
            (2026, 1, 2, 3, 30, 0)
        );
        // And at UTC-05:00 it's 15:30 the same day.
        assert_eq!(
            ymd_hms(1_767_225_600 + 20 * 3600 + 1800, -300),
            (2026, 1, 1, 15, 30, 0)
        );
    }

    #[test]
    fn every_day_increments_by_one() {
        // Walk 800 years of days and check the calendar never skips.
        let mut prev = DateTime::from_unix(0, 0);
        for d in 1..(800 * 366) {
            let cur = DateTime::from_unix(d * 86_400, 0);
            let next_expected = (prev.day + 1, prev.month, prev.year);
            assert!(
                (cur.day, cur.month, cur.year) == next_expected
                    || (cur.day == 1
                        && (cur.month == prev.month + 1
                            || (cur.month == 1 && cur.year == prev.year + 1))),
                "{prev:?} -> {cur:?}"
            );
            assert_eq!(cur.weekday, (prev.weekday + 1) % 7);
            prev = cur;
        }
    }
}
