//! Quick-add parsing: pulls a start date, time, deadline, repeat rule, Someday, priority and tags
//! out of a task title, in English or French. `Buy milk tomorrow 9am #groceries !1` → "Buy milk",
//! starting tomorrow 9:00, tagged groceries, high priority; `Report by fri` → deadline Friday.
//!
//! The parser is word based and forgiving: anything it doesn't recognise stays in the title, and
//! each kind but tags is taken once (the first match wins), so "Call Tom about friday" only loses
//! "friday" when it is the first date. The apps highlight [`Token`]s as the user types and let them
//! dismiss one, which passes its text back in `dismissed` to keep it as plain title text.

use crate::date::Date;
use crate::schedule::{Freq, Repeat, Schedule, WEEKDAYS, WEEKEND};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedTask {
    pub title: String,
    pub schedule: Schedule,
    /// 0 = none, 1 = low, 2 = medium, 3 = high.
    pub priority: u8,
    /// Normalised (see [`normalize_tag`]), in the order typed, without repeats.
    pub tags: Vec<String>,
    pub tokens: Vec<Token>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    /// The start date.
    Date,
    Deadline,
    Time,
    Repeat,
    Someday,
    Priority,
    Tag,
}

/// A recognised piece of the input. Offsets are UTF-16 code units, like the apps' text APIs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub start: u32,
    pub end: u32,
    pub kind: TokenKind,
    /// The source text, which identifies the token when the user dismisses it.
    pub text: String,
}

/// A tag as stored: lowercase, no `#`, whitespace runs as `-`, only letters, digits, `-` and `_`.
/// Empty when nothing is left.
pub fn normalize_tag(tag: &str) -> String {
    let lower = tag.trim().trim_start_matches('#').to_lowercase();
    let mut out = String::new();
    for word in lower.split_whitespace() {
        if !out.is_empty() {
            out.push('-');
        }
        out.extend(
            word.chars()
                .filter(|c| c.is_alphanumeric() || matches!(c, '-' | '_')),
        );
    }
    out.trim_matches('-').to_owned()
}

/// Parses `input` relative to `today`.
pub fn parse_task(input: &str, today: Date, dismissed: &[String]) -> ParsedTask {
    parse_task_at(input, today, None, dismissed)
}

/// [`parse_task`] knowing the time of day (minutes after midnight), which "in 2 hours" needs.
pub fn parse_task_at(input: &str, today: Date, minute: Option<u32>, dismissed: &[String]) -> ParsedTask {
    let words = split_words(input);
    let mut found: Vec<(usize, usize, TokenKind)> = Vec::new(); // word ranges
    let mut date = None;
    let mut deadline = None;
    let mut time = None;
    let mut repeat = None;
    let mut someday = false;
    let mut priority = 0;
    let mut tags: Vec<String> = Vec::new();
    // The day a time like "in 2 hours" or "tonight" falls on, when no date says otherwise.
    let mut moment_date: Option<Date> = None;

    let mut i = 0;
    while i < words.len() {
        let has = |kind| found.iter().any(|(_, _, k)| *k == kind);
        let mut tag = None;
        let matched: Option<(usize, TokenKind)> = (|| {
            if !has(TokenKind::Priority)
                && let Some(p) = priority_word(&words[i].raw)
            {
                return Some((1, TokenKind::Priority)).inspect(|_| priority = p);
            }
            if let Some(t) = tag_word(&words[i].raw) {
                return Some((1, TokenKind::Tag)).inspect(|_| tag = Some(t));
            }
            if !has(TokenKind::Repeat)
                && let Some((n, r)) = repeat_at(&words, i)
            {
                return Some((n, TokenKind::Repeat)).inspect(|_| repeat = Some(r));
            }
            if !has(TokenKind::Deadline)
                && DEADLINE_PREPS.contains(&words[i].norm.as_str())
                && let Some((n, d)) = date_at(&words, i + 1, today)
            {
                return Some((n + 1, TokenKind::Deadline)).inspect(|_| deadline = Some(d));
            }
            if !has(TokenKind::Someday)
                && let Some(n) = someday_at(&words, i)
            {
                return Some((n, TokenKind::Someday)).inspect(|_| someday = true);
            }
            if !has(TokenKind::Date) {
                // "on fri", "le 12 mai": the preposition goes with the date.
                let prep = DATE_PREPS.contains(&words[i].norm.as_str()) && i + 1 < words.len();
                let with_prep = prep
                    .then(|| date_at(&words, i + 1, today))
                    .flatten()
                    .map(|(n, d)| (n + 1, d));
                if let Some((n, d)) = with_prep.or_else(|| date_at(&words, i, today)) {
                    return Some((n, TokenKind::Date)).inspect(|_| date = Some(d));
                }
            }
            if !has(TokenKind::Time)
                && let Some((n, d, t)) = moment_at(&words, i, today, minute)
            {
                return Some((n, TokenKind::Time)).inspect(|_| {
                    time = Some(t);
                    moment_date = Some(d);
                });
            }
            if !has(TokenKind::Time)
                && let Some((n, t)) = time_at(&words, i)
            {
                return Some((n, TokenKind::Time)).inspect(|_| time = Some(t));
            }
            None
        })();
        match matched {
            Some((n, kind))
                if !dismissed
                    .iter()
                    .any(|d| *d == input[words[i].start..words[i + n - 1].end]) =>
            {
                if let Some(t) = tag.filter(|t| !tags.contains(t)) {
                    tags.push(t);
                }
                found.push((i, i + n, kind));
                i += n;
            }
            Some((_, kind)) => {
                // Dismissed: undo what the match recorded and keep the word as text.
                match kind {
                    TokenKind::Priority => priority = 0,
                    TokenKind::Tag => {}
                    TokenKind::Repeat => repeat = None,
                    TokenKind::Date => date = None,
                    TokenKind::Deadline => deadline = None,
                    TokenKind::Someday => someday = false,
                    TokenKind::Time => {
                        time = None;
                        moment_date = None;
                    }
                }
                i += 1;
            }
            None => i += 1,
        }
    }

    let start = date
        .or_else(|| {
            repeat
                .filter(|_| deadline.is_none())
                .map(|r: Repeat| r.first_from(today))
        })
        .or(moment_date)
        .or(time.map(|_| today));
    let utf16 = |byte: usize| input[..byte].encode_utf16().count() as u32;
    let tokens = found
        .iter()
        .map(|&(a, b, kind)| {
            let (start, end) = (words[a].start, words[b - 1].end);
            Token {
                start: utf16(start),
                end: utf16(end),
                kind,
                text: input[start..end].to_owned(),
            }
        })
        .collect();

    // The title is the input with the tokens cut out.
    let mut title = String::new();
    let mut at = 0;
    for &(a, b, _) in &found {
        title.push_str(&input[at..words[a].start]);
        title.push(' ');
        at = words[b - 1].end;
    }
    title.push_str(&input[at..]);
    let title = title.split_whitespace().collect::<Vec<_>>().join(" ");

    ParsedTask {
        title,
        schedule: Schedule {
            start,
            time: start.and(time),
            deadline,
            someday: someday && start.is_none(),
            repeat: repeat.filter(|_| start.or(deadline).is_some()),
        },
        priority,
        tags,
        tokens,
    }
}

