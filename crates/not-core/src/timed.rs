//! Timed habits: a habit that is on or off, with a goal in minutes per day — at least N minutes
//! on ("wear it 22 h"), or at most N minutes off ("out for 2 h at most").
//!
//! Each habit's meta holds a `switches` map, created with the habit (a map created concurrently
//! on two devices would lose one side): switch id → `at` in Unix millis, negated for a switch to
//! off. Every switch has its own key, so switches made offline on several devices all survive and
//! are read in time order. Before the first switch the habit isn't tracked yet: that time counts
//! neither on nor off.
//!
//! All edits go through one command, [`Store::set_habit_state`]: "it was on (or off) from A until
//! B, or until now". A tap is "from now", backdating is "from 15 minutes ago", and fixing the
//! timeline is a range.

use std::sync::atomic::{AtomicU32, Ordering};

use loro::{Container, LoroMap, TreeID, ValueOrContainer};

use crate::date::Date;
use crate::store::{Store, now_millis};
use crate::{Error, Result};

pub(crate) const SWITCHES: &str = "switches";
pub(crate) const MINUTE: i64 = 60_000;
pub(crate) const DAY: i64 = 24 * 60 * MINUTE;

/// How a timed habit is judged and worded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimedRule {
    /// The goal caps the time off ("at most 2 h off") instead of asking for time on.
    pub at_most_off: bool,
    /// It is normally on (something worn, a fast): the apps offer "Pause" and watch the time
    /// off. Normally off (reading, practice): "Start", and they watch the time on.
    pub usual_on: bool,
    /// Minutes away from the usual state before a reminder; 0 for none.
    pub nudge_minutes: u32,
}

impl TimedRule {
    /// The state the reminder and the live notification are about.
    pub fn unusual(&self) -> bool {
        !self.usual_on
    }
}

/// The current moment and the device's UTC offset, which places local days on the timeline.
/// A day is taken as 24 hours at the given offset, so on daylight-saving days an hour lands on
/// the neighbouring day.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Clock {
    pub now_ms: i64,
    /// Minutes east of UTC (UTC+1 = 60).
    pub offset_minutes: i32,
}

impl Clock {
    pub fn utc_now() -> Self {
        Self {
            now_ms: now_millis(),
            offset_minutes: 0,
        }
    }

    /// Local midnight starting `date`, in Unix millis.
    pub fn day_start(&self, date: Date) -> i64 {
        let epoch = Date::from_ymd(1970, 1, 1).expect("valid");
        date.days_since(epoch) * DAY - i64::from(self.offset_minutes) * MINUTE
    }
}

/// A timed habit's state on one day, up to now when that day is today.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimedState {
    /// On at the end of the day (or now); false before tracking starts.
    pub on: bool,
    /// When the current stretch started; `None` before the first switch.
    pub since: Option<i64>,
    /// Time on and off in that day, in millis; untracked time is neither.
    pub on_ms: i64,
    pub off_ms: i64,
}

/// One stretch of a day's timeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Segment {
    pub start: i64,
    pub end: i64,
    pub on: bool,
}

/// A switch, as read back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Switch {
    pub key: String,
    pub at: i64,
    pub on: bool,
}

/// Encodes a switch for the map: positive for on, negative for off.
fn encode(at: i64, on: bool) -> i64 {
    if on { at } else { -at }
}

pub(crate) fn decode(key: String, value: i64) -> Switch {
    Switch {
        key,
        at: value.abs(),
        on: value > 0,
    }
}

/// In time order; ties (switches at the same millisecond) by key, so every replica agrees.
pub(crate) fn sort(switches: &mut [Switch]) {
    switches.sort_by(|a, b| a.at.cmp(&b.at).then_with(|| a.key.cmp(&b.key)));
}

/// The state at `t` (switches at `t` included); `None` before tracking starts.
pub(crate) fn state_at(switches: &[Switch], t: i64) -> Option<bool> {
    switches.iter().take_while(|s| s.at <= t).last().map(|s| s.on)
}

