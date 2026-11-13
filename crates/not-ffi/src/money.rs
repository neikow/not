//! Budgeting: accounts, categories, transactions, budgets, quick entry, recurring.
//! Amounts are cents (negative = money out), dates `YYYY-MM-DD`. See `docs/budgeting.md`.

use crate::{NotError, NotStore};

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum AccountKind {
    Bank,
    Cash,
    Savings,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum CategoryKind {
    Expense,
    Income,
}

/// What a category's leftover (or overspend) does at the end of a period.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum Rollover {
    None,
    /// Unspent money carries over; overspending doesn't.
    Leftover,
    /// Both carry over.
    Both,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum TxnKind {
    /// In or out of one account; counts in the budget.
    Entry,
    /// From `account` to `to_account`, positive amount; never budget.
    Transfer,
    /// Written by Set balance; never budget.
    Adjustment,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct MoneyAccount {
    pub id: String,
    pub name: String,
    pub kind: AccountKind,
    pub opening_balance: i64,
    /// As of the day asked for.
    pub balance: i64,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct MoneyCategory {
    pub id: String,
    pub name: String,
    pub emoji: String,
    pub kind: CategoryKind,
    pub rollover: Rollover,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct MoneySplit {
    pub category: String,
    /// Same sign as the row's amount.
    pub amount: i64,
}

/// What the user enters or edits.
#[derive(Debug, Clone, uniffi::Record)]
pub struct TxnInput {
    pub kind: TxnKind,
    pub account: String,
    pub to_account: Option<String>,
    pub date: String,
    pub amount: i64,
    pub payee: String,
    pub note: String,
    pub category: Option<String>,
    /// Two or more parts adding up to `amount`, or empty.
    pub splits: Vec<MoneySplit>,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct MoneyTxn {
    pub id: String,
    pub kind: TxnKind,
    pub account: String,
    pub to_account: Option<String>,
    pub date: String,
    pub amount: i64,
    pub payee: String,
    /// Raw bank text of an imported row.
    pub label: Option<String>,
    pub note: String,
    pub category: Option<String>,
    pub splits: Vec<MoneySplit>,
    /// Entered from this recurring template.
    pub recurring: Option<String>,
    /// An entry with no category yet.
    pub needs_review: bool,
}

#[derive(Debug, Clone, Default, uniffi::Record)]
pub struct TxnFilter {
    pub account: Option<String>,
    pub category: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    pub to_review: bool,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct MoneySettings {
    pub period_start_day: u32,
    pub currency: String,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct CategoryBudget {
    pub category: MoneyCategory,
    /// 0 = no limit.
    pub limit: i64,
    pub carried: i64,
    /// Expense: money out net of refunds. Income: money in.
    pub spent: i64,
    pub left: i64,
    pub count: u32,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct MoneyBudget {
    pub start: String,
    /// Last day, inclusive.
    pub end: String,
    pub days_left: u32,
    pub expenses: Vec<CategoryBudget>,
    pub income: Vec<CategoryBudget>,
    pub unsorted_spent: i64,
    pub unsorted_income: i64,
    pub to_review: u32,
    pub available: i64,
    pub spent: i64,
    pub left: i64,
    pub income_total: i64,
    pub net: i64,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct CategoryAmount {
    pub category: String,
    pub amount: i64,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct PeriodReport {
    pub start: String,
    pub end: String,
    pub income: i64,
    pub spent: i64,
    pub net: i64,
    pub categories: Vec<CategoryAmount>,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct MoneyRecurring {
    pub id: String,
    pub name: String,
    pub amount: i64,
    pub account: String,
    pub category: Option<String>,
    pub repeat: String,
    pub next_due: String,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct RecurringInput {
    pub name: String,
    pub amount: i64,
    pub account: String,
    pub category: Option<String>,
    pub repeat: String,
    pub next_due: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum MoneyTokenKind {
    Amount,
    Category,
    Account,
    Date,
}

/// Offsets are UTF-16 code units.
#[derive(Debug, Clone, uniffi::Record)]
pub struct MoneyToken {
    pub start: u32,
    pub end: u32,
    pub kind: MoneyTokenKind,
    pub text: String,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct ParsedMoney {
    pub kind: TxnKind,
    pub amount: Option<i64>,
    pub payee: String,
    pub category: Option<String>,
    /// From the payee's history rather than a `#tag`.
    pub category_guessed: bool,
    pub account: Option<String>,
    pub to_account: Option<String>,
    pub date: String,
    pub splits: Vec<MoneySplit>,
    pub tokens: Vec<MoneyToken>,
    /// Why it can't be entered yet; `None` = ready.
    pub problem: Option<String>,
}

/// "−12,50 €", "+2 400,00 €": French grouping and decimal comma.
#[uniffi::export]
pub fn format_money(cents: i64, signed: bool) -> String {
    let abs = cents.unsigned_abs();
    let units = (abs / 100).to_string();
    let mut grouped = String::new();
    for (i, c) in units.chars().enumerate() {
        if i > 0 && (units.len() - i) % 3 == 0 {
            grouped.push('\u{202F}');
        }
        grouped.push(c);
    }
    let sign = match (signed, cents.signum()) {
        (true, 1) => "+",
        (_, -1) => "−",
        _ => "",
    };
    format!("{sign}{grouped},{:02}\u{00A0}€", abs % 100)
}

/// Cents from what a user typed in an amount field ("12", "12,5", "-3.40"); `None` if it isn't one.
#[uniffi::export]
pub fn parse_cents(text: String) -> Option<i64> {
    let t: String = text.trim().chars().filter(|c| !c.is_whitespace() && *c != '€' && *c != '\u{202F}').collect();
    let (neg, t) = match t.strip_prefix(['-', '−']) {
        Some(rest) => (true, rest.to_owned()),
        None => (false, t.strip_prefix('+').unwrap_or(&t).to_owned()),
    };
    let (units, frac) = t.split_once([',', '.']).unwrap_or((&t, ""));
    let digits = |s: &str| s.bytes().all(|b| b.is_ascii_digit());
    if units.is_empty() || units.len() > 9 || !digits(units) || frac.len() > 2 || !digits(frac) {
        return None;
    }
    let frac = match frac.len() {
        0 => 0,
        1 => frac.parse::<i64>().ok()? * 10,
        _ => frac.parse::<i64>().ok()?,
    };
    let cents = units.parse::<i64>().ok()? * 100 + frac;
    Some(if neg { -cents } else { cents })
}

#[uniffi::export]
impl NotStore {
    pub fn money_settings(&self) -> MoneySettings {
        let s = self.store.money_settings();
        MoneySettings {
            period_start_day: s.period_start_day,
            currency: s.currency,
        }
    }

    pub fn set_period_start_day(&self, day: u32) -> Result<(), NotError> {
        Ok(self.store.set_period_start_day(day)?)
    }

    /// Creates the starter categories the first time only. Call when Budget opens.
    pub fn seed_money(&self) -> Result<bool, NotError> {
        Ok(self.store.seed_money()?)
    }

    // ---- accounts ----

    pub fn create_account(&self, name: String, kind: AccountKind, opening_balance: i64) -> Result<String, NotError> {
        Ok(self.store.create_account(&name, kind.into(), opening_balance)?)
    }

    pub fn update_account(
        &self,
        id: String,
        name: String,
        kind: AccountKind,
        opening_balance: i64,
    ) -> Result<(), NotError> {
        Ok(self.store.update_account(&id, &name, kind.into(), opening_balance)?)
    }

    pub fn delete_account(&self, id: String) -> Result<(), NotError> {
        Ok(self.store.delete_account(&id)?)
    }

    pub fn move_account(&self, id: String, index: u32) -> Result<(), NotError> {
        Ok(self.store.move_account(&id, index as usize)?)
    }

    pub fn accounts(&self, date: String) -> Result<Vec<MoneyAccount>, NotError> {
        Ok(self.store.accounts(&date)?.into_iter().map(Into::into).collect())
    }

    /// Adds an adjustment so the balance on `date` reads `balance`; `None` if it already did.
    pub fn set_balance(&self, account: String, balance: i64, date: String) -> Result<Option<String>, NotError> {
        Ok(self.store.set_balance(&account, balance, &date)?)
    }

    // ---- categories ----

    pub fn create_category(&self, name: String, emoji: String, kind: CategoryKind) -> Result<String, NotError> {
        Ok(self.store.create_category(&name, &emoji, kind.into())?)
    }

    pub fn update_category(
        &self,
        id: String,
        name: String,
        emoji: String,
        kind: CategoryKind,
        rollover: Rollover,
    ) -> Result<(), NotError> {
        Ok(self.store.update_category(&id, &name, &emoji, kind.into(), rollover.into())?)
    }

    pub fn delete_category(&self, id: String) -> Result<(), NotError> {
        Ok(self.store.delete_category(&id)?)
    }

    pub fn move_category(&self, id: String, index: u32) -> Result<(), NotError> {
        Ok(self.store.move_category(&id, index as usize)?)
    }

    pub fn money_categories(&self) -> Vec<MoneyCategory> {
        self.store.categories().into_iter().map(Into::into).collect()
    }

    /// From the period containing `date` on; 0 = no limit.
    pub fn set_category_limit(&self, id: String, date: String, limit: i64) -> Result<(), NotError> {
        Ok(self.store.set_category_limit(&id, &date, limit)?)
    }

    // ---- transactions ----

    pub fn add_txn(&self, input: TxnInput) -> Result<String, NotError> {
        Ok(self.store.add_txn(&input.into())?)
    }

    pub fn update_txn(&self, id: String, input: TxnInput) -> Result<(), NotError> {
        Ok(self.store.update_txn(&id, &input.into())?)
    }

    pub fn set_txn_category(&self, id: String, category: Option<String>) -> Result<(), NotError> {
        Ok(self.store.set_txn_category(&id, category.as_deref())?)
    }

    pub fn delete_txn(&self, id: String) -> Result<(), NotError> {
        Ok(self.store.delete_txn(&id)?)
    }

    pub fn txn(&self, id: String) -> Result<MoneyTxn, NotError> {
        Ok(self.store.txn(&id)?.into())
    }

    /// Newest first.
    pub fn transactions(&self, filter: TxnFilter) -> Result<Vec<MoneyTxn>, NotError> {
        let filter = not_core::TxnFilter {
            account: filter.account,
            category: filter.category,
            from: filter.from,
            to: filter.to,
            to_review: filter.to_review,
        };
        Ok(self.store.transactions(&filter)?.into_iter().map(Into::into).collect())
    }

    // ---- budget ----

    pub fn budget(&self, date: String) -> Result<MoneyBudget, NotError> {
        let b = self.store.budget(&date)?;
        let lines = |v: Vec<not_core::CategoryBudget>| {
            v.into_iter()
                .map(|c| CategoryBudget {
                    category: c.category.into(),
                    limit: c.limit,
                    carried: c.carried,
                    spent: c.spent,
                    left: c.left,
                    count: c.count,
                })
                .collect()
        };
        Ok(MoneyBudget {
            start: b.start,
            end: b.end,
            days_left: b.days_left,
            expenses: lines(b.expenses),
            income: lines(b.income),
            unsorted_spent: b.unsorted_spent,
            unsorted_income: b.unsorted_income,
            to_review: b.to_review,
            available: b.available,
            spent: b.spent,
            left: b.left,
            income_total: b.income_total,
            net: b.net,
        })
    }

    /// `count` periods up to the one containing `date`, oldest first.
    pub fn money_reports(&self, date: String, count: u32) -> Result<Vec<PeriodReport>, NotError> {
        Ok(self
            .store
            .money_reports(&date, count)?
            .into_iter()
            .map(|r| PeriodReport {
                start: r.start,
                end: r.end,
                income: r.income,
                spent: r.spent,
                net: r.net,
                categories: r
                    .categories
                    .into_iter()
                    .map(|c| CategoryAmount {
                        category: c.category,
                        amount: c.amount,
                    })
                    .collect(),
            })
            .collect())
    }

    // ---- quick entry ----

    pub fn parse_money(&self, input: String, today: String) -> Result<ParsedMoney, NotError> {
        let p = self.store.parse_money(&input, &today)?;
        Ok(ParsedMoney {
            kind: p.kind.into(),
            amount: p.amount,
            payee: p.payee,
            category: p.category,
            category_guessed: p.category_guessed,
            account: p.account,
            to_account: p.to_account,
            date: p.date,
            splits: p.splits.into_iter().map(Into::into).collect(),
            tokens: p
                .tokens
                .into_iter()
                .map(|t| MoneyToken {
                    start: t.start,
                    end: t.end,
                    kind: match t.kind {
                        not_core::MoneyTokenKind::Amount => MoneyTokenKind::Amount,
                        not_core::MoneyTokenKind::Category => MoneyTokenKind::Category,
                        not_core::MoneyTokenKind::Account => MoneyTokenKind::Account,
                        not_core::MoneyTokenKind::Date => MoneyTokenKind::Date,
                    },
                    text: t.text,
                })
                .collect(),
            problem: p.problem,
        })
    }

    pub fn quick_add_money(&self, input: String, today: String) -> Result<String, NotError> {
        Ok(self.store.quick_add_money(&input, &today)?)
    }

    pub fn category_for_payee(&self, payee: String) -> Option<String> {
        self.store.category_for_payee(&payee)
    }

    pub fn last_used_account(&self) -> Option<String> {
        self.store.last_used_account()
    }

    // ---- recurring ----

    pub fn create_recurring(&self, input: RecurringInput) -> Result<String, NotError> {
        Ok(self.store.create_recurring(&input.into())?)
    }

    pub fn update_recurring(&self, id: String, input: RecurringInput) -> Result<(), NotError> {
        Ok(self.store.update_recurring(&id, &input.into())?)
    }

    pub fn delete_recurring(&self, id: String) -> Result<(), NotError> {
        Ok(self.store.delete_recurring(&id)?)
    }

    /// Soonest due first.
    pub fn recurring(&self) -> Vec<MoneyRecurring> {
        self.store.recurring().into_iter().map(Into::into).collect()
    }

    pub fn due_recurring(&self, today: String) -> Result<Vec<MoneyRecurring>, NotError> {
        Ok(self.store.due_recurring(&today)?.into_iter().map(Into::into).collect())
    }

    /// Enters the current occurrence (for `amount`, or the usual one) and moves to the next.
    pub fn enter_recurring(&self, id: String, amount: Option<i64>) -> Result<String, NotError> {
        Ok(self.store.enter_recurring(&id, amount)?)
    }

    pub fn skip_recurring(&self, id: String) -> Result<(), NotError> {
        Ok(self.store.skip_recurring(&id)?)
    }
}

// ---- conversions ----------------------------------------------------------------------------

impl From<AccountKind> for not_core::AccountKind {
    fn from(k: AccountKind) -> Self {
        match k {
            AccountKind::Bank => Self::Bank,
            AccountKind::Cash => Self::Cash,
            AccountKind::Savings => Self::Savings,
        }
    }
}

impl From<not_core::AccountKind> for AccountKind {
    fn from(k: not_core::AccountKind) -> Self {
        match k {
            not_core::AccountKind::Bank => Self::Bank,
            not_core::AccountKind::Cash => Self::Cash,
            not_core::AccountKind::Savings => Self::Savings,
        }
    }
}

impl From<CategoryKind> for not_core::CategoryKind {
    fn from(k: CategoryKind) -> Self {
        match k {
            CategoryKind::Expense => Self::Expense,
            CategoryKind::Income => Self::Income,
        }
    }
}

impl From<not_core::CategoryKind> for CategoryKind {
    fn from(k: not_core::CategoryKind) -> Self {
        match k {
            not_core::CategoryKind::Expense => Self::Expense,
            not_core::CategoryKind::Income => Self::Income,
        }
    }
}

impl From<Rollover> for not_core::Rollover {
    fn from(r: Rollover) -> Self {
        match r {
            Rollover::None => Self::None,
            Rollover::Leftover => Self::Leftover,
            Rollover::Both => Self::Both,
        }
    }
}

impl From<not_core::Rollover> for Rollover {
    fn from(r: not_core::Rollover) -> Self {
        match r {
            not_core::Rollover::None => Self::None,
            not_core::Rollover::Leftover => Self::Leftover,
            not_core::Rollover::Both => Self::Both,
        }
    }
}

impl From<TxnKind> for not_core::TxnKind {
    fn from(k: TxnKind) -> Self {
        match k {
            TxnKind::Entry => Self::Entry,
            TxnKind::Transfer => Self::Transfer,
            TxnKind::Adjustment => Self::Adjustment,
        }
    }
}

impl From<not_core::TxnKind> for TxnKind {
    fn from(k: not_core::TxnKind) -> Self {
        match k {
            not_core::TxnKind::Entry => Self::Entry,
            not_core::TxnKind::Transfer => Self::Transfer,
            not_core::TxnKind::Adjustment => Self::Adjustment,
        }
    }
}

impl From<not_core::Account> for MoneyAccount {
    fn from(a: not_core::Account) -> Self {
        Self {
            id: a.id,
            name: a.name,
            kind: a.kind.into(),
            opening_balance: a.opening_balance,
            balance: a.balance,
        }
    }
}

impl From<not_core::Category> for MoneyCategory {
    fn from(c: not_core::Category) -> Self {
        Self {
            id: c.id,
            name: c.name,
            emoji: c.emoji,
            kind: c.kind.into(),
            rollover: c.rollover.into(),
        }
    }
}

impl From<not_core::Split> for MoneySplit {
    fn from(s: not_core::Split) -> Self {
        Self {
            category: s.category,
            amount: s.amount,
        }
    }
}

impl From<TxnInput> for not_core::TxnInput {
    fn from(i: TxnInput) -> Self {
        Self {
            kind: i.kind.into(),
            account: i.account,
            to_account: i.to_account,
            date: i.date,
            amount: i.amount,
            payee: i.payee,
            note: i.note,
            category: i.category,
            splits: i
                .splits
                .into_iter()
                .map(|s| not_core::Split {
                    category: s.category,
                    amount: s.amount,
                })
                .collect(),
        }
    }
}

impl From<not_core::Txn> for MoneyTxn {
    fn from(t: not_core::Txn) -> Self {
        Self {
            needs_review: t.needs_review(),
            id: t.id,
            kind: t.kind.into(),
            account: t.account,
            to_account: t.to_account,
            date: t.date,
            amount: t.amount,
            payee: t.payee,
            label: t.label,
            note: t.note,
            category: t.category,
            splits: t.splits.into_iter().map(Into::into).collect(),
            recurring: t.recurring,
        }
    }
}

impl From<not_core::Recurring> for MoneyRecurring {
    fn from(r: not_core::Recurring) -> Self {
        Self {
            id: r.id,
            name: r.name,
            amount: r.amount,
            account: r.account,
            category: r.category,
            repeat: r.repeat,
            next_due: r.next_due,
        }
    }
}

impl From<RecurringInput> for not_core::RecurringInput {
    fn from(i: RecurringInput) -> Self {
        Self {
            name: i.name,
            amount: i.amount,
            account: i.account,
            category: i.category,
            repeat: i.repeat,
            next_due: i.next_due,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_french_style() {
        assert_eq!(format_money(-1_250, false), "−12,50\u{a0}€");
        assert_eq!(format_money(240_000, true), "+2\u{202f}400,00\u{a0}€");
        assert_eq!(format_money(5, false), "0,05\u{a0}€");
        assert_eq!(format_money(123_456_789, false), "1\u{202f}234\u{202f}567,89\u{a0}€");
        assert_eq!(format_money(0, true), "0,00\u{a0}€");
    }

    #[test]
    fn parses_typed_amounts() {
        assert_eq!(parse_cents("12".into()), Some(1_200));
        assert_eq!(parse_cents(" 12,5 € ".into()), Some(1_250));
        assert_eq!(parse_cents("-3.40".into()), Some(-340));
        assert_eq!(parse_cents("2 400".into()), Some(240_000));
        assert_eq!(parse_cents("abc".into()), None);
        assert_eq!(parse_cents("".into()), None);
        assert_eq!(parse_cents("1.234".into()), None);
    }
}