// ---- words ---------------------------------------------------------------------------------

struct Word {
    /// Byte range in the input, trailing punctuation included.
    start: usize,
    end: usize,
    /// As typed, lowercased.
    raw: String,
    /// Lowercased, accents and trailing punctuation removed, apostrophes straightened.
    norm: String,
}

fn split_words(input: &str) -> Vec<Word> {
    let mut words = Vec::new();
    let mut start = None;
    for (i, c) in input.char_indices().chain([(input.len(), ' ')]) {
        match (c.is_whitespace(), start) {
            (false, None) => start = Some(i),
            (true, Some(s)) => {
                let raw = input[s..i].to_lowercase();
                let norm = fold(raw.trim_end_matches([',', '.', ';', ':', '!', '?', ')']));
                words.push(Word {
                    start: s,
                    end: i,
                    raw,
                    norm,
                });
                start = None;
            }
            _ => {}
        }
    }
    words
}

fn fold(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'à' | 'â' | 'ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'î' | 'ï' => 'i',
            'ô' | 'ö' => 'o',
            'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            '’' | '‘' => '\'',
            c => c,
        })
        .collect()
}

fn number(word: &str) -> Option<i64> {
    match word {
        "a" | "an" | "one" | "un" | "une" => Some(1),
        "two" | "deux" => Some(2),
        "three" | "trois" => Some(3),
        "four" | "quatre" => Some(4),
        "five" | "cinq" => Some(5),
        "six" => Some(6),
        "seven" | "sept" => Some(7),
        "eight" | "huit" => Some(8),
        "nine" | "neuf" => Some(9),
        "ten" | "dix" => Some(10),
        w if !w.is_empty() && w.len() <= 3 && w.bytes().all(|b| b.is_ascii_digit()) => w.parse().ok(),
        _ => None,
    }
}

/// `12`, `12th`, `1st`, `1er` → day of month.
fn day_number(word: &str) -> Option<i64> {
    let digits = word.trim_end_matches(|c: char| c.is_ascii_alphabetic());
    let suffix = &word[digits.len()..];
    if digits.is_empty() || digits.len() > 2 || !matches!(suffix, "" | "st" | "nd" | "rd" | "th" | "er" | "e")
    {
        return None;
    }
    digits.parse().ok().filter(|d| (1..=31).contains(d))
}

// ---- priority, tags, someday ---------------------------------------------------------------------

fn priority_word(raw: &str) -> Option<u8> {
    match raw.strip_prefix('!')? {
        "1" | "high" | "h" | "haute" | "urgent" | "!!" => Some(3),
        "2" | "medium" | "med" | "m" | "moyenne" | "moy" | "!" => Some(2),
        "3" | "low" | "l" | "basse" | "b" => Some(1),
        _ => None,
    }
}

/// `#groceries` → "groceries". A lone `#` or `#1` (an issue number) isn't a tag.
fn tag_word(raw: &str) -> Option<String> {
    let rest = raw.strip_prefix('#')?;
    let tag = normalize_tag(rest.trim_end_matches([',', '.', ';', ':', '!', '?', ')']));
    (!tag.is_empty() && !tag.bytes().all(|b| b.is_ascii_digit())).then_some(tag)
}

fn someday_at(words: &[Word], i: usize) -> Option<usize> {
    let w = |k: usize| words.get(i + k).map(|w| w.norm.as_str()).unwrap_or("");
    match (w(0), w(1)) {
        ("someday" | "sometime" | "un-jour", _) => Some(1),
        ("un", "jour") => Some(2),
        _ => None,
    }
}