/// The tracked stretches within `from..to`, neighbours with the same state merged.
pub(crate) fn segments(switches: &[Switch], from: i64, to: i64) -> Vec<Segment> {
    let mut out: Vec<Segment> = Vec::new();
    if to <= from {
        return out;
    }
    let mut push = |start: i64, end: i64, on: bool| {
        if end <= start {
            return;
        }
        match out.last_mut() {
            Some(last) if last.on == on && last.end == start => last.end = end,
            _ => out.push(Segment { start, end, on }),
        }
    };
    let mut current = state_at(switches, from);
    let mut at = from;
    for s in switches.iter().filter(|s| s.at > from && s.at < to) {
        if let Some(on) = current {
            push(at, s.at, on);
        }
        current = Some(s.on);
        at = s.at;
    }
    if let Some(on) = current {
        push(at, to, on);
    }
    out
}

/// The state of the day `start..start + DAY` as of `now`.
pub(crate) fn day_state(switches: &[Switch], start: i64, now: i64) -> TimedState {
    let end = (start + DAY).min(now);
    let (mut on_ms, mut off_ms) = (0, 0);
    for s in segments(switches, start, end) {
        if s.on {
            on_ms += s.end - s.start;
        } else {
            off_ms += s.end - s.start;
        }
    }
    // The current stretch began at the last switch that actually changed the state.
    let (mut on, mut since) = (None, None);
    for s in switches.iter().take_while(|s| s.at <= end) {
        if on != Some(s.on) {
            on = Some(s.on);
            since = Some(s.at);
        }
    }
    TimedState {
        on: on.unwrap_or(false),
        since,
        on_ms,
        off_ms,
    }
}

/// Minutes that count toward the goal that day: time on, or time off for "at most" habits.
pub(crate) fn counted_minutes(rule: &TimedRule, state: &TimedState) -> u32 {
    let ms = if rule.at_most_off {
        state.off_ms
    } else {
        state.on_ms
    };
    (ms / MINUTE) as u32
}

/// The day's goal is met: enough time on, or no more time off than allowed.
pub(crate) fn met(rule: &TimedRule, goal_minutes: u32, state: &TimedState) -> bool {
    if rule.at_most_off {
        state.off_ms <= i64::from(goal_minutes) * MINUTE
    } else {
        state.on_ms >= i64::from(goal_minutes) * MINUTE
    }
}

