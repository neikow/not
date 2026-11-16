//! When a task starts, when it must be done, and how it repeats (ADR 0014).
//!
//! Dates and times are floating: a task starting "Friday 9:00" starts at 9:00 wherever the user is,
//! like a paper planner. Repeat rules are stored as a small subset of iCalendar RRULE
//! (`FREQ=WEEKLY;INTERVAL=2;BYDAY=MO,TH`, `FREQ=MONTHLY;BYDAY=-1FR`), which is also what TickTick
//! exports, plus `X-FROM=COMPLETION` for rules counted from the day the task was done.

use std::fmt::Write;

use crate::date::Date;
use crate::{Error, Result};

/// Start date (when the user plans to do it; hidden before), optional time of day, deadline,
/// Someday, repeat rule.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Schedule {
    pub start: Option<Date>,
    /// Minutes after midnight; only with a start date.
    pub time: Option<u32>,
    /// The day it must be done by.
    pub deadline: Option<Date>,
    /// Parked for later; never together with a start date.
    pub someday: bool,
    /// Anchored on the start date, else the deadline.
    pub repeat: Option<Repeat>,
}

impl Schedule {
    /// The date a repeat moves on from.
    pub fn anchor(&self) -> Option<Date> {
        self.start.or(self.deadline)
    }
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
    /// Weekly rules: bit 0 = Monday … bit 6 = Sunday; 0 means the start date's weekday. Monthly
    /// rules with [`Repeat::nth`]: the one weekday.
    pub weekdays: u8,
    /// Monthly rules: which weekday of the month — 1…4, or -1 for the last. 0 = same day of month.
    pub nth: i8,
    /// Count from the day it was completed rather than from the date it was planned for.
    pub from_completion: bool,
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
            nth: 0,
            from_completion: false,
        }
    }

    pub fn weekly_on(weekdays: u8) -> Self {
        Self {
            weekdays,
            ..Self::new(Freq::Weekly, 1)
        }
    }

    /// The `nth` (1…4, -1 = last) `weekday` (0 = Monday) of every month.
    pub fn monthly_on(nth: i8, weekday: u32) -> Self {
        Self {
            weekdays: 1 << weekday,
            nth,
            ..Self::new(Freq::Monthly, 1)
        }
    }

    /// Parses the RRULE subset this app writes (an `RRULE:` prefix is accepted). Unknown parts are
    /// ignored rather than rejected, so rules from other apps degrade to their frequency.
    pub fn parse_rrule(rule: &str) -> Result<Self> {
        let bad = || Error::Invalid(format!("unsupported repeat rule {rule:?}"));
        let rule = rule.strip_prefix("RRULE:").unwrap_or(rule);
        let (mut freq, mut interval, mut weekdays, mut nth, mut setpos) = (None, 1, 0u8, 0i8, 0i8);
        let mut from_completion = false;
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
                        // "2MO", "-1FR": an ordinal for monthly rules.
                        let (ordinal, code) = day.split_at(day.len().saturating_sub(2));
                        let i = DAY_CODES.iter().position(|c| *c == code).ok_or_else(bad)?;
                        weekdays |= 1 << i;
                        if !ordinal.is_empty() {
                            nth = ordinal.parse().map_err(|_| bad())?;
                        }
                    }
                }
                "BYSETPOS" => setpos = value.parse().map_err(|_| bad())?,
                "X-FROM" => from_completion = value == "COMPLETION",
                _ => {}
            }
        }
        let freq = freq.ok_or_else(bad)?;
        if nth == 0 {
            nth = setpos;
        }
        // One weekday a month is all a monthly rule can say here.
        let nth = if freq == Freq::Monthly && weekdays.count_ones() == 1 && matches!(nth, -1 | 1..=4) {
            nth
        } else {
            0
        };
        Ok(Self {
            freq,
            interval,
            weekdays: match freq {
                Freq::Weekly => weekdays,
                Freq::Monthly if nth != 0 => weekdays,
                _ => 0,
            },
            nth,
            from_completion,
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
        if self.nth != 0 {
            let day = days_of(self.weekdays).next().unwrap_or(0);
            let _ = write!(rule, ";BYDAY={}{}", self.nth, DAY_CODES[day]);
        } else if self.weekdays != 0 {
            let days: Vec<_> = days_of(self.weekdays).map(|d| DAY_CODES[d]).collect();
            let _ = write!(rule, ";BYDAY={}", days.join(","));
        }
        if self.from_completion {
            rule.push_str(";X-FROM=COMPLETION");
        }
        rule
    }

    /// Where a task completed on `done` goes next, its repeat anchored on `anchor`: the first
    /// occurrence after `done` — counted from `done` itself for rules [`Repeat::from_completion`].
    pub fn next_for_completion(&self, anchor: Date, done: Date) -> Date {
        if self.from_completion {
            return self.next_after(done.max(anchor));
        }
        let mut next = self.next_after(anchor);
        while next <= done {
            next = self.next_after(next);
        }
        next
    }

    /// The first occurrence after `date`, an occurrence itself (the start date being moved on).
    pub fn next_after(&self, date: Date) -> Date {
        let n = i64::from(self.interval);
        match self.freq {
            Freq::Monthly if self.nth != 0 => {
                let day = days_of(self.weekdays).next().unwrap_or(0) as u32;
                let this = nth_weekday(date.year(), date.month(), self.nth, day);
                // Not an occurrence itself (set by hand): the one this month if still ahead.
                if this > date && !self.is_occurrence(date) {
                    return this;
                }
                let month = date.with_first_day().add_months(n);
                nth_weekday(month.year(), month.month(), self.nth, day)
            }
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

    fn is_occurrence(&self, date: Date) -> bool {
        let day = days_of(self.weekdays).next().unwrap_or(0) as u32;
        nth_weekday(date.year(), date.month(), self.nth, day) == date
    }

    /// The first occurrence on or after `date`, for a rule given without a start date.
    pub fn first_from(&self, date: Date) -> Date {
        if self.freq == Freq::Monthly && self.nth != 0 {
            if self.is_occurrence(date) {
                return date;
            }
            let day = days_of(self.weekdays).next().unwrap_or(0) as u32;
            let this = nth_weekday(date.year(), date.month(), self.nth, day);
            if this >= date {
                return this;
            }
            let next = date.with_first_day().add_months(1);
            return nth_weekday(next.year(), next.month(), self.nth, day);
        }
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

    /// "Daily", "Every 2 weeks on Mon, Thu", "Weekdays", "Monthly on the last Fri", "Every 3 days
    /// after done", …
    pub fn label(&self) -> String {
        let label = self.base_label();
        if self.from_completion {
            format!("{label} after done")
        } else {
            label
        }
    }

    fn base_label(&self) -> String {
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
            Freq::Monthly if self.nth != 0 => {
                let day = DAY_NAMES[days_of(self.weekdays).next().unwrap_or(0)];
                let which = match self.nth {
                    1 => "1st",
                    2 => "2nd",
                    3 => "3rd",
                    4 => "4th",
                    _ => "last",
                };
                if n == 1 {
                    format!("Monthly on the {which} {day}")
                } else {
                    format!("Every {n} months on the {which} {day}")
                }
            }
            Freq::Monthly => unit("Monthly", "months"),
            Freq::Yearly => unit("Yearly", "years"),
        }
    }
}

fn days_of(mask: u8) -> impl Iterator<Item = usize> {
    (0..7).filter(move |d| mask & (1 << d) != 0)
}

/// The `nth` (1…4, -1 = last) `weekday` of month `m` of year `y`.
fn nth_weekday(y: i64, m: i64, nth: i8, weekday: u32) -> Date {
    let first = Date::from_ymd(y, m, 1).expect("valid month");
    if nth < 0 {
        let last = first.add_months(1).add_days(-1);
        return last.add_days(-i64::from((last.weekday() + 7 - weekday) % 7));
    }
    let first_match = first.add_days(i64::from((weekday + 7 - first.weekday()) % 7));
    first_match.add_days(7 * (i64::from(nth) - 1))
}

trait FirstDay {
    fn with_first_day(self) -> Self;
}

impl FirstDay for Date {
    fn with_first_day(self) -> Self {
        Date::from_ymd(self.year(), self.month(), 1).unwrap_or(self)
    }
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
        for rule in [
            "FREQ=MONTHLY;BYDAY=-1FR",
            "FREQ=MONTHLY;INTERVAL=2;BYDAY=2TU",
            "FREQ=DAILY;INTERVAL=3;X-FROM=COMPLETION",
        ] {
            assert_eq!(Repeat::parse_rrule(rule).unwrap().to_rrule(), rule);
        }
        assert_eq!(
            Repeat::parse_rrule("FREQ=MONTHLY;BYDAY=TH;BYSETPOS=3")
                .unwrap()
                .to_rrule(),
            "FREQ=MONTHLY;BYDAY=3TH"
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
    fn nth_weekday_of_month() {
        let last_fri = Repeat::monthly_on(-1, 4);
        assert_eq!(last_fri.first_from(d("2026-11-08")), d("2026-11-27"));
        assert_eq!(last_fri.next_after(d("2026-11-27")), d("2026-12-25"));
        assert_eq!(last_fri.next_after(d("2026-12-25")), d("2027-01-29"));
        let second_tue = Repeat::monthly_on(2, 1);
        assert_eq!(second_tue.first_from(d("2026-11-08")), d("2026-11-10"));
        assert_eq!(second_tue.first_from(d("2026-11-11")), d("2026-12-08"));
        assert_eq!(second_tue.next_after(d("2026-11-10")), d("2026-12-08"));
        // A start date set by hand that isn't an occurrence moves to the next one.
        assert_eq!(second_tue.next_after(d("2026-11-02")), d("2026-11-10"));
        let first_mon = Repeat::monthly_on(1, 0);
        assert_eq!(first_mon.next_after(d("2026-11-02")), d("2026-12-07"));
        assert_eq!(
            Repeat {
                interval: 3,
                ..last_fri
            }
            .next_after(d("2026-11-27")),
            d("2027-02-26")
        );
    }

    #[test]
    fn from_completion() {
        let every3 = Repeat {
            from_completion: true,
            ..Repeat::new(Freq::Daily, 3)
        };
        // Planned for the 1st, done on the 9th: next is the 12th, not the 10th.
        assert_eq!(
            every3.next_for_completion(d("2026-11-01"), d("2026-11-09")),
            d("2026-11-12")
        );
        // Done early: counted from the planned day, never earlier.
        assert_eq!(
            every3.next_for_completion(d("2026-11-10"), d("2026-11-09")),
            d("2026-11-13")
        );
        let plain = Repeat::new(Freq::Daily, 3);
        assert_eq!(
            plain.next_for_completion(d("2026-11-01"), d("2026-11-09")),
            d("2026-11-10")
        );
        assert_eq!(
            plain.next_for_completion(d("2026-11-10"), d("2026-11-09")),
            d("2026-11-13")
        );
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
        assert_eq!(Repeat::monthly_on(-1, 4).label(), "Monthly on the last Fri");
        assert_eq!(Repeat::monthly_on(2, 1).label(), "Monthly on the 2nd Tue");
        let after = Repeat {
            from_completion: true,
            ..Repeat::new(Freq::Daily, 3)
        };
        assert_eq!(after.label(), "Every 3 days after done");
    }
}