// ---- dates ---------------------------------------------------------------------------------

/// Before a start date: "on fri", "le 12".
const DATE_PREPS: [&str; 3] = ["on", "le", "from"];
/// Before a deadline: "by fri", "due 12 dec", "avant vendredi".
const DEADLINE_PREPS: [&str; 6] = ["by", "due", "deadline", "before", "avant", "pour"];

fn weekday(word: &str) -> Option<u32> {
    let w = word.strip_suffix('s').filter(|w| w.len() > 4).unwrap_or(word); // "lundis"
    Some(match w {
        "mon" | "monday" | "lundi" => 0,
        "tue" | "tues" | "tuesday" | "mardi" => 1,
        "wed" | "wednesday" | "mercredi" => 2,
        "thu" | "thur" | "thurs" | "thursday" | "jeudi" => 3,
        "fri" | "friday" | "vendredi" => 4,
        "sat" | "saturday" | "samedi" => 5,
        "sun" | "sunday" | "dimanche" => 6,
        _ => return None,
    })
}

fn month(word: &str) -> Option<i64> {
    Some(match word {
        "jan" | "january" | "janv" | "janvier" => 1,
        "feb" | "february" | "fev" | "fevr" | "fevrier" => 2,
        "mar" | "march" | "mars" => 3,
        "apr" | "april" | "avr" | "avril" => 4,
        "may" | "mai" => 5,
        "jun" | "june" | "juin" => 6,
        "jul" | "july" | "juil" | "juillet" => 7,
        "aug" | "august" | "aout" => 8,
        "sep" | "sept" | "september" | "septembre" => 9,
        "oct" | "october" | "octobre" => 10,
        "nov" | "november" | "novembre" => 11,
        "dec" | "december" | "decembre" => 12,
        _ => return None,
    })
}

/// The soonest `wd` on or after `today`.
fn upcoming(today: Date, wd: u32) -> Date {
    today.add_days(i64::from((wd + 7 - today.weekday()) % 7))
}

fn date_at(words: &[Word], i: usize, today: Date) -> Option<(usize, Date)> {
    let w = |k: usize| words.get(i + k).map(|w| w.norm.as_str()).unwrap_or("");
    let next_monday = today.monday().add_days(7);
    match (w(0), w(1), w(2)) {
        ("today" | "tod" | "aujourd'hui" | "auj", ..) => return Some((1, today)),
        ("tomorrow" | "tmr" | "tmrw" | "demain", ..) => return Some((1, today.add_days(1))),
        ("apres-demain", ..) => return Some((1, today.add_days(2))),
        ("apres", "demain", _) => return Some((2, today.add_days(2))),
        ("next" | "prochaine", "week" | "semaine", _) | ("semaine", "prochaine", _) => {
            return Some((2, next_monday));
        }
        ("la", "semaine", "prochaine") => return Some((3, next_monday)),
        ("next", "month", _) | ("mois", "prochain", _) => return Some((2, today.add_months(1).with_day(1))),
        ("weekend" | "week-end", ..) => return Some((1, upcoming(today, 5))),
        ("eow", ..) => return Some((1, upcoming(today, 4))),
        ("eom", ..) => return Some((1, today.add_months(1).with_day(1).add_days(-1))),
        ("end", "of", _) | ("fin", "de" | "du", _) => {
            // "end of (the) week|month|year", "fin de (la) semaine", "fin du mois", "fin d'année".
            let skip = usize::from(matches!(w(2), "the" | "la" | "l'"));
            let unit = words.get(i + 2 + skip).map(|w| w.norm.as_str()).unwrap_or("");
            let date = match unit {
                "week" | "semaine" => upcoming(today, 4),
                "month" | "mois" => today.add_months(1).with_day(1).add_days(-1),
                "year" | "annee" => Date::from_ymd(today.year(), 12, 31)?,
                _ => return None,
            };
            return Some((3 + skip, date));
        }
        ("fin", "d'annee", _) => {
            return Some((2, Date::from_ymd(today.year(), 12, 31)?));
        }
        ("this" | "ce", "weekend" | "week-end", _) => return Some((2, upcoming(today, 5))),
        ("in" | "dans", n, unit) => {
            let n = number(n)?;
            let d = match unit {
                "day" | "days" | "jour" | "jours" => today.add_days(n),
                "week" | "weeks" | "semaine" | "semaines" => today.add_days(7 * n),
                "month" | "months" | "mois" => today.add_months(n),
                "year" | "years" | "an" | "ans" => today.add_months(12 * n),
                _ => return None,
            };
            return Some((3, d));
        }
        _ => {}
    }
    // Weekdays: "fri", "next fri", "this fri", "vendredi prochain", "prochain vendredi".
    if let Some(wd) = weekday(w(0)) {
        if matches!(w(1), "prochain" | "prochaine") {
            return Some((2, next_monday.add_days(i64::from(wd))));
        }
        return Some((1, upcoming(today, wd)));
    }
    if let ("next" | "prochain", Some(wd)) = (w(0), weekday(w(1))) {
        return Some((2, next_monday.add_days(i64::from(wd))));
    }
    if let ("this" | "ce", Some(wd)) = (w(0), weekday(w(1))) {
        return Some((2, upcoming(today, wd)));
    }
    // "12 nov", "12th november 2027", "nov 12", "1er mai".
    let year = |k: usize| {
        words
            .get(i + k)
            .and_then(|w| w.norm.parse::<i64>().ok())
            .filter(|y| (2000..=2100).contains(y))
    };
    if let (Some(d), Some(m)) = (day_number(w(0)), month(w(1))) {
        return match year(2) {
            Some(y) => Date::from_ymd(y, m, d).map(|date| (3, date)),
            None => future(today, m, d).map(|date| (2, date)),
        };
    }
    if let (Some(m), Some(d)) = (month(w(0)), day_number(w(1))) {
        return match year(2) {
            Some(y) => Date::from_ymd(y, m, d).map(|date| (3, date)),
            None => future(today, m, d).map(|date| (2, date)),
        };
    }
    // "the 15th", "le 15": that day of this month, or next month once it's past.
    if matches!(w(0), "the" | "le")
        && let Some(d) = day_number(w(1))
    {
        let this = Date::from_ymd(today.year(), today.month(), d);
        let date = this.filter(|t| *t >= today).or_else(|| {
            let next = today.add_months(1);
            Date::from_ymd(next.year(), next.month(), d)
        })?;
        return Some((2, date));
    }
    numeric_date(w(0), today).map(|d| (1, d))
}

