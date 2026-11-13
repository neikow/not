//! Budgeting: accounts, categories with a limit per period, transactions, recurring templates.
//! See `docs/budgeting.md`.
//!
//! Everything lives in root maps of the workspace doc, keyed by id, each value a nested map so
//! fields merge one by one. Maps rather than trees: transactions point at accounts and categories by
//! id, and undoing then redoing a tree create gives the node a new id, which would orphan them; a
//! map key comes back as it was. Order is an `order` number (new ones go last, moves take the
//! midpoint). Deletes are soft (`deleted_at`), like everywhere else.
//!
//! Nothing is projected into SQLite: a few thousand rows a year read straight from the doc in well
//! under a millisecond per hundred, and every figure is derived on demand.
//!
//! Amounts are cents, negative = money out. Dates are `YYYY-MM-DD`, floating (the user's day).

use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicU64, Ordering};

use loro::{Container, LoroMap, LoroValue, ValueOrContainer};

use crate::date::{Date, days_in_month};
use crate::parse::squash;
use crate::projection::{DELETED_AT, meta_i64, meta_str};
use crate::schedule::Repeat;
use crate::store::{Store, now_millis};
use crate::{Error, Result};

pub(crate) const SETTINGS: &str = "money_settings";
pub(crate) const ACCOUNTS: &str = "money_accounts";
pub(crate) const CATEGORIES: &str = "money_categories";
pub(crate) const TXNS: &str = "money_txns";
pub(crate) const RECURRING: &str = "money_recurring";

const PERIOD_START_DAY: &str = "period_start_day";
const CURRENCY: &str = "currency";
const SEEDED: &str = "seeded";

const NAME: &str = "name";
const KIND: &str = "kind";
const ORDER: &str = "order";
const OPENING_BALANCE: &str = "opening_balance";
const EMOJI: &str = "emoji";
const ROLLOVER: &str = "rollover";
const LIMITS: &str = "limits";
const ACCOUNT: &str = "account";
const TO_ACCOUNT: &str = "to_account";
const DATE: &str = "date";
const AMOUNT: &str = "amount";
const PAYEE: &str = "payee";
const LABEL: &str = "label";
const NOTE: &str = "note";
const CATEGORY: &str = "category";
const SPLITS: &str = "splits";
const SOURCE: &str = "source";
const MERGED_INTO: &str = "merged_into";
const RECURRING_ID: &str = "recurring";
const CREATED: &str = "created";
const REPEAT: &str = "repeat";
const NEXT_DUE: &str = "next_due";

/// The starter categories, created once per workspace. Fixed ids, so two devices seeding at the
/// same time end up with one set.
const STARTER: [(&str, &str, &str, CategoryKind); 10] = [
    ("cat-courses", "Courses", "🛒", CategoryKind::Expense),
    ("cat-resto", "Resto", "🍽️", CategoryKind::Expense),
    ("cat-transport", "Transport", "🚆", CategoryKind::Expense),
    ("cat-logement", "Logement", "🏠", CategoryKind::Expense),
    ("cat-abonnements", "Abonnements", "🔁", CategoryKind::Expense),
    ("cat-loisirs", "Loisirs", "🎉", CategoryKind::Expense),
    ("cat-sante", "Santé", "💊", CategoryKind::Expense),
    ("cat-shopping", "Shopping", "🛍️", CategoryKind::Expense),
    ("cat-revenus", "Revenus", "💶", CategoryKind::Income),
    ("cat-autre", "Autre", "📦", CategoryKind::Expense),
];

