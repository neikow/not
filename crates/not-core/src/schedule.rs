//! When a task is due and how it repeats.
//!
//! Dates and times are floating: a task due "Friday 9:00" is due at 9:00 wherever the user is, like
//! a paper planner. Repeat rules are stored as a small subset of iCalendar RRULE
//! (`FREQ=WEEKLY;INTERVAL=2;BYDAY=MO,TH`), which is also what TickTick exports.

use std::fmt::Write;

use crate::date::Date;
use crate::{Error, Result};

/// Due date, optional time of day, optional repeat rule.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Schedule {
    pub due: Option<Date>,
    /// Minutes after midnight.
    pub time: Option<u32>,
    pub repeat: Option<Repeat>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Freq {
    Daily,
    Weekly,
    Monthly,
    Yearly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Repeat {
    pub freq: Freq,
    /// Every `interval` days, weeks, …; at least 1.
    pub interval: u32,
    /// Weekly rules only: bit 0 = Monday … bit 6 = Sunday. 0 means the due date's weekday.
    pub weekdays: u8,
}

pub const WEEKDAYS: u8 = 0b001_1111;
pub const WEEKEND: u8 = 0b110_0000;

const DAY_CODES: [&str; 7] = ["MO", "TU", "WE", "TH", "FR", "SA", "SU"];
const DAY_NAMES: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
const MONTH_NAMES: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

impl Repeat {
    pub fn new(freq: Freq, interval: u32) -> Self {
        Self {
            freq,
            interval: interval.max(1),
            weekdays: 0,
        }
    }

    pub fn weekly_on(weekdays: u8) -> Self {
        Self {
            freq: Freq::Weekly,
            interval: 1,
            weekdays,
        }
    }

    /// Parses the RRULE subset this app writes (an `RRULE:` prefix is accepted). Unknown parts are
    /// ignored rather than rejected, so rules from other apps degrade to their frequency.
    pub fn parse_rrule(rule: &str) -> Result<Self> {
        let bad = || Error::Invalid(format!("unsupported repeat rule {rule:?}"));
        let rule = rule.strip_prefix("RRULE:").unwrap_or(rule);
        let (mut freq, mut interval, mut weekdays) = (None, 1, 0);
        for part in rule.split(';').filter(|p| !p.is_empty()) {
            let (key, value) = part.split_once('=').ok_or_else(bad)?;
            match key {
                "FREQ" => {
                    freq = Some(match value {
                        "DAILY" => Freq::Daily,
                        "WEEKLY" => Freq::Weekly,
                        "MONTHLY" => Freq::Monthly,
                        "YEARLY" => Freq::Yearly,
                        _ => return Err(bad()),
                    })
                }
                "INTERVAL" => interval = value.parse::<u32>().map_err(|_| bad())?.max(1),
                "BYDAY" => {
                    for day in value.split(',') {
                        // Ignore ordinals like "2MO" (monthly rules) and keep the weekday.
                        let code = &day[day.len().saturating_sub(2)..];
                        let i = DAY_CODES.iter().position(|c| *c == code).ok_or_else(bad)?;
                        weekdays |= 1 << i;
                    }
                }
                _ => {}
            }
        }
        let freq = freq.ok_or_else(bad)?;
        Ok(Self {
            freq,
            interval,
            weekdays: if freq == Freq::Weekly { weekdays } else { 0 },
        })
    }

    pub fn to_rrule(&self) -> String {
        let freq = match self.freq {
            Freq::Daily => "DAILY",
            Freq::Weekly => "WEEKLY",
            Freq::Monthly => "MONTHLY",
            Freq::Yearly => "YEARLY",
        };
        let mut rule = format!("FREQ={freq}");
        if self.interval > 1 {
            let _ = write!(rule, ";INTERVAL={}", self.interval);
        }
        if self.weekdays != 0 {
            let days: Vec<_> = days_of(self.weekdays).map(|d| DAY_CODES[d]).collect();
            let _ = write!(rule, ";BYDAY={}", days.join(","));
        }
        rule
    }

    /// The first occurrence after `date`, an occurrence itself (the due date being moved on).
    pub fn next_after(&self, date: Date) -> Date {
        let n = i64::from(self.interval);
        match self.freq {
            Freq::Daily => date.add_days(n),
            Freq::Weekly if self.weekdays == 0 => date.add_days(7 * n),
            Freq::Weekly => {
                // Weeks are counted from the due date's week; only every `interval`-th one counts.
                let anchor = date.monday();
                (1..=7 * n + 7)
                    .map(|i| date.add_days(i))
                    .find(|d| self.on_weekday(*d) && (d.monday().days_since(anchor) / 7) % n == 0)
                    .unwrap_or_else(|| date.add_days(7 * n))
            }
            Freq::Monthly => date.add_months(n),
            Freq::Yearly => date.add_months(12 * n),
        }
    }

    /// The first occurrence on or after `date`, for a rule given without a due date.
    pub fn first_from(&self, date: Date) -> Date {
        if self.freq == Freq::Weekly && self.weekdays != 0 {
            (0..7)
                .map(|i| date.add_days(i))
                .find(|d| self.on_weekday(*d))
                .unwrap_or(date)
        } else {
            date
        }
    }

    fn on_weekday(&self, date: Date) -> bool {
        self.weekdays & (1 << date.weekday()) != 0
    }

    /// "Daily", "Every 2 weeks on Mon, Thu", "Weekdays", …
    pub fn label(&self) -> String {
        let n = self.interval;
        let unit = |one: &str, many: &str| {
            if n == 1 {
                one.to_owned()
            } else {
                format!("Every {n} {many}")
            }
        };
        match self.freq {
            Freq::Daily => unit("Daily", "days"),
            Freq::Weekly if self.weekdays == WEEKDAYS && n == 1 => "Weekdays".into(),
            Freq::Weekly if self.weekdays == WEEKEND && n == 1 => "Weekends".into(),
            Freq::Weekly if self.weekdays != 0 => {
                let days: Vec<_> = days_of(self.weekdays).map(|d| DAY_NAMES[d]).collect();
                if n == 1 {
                    format!("Every {}", days.join(", "))
                } else {
                    format!("Every {n} weeks on {}", days.join(", "))
                }
            }
            Freq::Weekly => unit("Weekly", "weeks"),
            Freq::Monthly => unit("Monthly", "months"),
            Freq::Yearly => unit("Yearly", "years"),
        }
    }
}

fn days_of(mask: u8) -> impl Iterator<Item = usize> {
    (0..7).filter(move |d| mask & (1 << d) != 0)
}

/// "Today", "Tomorrow 9:00", "Fri", "12 Nov", "3 Jan 2027", relative to `today`.
pub fn due_label(due: Date, time: Option<u32>, today: Date) -> String {
    let mut label = match due.days_since(today) {
        0 => "Today".to_owned(),
        1 => "Tomorrow".to_owned(),
        -1 => "Yesterday".to_owned(),
        2..=6 => DAY_NAMES[due.weekday() as usize].to_owned(),
        _ if due.year() == today.year() => format!("{} {}", due.day(), MONTH_NAMES[due.month() as usize - 1]),
        _ => format!(
            "{} {} {}",
            due.day(),
            MONTH_NAMES[due.month() as usize - 1],
            due.year()
        ),
    };
    if let Some(t) = time {
        let _ = write!(label, " {}:{:02}", t / 60, t % 60);
    }
    label
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> Date {
        Date::parse(s).unwrap()
    }

    #[test]
    fn rrule_round_trips() {
        for rule in [
            "FREQ=DAILY",
            "FREQ=WEEKLY;INTERVAL=2;BYDAY=MO,TH",
            "FREQ=MONTHLY",
            "FREQ=YEARLY;INTERVAL=3",
        ] {
            assert_eq!(Repeat::parse_rrule(rule).unwrap().to_rrule(), rule);
        }
        assert_eq!(
            Repeat::parse_rrule("RRULE:FREQ=WEEKLY;WKST=SU;BYDAY=FR")
                .unwrap()
                .to_rrule(),
            "FREQ=WEEKLY;BYDAY=FR"
        );
        assert!(Repeat::parse_rrule("FREQ=HOURLY").is_err());
        assert!(Repeat::parse_rrule("").is_err());
    }

    #[test]
    fn next_occurrences() {
        // 2026-11-09 is a Monday.
        let mon = d("2026-11-09");
        assert_eq!(Repeat::new(Freq::Daily, 3).next_after(mon), d("2026-11-12"));
        assert_eq!(Repeat::new(Freq::Weekly, 1).next_after(mon), d("2026-11-16"));
        let mon_thu = Repeat::parse_rrule("FREQ=WEEKLY;BYDAY=MO,TH").unwrap();
        assert_eq!(mon_thu.next_after(mon), d("2026-11-12"));
        assert_eq!(mon_thu.next_after(d("2026-11-12")), d("2026-11-16"));
        let fortnightly = Repeat::parse_rrule("FREQ=WEEKLY;INTERVAL=2;BYDAY=MO,TH").unwrap();
        assert_eq!(fortnightly.next_after(mon), d("2026-11-12"));
        assert_eq!(fortnightly.next_after(d("2026-11-12")), d("2026-11-23"));
        assert_eq!(
            Repeat::new(Freq::Monthly, 1).next_after(d("2026-01-31")),
            d("2026-02-28")
        );
        assert_eq!(
            Repeat::new(Freq::Yearly, 1).next_after(d("2024-02-29")),
            d("2025-02-28")
        );
        let weekdays = Repeat::weekly_on(WEEKDAYS);
        assert_eq!(weekdays.next_after(d("2026-11-13")), d("2026-11-16"));
        assert_eq!(weekdays.first_from(d("2026-11-08")), d("2026-11-09"));
    }

    #[test]
    fn labels() {
        let today = d("2026-11-08"); // Sunday
        assert_eq!(due_label(today, None, today), "Today");
        assert_eq!(due_label(d("2026-11-09"), Some(9 * 60), today), "Tomorrow 9:00");
        assert_eq!(due_label(d("2026-11-13"), Some(18 * 60 + 30), today), "Fri 18:30");
        assert_eq!(due_label(d("2026-12-24"), None, today), "24 Dec");
        assert_eq!(due_label(d("2027-01-03"), None, today), "3 Jan 2027");
        assert_eq!(due_label(d("2026-11-01"), None, today), "1 Nov");
        assert_eq!(Repeat::new(Freq::Daily, 1).label(), "Daily");
        assert_eq!(Repeat::new(Freq::Weekly, 2).label(), "Every 2 weeks");
        assert_eq!(Repeat::weekly_on(WEEKDAYS).label(), "Weekdays");
        assert_eq!(
            Repeat::parse_rrule("FREQ=WEEKLY;BYDAY=MO,TH").unwrap().label(),
            "Every Mon, Thu"
        );
        assert_eq!(
            Repeat::parse_rrule("FREQ=WEEKLY;INTERVAL=2;BYDAY=FR")
                .unwrap()
                .label(),
            "Every 2 weeks on Fri"
        );
    }
}