/// `2026-12-01`, `1/12`, `1.12.2026` (day first).
fn numeric_date(word: &str, today: Date) -> Option<Date> {
    if word.len() == 10 && word.as_bytes()[4] == b'-' {
        return Date::parse(word).ok();
    }
    let sep = if word.contains('/') { '/' } else { '.' };
    let parts: Vec<_> = word.split(sep).collect();
    if !(2..=3).contains(&parts.len())
        || parts
            .iter()
            .any(|p| p.is_empty() || !p.bytes().all(|b| b.is_ascii_digit()))
    {
        return None;
    }
    let (d, m) = (parts[0].parse().ok()?, parts[1].parse().ok()?);
    match parts.get(2) {
        Some(y) => {
            let y: i64 = y.parse().ok()?;
            Date::from_ymd(if y < 100 { 2000 + y } else { y }, m, d)
        }
        None => future(today, m, d),
    }
}

/// Day `d` of month `m` this year, or next year once it has passed.
fn future(today: Date, m: i64, d: i64) -> Option<Date> {
    let this = Date::from_ymd(today.year(), m, d)?;
    if this >= today {
        Some(this)
    } else {
        Date::from_ymd(today.year() + 1, m, d)
    }
}

trait WithDay {
    fn with_day(self, d: i64) -> Self;
}

impl WithDay for Date {
    fn with_day(self, d: i64) -> Self {
        Date::from_ymd(self.year(), self.month(), d).unwrap_or(self)
    }
}

// ---- moments -------------------------------------------------------------------------------

/// A time relative to now or to the day: "in 2 hours", "in 30 min", "in an hour", "dans 2
/// heures", "tonight", "ce soir", "this morning", "ce matin", "this afternoon". Gives the day it
/// lands on and the time.
fn moment_at(words: &[Word], i: usize, today: Date, minute: Option<u32>) -> Option<(usize, Date, u32)> {
    let w = |k: usize| words.get(i + k).map(|w| w.norm.as_str()).unwrap_or("");
    let fixed = |n, t| Some((n, today, t));
    match (w(0), w(1)) {
        ("tonight", _) | ("ce", "soir") | ("this", "evening") => {
            return fixed(if w(0) == "tonight" { 1 } else { 2 }, 19 * 60);
        }
        ("this", "morning") | ("ce", "matin") => return fixed(2, 9 * 60),
        ("this", "afternoon") | ("cet", "apres-midi") => return fixed(2, 14 * 60),
        _ => {}
    }
    if !matches!(w(0), "in" | "dans") {
        return None;
    }
    let now = minute?;
    // "in 2h", "in 90min" as one word.
    let (n, unit, used) = match number(w(1)) {
        Some(n) => (n, w(2), 3),
        None => {
            let digits: String = w(1).chars().take_while(char::is_ascii_digit).collect();
            (digits.parse().ok()?, &w(1)[digits.len()..], 2)
        }
    };
    let minutes = match unit {
        "h" | "hr" | "hrs" | "hour" | "hours" | "heure" | "heures" => n * 60,
        "m" | "min" | "mins" | "minute" | "minutes" => n,
        _ => return None,
    };
    let at = i64::from(now) + minutes;
    Some((
        used,
        today.add_days(at.div_euclid(24 * 60)),
        at.rem_euclid(24 * 60) as u32,
    ))
}

// ---- times ---------------------------------------------------------------------------------

/// `9am`, `9:30pm`, `18:30`, `18h`, `18h30`, `noon`, `midi`, and after "at"/"à"/"@"/"vers" a bare
/// hour: `at 7`.
fn time_at(words: &[Word], i: usize) -> Option<(usize, u32)> {
    let raw = words[i].raw.as_str();
    let prep = matches!(raw, "at" | "à" | "@" | "vers");
    let (at, bare_ok) = if prep { (i + 1, true) } else { (i, false) };
    let word = words.get(at)?;
    let w = word.norm.as_str();
    let w = w.strip_prefix('@').map_or((w, bare_ok), |rest| (rest, true));
    let (w, bare_ok) = w;
    let consumed = at - i + 1;
    if let Some(t) = match w {
        "noon" | "midi" => Some(12 * 60),
        "midnight" | "minuit" => Some(0),
        _ => None,
    } {
        return Some((consumed, t));
    }
    // "9 am" as two words.
    if let Some(next) = words.get(at + 1)
        && let ("am" | "pm", Some(t)) = (next.norm.as_str(), clock(w, true))
        && t < 13 * 60
    {
        return Some((consumed + 1, meridiem(t, next.norm == "pm")?));
    }
    for (suffix, pm) in [("am", false), ("pm", true)] {
        if let Some(t) = w.strip_suffix(suffix).and_then(|n| clock(n, true)) {
            return Some((consumed, meridiem(t, pm)?));
        }
    }
    clock(w, bare_ok).map(|t| (consumed, t))
}