// ---- types ----------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AccountKind {
    #[default]
    Bank,
    Cash,
    Savings,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Account {
    pub id: String,
    pub name: String,
    pub kind: AccountKind,
    pub opening_balance: i64,
    /// As of the day the accounts were asked for.
    pub balance: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CategoryKind {
    #[default]
    Expense,
    Income,
}

/// What happens to a category's leftover (or overspend) at the end of a period.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Rollover {
    /// Each period starts from its limit.
    #[default]
    None,
    /// Unspent money carries over; overspending doesn't.
    Leftover,
    /// Both carry over: overspending lowers the next period's budget.
    Both,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Category {
    pub id: String,
    pub name: String,
    pub emoji: String,
    pub kind: CategoryKind,
    pub rollover: Rollover,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TxnKind {
    /// Money in or out of one account, counted in the budget.
    #[default]
    Entry,
    /// Money moved from `account` to `to_account`; `amount` is positive. Never budget.
    Transfer,
    /// Correction written by [`Store::set_balance`]. Moves the balance, never budget.
    Adjustment,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TxnSource {
    #[default]
    Manual,
    Import,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Split {
    pub category: String,
    /// Same sign as the transaction's amount.
    pub amount: i64,
}

/// What the user enters or edits.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TxnInput {
    pub kind: TxnKind,
    pub account: String,
    /// Transfers only.
    pub to_account: Option<String>,
    /// `YYYY-MM-DD`.
    pub date: String,
    pub amount: i64,
    pub payee: String,
    pub note: String,
    /// Entries only; ignored when `splits` is set.
    pub category: Option<String>,
    /// Entries only: two or more parts adding up to `amount`.
    pub splits: Vec<Split>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Txn {
    pub id: String,
    pub kind: TxnKind,
    pub account: String,
    pub to_account: Option<String>,
    pub date: String,
    pub amount: i64,
    pub payee: String,
    /// Raw bank text, for imported rows.
    pub label: Option<String>,
    pub note: String,
    pub category: Option<String>,
    pub splits: Vec<Split>,
    pub source: TxnSource,
    /// Template this was entered from.
    pub recurring: Option<String>,
    /// Unix millis; orders rows of the same day.
    pub created: i64,
}

impl Txn {
    /// An entry nobody has given a category yet.
    pub fn needs_review(&self) -> bool {
        self.kind == TxnKind::Entry && self.category.is_none() && self.splits.is_empty()
    }

    /// (category, amount) parts this row counts towards; `None` = no category.
    fn parts(&self) -> Vec<(Option<&str>, i64)> {
        if self.splits.is_empty() {
            vec![(self.category.as_deref(), self.amount)]
        } else {
            self.splits
                .iter()
                .map(|s| (Some(s.category.as_str()), s.amount))
                .collect()
        }
    }
}

/// Which transactions to list. Empty fields don't filter.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TxnFilter {
    /// Rows on this account, transfers to or from it included.
    pub account: Option<String>,
    /// Rows (or split parts) in this category.
    pub category: Option<String>,
    /// Inclusive `YYYY-MM-DD` bounds.
    pub from: Option<String>,
    pub to: Option<String>,
    /// Only rows that [`Txn::needs_review`].
    pub to_review: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MoneySettings {
    /// The day of the month each period starts on (1–31, clamped to short months).
    pub period_start_day: u32,
    pub currency: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CategoryBudget {
    pub category: Category,
    /// This period's limit; 0 = none set. Income categories have none.
    pub limit: i64,
    /// Brought over from the previous period by the rollover mode (negative after overspending).
    pub carried: i64,
    /// Expense categories: money out, net of refunds. Income categories: money in.
    pub spent: i64,
    /// `limit + carried − spent`; 0 for income categories.
    pub left: i64,
    pub count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Budget {
    /// First day, `YYYY-MM-DD`.
    pub start: String,
    /// Last day (inclusive).
    pub end: String,
    /// Days from the given date to the end, both counted; 0 when the date is outside the period.
    pub days_left: u32,
    pub expenses: Vec<CategoryBudget>,
    pub income: Vec<CategoryBudget>,
    /// Out and in of rows with no category.
    pub unsorted_spent: i64,
    pub unsorted_income: i64,
    pub to_review: u32,
    /// Σ (limit + carried) over expense categories.
    pub available: i64,
    /// All money out of the budget: expense categories plus unsorted.
    pub spent: i64,
    /// `available − spent`: the "312 € left" figure.
    pub left: i64,
    /// All money in: income categories plus unsorted income.
    pub income_total: i64,
    /// `income_total − spent`.
    pub net: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CategoryAmount {
    pub category: String,
    pub amount: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeriodReport {
    pub start: String,
    pub end: String,
    pub income: i64,
    pub spent: i64,
    pub net: i64,
    /// Per expense category with any spending, as in [`CategoryBudget::spent`].
    pub categories: Vec<CategoryAmount>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recurring {
    pub id: String,
    pub name: String,
    /// Usual amount, signed; entering it can change it.
    pub amount: i64,
    pub account: String,
    pub category: Option<String>,
    /// RRULE subset, see [`Repeat`].
    pub repeat: String,
    /// Next date it is due, `YYYY-MM-DD`.
    pub next_due: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecurringInput {
    pub name: String,
    pub amount: i64,
    pub account: String,
    pub category: Option<String>,
    pub repeat: String,
    /// First (or next) due date.
    pub next_due: String,
}

// ---- periods --------------------------------------------------------------------------------

/// The period `date` falls in, for periods starting on `start_day`: `(first day, first day of the
/// next period)`. A start day past the end of a month starts that month's period on its last day.
pub fn period_of(date: Date, start_day: u32) -> (Date, Date) {
    let start_in = |y: i64, m: i64| {
        Date::from_ymd(y, m, i64::from(start_day).clamp(1, days_in_month(y, m))).expect("valid day")
    };
    let this = start_in(date.year(), date.month());
    let start = if date >= this {
        this
    } else {
        let prev = Date::from_ymd(date.year(), date.month(), 1)
            .expect("first")
            .add_days(-1);
        start_in(prev.year(), prev.month())
    };
    let after = Date::from_ymd(start.year(), start.month(), 1)
        .expect("first")
        .add_months(1);
    (start, start_in(after.year(), after.month()))
}

// ---- store ----------------------------------------------------------------------------------

impl Store {
    pub fn money_settings(&self) -> MoneySettings {
        let s = &self.money.settings;
        MoneySettings {
            period_start_day: meta_i64(s, PERIOD_START_DAY).map_or(1, |d| d.clamp(1, 31) as u32),
            currency: meta_str(s, CURRENCY).unwrap_or_else(|| "EUR".into()),
        }
    }

    pub fn set_period_start_day(&self, day: u32) -> Result<()> {
        if !(1..=31).contains(&day) {
            return Err(Error::Invalid("the period starts on a day from 1 to 31".into()));
        }
        self.transact(|| Ok(self.money.settings.insert(PERIOD_START_DAY, i64::from(day))?))
    }

    /// The period `date` (`YYYY-MM-DD`) falls in, as `(first day, last day)`.
    pub fn money_period(&self, date: &str) -> Result<(String, String)> {
        let (start, next) = period_of(Date::parse(date)?, self.money_settings().period_start_day);
        Ok((start.to_string(), next.add_days(-1).to_string()))
    }

    /// Creates the starter categories the first time; later calls do nothing, even after the user
    /// deleted them all. Returns whether it created them.
    pub fn seed_money(&self) -> Result<bool> {
        if self.money.settings.get(SEEDED).is_some() {
            return Ok(false);
        }
        self.transact(|| {
            for (i, (id, name, emoji, kind)) in STARTER.iter().enumerate() {
                if self.money.categories.get(id).is_some() {
                    continue;
                }
                // Mergeable: two devices seeding at once share one map per category.
                let m = self.money.categories.ensure_mergeable_map(id)?;
                m.insert(NAME, *name)?;
                m.insert(EMOJI, *emoji)?;
                m.insert(KIND, kind_str(*kind))?;
                m.insert(ORDER, i as f64)?;
            }
            self.money.settings.insert(SEEDED, true)?;
            Ok(true)
        })
    }

    // ---- accounts --------------------------------------------------------------------------

    pub fn create_account(&self, name: &str, kind: AccountKind, opening_balance: i64) -> Result<String> {
        let name = clean_name(name)?;
        self.transact(|| {
            let id = self.new_money_id("acc");
            let m = self.money.accounts.insert_container(&id, LoroMap::new())?;
            m.insert(NAME, name)?;
            m.insert(KIND, account_kind_str(kind))?;
            m.insert(OPENING_BALANCE, opening_balance)?;
            m.insert(ORDER, next_order(&self.money.accounts))?;
            Ok(id)
        })
    }

    pub fn update_account(
        &self,
        id: &str,
        name: &str,
        kind: AccountKind,
        opening_balance: i64,
    ) -> Result<()> {
        let name = clean_name(name)?;
        let m = self.live(&self.money.accounts, id)?;
        self.transact(|| {
            set_str(&m, NAME, &name)?;
            set_str(&m, KIND, account_kind_str(kind))?;
            set_i64(&m, OPENING_BALANCE, opening_balance)?;
            Ok(())
        })
    }

    /// Hides the account; its transactions stay (and stop showing), so undo brings it back whole.
    pub fn delete_account(&self, id: &str) -> Result<()> {
        let m = self.live(&self.money.accounts, id)?;
        self.transact(|| Ok(m.insert(DELETED_AT, now_millis())?))
    }

    pub fn move_account(&self, id: &str, index: usize) -> Result<()> {
        self.move_in(&self.money.accounts, id, index)
    }

    /// Accounts in user order, with balances as of `date`.
    pub fn accounts(&self, date: &str) -> Result<Vec<Account>> {
        let date = Date::parse(date)?;
        let txns = self.all_txns();
        Ok(ordered(&self.money.accounts)
            .into_iter()
            .map(|(id, m)| {
                let opening_balance = meta_i64(&m, OPENING_BALANCE).unwrap_or(0);
                Account {
                    balance: opening_balance + movement(&txns, &id, date),
                    name: meta_str(&m, NAME).unwrap_or_default(),
                    kind: parse_account_kind(&meta_str(&m, KIND).unwrap_or_default()),
                    opening_balance,
                    id,
                }
            })
            .collect())
    }

    /// Makes the account's balance on `date` read `balance` by adding an adjustment that day.
    /// Returns its id, or `None` when the balance was already right.
    pub fn set_balance(&self, account: &str, balance: i64, date: &str) -> Result<Option<String>> {
        let current = self
            .accounts(date)?
            .into_iter()
            .find(|a| a.id == account)
            .ok_or_else(|| Error::NotFound(account.to_owned()))?
            .balance;
        if current == balance {
            return Ok(None);
        }
        self.add_txn(&TxnInput {
            kind: TxnKind::Adjustment,
            account: account.to_owned(),
            date: date.to_owned(),
            amount: balance - current,
            ..TxnInput::default()
        })
        .map(Some)
    }

    // ---- categories ------------------------------------------------------------------------

    pub fn create_category(&self, name: &str, emoji: &str, kind: CategoryKind) -> Result<String> {
        let name = clean_name(name)?;
        self.transact(|| {
            let id = self.new_money_id("cat");
            let m = self.money.categories.insert_container(&id, LoroMap::new())?;
            m.insert(NAME, name)?;
            m.insert(EMOJI, emoji.trim())?;
            m.insert(KIND, kind_str(kind))?;
            m.insert(ORDER, next_order(&self.money.categories))?;
            Ok(id)
        })
    }

    pub fn update_category(
        &self,
        id: &str,
        name: &str,
        emoji: &str,
        kind: CategoryKind,
        rollover: Rollover,
    ) -> Result<()> {
        let name = clean_name(name)?;
        let m = self.live(&self.money.categories, id)?;
        self.transact(|| {
            set_str(&m, NAME, &name)?;
            set_str(&m, EMOJI, emoji.trim())?;
            set_str(&m, KIND, kind_str(kind))?;
            set_str(&m, ROLLOVER, rollover_str(rollover))?;
            Ok(())
        })
    }

    /// Hides the category. Its rows count as unsorted until it is restored (undo).
    pub fn delete_category(&self, id: &str) -> Result<()> {
        let m = self.live(&self.money.categories, id)?;
        self.transact(|| Ok(m.insert(DELETED_AT, now_millis())?))
    }

    pub fn move_category(&self, id: &str, index: usize) -> Result<()> {
        self.move_in(&self.money.categories, id, index)
    }

    pub fn categories(&self) -> Vec<Category> {
        ordered(&self.money.categories)
            .into_iter()
            .map(|(id, m)| read_category(id, &m))
            .collect()
    }

    /// Sets the limit from the period containing `date` on (0 = no limit). Later periods' limits
    /// are dropped, so it really applies from here on; earlier periods keep theirs.
    pub fn set_category_limit(&self, id: &str, date: &str, limit: i64) -> Result<()> {
        if limit < 0 {
            return Err(Error::Invalid("a limit can't be negative".into()));
        }
        let m = self.live(&self.money.categories, id)?;
        let (start, _) = period_of(Date::parse(date)?, self.money_settings().period_start_day);
        self.transact(|| {
            let limits = m.ensure_mergeable_map(LIMITS)?;
            let start = start.to_string();
            for key in limits.keys().map(|k| k.to_string()).collect::<Vec<_>>() {
                if key > start {
                    limits.delete(&key)?;
                }
            }
            set_i64(&limits, &start, limit)?;
            Ok(())
        })
    }

    // ---- transactions ----------------------------------------------------------------------

    pub fn add_txn(&self, input: &TxnInput) -> Result<String> {
        self.check_txn(input)?;
        self.transact(|| {
            let id = self.new_money_id("tx");
            let m = self.money.txns.insert_container(&id, LoroMap::new())?;
            write_txn(&m, input, true)?;
            m.insert(SOURCE, "manual")?;
            m.insert(CREATED, now_millis())?;
            Ok(id)
        })
    }

    /// Writes only the fields that changed, so another device's edit of a different field survives.
    pub fn update_txn(&self, id: &str, input: &TxnInput) -> Result<()> {
        self.check_txn(input)?;
        let m = self.live(&self.money.txns, id)?;
        self.transact(|| write_txn(&m, input, false))
    }

    /// Sets just the category (clearing any split), e.g. from To review.
    pub fn set_txn_category(&self, id: &str, category: Option<&str>) -> Result<()> {
        let txn = self.txn(id)?;
        self.update_txn(
            id,
            &TxnInput {
                category: category.map(str::to_owned),
                splits: Vec::new(),
                ..txn_input(&txn)
            },
        )
    }

    pub fn delete_txn(&self, id: &str) -> Result<()> {
        let m = self.live(&self.money.txns, id)?;
        self.transact(|| Ok(m.insert(DELETED_AT, now_millis())?))
    }

    pub fn txn(&self, id: &str) -> Result<Txn> {
        let m = self.live(&self.money.txns, id)?;
        Ok(read_txn(id.to_owned(), &m))
    }

    /// Matching rows, newest first (by date, then by when they were entered).
    pub fn transactions(&self, filter: &TxnFilter) -> Result<Vec<Txn>> {
        let from = filter
            .from
            .as_deref()
            .map(Date::parse)
            .transpose()?
            .map(|d| d.to_string());
        let to = filter
            .to
            .as_deref()
            .map(Date::parse)
            .transpose()?
            .map(|d| d.to_string());
        let mut rows: Vec<Txn> = self
            .all_txns()
            .into_iter()
            .filter(|t| {
                filter
                    .account
                    .as_ref()
                    .is_none_or(|a| t.account == *a || t.to_account.as_ref() == Some(a))
                    && filter
                        .category
                        .as_ref()
                        .is_none_or(|c| t.parts().iter().any(|(pc, _)| *pc == Some(c.as_str())))
                    && from.as_ref().is_none_or(|f| t.date >= *f)
                    && to.as_ref().is_none_or(|e| t.date <= *e)
                    && (!filter.to_review || t.needs_review())
            })
            .collect();
        rows.sort_by(|a, b| {
            b.date
                .cmp(&a.date)
                .then(b.created.cmp(&a.created))
                .then(b.id.cmp(&a.id))
        });
        Ok(rows)
    }

    /// The category the payee's most recent categorised entry had, for a new entry.
    pub fn category_for_payee(&self, payee: &str) -> Option<String> {
        let key = squash(payee);
        if key.is_empty() {
            return None;
        }
        let live: Vec<String> = self.categories().into_iter().map(|c| c.id).collect();
        self.all_txns()
            .into_iter()
            .filter(|t| t.kind == TxnKind::Entry && squash(&t.payee) == key)
            .filter_map(|t| {
                t.category
                    .clone()
                    .filter(|c| live.contains(c))
                    .map(|c| (t.date, t.created, c))
            })
            .max()
            .map(|(_, _, c)| c)
    }

    /// The account of the most recently entered row, the default for a new one.
    pub fn last_used_account(&self) -> Option<String> {
        let live: Vec<String> = ordered(&self.money.accounts)
            .into_iter()
            .map(|(id, _)| id)
            .collect();
        self.all_txns()
            .into_iter()
            .filter(|t| t.source == TxnSource::Manual && live.contains(&t.account))
            .max_by_key(|t| t.created)
            .map(|t| t.account)
            .or_else(|| live.into_iter().next())
    }

    // ---- budget and reports ----------------------------------------------------------------

    /// The budget of the period containing `date`.
    pub fn budget(&self, date: &str) -> Result<Budget> {
        let date = Date::parse(date)?;
        let start_day = self.money_settings().period_start_day;
        let (start, next) = period_of(date, start_day);
        let ledger = Ledger::new(self.all_txns(), self.categories(), start_day);

        let mut b = Budget {
            start: start.to_string(),
            end: next.add_days(-1).to_string(),
            days_left: if date < next {
                next.days_since(date) as u32
            } else {
                0
            },
            expenses: Vec::new(),
            income: Vec::new(),
            unsorted_spent: 0,
            unsorted_income: 0,
            to_review: 0,
            available: 0,
            spent: 0,
            left: 0,
            income_total: 0,
            net: 0,
        };
        for category in &ledger.categories {
            let m = self.money.categories.get(&category.id).and_then(as_map);
            let limits = m.as_ref().map(read_limits).unwrap_or_default();
            let (spent, count) = ledger.total(Some(&category.id), start);
            match category.kind {
                CategoryKind::Income => {
                    b.income_total += spent;
                    b.income.push(CategoryBudget {
                        category: category.clone(),
                        limit: 0,
                        carried: 0,
                        spent,
                        left: 0,
                        count,
                    });
                }
                CategoryKind::Expense => {
                    let limit = limit_at(&limits, start);
                    let carried = ledger.carried(category, &limits, start);
                    b.available += limit + carried;
                    b.spent += spent;
                    b.expenses.push(CategoryBudget {
                        category: category.clone(),
                        limit,
                        carried,
                        spent,
                        left: limit + carried - spent,
                        count,
                    });
                }
            }
        }
        for t in ledger.in_period(start) {
            for (category, amount) in t.parts() {
                if category.is_none_or(|c| !ledger.is_live(c)) {
                    if amount < 0 {
                        b.unsorted_spent -= amount;
                    } else {
                        b.unsorted_income += amount;
                    }
                }
            }
            if t.needs_review() {
                b.to_review += 1;
            }
        }
        b.spent += b.unsorted_spent;
        b.income_total += b.unsorted_income;
        b.left = b.available - b.spent;
        b.net = b.income_total - b.spent;
        Ok(b)
    }

    /// The `count` periods up to and including the one containing `date`, oldest first.
    pub fn money_reports(&self, date: &str, count: u32) -> Result<Vec<PeriodReport>> {
        let date = Date::parse(date)?;
        let start_day = self.money_settings().period_start_day;
        let ledger = Ledger::new(self.all_txns(), self.categories(), start_day);
        let mut starts = vec![period_of(date, start_day).0];
        while starts.len() < count as usize {
            let earlier = period_of(starts.last().expect("one").add_days(-1), start_day).0;
            starts.push(earlier);
        }
        starts.reverse();
        Ok(starts
            .into_iter()
            .map(|start| {
                let next = period_of(start, start_day).1;
                let (mut income, mut spent) = (0, 0);
                let mut categories = Vec::new();
                for c in &ledger.categories {
                    let (amount, _) = ledger.total(Some(&c.id), start);
                    match c.kind {
                        CategoryKind::Income => income += amount,
                        CategoryKind::Expense => {
                            spent += amount;
                            if amount != 0 {
                                categories.push(CategoryAmount {
                                    category: c.id.clone(),
                                    amount,
                                });
                            }
                        }
                    }
                }
                for t in ledger.in_period(start) {
                    for (category, amount) in t.parts() {
                        if category.is_none_or(|c| !ledger.is_live(c)) {
                            if amount < 0 {
                                spent -= amount;
                            } else {
                                income += amount;
                            }
                        }
                    }
                }
                PeriodReport {
                    start: start.to_string(),
                    end: next.add_days(-1).to_string(),
                    income,
                    spent,
                    net: income - spent,
                    categories,
                }
            })
            .collect())
    }

    // ---- recurring -------------------------------------------------------------------------

    pub fn create_recurring(&self, input: &RecurringInput) -> Result<String> {
        self.check_recurring(input)?;
        self.transact(|| {
            let id = self.new_money_id("rec");
            let m = self.money.recurring.insert_container(&id, LoroMap::new())?;
            write_recurring(&m, input)?;
            Ok(id)
        })
    }

    pub fn update_recurring(&self, id: &str, input: &RecurringInput) -> Result<()> {
        self.check_recurring(input)?;
        let m = self.live(&self.money.recurring, id)?;
        self.transact(|| write_recurring(&m, input))
    }

    pub fn delete_recurring(&self, id: &str) -> Result<()> {
        let m = self.live(&self.money.recurring, id)?;
        self.transact(|| Ok(m.insert(DELETED_AT, now_millis())?))
    }

    /// All templates, soonest due first.
    pub fn recurring(&self) -> Vec<Recurring> {
        let mut all: Vec<Recurring> = live_entries(&self.money.recurring)
            .into_iter()
            .map(|(id, m)| Recurring {
                id,
                name: meta_str(&m, NAME).unwrap_or_default(),
                amount: meta_i64(&m, AMOUNT).unwrap_or(0),
                account: meta_str(&m, ACCOUNT).unwrap_or_default(),
                category: meta_str(&m, CATEGORY).filter(|c| !c.is_empty()),
                repeat: meta_str(&m, REPEAT).unwrap_or_default(),
                next_due: meta_str(&m, NEXT_DUE).unwrap_or_default(),
            })
            .collect();
        all.sort_by(|a, b| a.next_due.cmp(&b.next_due).then(a.name.cmp(&b.name)));
        all
    }

    /// Templates due on or before `today`.
    pub fn due_recurring(&self, today: &str) -> Result<Vec<Recurring>> {
        let today = Date::parse(today)?.to_string();
        Ok(self
            .recurring()
            .into_iter()
            .filter(|r| r.next_due <= today)
            .collect())
    }

    /// Enters the template's current occurrence (on its due date, for `amount`, or its usual
    /// amount) and moves it to the next one, in one undo step. Returns the new row's id.
    pub fn enter_recurring(&self, id: &str, amount: Option<i64>) -> Result<String> {
        let m = self.live(&self.money.recurring, id)?;
        let r = self
            .recurring()
            .into_iter()
            .find(|r| r.id == id)
            .ok_or_else(|| Error::NotFound(id.into()))?;
        let due = Date::parse(&r.next_due)?;
        let next = Repeat::parse_rrule(&r.repeat)?.next_after(due);
        let input = TxnInput {
            kind: TxnKind::Entry,
            account: r.account.clone(),
            date: r.next_due.clone(),
            amount: amount.unwrap_or(r.amount),
            payee: r.name.clone(),
            category: r.category.clone(),
            ..TxnInput::default()
        };
        self.check_txn(&input)?;
        self.transact(|| {
            let tx = self.new_money_id("tx");
            let t = self.money.txns.insert_container(&tx, LoroMap::new())?;
            write_txn(&t, &input, true)?;
            t.insert(SOURCE, "manual")?;
            t.insert(RECURRING_ID, id)?;
            t.insert(CREATED, now_millis())?;
            m.insert(NEXT_DUE, next.to_string())?;
            Ok(tx)
        })
    }

    /// Moves the template past its current occurrence without entering anything.
    pub fn skip_recurring(&self, id: &str) -> Result<()> {
        let m = self.live(&self.money.recurring, id)?;
        let r = self
            .recurring()
            .into_iter()
            .find(|r| r.id == id)
            .ok_or_else(|| Error::NotFound(id.into()))?;
        let next = Repeat::parse_rrule(&r.repeat)?.next_after(Date::parse(&r.next_due)?);
        self.transact(|| Ok(m.insert(NEXT_DUE, next.to_string())?))
    }

    // ---- internals -------------------------------------------------------------------------

    fn new_money_id(&self, prefix: &str) -> String {
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let seq = SEQ.fetch_add(1, Ordering::Relaxed);
        format!("{prefix}-{:x}-{:x}-{seq:x}", self.doc.peer_id(), now_millis())
    }

    /// The nested map of a live (not deleted) entry.
    fn live(&self, map: &LoroMap, id: &str) -> Result<LoroMap> {
        map.get(id)
            .and_then(as_map)
            .filter(|m| m.get(DELETED_AT).is_none())
            .ok_or_else(|| Error::NotFound(id.to_owned()))
    }

    fn all_txns(&self) -> Vec<Txn> {
        live_entries(&self.money.txns)
            .into_iter()
            .filter(|(_, m)| m.get(MERGED_INTO).is_none())
            .map(|(id, m)| read_txn(id, &m))
            .collect()
    }

    fn check_txn(&self, input: &TxnInput) -> Result<()> {
        Date::parse(&input.date)?;
        let account_ok = |id: &str| self.live(&self.money.accounts, id).is_ok();
        if !account_ok(&input.account) {
            return Err(Error::NotFound(input.account.clone()));
        }
        if input.amount == 0 {
            return Err(Error::Invalid("the amount is zero".into()));
        }
        match input.kind {
            TxnKind::Transfer => {
                let to = input
                    .to_account
                    .as_deref()
                    .ok_or_else(|| Error::Invalid("a transfer needs an account to move to".into()))?;
                if !account_ok(to) {
                    return Err(Error::NotFound(to.to_owned()));
                }
                if to == input.account {
                    return Err(Error::Invalid("a transfer needs two different accounts".into()));
                }
                if input.amount < 0 {
                    return Err(Error::Invalid("a transfer moves a positive amount".into()));
                }
            }
            TxnKind::Entry => {
                let category_ok = |id: &str| self.live(&self.money.categories, id).is_ok();
                if let Some(c) = &input.category
                    && input.splits.is_empty()
                    && !category_ok(c)
                {
                    return Err(Error::NotFound(c.clone()));
                }
                if !input.splits.is_empty() {
                    if input.splits.len() < 2 {
                        return Err(Error::Invalid("a split needs at least two parts".into()));
                    }
                    if let Some(s) = input.splits.iter().find(|s| !category_ok(&s.category)) {
                        return Err(Error::NotFound(s.category.clone()));
                    }
                    if input
                        .splits
                        .iter()
                        .any(|s| s.amount == 0 || s.amount.signum() != input.amount.signum())
                    {
                        return Err(Error::Invalid(
                            "each part of a split goes the same way as the total".into(),
                        ));
                    }
                    if input.splits.iter().map(|s| s.amount).sum::<i64>() != input.amount {
                        return Err(Error::Invalid(
                            "the parts of a split don't add up to the total".into(),
                        ));
                    }
                }
            }
            TxnKind::Adjustment => {}
        }
        Ok(())
    }

    fn check_recurring(&self, input: &RecurringInput) -> Result<()> {
        clean_name(&input.name)?;
        Date::parse(&input.next_due)?;
        Repeat::parse_rrule(&input.repeat)?;
        if input.amount == 0 {
            return Err(Error::Invalid("the amount is zero".into()));
        }
        self.live(&self.money.accounts, &input.account)?;
        if let Some(c) = &input.category {
            self.live(&self.money.categories, c)?;
        }
        Ok(())
    }

    fn move_in(&self, map: &LoroMap, id: &str, index: usize) -> Result<()> {
        let m = self.live(map, id)?;
        let others: Vec<f64> = ordered(map)
            .into_iter()
            .filter(|(other, _)| other != id)
            .map(|(_, m)| order_of(&m))
            .collect();
        let order = match (
            index.checked_sub(1).and_then(|i| others.get(i)),
            others.get(index),
        ) {
            (None, None) => 0.0,
            (None, Some(&next)) => next - 1.0,
            (Some(&prev), None) => prev + 1.0,
            (Some(&prev), Some(&next)) => (prev + next) / 2.0,
        };
        self.transact(|| Ok(m.insert(ORDER, order)?))
    }
}

/// The containers, resolved once when the store opens.
pub(crate) struct Money {
    pub(crate) settings: LoroMap,
    pub(crate) accounts: LoroMap,
    pub(crate) categories: LoroMap,
    pub(crate) txns: LoroMap,
    pub(crate) recurring: LoroMap,
}

impl Money {
    pub(crate) fn new(doc: &loro::LoroDoc) -> Self {
        Self {
            settings: doc.get_map(SETTINGS),
            accounts: doc.get_map(ACCOUNTS),
            categories: doc.get_map(CATEGORIES),
            txns: doc.get_map(TXNS),
            recurring: doc.get_map(RECURRING),
        }
    }
}

// ---- ledger: per-category, per-period totals ------------------------------------------------

struct Ledger {
    /// Budget rows (entries) by period start.
    by_period: BTreeMap<Date, Vec<Txn>>,
    categories: Vec<Category>,
    start_day: u32,
}

impl Ledger {
    fn new(txns: Vec<Txn>, categories: Vec<Category>, start_day: u32) -> Self {
        let mut by_period: BTreeMap<Date, Vec<Txn>> = BTreeMap::new();
        for t in txns.into_iter().filter(|t| t.kind == TxnKind::Entry) {
            if let Ok(d) = Date::parse(&t.date) {
                by_period.entry(period_of(d, start_day).0).or_default().push(t);
            }
        }
        Self {
            by_period,
            categories,
            start_day,
        }
    }

    fn is_live(&self, id: &str) -> bool {
        self.categories.iter().any(|c| c.id == id)
    }

    fn in_period(&self, start: Date) -> &[Txn] {
        self.by_period.get(&start).map_or(&[], Vec::as_slice)
    }

    /// (spent or received, rows) for the category in the period starting `start`: expense
    /// categories count money out (refunds subtract), income ones money in.
    fn total(&self, category: Option<&str>, start: Date) -> (i64, u32) {
        let kind = category
            .and_then(|c| self.categories.iter().find(|x| x.id == c))
            .map_or(CategoryKind::Expense, |c| c.kind);
        let (mut sum, mut count) = (0, 0);
        for t in self.in_period(start) {
            let mut hit = false;
            for (c, amount) in t.parts() {
                if c == category {
                    sum += amount;
                    hit = true;
                }
            }
            count += u32::from(hit);
        }
        (if kind == CategoryKind::Expense { -sum } else { sum }, count)
    }

    /// What the rollover mode brings into the period starting `start`, walking from the first
    /// period that has a limit or any row in the category.
    fn carried(&self, category: &Category, limits: &BTreeMap<Date, i64>, start: Date) -> i64 {
        if category.rollover == Rollover::None {
            return 0;
        }
        let first_row = self
            .by_period
            .iter()
            .find(|(_, rows)| {
                rows.iter()
                    .any(|t| t.parts().iter().any(|(c, _)| *c == Some(category.id.as_str())))
            })
            .map(|(p, _)| *p);
        let first_limit = limits.keys().next().map(|d| period_of(*d, self.start_day).0);
        let Some(mut p) = [first_row, first_limit].into_iter().flatten().min() else {
            return 0;
        };
        let mut carry = 0;
        while p < start {
            let left = limit_at(limits, p) + carry - self.total(Some(&category.id), p).0;
            carry = match category.rollover {
                Rollover::Leftover => left.max(0),
                _ => left,
            };
            p = period_of(p, self.start_day).1;
        }
        carry
    }
}

/// The limit in force for the period starting `start`: the latest one set at or before it.
fn limit_at(limits: &BTreeMap<Date, i64>, start: Date) -> i64 {
    limits.range(..=start).next_back().map_or(0, |(_, &l)| l)
}

fn read_limits(m: &LoroMap) -> BTreeMap<Date, i64> {
    let Some(limits) = m.get(LIMITS).and_then(as_map) else {
        return BTreeMap::new();
    };
    limits
        .keys()
        .filter_map(|k| {
            let date = Date::parse(&k).ok()?;
            Some((date, meta_i64(&limits, &k)?))
        })
        .collect()
}

/// Net change of the account's balance from rows dated up to `date`.
fn movement(txns: &[Txn], account: &str, date: Date) -> i64 {
    let date = date.to_string();
    txns.iter()
        .filter(|t| t.date <= date)
        .map(|t| match t.kind {
            TxnKind::Transfer if t.account == account => -t.amount,
            TxnKind::Transfer if t.to_account.as_deref() == Some(account) => t.amount,
            TxnKind::Transfer => 0,
            _ if t.account == account => t.amount,
            _ => 0,
        })
        .sum()
}

// ---- reading and writing maps ---------------------------------------------------------------

pub(crate) fn as_map(v: ValueOrContainer) -> Option<LoroMap> {
    match v {
        ValueOrContainer::Container(Container::Map(m)) => Some(m),
        _ => None,
    }
}

fn live_entries(map: &LoroMap) -> Vec<(String, LoroMap)> {
    map.keys()
        .filter_map(|k| {
            let m = map.get(&k).and_then(as_map)?;
            m.get(DELETED_AT).is_none().then(|| (k.to_string(), m))
        })
        .collect()
}

fn order_of(m: &LoroMap) -> f64 {
    match m.get(ORDER) {
        Some(ValueOrContainer::Value(LoroValue::Double(d))) => d,
        Some(ValueOrContainer::Value(LoroValue::I64(i))) => i as f64,
        _ => 0.0,
    }
}

/// Live entries by `order`, ties by id (two devices adding at once).
fn ordered(map: &LoroMap) -> Vec<(String, LoroMap)> {
    let mut all = live_entries(map);
    all.sort_by(|(a, ma), (b, mb)| order_of(ma).total_cmp(&order_of(mb)).then(a.cmp(b)));
    all
}

fn next_order(map: &LoroMap) -> f64 {
    ordered(map).last().map_or(0.0, |(_, m)| order_of(m) + 1.0)
}

fn set_str(m: &LoroMap, key: &str, value: &str) -> Result<()> {
    if meta_str(m, key).as_deref() != Some(value) {
        m.insert(key, value)?;
    }
    Ok(())
}

fn set_i64(m: &LoroMap, key: &str, value: i64) -> Result<()> {
    if meta_i64(m, key) != Some(value) {
        m.insert(key, value)?;
    }
    Ok(())
}

fn set_opt_str(m: &LoroMap, key: &str, value: Option<&str>) -> Result<()> {
    match value {
        Some(v) => set_str(m, key, v),
        None if m.get(key).is_some() => Ok(m.delete(key)?),
        None => Ok(()),
    }
}

fn clean_name(name: &str) -> Result<String> {
    let name = name.trim();
    if name.is_empty() {
        return Err(Error::Invalid("the name is empty".into()));
    }
    Ok(name.to_owned())
}

fn read_category(id: String, m: &LoroMap) -> Category {
    Category {
        id,
        name: meta_str(m, NAME).unwrap_or_default(),
        emoji: meta_str(m, EMOJI).unwrap_or_default(),
        kind: match meta_str(m, KIND).as_deref() {
            Some("income") => CategoryKind::Income,
            _ => CategoryKind::Expense,
        },
        rollover: match meta_str(m, ROLLOVER).as_deref() {
            Some("leftover") => Rollover::Leftover,
            Some("both") => Rollover::Both,
            _ => Rollover::None,
        },
    }
}

fn read_txn(id: String, m: &LoroMap) -> Txn {
    let splits = match m.get(SPLITS) {
        Some(ValueOrContainer::Value(LoroValue::List(parts))) => parts
            .iter()
            .filter_map(|p| match p {
                LoroValue::Map(part) => Some(Split {
                    category: match part.get(CATEGORY)? {
                        LoroValue::String(s) => s.to_string(),
                        _ => return None,
                    },
                    amount: match part.get(AMOUNT)? {
                        LoroValue::I64(a) => *a,
                        _ => return None,
                    },
                }),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    };
    Txn {
        id,
        kind: match meta_str(m, KIND).as_deref() {
            Some("transfer") => TxnKind::Transfer,
            Some("adjustment") => TxnKind::Adjustment,
            _ => TxnKind::Entry,
        },
        account: meta_str(m, ACCOUNT).unwrap_or_default(),
        to_account: meta_str(m, TO_ACCOUNT),
        date: meta_str(m, DATE).unwrap_or_default(),
        amount: meta_i64(m, AMOUNT).unwrap_or(0),
        payee: meta_str(m, PAYEE).unwrap_or_default(),
        label: meta_str(m, LABEL),
        note: meta_str(m, NOTE).unwrap_or_default(),
        category: meta_str(m, CATEGORY),
        splits,
        source: match meta_str(m, SOURCE).as_deref() {
            Some("import") => TxnSource::Import,
            _ => TxnSource::Manual,
        },
        recurring: meta_str(m, RECURRING_ID),
        created: meta_i64(m, CREATED).unwrap_or(0),
    }
}

fn txn_input(t: &Txn) -> TxnInput {
    TxnInput {
        kind: t.kind,
        account: t.account.clone(),
        to_account: t.to_account.clone(),
        date: t.date.clone(),
        amount: t.amount,
        payee: t.payee.clone(),
        note: t.note.clone(),
        category: t.category.clone(),
        splits: t.splits.clone(),
    }
}

fn write_txn(m: &LoroMap, input: &TxnInput, new: bool) -> Result<()> {
    let is_entry = input.kind == TxnKind::Entry;
    set_str(m, KIND, txn_kind_str(input.kind))?;
    set_str(m, ACCOUNT, &input.account)?;
    set_opt_str(
        m,
        TO_ACCOUNT,
        input
            .to_account
            .as_deref()
            .filter(|_| input.kind == TxnKind::Transfer),
    )?;
    set_str(m, DATE, &Date::parse(&input.date)?.to_string())?;
    set_i64(m, AMOUNT, input.amount)?;
    if !new || !input.payee.trim().is_empty() {
        set_str(m, PAYEE, input.payee.trim())?;
    }
    if !new || !input.note.trim().is_empty() {
        set_str(m, NOTE, input.note.trim())?;
    }
    let split = is_entry && !input.splits.is_empty();
    set_opt_str(
        m,
        CATEGORY,
        input.category.as_deref().filter(|_| is_entry && !split),
    )?;
    let current = match m.get(SPLITS) {
        Some(ValueOrContainer::Value(v)) => Some(v),
        _ => None,
    };
    if split {
        let value = LoroValue::List(
            input
                .splits
                .iter()
                .map(|s| {
                    LoroValue::Map(
                        HashMap::from([
                            (CATEGORY.to_owned(), LoroValue::from(s.category.as_str())),
                            (AMOUNT.to_owned(), LoroValue::I64(s.amount)),
                        ])
                        .into(),
                    )
                })
                .collect::<Vec<_>>()
                .into(),
        );
        if current.as_ref() != Some(&value) {
            m.insert(SPLITS, value)?;
        }
    } else if current.is_some() {
        m.delete(SPLITS)?;
    }
    Ok(())
}

fn write_recurring(m: &LoroMap, input: &RecurringInput) -> Result<()> {
    set_str(m, NAME, input.name.trim())?;
    set_i64(m, AMOUNT, input.amount)?;
    set_str(m, ACCOUNT, &input.account)?;
    set_opt_str(m, CATEGORY, input.category.as_deref())?;
    set_str(m, REPEAT, &Repeat::parse_rrule(&input.repeat)?.to_rrule())?;
    set_str(m, NEXT_DUE, &Date::parse(&input.next_due)?.to_string())?;
    Ok(())
}

fn kind_str(kind: CategoryKind) -> &'static str {
    match kind {
        CategoryKind::Expense => "expense",
        CategoryKind::Income => "income",
    }
}

fn rollover_str(r: Rollover) -> &'static str {
    match r {
        Rollover::None => "none",
        Rollover::Leftover => "leftover",
        Rollover::Both => "both",
    }
}

fn account_kind_str(kind: AccountKind) -> &'static str {
    match kind {
        AccountKind::Bank => "bank",
        AccountKind::Cash => "cash",
        AccountKind::Savings => "savings",
    }
}

fn parse_account_kind(s: &str) -> AccountKind {
    match s {
        "cash" => AccountKind::Cash,
        "savings" => AccountKind::Savings,
        _ => AccountKind::Bank,
    }
}

fn txn_kind_str(kind: TxnKind) -> &'static str {
    match kind {
        TxnKind::Entry => "entry",
        TxnKind::Transfer => "transfer",
        TxnKind::Adjustment => "adjustment",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> Date {
        Date::parse(s).unwrap()
    }

    #[test]
    fn periods_start_on_the_chosen_day() {
        let p = |date, day| {
            let (a, b) = period_of(d(date), day);
            (a.to_string(), b.to_string())
        };
        assert_eq!(p("2026-11-13", 1), ("2026-11-01".into(), "2026-12-01".into()));
        assert_eq!(p("2026-11-13", 25), ("2026-10-25".into(), "2026-11-25".into()));
        assert_eq!(p("2026-11-25", 25), ("2026-11-25".into(), "2026-12-25".into()));
        assert_eq!(p("2026-01-10", 25), ("2025-12-25".into(), "2026-01-25".into()));
        // 31st: short months start on their last day.
        assert_eq!(p("2026-02-28", 31), ("2026-02-28".into(), "2026-03-31".into()));
        assert_eq!(p("2026-02-27", 31), ("2026-01-31".into(), "2026-02-28".into()));
        assert_eq!(p("2024-02-29", 30), ("2024-02-29".into(), "2024-03-30".into()));
    }

    #[test]
    fn periods_tile_the_calendar() {
        for day in [1, 15, 28, 29, 30, 31] {
            let (mut start, _) = period_of(d("2025-01-01"), day);
            for _ in 0..30 {
                let (s, next) = period_of(start, day);
                assert_eq!(s, start);
                assert_eq!(period_of(next.add_days(-1), day).0, start, "day {day}");
                assert!(next > start);
                start = next;
            }
        }
    }
}