/// Unique per replica: the session's peer id, the time, and a counter.
fn switch_key(peer: u64) -> String {
    static NEXT: AtomicU32 = AtomicU32::new(0);
    format!(
        "{peer:x}-{:x}-{:x}",
        now_millis(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )
}

pub(crate) fn switches_of(meta: &LoroMap) -> Vec<Switch> {
    let Some(ValueOrContainer::Container(Container::Map(map))) = meta.get(SWITCHES) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    map.for_each(|key, value| {
        if let ValueOrContainer::Value(loro::LoroValue::I64(v)) = value {
            out.push(decode(key.to_string(), v));
        }
    });
    sort(&mut out);
    out
}

impl Store {
    /// Records that the timed habit `id` was on (or off) from `from` until `to`, or from `from`
    /// on when `to` is `None` (a tap: "since now", or backdated "since 15 minutes ago").
    /// Switches inside the range are replaced; what came after `to` is kept. Times are Unix
    /// millis and can't be in the future. One undo step.
    pub fn set_habit_state(&self, id: &str, on: bool, from: i64, to: Option<i64>, now: i64) -> Result<()> {
        // A minute of slack for clocks that disagree a little.
        if from > now + MINUTE {
            return Err(Error::Invalid("that time hasn't come yet".into()));
        }
        let to = to.filter(|&t| t < now);
        if to.is_some_and(|t| t <= from) {
            return Err(Error::Invalid("the end must come after the start".into()));
        }
        let node = self.timed_node(id)?;
        let peer = self.doc.peer_id();
        self.transact(|| {
            let meta = self.habits.get_meta(node)?;
            let Some(ValueOrContainer::Container(Container::Map(map))) = meta.get(SWITCHES) else {
                return Err(Error::Invalid("not a timed habit".into()));
            };
            let switches = switches_of(&meta);
            let after = to.map(|t| state_at(&switches, t));
            let inside = |at: i64| at >= from && to.is_none_or(|t| at < t);
            for s in switches.iter().filter(|s| inside(s.at)) {
                map.delete(&s.key)?;
            }
            let kept: Vec<Switch> = switches.into_iter().filter(|s| !inside(s.at)).collect();
            if state_at(&kept, from - 1) != Some(on) {
                map.insert(&switch_key(peer), encode(from, on))?;
            }
            if let Some(t) = to {
                let at_t: Vec<&Switch> = kept.iter().filter(|s| s.at == t).collect();
                if at_t.is_empty() {
                    // What was there after the range carries on; before tracking started, the
                    // other state.
                    let after = after.flatten().unwrap_or(!on);
                    if after != on {
                        map.insert(&switch_key(peer), encode(t, after))?;
                    }
                } else if at_t.last().is_some_and(|s| s.on == on) {
                    // The range now runs into a switch to the same state: it changes nothing.
                    for s in at_t {
                        map.delete(&s.key)?;
                    }
                }
            }
            Ok(())
        })
    }

    /// Flips a timed habit now.
    pub fn switch_habit_now(&self, id: &str) -> Result<()> {
        let node = self.timed_node(id)?;
        let now = now_millis();
        let on = state_at(&switches_of(&self.habits.get_meta(node)?), now).unwrap_or(false);
        self.set_habit_state(id, !on, now, None, now)
    }

    /// The tracked stretches of `date`, up to now when it's today.
    pub fn habit_timeline(&self, id: &str, date: &str, clock: Clock) -> Result<Vec<Segment>> {
        let start = clock.day_start(Date::parse(date)?);
        let switches = {
            let inner = self.lock();
            crate::habits::load_switches(&inner.db, id)?
        };
        Ok(segments(&switches, start, (start + DAY).min(clock.now_ms)))
    }

    /// Every switch of a timed habit in time order, as (Unix millis, on).
    pub fn habit_switches(&self, id: &str) -> Result<Vec<(i64, bool)>> {
        let inner = self.lock();
        Ok(crate::habits::load_switches(&inner.db, id)?
            .into_iter()
            .map(|s| (s.at, s.on))
            .collect())
    }

    pub(crate) fn timed_node(&self, id: &str) -> Result<TreeID> {
        let node = self.habit_node(id)?;
        match self.habits.get_meta(node)?.get(SWITCHES) {
            Some(ValueOrContainer::Container(Container::Map(_))) => Ok(node),
            _ => Err(Error::Invalid("not a timed habit".into())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sw(at: i64, on: bool) -> Switch {
        Switch {
            key: format!("{at:08}"),
            at,
            on,
        }
    }

    #[test]
    fn segments_merge_repeats_and_skip_untracked_time() {
        let log = [sw(10, true), sw(20, true), sw(30, false), sw(50, true)];
        assert_eq!(
            segments(&log, 0, 60),
            [
                Segment {
                    start: 10,
                    end: 30,
                    on: true
                },
                Segment {
                    start: 30,
                    end: 50,
                    on: false
                },
                Segment {
                    start: 50,
                    end: 60,
                    on: true
                },
            ]
        );
        assert_eq!(
            segments(&log, 35, 40),
            [Segment {
                start: 35,
                end: 40,
                on: false
            }]
        );
        assert!(segments(&log, 0, 5).is_empty());
    }

    #[test]
    fn a_day_counts_from_its_midnight_to_now() {
        let log = [sw(10, true), sw(30, false), sw(50, true)];
        let s = day_state(&log, 0, 60);
        assert_eq!((s.on, s.since, s.on_ms, s.off_ms), (true, Some(50), 30, 20));
        // A repeated switch doesn't restart the stretch.
        let log = [sw(10, true), sw(30, true)];
        assert_eq!(day_state(&log, 0, 60).since, Some(10));
        assert_eq!(day_state(&[], 0, 60).since, None);
    }

    #[test]
    fn days_start_at_local_midnight() {
        let paris = Clock {
            now_ms: 0,
            offset_minutes: 60,
        };
        let day = Date::parse("1970-01-02").unwrap();
        assert_eq!(paris.day_start(day), DAY - 60 * MINUTE);
    }
}