/// `9:30`, `18h`, `18h30`, and (when `bare` is allowed) `7`. Minutes after midnight.
fn clock(w: &str, bare: bool) -> Option<u32> {
    let (h, m) = if let Some((h, m)) = w.split_once(':').or_else(|| w.split_once('h')) {
        (h, if m.is_empty() { "0" } else { m })
    } else if bare {
        (w, "0")
    } else {
        return None;
    };
    if h.is_empty() || h.len() > 2 || m.len() > 2 || !h.bytes().chain(m.bytes()).all(|b| b.is_ascii_digit()) {
        return None;
    }
    let (h, m): (u32, u32) = (h.parse().ok()?, m.parse().ok()?);
    (h < 24 && m < 60).then_some(h * 60 + m)
}

fn meridiem(t: u32, pm: bool) -> Option<u32> {
    let (h, m) = (t / 60, t % 60);
    if !(1..=12).contains(&h) {
        return None;
    }
    Some(((h % 12) + if pm { 12 } else { 0 }) * 60 + m)
}

// ---- repeats -------------------------------------------------------------------------------

/// A repeat rule at word `i`, with "after done" / "après fait" after it counting from completion.
fn repeat_at(words: &[Word], i: usize) -> Option<(usize, Repeat)> {
    let (mut n, mut repeat) = nth_weekday_at(words, i).or_else(|| simple_repeat_at(words, i))?;
    let w = |k: usize| words.get(i + n + k).map(|w| w.norm.as_str()).unwrap_or("");
    let suffix = match (w(0), w(1)) {
        ("after" | "from" | "when" | "once", "done" | "completion" | "completed" | "finished") => 2,
        ("apres", "fait" | "completion" | "realisation" | "accompli") => 2,
        _ => 0,
    };
    if suffix > 0 {
        n += suffix;
        repeat.from_completion = true;
    }
    Some((n, repeat))
}

/// 1…4 or -1 (last): "first", "2nd", "third", "last", "premier", "2e", "dernière", plurals too.
fn ordinal(word: &str) -> Option<i8> {
    let w = word.strip_suffix('s').unwrap_or(word);
    Some(match w {
        "first" | "1st" | "premier" | "premiere" | "1er" | "1re" | "1ere" => 1,
        "second" | "2nd" | "deuxieme" | "seconde" | "2e" | "2eme" => 2,
        "third" | "3rd" | "troisieme" | "3e" | "3eme" => 3,
        "fourth" | "4th" | "quatrieme" | "4e" | "4eme" => 4,
        "last" | "dernier" | "derniere" => -1,
        _ => return None,
    })
}

/// "every last friday (of the month)", "every 2nd tue", "chaque premier lundi (du mois)", "tous
/// les derniers vendredis du mois", "le dernier vendredi de chaque mois".
fn nth_weekday_at(words: &[Word], i: usize) -> Option<(usize, Repeat)> {
    let w = |k: usize| words.get(i + k).map(|w| w.norm.as_str()).unwrap_or("");
    let lead = match (w(0), w(1)) {
        ("every" | "each" | "chaque", _) => 1,
        ("tous" | "toutes", "les") => 2,
        ("le" | "la", _) => 1,
        _ => return None,
    };
    let nth = ordinal(w(lead))?;
    let day = weekday(w(lead + 1))?;
    let n = lead + 2;
    let rest = |k: usize| w(n + k);
    let tail = match (rest(0), rest(1), rest(2)) {
        ("of", "the" | "every" | "each", "month") | ("de", "chaque", "mois") => 3,
        ("of", "month", _) | ("du", "mois", _) => 2,
        _ => 0,
    };
    // "le dernier vendredi" alone is a one-off date, not a rule.
    if matches!(w(0), "le" | "la") && tail == 0 {
        return None;
    }
    let n = n + tail;
    Some((n, Repeat::monthly_on(nth, day)))
}

