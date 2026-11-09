//! Calendar dates without time zones. The apps pass the user's local date; the core never asks the
//! clock which day it is, so check-ins land on the day the user saw.

use std::fmt;

use crate::{Error, Result};

/// A proleptic Gregorian date, stored as days since 1970-01-01.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Date(i64);

impl Date {
    /// Parses `YYYY-MM-DD`.
    pub fn parse(s: &str) -> Result<Self> {
        let bad = || Error::Invalid(format!("bad date {s:?}, expected YYYY-MM-DD"));
        let mut parts = s.splitn(3, '-');
        let mut next = || parts.next().and_then(|p| p.parse::<i64>().ok()).ok_or_else(bad);
        let (y, m, d) = (next()?, next()?, next()?);
        if s.len() != 10 || !(1..=12).contains(&m) || d < 1 || d > days_in_month(y, m) {
            return Err(bad());
        }
        Ok(Self(days_from_civil(y, m, d)))
    }

    pub fn from_ymd(y: i64, m: i64, d: i64) -> Option<Self> {
        ((1..=12).contains(&m) && d >= 1 && d <= days_in_month(y, m)).then(|| Self(days_from_civil(y, m, d)))
    }

    pub fn add_days(self, days: i64) -> Self {
        Self(self.0 + days)
    }

    /// Same day `months` later, clamped to the end of shorter months (Jan 31 + 1 → Feb 28).
    pub fn add_months(self, months: i64) -> Self {
        let (y, m, d) = self.civil();
        let total = y * 12 + (m - 1) + months;
        let (y, m) = (total.div_euclid(12), total.rem_euclid(12) + 1);
        Self(days_from_civil(y, m, d.min(days_in_month(y, m))))
    }

    /// Days from `other` to `self`.
    pub fn days_since(self, other: Date) -> i64 {
        self.0 - other.0
    }

    /// The Monday starting this date's week.
    pub fn monday(self) -> Self {
        self.add_days(-i64::from(self.weekday()))
    }

    pub fn year(self) -> i64 {
        self.civil().0
    }

    /// 1 = January.
    pub fn month(self) -> i64 {
        self.civil().1
    }

    pub fn day(self) -> i64 {
        self.civil().2
    }

    /// 0 = Monday … 6 = Sunday.
    pub fn weekday(self) -> u32 {
        // 1970-01-01 was a Thursday.
        (self.0 + 3).rem_euclid(7) as u32
    }

    fn civil(self) -> (i64, i64, i64) {
        // Howard Hinnant's civil_from_days.
        let z = self.0 + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z.rem_euclid(146_097);
        let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let d = doy - (153 * mp + 2) / 5 + 1;
        let m = if mp < 10 { mp + 3 } else { mp - 9 };
        let y = yoe + era * 400 + i64::from(m <= 2);
        (y, m, d)
    }
}

impl fmt::Display for Date {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (y, m, d) = self.civil();
        write!(f, "{y:04}-{m:02}-{d:02}")
    }
}

fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

pub(crate) fn days_in_month(y: i64, m: i64) -> i64 {
    match m {
        2 if y % 4 == 0 && (y % 100 != 0 || y % 400 == 0) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_weekdays() {
        for s in ["1970-01-01", "2000-02-29", "2026-11-02", "2100-12-31"] {
            assert_eq!(Date::parse(s).unwrap().to_string(), s);
        }
        assert_eq!(Date::parse("2026-11-02").unwrap().weekday(), 0); // Monday
        assert_eq!(Date::parse("1970-01-01").unwrap().weekday(), 3); // Thursday
        let d = Date::parse("2024-02-28").unwrap();
        assert_eq!(d.add_days(1).to_string(), "2024-02-29");
        assert_eq!(d.add_days(2).to_string(), "2024-03-01");
        let jan31 = Date::parse("2026-01-31").unwrap();
        assert_eq!(jan31.add_months(1).to_string(), "2026-02-28");
        assert_eq!(jan31.add_months(-2).to_string(), "2025-11-30");
        assert_eq!(jan31.add_months(12).to_string(), "2027-01-31");
        assert_eq!(
            Date::parse("2026-11-08").unwrap().monday().to_string(),
            "2026-11-02"
        );
    }

    #[test]
    fn rejects_bad_dates() {
        for s in ["2026-13-01", "2026-02-29", "2026-1-01", "today", ""] {
            assert!(Date::parse(s).is_err(), "{s}");
        }
    }
}
