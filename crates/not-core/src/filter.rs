//! Smart list queries (ADR 0014): what a saved view shows, and what a task added in it gets.
//!
//! Whitespace between terms means AND, `or` separates groups, `-` negates a term:
//! `#work !1 or #urgent -someday`. Terms: `#tag`, `!1`/`!2`/`!3` (high/medium/low, as in quick
//! add), `!` (any priority), the date words in [`Atom`], and any other word or `"phrase"`, looked
//! for in the title and notes.
//!
//! Unless a query names `upcoming`, `tomorrow`, `week`, `someday` or `all`, it only matches tasks
//! that are available now: no future start date and not Someday, nor under a task that is.

use crate::date::Date;
use crate::parse::normalize_tag;
use crate::{Error, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Query {
    groups: Vec<Vec<Term>>,
    /// Only tasks available now (see the module docs).
    available_only: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Term {
    negated: bool,
    atom: Atom,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Atom {
    Tag(String),
    /// `None` = any priority.
    Priority(Option<u8>),
    /// Starts or is due on or before today.
    Today,
    Tomorrow,
    /// Starts or is due within the next 7 days (overdue included).
    Week,
    /// Deadline passed, or start date before today.
    Overdue,
    /// Starts or is due after today.
    Upcoming,
    Someday,
    /// No start date, no deadline, not Someday.
    NoDate,
    Deadline,
    Repeating,
    Tagged,
    Untagged,
    All,
    Text(String),
}

/// What [`Query::matches`] looks at: a task with what it inherits from the tasks above it.
pub(crate) struct Candidate<'a> {
    pub title: &'a str,
    pub notes: &'a str,
    pub start: Option<Date>,
    pub deadline: Option<Date>,
    pub someday: bool,
    pub priority: u8,
    pub repeating: bool,
    /// Its own tags and its ancestors'.
    pub tags: &'a [String],
    /// It or an ancestor starts after today or is Someday.
    pub deferred: bool,
}

/// What a task added in a view starts with, so it shows there.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Defaults {
    pub tags: Vec<String>,
    pub priority: u8,
    pub start: Option<Date>,
    pub someday: bool,
}

const KEYWORDS: &[(&str, Atom)] = &[
    ("today", Atom::Today),
    ("tomorrow", Atom::Tomorrow),
    ("week", Atom::Week),
    ("overdue", Atom::Overdue),
    ("upcoming", Atom::Upcoming),
    ("someday", Atom::Someday),
    ("nodate", Atom::NoDate),
    ("deadline", Atom::Deadline),
    ("repeating", Atom::Repeating),
    ("tagged", Atom::Tagged),
    ("untagged", Atom::Untagged),
    ("all", Atom::All),
];

impl Query {
    pub fn parse(query: &str) -> Result<Self> {
        let mut groups = vec![Vec::new()];
        for raw in split_terms(query)? {
            if raw.quoted {
                let text = raw.text.trim().to_lowercase();
                if !text.is_empty() {
                    groups.last_mut().unwrap().push(Term {
                        negated: raw.negated,
                        atom: Atom::Text(text),
                    });
                }
                continue;
            }
            let word = raw.text.to_lowercase();
            if word == "or" && !raw.negated {
                groups.push(Vec::new());
                continue;
            }
            let atom = if let Some(tag) = word.strip_prefix('#') {
                let tag = normalize_tag(tag);
                if tag.is_empty() {
                    return Err(Error::Invalid(format!("{:?} isn't a tag", raw.text)));
                }
                Atom::Tag(tag)
            } else if let Some(level) = word.strip_prefix('!') {
                Atom::Priority(match level {
                    "" => None,
                    "1" | "high" => Some(3),
                    "2" | "medium" => Some(2),
                    "3" | "low" => Some(1),
                    _ => {
                        return Err(Error::Invalid(format!(
                            "{:?}: priority is !1, !2, !3 or !",
                            raw.text
                        )));
                    }
                })
            } else if let Some((_, atom)) = KEYWORDS.iter().find(|(k, _)| *k == word) {
                atom.clone()
            } else {
                Atom::Text(word)
            };
            groups.last_mut().unwrap().push(Term {
                negated: raw.negated,
                atom,
            });
        }
        if groups.iter().any(Vec::is_empty) {
            return Err(Error::Invalid(if groups.len() > 1 {
                "nothing on one side of \"or\"".into()
            } else {
                "the filter is empty".into()
            }));
        }
        let opens_up = |a: &Atom| {
            matches!(
                a,
                Atom::Upcoming | Atom::Tomorrow | Atom::Week | Atom::Someday | Atom::All
            )
        };
        let available_only = !groups.iter().flatten().any(|t| !t.negated && opens_up(&t.atom));
        Ok(Self {
            groups,
            available_only,
        })
    }

    pub fn matches(&self, task: &Candidate<'_>, today: Date) -> bool {
        if self.available_only && task.deferred {
            return false;
        }
        self.groups
            .iter()
            .any(|group| group.iter().all(|t| t.atom.matches(task, today) != t.negated))
    }

    /// Like [`Query::matches`] for a completed task: whether it's deferred doesn't matter any more.
    pub fn matches_done(&self, task: &Candidate<'_>, today: Date) -> bool {
        self.groups
            .iter()
            .any(|group| group.iter().all(|t| t.atom.matches(task, today) != t.negated))
    }

    /// From the first group's plain terms: its tags, priority, `today`/`tomorrow` as start date,
    /// `someday`.
    pub fn defaults(&self, today: Date) -> Defaults {
        let mut d = Defaults::default();
        for term in self.groups[0].iter().filter(|t| !t.negated) {
            match &term.atom {
                Atom::Tag(tag) if !d.tags.contains(tag) => d.tags.push(tag.clone()),
                Atom::Priority(Some(p)) => d.priority = *p,
                Atom::Priority(None) if d.priority == 0 => d.priority = 2,
                Atom::Today | Atom::Overdue => d.start = Some(today),
                Atom::Tomorrow => d.start = Some(today.add_days(1)),
                Atom::Someday => d.someday = true,
                _ => {}
            }
        }
        if d.start.is_some() {
            d.someday = false;
        }
        d
    }
}

impl Atom {
    fn matches(&self, t: &Candidate<'_>, today: Date) -> bool {
        let dated = |f: &dyn Fn(Date) -> bool| t.start.is_some_and(f) || t.deadline.is_some_and(f);
        match self {
            Self::Tag(tag) => t.tags.iter().any(|x| x == tag),
            Self::Priority(None) => t.priority > 0,
            Self::Priority(Some(p)) => t.priority == *p,
            Self::Today => dated(&|d| d <= today),
            Self::Tomorrow => dated(&|d| d == today.add_days(1)),
            Self::Week => dated(&|d| d <= today.add_days(6)),
            Self::Overdue => dated(&|d| d < today),
            Self::Upcoming => dated(&|d| d > today),
            Self::Someday => t.someday,
            Self::NoDate => t.start.is_none() && t.deadline.is_none() && !t.someday,
            Self::Deadline => t.deadline.is_some(),
            Self::Repeating => t.repeating,
            Self::Tagged => !t.tags.is_empty(),
            Self::Untagged => t.tags.is_empty(),
            Self::All => true,
            Self::Text(text) => {
                t.title.to_lowercase().contains(text.as_str())
                    || t.notes.to_lowercase().contains(text.as_str())
            }
        }
    }
}

/// Renames tag `old` to `new` in `query`, leaving everything else as typed. `None` when the query
/// doesn't name `old`.
pub(crate) fn rename_tag_in(query: &str, old: &str, new: &str) -> Option<String> {
    let mut changed = false;
    let words: Vec<String> = query
        .split(' ')
        .map(|w| {
            let (neg, rest) = w.strip_prefix('-').map_or(("", w), |r| ("-", r));
            match rest.strip_prefix('#') {
                Some(tag) if normalize_tag(tag) == old => {
                    changed = true;
                    format!("{neg}#{new}")
                }
                _ => w.to_owned(),
            }
        })
        .collect();
    changed.then(|| words.join(" "))
}

struct RawTerm<'a> {
    text: &'a str,
    negated: bool,
    quoted: bool,
}