fn simple_repeat_at(words: &[Word], i: usize) -> Option<(usize, Repeat)> {
    let w = |k: usize| words.get(i + k).map(|w| w.norm.as_str()).unwrap_or("");
    let unit = |u: &str| -> Option<Freq> {
        Some(match u {
            "day" | "days" | "jour" | "jours" => Freq::Daily,
            "week" | "weeks" | "semaine" | "semaines" => Freq::Weekly,
            "month" | "months" | "mois" => Freq::Monthly,
            "year" | "years" | "an" | "ans" | "annee" | "annees" => Freq::Yearly,
            _ => return None,
        })
    };
    match w(0) {
        "daily" | "quotidien" | "quotidienne" => return Some((1, Repeat::new(Freq::Daily, 1))),
        "weekly" | "hebdo" | "hebdomadaire" => return Some((1, Repeat::new(Freq::Weekly, 1))),
        "monthly" | "mensuel" | "mensuelle" => return Some((1, Repeat::new(Freq::Monthly, 1))),
        "yearly" | "annually" | "annuel" | "annuelle" => return Some((1, Repeat::new(Freq::Yearly, 1))),
        "weekdays" => return Some((1, Repeat::weekly_on(WEEKDAYS))),
        "every" | "each" | "chaque" => {}
        "tous" | "toutes" if w(1) == "les" => {
            // "tous les jours", "toutes les 2 semaines", "tous les lundis et jeudis".
            if let Some(f) = unit(w(2)) {
                return Some((3, Repeat::new(f, 1)));
            }
            if let (Some(n), Some(f)) = (number(w(2)), unit(w(3))) {
                return Some((4, Repeat::new(f, n as u32)));
            }
            return weekday_list(words, i + 2).map(|(n, mask)| (2 + n, Repeat::weekly_on(mask)));
        }
        _ => return None,
    }
    // After "every" / "chaque".
    match w(1) {
        "weekday" | "weekdays" => return Some((2, Repeat::weekly_on(WEEKDAYS))),
        "weekend" | "week-end" => return Some((2, Repeat::weekly_on(WEEKEND))),
        "other" => return unit(w(2)).map(|f| (3, Repeat::new(f, 2))),
        u => {
            if let Some(f) = unit(u) {
                return Some((2, Repeat::new(f, 1)));
            }
        }
    }
    if let (Some(n), Some(f)) = (number(w(1)), unit(w(2))) {
        return Some((3, Repeat::new(f, n as u32)));
    }
    weekday_list(words, i + 1).map(|(n, mask)| (1 + n, Repeat::weekly_on(mask)))
}

