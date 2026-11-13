//! Quick entry for budgeting, in English or French:
//!
//! ```text
//! 12.50 coffee                → 12.50 out, payee "coffee"
//! +2400 salaire @sg           → income on SG
//! 45,80 courses #courses hier → comma decimal, yesterday
//! 120 carrefour #courses 90 / #shopping   → split: 90 groceries, the other 30 shopping
//! 500 @sg > @bourso           → transfer
//! ```
//!
//! The first amount is the total. `#` picks a category and `@` an account, by name or name prefix,
//! accents and case ignored; `> @x` (or `@a>@b`) makes it a transfer to `x`. Dates look back, since
//! money is usually entered after it's spent: "lundi" is the last Monday, "12/11" the last 12 Nov.
//! ` / ` separates split parts, each `#category [amount]`; one part may leave its amount out and
//! take what remains. Whatever isn't recognised in the first part is the payee.

use crate::date::Date;
use crate::money::{CategoryKind, Split, TxnInput, TxnKind};
use crate::parse::{Word, day_number, month, split_words, squash, weekday};
use crate::store::Store;
use crate::{Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MoneyTokenKind {
    Amount,
    Category,
    Account,
    Date,
}

/// A recognised piece of the input. Offsets are UTF-16 code units, like the apps' text APIs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MoneyToken {
    pub start: u32,
    pub end: u32,
    pub kind: MoneyTokenKind,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedMoney {
    /// `Entry` or `Transfer`.
    pub kind: TxnKind,
    /// Signed for entries (out is negative), positive for transfers; `None` until one is typed.
    pub amount: Option<i64>,
    pub payee: String,
    pub category: Option<String>,
    /// The category came from the payee's last entry rather than a `#tag`.
    pub category_guessed: bool,
    pub account: Option<String>,
    pub to_account: Option<String>,
    /// `YYYY-MM-DD`; today unless the input says otherwise.
    pub date: String,
    pub splits: Vec<Split>,
    pub tokens: Vec<MoneyToken>,
    /// Why it can't be entered yet, in words for the preview ("Type an amount").
    pub problem: Option<String>,
}

impl Store {
    /// Parses quick-entry text against this workspace's categories and accounts, filling in what
    /// it leaves out: the payee's usual category, the last used account, the sign an income
    /// category implies.
    pub fn parse_money(&self, input: &str, today: &str) -> Result<ParsedMoney> {
        let today = Date::parse(today)?;
        let categories: Vec<(String, String, CategoryKind)> = self
            .categories()
            .into_iter()
            .map(|c| (c.id, c.name, c.kind))
            .collect();
        let accounts: Vec<(String, String)> = self
            .accounts(&today.to_string())?
            .into_iter()
            .map(|a| (a.id, a.name))
            .collect();
        let raw = parse(input, today, &categories, &accounts);

        let mut out = ParsedMoney {
            kind: if raw.transfer {
                TxnKind::Transfer
            } else {
                TxnKind::Entry
            },
            amount: None,
            payee: raw.payee,
            category: raw.category,
            category_guessed: false,
            account: raw.account,
            to_account: raw.to_account,
            date: raw.date.unwrap_or(today).to_string(),
            splits: Vec::new(),
            tokens: raw.tokens,
            problem: None,
        };
        if out.account.is_none() {
            out.account = self.last_used_account();
        }
        if out.kind == TxnKind::Entry && out.category.is_none() && raw.parts.is_empty() {
            out.category = self.category_for_payee(&out.payee);
            out.category_guessed = out.category.is_some();
        }
        let income = |id: &str| {
            categories
                .iter()
                .any(|(c, _, k)| c == id && *k == CategoryKind::Income)
        };
        let sign = match raw.sign {
            Some(s) => s,
            None if out.kind == TxnKind::Transfer => 1,
            None => {
                let first = raw
                    .parts
                    .first()
                    .map(|p| p.0.as_str())
                    .or(out.category.as_deref());
                if first.is_some_and(income) { 1 } else { -1 }
            }
        };
        out.amount = raw.amount.map(|a| {
            if out.kind == TxnKind::Transfer {
                a
            } else {
                sign * a
            }
        });

        if raw.segments > 1 && raw.parts.len() < raw.segments {
            out.problem = Some("Give each part of the split a #category".into());
        } else if !raw.parts.is_empty()
            && let Some(total) = raw.amount
        {
            match resolve_splits(&raw.parts, total) {
                Ok(parts) => {
                    out.splits = parts
                        .into_iter()
                        .map(|(category, amount)| Split {
                            category,
                            amount: sign * amount,
                        })
                        .collect();
                    out.category = None;
                }
                Err(problem) => out.problem = Some(problem),
            }
        }
        if out.problem.is_none() {
            out.problem = if out.amount.is_none() {
                Some("Type an amount".into())
            } else if out.account.is_none() {
                Some("Add an account first".into())
            } else if out.kind == TxnKind::Transfer && out.to_account.is_none() {
                Some("Name the account to move it to".into())
            } else {
                None
            };
        }
        Ok(out)
    }

    /// Parses `input` and enters it. Returns the new row's id.
    pub fn quick_add_money(&self, input: &str, today: &str) -> Result<String> {
        let p = self.parse_money(input, today)?;
        if let Some(problem) = p.problem {
            return Err(Error::Invalid(problem));
        }
        self.add_txn(&TxnInput {
            kind: p.kind,
            account: p.account.unwrap_or_default(),
            to_account: p.to_account,
            date: p.date,
            amount: p.amount.unwrap_or_default(),
            payee: p.payee,
            note: String::new(),
            category: p.category,
            splits: p.splits,
        })
    }
}

/// Split parts as (category, amount) with the one missing amount filled in.
fn resolve_splits(
    parts: &[(String, Option<i64>)],
    total: i64,
) -> std::result::Result<Vec<(String, i64)>, String> {
    let missing = parts.iter().filter(|p| p.1.is_none()).count();
    let given: i64 = parts.iter().filter_map(|p| p.1).sum();
    if missing > 1 {
        return Err("Give every part but one an amount".into());
    }
    let rest = total - given;
    if (missing == 1 && rest <= 0) || (missing == 0 && rest != 0) {
        return Err(format!(
            "The parts add up to {}, not {}",
            cents(given),
            cents(total)
        ));
    }
    Ok(parts
        .iter()
        .map(|(c, a)| (c.clone(), a.unwrap_or(rest)))
        .collect())
}

fn cents(c: i64) -> String {
    format!("{}.{:02}", c / 100, (c % 100).abs())
}

// ---- the word-level parse -------------------------------------------------------------------

#[derive(Debug, Default)]
struct Raw {
    /// Unsigned cents.
    amount: Option<i64>,
    /// `+` → 1, `-` → -1.
    sign: Option<i64>,
    transfer: bool,
    payee: String,
    category: Option<String>,
    account: Option<String>,
    to_account: Option<String>,
    date: Option<Date>,
    /// Split parts (category, unsigned amount); empty unless the input has ` / `.
    parts: Vec<(String, Option<i64>)>,
    /// Pieces between ` / `; more than one means a split.
    segments: usize,
    tokens: Vec<MoneyToken>,
}

fn parse(
    input: &str,
    today: Date,
    categories: &[(String, String, CategoryKind)],
    accounts: &[(String, String)],
) -> Raw {
    let words = split_words(input);
    let mut used = vec![false; words.len()];
    let mut raw = Raw::default();
    let mut found: Vec<(usize, usize, MoneyTokenKind)> = Vec::new();
    let names: Vec<(String, String)> = categories
        .iter()
        .map(|(id, n, _)| (id.clone(), n.clone()))
        .collect();

    // Segments between "/" words.
    let mut segments: Vec<Vec<usize>> = vec![Vec::new()];
    for (i, w) in words.iter().enumerate() {
        if w.raw == "/" {
            used[i] = true;
            segments.push(Vec::new());
        } else {
            segments.last_mut().expect("one").push(i);
        }
    }
    let split = segments.len() > 1;

    // The total: the first amount anywhere in the first segment.
    if let Some(&i) = segments[0].iter().find(|&&i| amount(&words[i].raw).is_some()) {
        let (a, sign) = amount(&words[i].raw).expect("checked");
        raw.amount = Some(a);
        raw.sign = sign;
        used[i] = true;
        found.push((i, i + 1, MoneyTokenKind::Amount));
    }

    // Accounts: "@a", "@a>@b", "> @b", ">@b", "→ @b".
    let mut to_next = false;
    for i in 0..words.len() {
        if used[i] {
            continue;
        }
        let w = words[i].raw.as_str();
        if matches!(w, ">" | "→" | "->") {
            used[i] = true;
            raw.transfer = true;
            to_next = true;
            found.push((i, i + 1, MoneyTokenKind::Account));
            continue;
        }
        let (from, to) = match w.split_once('>') {
            Some((a, b)) => (a.trim_end_matches('-'), Some(b)),
            None => (w, None),
        };
        let from = from.strip_prefix('@').and_then(|n| pick(n, accounts));
        let to = to
            .and_then(|t| t.strip_prefix('@'))
            .and_then(|n| pick(n, accounts));
        if w.starts_with('>') || w.starts_with('→') {
            if let Some(id) = w
                .trim_start_matches(['>', '→'])
                .strip_prefix('@')
                .and_then(|n| pick(n, accounts))
            {
                raw.transfer = true;
                raw.to_account.get_or_insert(id);
                used[i] = true;
                found.push((i, i + 1, MoneyTokenKind::Account));
            }
            continue;
        }
        if from.is_none() && to.is_none() {
            continue;
        }
        used[i] = true;
        found.push((i, i + 1, MoneyTokenKind::Account));
        if let Some(to) = to {
            raw.transfer = true;
            raw.to_account.get_or_insert(to);
        }
        if let Some(id) = from {
            if to_next {
                raw.to_account.get_or_insert(id);
                to_next = false;
            } else if raw.account.is_none() {
                raw.account = Some(id);
            } else {
                raw.transfer = true;
                raw.to_account.get_or_insert(id);
            }
        }
    }

    // Categories, and with a split each part's amount.
    for (s, segment) in segments.iter().enumerate() {
        let mut category = None;
        let mut part_amount = None;
        for &i in segment {
            if used[i] {
                continue;
            }
            if category.is_none()
                && let Some(id) = words[i].raw.strip_prefix('#').and_then(|n| pick(n, &names))
            {
                category = Some(id);
                used[i] = true;
                found.push((i, i + 1, MoneyTokenKind::Category));
            } else if split
                && part_amount.is_none()
                && let Some((a, _)) = amount(&words[i].raw)
            {
                part_amount = Some(a);
                used[i] = true;
                found.push((i, i + 1, MoneyTokenKind::Amount));
            }
        }
        if split {
            if let Some(c) = category {
                raw.parts.push((c, part_amount));
            }
        } else if s == 0 {
            raw.category = category;
        }
    }
    raw.segments = segments.len();

    // The date: the first one among the words left.
    let mut i = 0;
    while i < words.len() && raw.date.is_none() {
        if !used[i]
            && let Some((n, date)) = past_date_at(&words, i, today)
            && (i..i + n).all(|k| !used[k])
        {
            raw.date = Some(date);
            used[i..i + n].fill(true);
            found.push((i, i + n, MoneyTokenKind::Date));
        }
        i += 1;
    }

    // The payee: what's left of the first segment, as typed.
    raw.payee = segments[0]
        .iter()
        .filter(|&&i| !used[i])
        .map(|&i| input[words[i].start..words[i].end].trim_end_matches([',', ';', ':']))
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join(" ");

    let utf16 = |byte: usize| input[..byte].encode_utf16().count() as u32;
    found.sort_by_key(|f| f.0);
    raw.tokens = found
        .into_iter()
        .map(|(a, b, kind)| {
            let (start, end) = (words[a].start, words[b - 1].end);
            MoneyToken {
                start: utf16(start),
                end: utf16(end),
                kind,
                text: input[start..end].to_owned(),
            }
        })
        .collect();
    raw
}

/// `12`, `12.5`, `12,50`, `+2400`, `-3`, `4€`, `€4.20` → (unsigned cents, explicit sign).
fn amount(word: &str) -> Option<(i64, Option<i64>)> {
    let w = word.trim_end_matches([',', ';', ':']);
    let (sign, w) = match w.chars().next()? {
        '+' => (Some(1), &w[1..]),
        '-' | '−' => (Some(-1), &w[w.chars().next()?.len_utf8()..]),
        _ => (None, w),
    };
    let w = w.trim_start_matches('€').trim_end_matches('€');
    let (units, frac) = match w.find(['.', ',']) {
        Some(p) => (&w[..p], &w[p + 1..]),
        None => (w, ""),
    };
    let digits = |s: &str| s.bytes().all(|b| b.is_ascii_digit());
    if units.is_empty() || units.len() > 9 || !digits(units) || frac.len() > 2 || !digits(frac) {
        return None;
    }
    let frac_cents = match frac.len() {
        0 => 0,
        1 => frac.parse::<i64>().ok()? * 10,
        _ => frac.parse::<i64>().ok()?,
    };
    let value = units.parse::<i64>().ok()? * 100 + frac_cents;
    (value > 0).then_some((value, sign))
}

/// The id whose name matches `typed` exactly, else the first it starts.
fn pick(typed: &str, named: &[(String, String)]) -> Option<String> {
    let key = squash(typed);
    if key.is_empty() {
        return None;
    }
    let names: Vec<_> = named.iter().map(|(id, n)| (id, squash(n))).collect();
    names
        .iter()
        .find(|(_, n)| *n == key)
        .or_else(|| names.iter().find(|(_, n)| n.starts_with(&key)))
        .map(|(id, _)| (*id).clone())
}

/// Dates looking back from `today`: (words taken, date).
fn past_date_at(words: &[Word], i: usize, today: Date) -> Option<(usize, Date)> {
    let w = |k: usize| words.get(i + k).map(|w| w.norm.as_str()).unwrap_or("");
    match (w(0), w(1)) {
        ("today" | "tod" | "aujourd'hui" | "auj", _) => return Some((1, today)),
        ("yesterday" | "yday" | "hier", _) => return Some((1, today.add_days(-1))),
        ("avant-hier", _) => return Some((1, today.add_days(-2))),
        ("avant", "hier") => return Some((2, today.add_days(-2))),
        _ => {}
    }
    // "lundi" = the last Monday, today included; "last monday" / "lundi dernier" = before today.
    let before = |wd: u32, strict: bool| {
        let back = (today.weekday() + 7 - wd) % 7;
        today.add_days(-i64::from(if strict && back == 0 { 7 } else { back }))
    };
    if let ("last", Some(wd)) = (w(0), weekday(w(1))) {
        return Some((2, before(wd, true)));
    }
    if let Some(wd) = weekday(w(0)) {
        return Some(if matches!(w(1), "dernier" | "derniere") {
            (2, before(wd, true))
        } else {
            (1, before(wd, false))
        });
    }
    let year = |k: usize| {
        words
            .get(i + k)
            .and_then(|w| w.norm.parse::<i64>().ok())
            .filter(|y| (2000..=2100).contains(y))
    };
    let dated = |m: i64, d: i64, n: usize| match year(n) {
        Some(y) => Date::from_ymd(y, m, d).map(|date| (n + 1, date)),
        None => past(today, m, d).map(|date| (n, date)),
    };
    if let (Some(d), Some(m)) = (day_number(w(0)), month(w(1))) {
        return dated(m, d, 2);
    }
    if let (Some(m), Some(d)) = (month(w(0)), day_number(w(1))) {
        return dated(m, d, 2);
    }
    if matches!(w(0), "the" | "le")
        && let Some(d) = day_number(w(1))
    {
        let this = Date::from_ymd(today.year(), today.month(), d).filter(|t| *t <= today);
        let date = this.or_else(|| {
            let prev = today.add_months(-1);
            Date::from_ymd(prev.year(), prev.month(), d)
        })?;
        return Some((2, date));
    }
    numeric(w(0), today).map(|d| (1, d))
}

/// `2026-11-12`, `12/11`, `12.11.2026` (day first); without a year, the last such day.
fn numeric(word: &str, today: Date) -> Option<Date> {
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
        None => past(today, m, d),
    }
}

/// Day `d` of month `m` this year, or last year when that's still ahead.
fn past(today: Date, m: i64, d: i64) -> Option<Date> {
    let this = Date::from_ymd(today.year(), m, d)?;
    if this <= today {
        Some(this)
    } else {
        Date::from_ymd(today.year() - 1, m, d)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn amounts() {
        assert_eq!(amount("12"), Some((1200, None)));
        assert_eq!(amount("12.5"), Some((1250, None)));
        assert_eq!(amount("12,50"), Some((1250, None)));
        assert_eq!(amount("+2400"), Some((240_000, Some(1))));
        assert_eq!(amount("-3"), Some((300, Some(-1))));
        assert_eq!(amount("4€"), Some((400, None)));
        assert_eq!(amount("€4.20"), Some((420, None)));
        for w in ["12/11", "1.234", "abc", "0", "", "+", "1,2,3", "#12"] {
            assert_eq!(amount(w), None, "{w}");
        }
    }
}