fn split_terms(query: &str) -> Result<Vec<RawTerm<'_>>> {
    let mut terms = Vec::new();
    let mut rest = query.trim_start();
    while !rest.is_empty() {
        let (negated, after) = match rest.strip_prefix('-') {
            Some(r) if !r.is_empty() && !r.starts_with(char::is_whitespace) => (true, r),
            _ => (false, rest),
        };
        if let Some(inner) = after.strip_prefix('"') {
            let end = inner
                .find('"')
                .ok_or_else(|| Error::Invalid("a quote isn't closed".into()))?;
            terms.push(RawTerm {
                text: &inner[..end],
                negated,
                quoted: true,
            });
            rest = inner[end + 1..].trim_start();
        } else {
            let end = after.find(char::is_whitespace).unwrap_or(after.len());
            terms.push(RawTerm {
                text: &after[..end],
                negated,
                quoted: false,
            });
            rest = after[end..].trim_start();
        }
    }
    Ok(terms)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn today() -> Date {
        Date::parse("2026-11-16").unwrap()
    }

    fn d(s: &str) -> Option<Date> {
        Some(Date::parse(s).unwrap())
    }

    fn task<'a>(tags: &'a [String]) -> Candidate<'a> {
        Candidate {
            title: "Write report",
            notes: "for Anna",
            start: None,
            deadline: None,
            someday: false,
            priority: 0,
            repeating: false,
            tags,
            deferred: false,
        }
    }

    fn q(s: &str) -> Query {
        Query::parse(s).unwrap()
    }

    #[test]
    fn tags_priority_and_text() {
        let work = vec!["work".to_owned()];
        let t = Candidate {
            priority: 3,
            ..task(&work)
        };
        assert!(q("#work").matches(&t, today()));
        assert!(q("#Work !1").matches(&t, today()));
        assert!(!q("#work !2").matches(&t, today()));
        assert!(q("!").matches(&t, today()));
        assert!(q("#home or !1").matches(&t, today()));
        assert!(!q("#home or -#work").matches(&t, today()));
        assert!(q("report").matches(&t, today()));
        assert!(q("\"for anna\"").matches(&t, today()));
        assert!(!q("-anna").matches(&t, today()));
        assert!(q("tagged").matches(&t, today()) && !q("untagged").matches(&t, today()));
    }

    #[test]
    fn dates_and_availability() {
        let none: Vec<String> = Vec::new();
        let today_t = Candidate {
            start: d("2026-11-16"),
            ..task(&none)
        };
        let late = Candidate {
            start: d("2026-11-10"),
            ..task(&none)
        };
        let due_soon = Candidate {
            deadline: d("2026-11-20"),
            ..task(&none)
        };
        let later = Candidate {
            start: d("2026-11-30"),
            deferred: true,
            ..task(&none)
        };
        let parked = Candidate {
            someday: true,
            deferred: true,
            ..task(&none)
        };
        let tomorrow = Candidate {
            start: d("2026-11-17"),
            deferred: true,
            ..task(&none)
        };

        assert!(q("today").matches(&today_t, today()));
        assert!(q("today").matches(&late, today()));
        assert!(q("overdue").matches(&late, today()) && !q("overdue").matches(&today_t, today()));
        assert!(!q("today").matches(&due_soon, today()));
        assert!(q("week").matches(&due_soon, today()));
        assert!(q("deadline").matches(&due_soon, today()));
        assert!(q("upcoming").matches(&later, today()));
        assert!(q("upcoming").matches(&due_soon, today()));
        assert!(q("tomorrow").matches(&tomorrow, today()));
        // Deferred tasks only show when the query opens up to them.
        assert!(!q("untagged").matches(&later, today()));
        assert!(q("untagged all").matches(&later, today()));
        assert!(!q("untagged").matches(&parked, today()));
        assert!(q("someday").matches(&parked, today()));
        assert!(q("nodate").matches(&task(&none), today()) && !q("nodate").matches(&parked, today()));
    }

    #[test]
    fn defaults_come_from_the_first_group() {
        let d0 = q("#work #deep !1 today or #home").defaults(today());
        assert_eq!(d0.tags, ["work", "deep"]);
        assert_eq!(d0.priority, 3);
        assert_eq!(d0.start, Some(today()));
        assert!(q("someday #ideas").defaults(today()).someday);
        assert_eq!(q("-#work report").defaults(today()), Defaults::default());
    }

    #[test]
    fn bad_queries() {
        for bad in ["", "   ", "#", "!9", "#work or", "or #work", "\"open"] {
            assert!(Query::parse(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn renaming_tags() {
        assert_eq!(
            rename_tag_in("#work !1 or -#Work", "work", "job").as_deref(),
            Some("#job !1 or -#job")
        );
        assert_eq!(rename_tag_in("#home", "work", "job"), None);
    }
}