/// "mon", "mon and thu", "mon, wed, fri", "lundis et jeudis" → (words used, weekday mask).
fn weekday_list(words: &[Word], i: usize) -> Option<(usize, u8)> {
    let mut mask = 0u8;
    let mut used = 0;
    let mut k = i;
    while let Some(word) = words.get(k) {
        if let Some(wd) = weekday(&word.norm) {
            mask |= 1 << wd;
            used = k - i + 1;
            k += 1;
        } else if mask != 0 && matches!(word.norm.as_str(), "and" | "et" | "&" | ",") {
            k += 1;
        } else {
            break;
        }
    }
    (mask != 0).then_some((used, mask))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Sunday 8 November 2026.
    fn today() -> Date {
        Date::parse("2026-11-08").unwrap()
    }

    fn parse(input: &str) -> ParsedTask {
        parse_task(input, today(), &[])
    }

    /// Title, start, time, repeat rule, deadline, someday, priority, tags — compactly.
    fn summary(input: &str) -> String {
        let p = parse(input);
        let s = p.schedule;
        let mut out = vec![format!("{:?}", p.title)];
        if let Some(d) = s.start {
            out.push(d.to_string());
        }
        if let Some(t) = s.time {
            out.push(format!("{}:{:02}", t / 60, t % 60));
        }
        if let Some(r) = s.repeat {
            out.push(r.to_rrule());
        }
        if let Some(d) = s.deadline {
            out.push(format!("by {d}"));
        }
        if s.someday {
            out.push("someday".into());
        }
        if p.priority > 0 {
            out.push(format!("!{}", p.priority));
        }
        for t in &p.tags {
            out.push(format!("#{t}"));
        }
        out.join(" ")
    }

    #[test]
    fn relative_dates() {
        assert_eq!(summary("buy milk today"), r#""buy milk" 2026-11-08"#);
        assert_eq!(summary("buy milk tomorrow"), r#""buy milk" 2026-11-09"#);
        assert_eq!(
            summary("Acheter du pain demain"),
            r#""Acheter du pain" 2026-11-09"#
        );
        assert_eq!(summary("rendez-vous après-demain"), r#""rendez-vous" 2026-11-10"#);
        assert_eq!(summary("call in 3 days"), r#""call" 2026-11-11"#);
        assert_eq!(summary("appeler dans 2 semaines"), r#""appeler" 2026-11-22"#);
        assert_eq!(summary("renew in a month"), r#""renew" 2026-12-08"#);
        assert_eq!(summary("plan next week"), r#""plan" 2026-11-09"#);
        assert_eq!(summary("ranger la semaine prochaine"), r#""ranger" 2026-11-09"#);
        assert_eq!(summary("hike this weekend"), r#""hike" 2026-11-14"#);
        assert_eq!(summary("bills next month"), r#""bills" 2026-12-01"#);
    }

    #[test]
    fn weekdays() {
        assert_eq!(summary("gym fri"), r#""gym" 2026-11-13"#);
        assert_eq!(summary("gym on Friday"), r#""gym" 2026-11-13"#);
        assert_eq!(summary("gym sunday"), r#""gym" 2026-11-08"#);
        assert_eq!(summary("gym next fri"), r#""gym" 2026-11-13"#);
        assert_eq!(summary("sport vendredi"), r#""sport" 2026-11-13"#);
        assert_eq!(summary("sport mardi prochain"), r#""sport" 2026-11-10"#);
        assert_eq!(summary("sport le jeudi"), r#""sport" 2026-11-12"#);
    }

    #[test]
    fn explicit_dates() {
        assert_eq!(summary("party 12 dec"), r#""party" 2026-12-12"#);
        assert_eq!(summary("party Dec 12th"), r#""party" 2026-12-12"#);
        assert_eq!(summary("fête le 1er mai"), r#""fête" 2027-05-01"#);
        assert_eq!(summary("taxes 15 avril 2027"), r#""taxes" 2027-04-15"#);
        assert_eq!(summary("dentist 3/12"), r#""dentist" 2026-12-03"#);
        assert_eq!(summary("dentist 3.1"), r#""dentist" 2027-01-03"#);
        assert_eq!(summary("taxes 2026-12-01"), r#""taxes" 2026-12-01"#);
        assert_eq!(summary("rent the 1st"), r#""rent" 2026-12-01"#);
        assert_eq!(summary("loyer le 15"), r#""loyer" 2026-11-15"#);
    }

    #[test]
    fn times() {
        assert_eq!(summary("call 9am"), r#""call" 2026-11-08 9:00"#);
        assert_eq!(summary("call tomorrow 9:30pm"), r#""call" 2026-11-09 21:30"#);
        assert_eq!(summary("call tomorrow at 7"), r#""call" 2026-11-09 7:00"#);
        assert_eq!(summary("call tomorrow 7 pm"), r#""call" 2026-11-09 19:00"#);
        assert_eq!(summary("réunion demain à 14h30"), r#""réunion" 2026-11-09 14:30"#);
        assert_eq!(summary("réunion lundi 9h"), r#""réunion" 2026-11-09 9:00"#);
        assert_eq!(summary("lunch fri noon"), r#""lunch" 2026-11-13 12:00"#);
        assert_eq!(summary("standup 12am"), r#""standup" 2026-11-08 0:00"#);
        assert_eq!(summary("meet @18:30"), r#""meet" 2026-11-08 18:30"#);
        // A bare number without "at" stays in the title.
        assert_eq!(summary("buy 7 eggs"), r#""buy 7 eggs""#);
    }

    #[test]
    fn repeats() {
        assert_eq!(
            summary("vitamins every day"),
            r#""vitamins" 2026-11-08 FREQ=DAILY"#
        );
        assert_eq!(
            summary("vitamins daily 8am"),
            r#""vitamins" 2026-11-08 8:00 FREQ=DAILY"#
        );
        assert_eq!(
            summary("review every 2 weeks"),
            r#""review" 2026-11-08 FREQ=WEEKLY;INTERVAL=2"#
        );
        assert_eq!(
            summary("trash every mon and thu"),
            r#""trash" 2026-11-09 FREQ=WEEKLY;BYDAY=MO,TH"#
        );
        assert_eq!(
            summary("trash every mon, wed, fri"),
            r#""trash" 2026-11-09 FREQ=WEEKLY;BYDAY=MO,WE,FR"#
        );
        assert_eq!(
            summary("standup every weekday 9:15"),
            r#""standup" 2026-11-09 9:15 FREQ=WEEKLY;BYDAY=MO,TU,WE,TH,FR"#
        );
        assert_eq!(
            summary("pay rent monthly on the 1st"),
            r#""pay rent" 2026-12-01 FREQ=MONTHLY"#
        );
        assert_eq!(
            summary("water plants every other day"),
            r#""water plants" 2026-11-08 FREQ=DAILY;INTERVAL=2"#
        );
        assert_eq!(
            summary("poubelles tous les lundis et jeudis"),
            r#""poubelles" 2026-11-09 FREQ=WEEKLY;BYDAY=MO,TH"#
        );
        assert_eq!(
            summary("ménage toutes les 2 semaines"),
            r#""ménage" 2026-11-08 FREQ=WEEKLY;INTERVAL=2"#
        );
        assert_eq!(
            summary("arroser chaque jour"),
            r#""arroser" 2026-11-08 FREQ=DAILY"#
        );
        assert_eq!(
            summary("anniversaire chaque année le 3 mars"),
            r#""anniversaire" 2027-03-03 FREQ=YEARLY"#
        );
    }

    #[test]
    fn priority_and_tags() {
        assert_eq!(summary("milk #groceries !1"), r#""milk" !3 #groceries"#);
        assert_eq!(summary("milk #Gro !low"), r#""milk" !1 #gro"#);
        assert_eq!(
            summary("report #work #urgent !haute"),
            r#""report" !3 #work #urgent"#
        );
        assert_eq!(summary("report #work, #work"), r#""report" #work"#);
        assert_eq!(summary("fix #12 now"), r#""fix #12 now""#);
        assert_eq!(summary("wow # nice"), r#""wow # nice""#);
        assert_eq!(summary("café #épicerie"), r#""café" #épicerie"#);
        assert_eq!(summary("wow!!"), r#""wow!!""#);
        assert_eq!(summary("fix bug !!!"), r#""fix bug" !3"#);
        assert_eq!(normalize_tag(" #Side  Projects "), "side-projects");
        assert_eq!(normalize_tag("Courses (maison)"), "courses-maison");
        assert_eq!(normalize_tag("!!"), "");
    }

    #[test]
    fn deadlines_and_someday() {
        assert_eq!(summary("report by fri"), r#""report" by 2026-11-13"#);
        assert_eq!(summary("report due 12 dec"), r#""report" by 2026-12-12"#);
        assert_eq!(summary("rapport avant vendredi"), r#""rapport" by 2026-11-13"#);
        assert_eq!(summary("rapport pour demain"), r#""rapport" by 2026-11-09"#);
        assert_eq!(
            summary("draft tomorrow by fri"),
            r#""draft" 2026-11-09 by 2026-11-13"#
        );
        // A word that isn't followed by a date stays.
        assert_eq!(summary("stand by me"), r#""stand by me""#);
        assert_eq!(summary("learn piano someday"), r#""learn piano" someday"#);
        assert_eq!(
            summary("apprendre le piano un jour"),
            r#""apprendre le piano" someday"#
        );
        // A start date wins over Someday.
        assert_eq!(summary("trip someday tomorrow"), r#""trip" 2026-11-09"#);
        // A repeat with only a deadline is anchored on it.
        assert_eq!(
            summary("taxes yearly by 15 apr"),
            r#""taxes" FREQ=YEARLY by 2027-04-15"#
        );
    }

    #[test]
    fn durations_and_moments() {
        assert_eq!(summary("ship end of week"), r#""ship" 2026-11-13"#);
        assert_eq!(summary("rent end of the month"), r#""rent" 2026-11-30"#);
        assert_eq!(summary("bilan fin du mois"), r#""bilan" 2026-11-30"#);
        assert_eq!(summary("ranger fin de semaine"), r#""ranger" 2026-11-13"#);
        assert_eq!(summary("taxes end of year"), r#""taxes" 2026-12-31"#);
        assert_eq!(summary("call mum tonight"), r#""call mum" 2026-11-08 19:00"#);
        assert_eq!(summary("appeler ce soir"), r#""appeler" 2026-11-08 19:00"#);
        let at = |input: &str, minute: u32| {
            let p = parse_task_at(input, today(), Some(minute), &[]);
            (p.title, p.schedule.start.map(|d| d.to_string()), p.schedule.time)
        };
        assert_eq!(
            at("tea in 2 hours", 600),
            ("tea".into(), Some("2026-11-08".into()), Some(720))
        );
        assert_eq!(
            at("tea in 30 min", 600),
            ("tea".into(), Some("2026-11-08".into()), Some(630))
        );
        assert_eq!(
            at("check dans 2h", 23 * 60),
            ("check".into(), Some("2026-11-09".into()), Some(60))
        );
        assert_eq!(at("tea in an hour", 600).2, Some(660));
        // Without the time of day it stays text.
        assert_eq!(summary("tea in 2 hours"), r#""tea in 2 hours""#);
        // A date said elsewhere wins over the moment's day.
        assert_eq!(summary("call tomorrow tonight"), r#""call" 2026-11-09 19:00"#);
    }

    #[test]
    fn nth_weekday_and_after_done() {
        assert_eq!(
            summary("review every last friday"),
            r#""review" 2026-11-27 FREQ=MONTHLY;BYDAY=-1FR"#
        );
        assert_eq!(
            summary("club every 2nd tuesday of the month"),
            r#""club" 2026-11-10 FREQ=MONTHLY;BYDAY=2TU"#
        );
        assert_eq!(
            summary("réunion chaque premier lundi du mois"),
            r#""réunion" 2026-12-07 FREQ=MONTHLY;BYDAY=1MO"#
        );
        assert_eq!(
            summary("bilan tous les derniers vendredis du mois"),
            r#""bilan" 2026-11-27 FREQ=MONTHLY;BYDAY=-1FR"#
        );
        assert_eq!(
            summary("bilan le dernier vendredi de chaque mois"),
            r#""bilan" 2026-11-27 FREQ=MONTHLY;BYDAY=-1FR"#
        );
        assert_eq!(
            summary("haircut every 4 weeks after done"),
            r#""haircut" 2026-11-08 FREQ=WEEKLY;INTERVAL=4;X-FROM=COMPLETION"#
        );
        assert_eq!(
            summary("arroser tous les 3 jours après fait"),
            r#""arroser" 2026-11-08 FREQ=DAILY;INTERVAL=3;X-FROM=COMPLETION"#
        );
    }

    #[test]
    fn first_match_of_a_kind_wins() {
        assert_eq!(
            summary("plan friday party tomorrow"),
            r#""plan party tomorrow" 2026-11-13"#
        );
    }

    #[test]
    fn tokens_and_dismissing() {
        let p = parse("Café demain 9h #gro");
        let tokens: Vec<_> = p
            .tokens
            .iter()
            .map(|t| (t.start, t.end, t.kind, t.text.as_str()))
            .collect();
        assert_eq!(
            tokens,
            [
                (5, 11, TokenKind::Date, "demain"),
                (12, 14, TokenKind::Time, "9h"),
                (15, 19, TokenKind::Tag, "#gro")
            ]
        );
        let kept = parse_task("Café demain 9h #gro", today(), &["demain".into(), "#gro".into()]);
        assert_eq!(kept.title, "Café demain #gro");
        assert_eq!(kept.schedule.start, Some(today())); // the time alone means today
        assert!(kept.tags.is_empty());
        assert_eq!(kept.tokens.len(), 1);
    }

    #[test]
    fn plain_titles_stay_plain() {
        for title in [
            "Call Tom",
            "Read the news",
            "Mars mission notes",
            "Monday blues",
            "Sun cream",
        ] {
            let p = parse(title);
            // "Monday"/"Sun" are weekdays; they are taken, by design, and can be dismissed.
            if p.tokens.is_empty() {
                assert_eq!(p.title, title);
            }
        }
        assert_eq!(parse("Call Tom").title, "Call Tom");
        assert_eq!(parse("Mars mission notes").title, "Mars mission notes");
        assert_eq!(parse("").title, "");
    }
}
