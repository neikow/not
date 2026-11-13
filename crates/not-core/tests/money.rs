mod common;

use common::Log;
use not_core::{
    AccountKind, CategoryKind, MoneyTokenKind, Rollover, Split, Store, TxnFilter, TxnInput, TxnKind,
};

/// A Friday.
const TODAY: &str = "2026-11-13";

struct World {
    s: Store,
    sg: String,
    bourso: String,
    cash: String,
}

fn world() -> World {
    let s = Store::open_in_memory().unwrap();
    assert!(s.seed_money().unwrap());
    let sg = s.create_account("SG", AccountKind::Bank, 100_000).unwrap();
    let bourso = s.create_account("Boursobank", AccountKind::Bank, 50_000).unwrap();
    let cash = s.create_account("Cash", AccountKind::Cash, 0).unwrap();
    World { s, sg, bourso, cash }
}

fn cat(s: &Store, name: &str) -> String {
    s.categories().into_iter().find(|c| c.name == name).unwrap().id
}

fn entry(account: &str, date: &str, amount: i64, category: Option<&str>) -> TxnInput {
    TxnInput {
        kind: TxnKind::Entry,
        account: account.into(),
        date: date.into(),
        amount,
        category: category.map(Into::into),
        ..TxnInput::default()
    }
}

fn balance(s: &Store, account: &str, date: &str) -> i64 {
    s.accounts(date)
        .unwrap()
        .into_iter()
        .find(|a| a.id == account)
        .unwrap()
        .balance
}

fn line<'a>(b: &'a not_core::Budget, name: &str) -> &'a not_core::CategoryBudget {
    b.expenses
        .iter()
        .chain(&b.income)
        .find(|c| c.category.name == name)
        .unwrap()
}

// ---- setup ----------------------------------------------------------------------------------

#[test]
fn seeding_happens_once_even_after_deleting_everything() {
    let s = Store::open_in_memory().unwrap();
    assert!(s.seed_money().unwrap());
    let names: Vec<_> = s.categories().into_iter().map(|c| c.name).collect();
    assert_eq!(names[0], "Courses");
    assert_eq!(names.len(), 10);
    assert_eq!(
        s.categories()
            .iter()
            .filter(|c| c.kind == CategoryKind::Income)
            .count(),
        1
    );
    for c in s.categories() {
        s.delete_category(&c.id).unwrap();
    }
    assert!(!s.seed_money().unwrap());
    assert!(s.categories().is_empty());
}

#[test]
fn two_devices_seeding_offline_end_up_with_one_set() {
    let (a, b) = (Store::open_in_memory().unwrap(), Store::open_in_memory().unwrap());
    a.seed_money().unwrap();
    b.seed_money().unwrap();
    // Each renames a different category before they meet.
    a.update_category(
        &cat(&a, "Resto"),
        "Restaurants",
        "🍽️",
        CategoryKind::Expense,
        Rollover::None,
    )
    .unwrap();
    b.update_category(
        &cat(&b, "Loisirs"),
        "Sorties",
        "🎉",
        CategoryKind::Expense,
        Rollover::None,
    )
    .unwrap();
    let mut log = Log::default();
    log.sync(&a);
    log.sync(&b);
    log.sync(&a);
    let names = |s: &Store| s.categories().into_iter().map(|c| c.name).collect::<Vec<_>>();
    assert_eq!(names(&a), names(&b));
    assert_eq!(names(&a).len(), 10);
    assert!(names(&a).contains(&"Restaurants".to_string()));
    assert!(names(&a).contains(&"Sorties".to_string()));
}

#[test]
fn categories_and_accounts_keep_their_order_and_move() {
    let w = world();
    let s = &w.s;
    let order = |s: &Store| {
        s.accounts(TODAY)
            .unwrap()
            .into_iter()
            .map(|a| a.name)
            .collect::<Vec<_>>()
    };
    assert_eq!(order(s), ["SG", "Boursobank", "Cash"]);
    s.move_account(&w.cash, 0).unwrap();
    assert_eq!(order(s), ["Cash", "SG", "Boursobank"]);
    s.move_account(&w.cash, 1).unwrap();
    assert_eq!(order(s), ["SG", "Cash", "Boursobank"]);
    s.move_account(&w.sg, 2).unwrap();
    assert_eq!(order(s), ["Cash", "Boursobank", "SG"]);

    let autre = cat(s, "Autre");
    s.move_category(&autre, 0).unwrap();
    assert_eq!(s.categories()[0].name, "Autre");
    let new = s.create_category("Cadeaux", "🎁", CategoryKind::Expense).unwrap();
    assert_eq!(s.categories().last().unwrap().id, new);
}

#[test]
fn bad_input_is_refused() {
    let w = world();
    let s = &w.s;
    assert!(s.create_account("  ", AccountKind::Cash, 0).is_err());
    assert!(s.add_txn(&entry(&w.sg, TODAY, 0, None)).is_err(), "zero");
    assert!(s.add_txn(&entry("nope", TODAY, -100, None)).is_err(), "account");
    assert!(s.add_txn(&entry(&w.sg, "13/11", -100, None)).is_err(), "date");
    assert!(
        s.add_txn(&entry(&w.sg, TODAY, -100, Some("nope"))).is_err(),
        "category"
    );
    let transfer = |to: Option<&str>, amount| TxnInput {
        kind: TxnKind::Transfer,
        account: w.sg.clone(),
        to_account: to.map(Into::into),
        date: TODAY.into(),
        amount,
        ..TxnInput::default()
    };
    assert!(s.add_txn(&transfer(None, 100)).is_err());
    assert!(s.add_txn(&transfer(Some(&w.sg), 100)).is_err(), "same account");
    assert!(s.add_txn(&transfer(Some(&w.cash), -100)).is_err(), "negative");
    assert!(s.add_txn(&transfer(Some(&w.cash), 100)).is_ok());
    assert!(s.set_category_limit(&cat(s, "Resto"), TODAY, -5).is_err());
    assert!(s.set_period_start_day(0).is_err());
    assert!(s.set_period_start_day(32).is_err());
}

// ---- balances -------------------------------------------------------------------------------

#[test]
fn balances_follow_entries_transfers_and_adjustments() {
    let w = world();
    let s = &w.s;
    s.add_txn(&entry(&w.sg, "2026-11-10", -2_500, None)).unwrap();
    s.add_txn(&entry(&w.sg, "2026-11-11", 240_000, Some(&cat(s, "Revenus"))))
        .unwrap();
    s.add_txn(&TxnInput {
        kind: TxnKind::Transfer,
        account: w.sg.clone(),
        to_account: Some(w.bourso.clone()),
        date: "2026-11-12".into(),
        amount: 50_000,
        ..TxnInput::default()
    })
    .unwrap();
    // Future rows don't count yet.
    s.add_txn(&entry(&w.sg, "2026-11-20", -1_000, None)).unwrap();

    assert_eq!(balance(s, &w.sg, TODAY), 100_000 - 2_500 + 240_000 - 50_000);
    assert_eq!(balance(s, &w.bourso, TODAY), 100_000);
    assert_eq!(balance(s, &w.sg, "2026-11-11"), 337_500);
    assert_eq!(balance(s, &w.sg, "2026-11-30"), 286_500);

    // The bank says 2 860 € today, not 2 875 €: a 15 € adjustment, outside the budget.
    let adj = s.set_balance(&w.sg, 286_000, TODAY).unwrap().unwrap();
    assert_eq!(s.txn(&adj).unwrap().amount, -1_500);
    assert_eq!(s.txn(&adj).unwrap().kind, TxnKind::Adjustment);
    assert_eq!(balance(s, &w.sg, TODAY), 286_000);
    assert_eq!(
        s.set_balance(&w.sg, 286_000, TODAY).unwrap(),
        None,
        "already right"
    );
    let b = s.budget(TODAY).unwrap();
    assert_eq!(
        b.spent,
        2_500 + 1_000,
        "transfer and adjustment aren't spending; later rows of the period are"
    );
    assert_eq!(b.income_total, 240_000);
}

#[test]
fn transfers_list_under_both_accounts() {
    let w = world();
    let s = &w.s;
    let t = s
        .add_txn(&TxnInput {
            kind: TxnKind::Transfer,
            account: w.sg.clone(),
            to_account: Some(w.cash.clone()),
            date: TODAY.into(),
            amount: 4_000,
            ..TxnInput::default()
        })
        .unwrap();
    for acc in [&w.sg, &w.cash] {
        let rows = s
            .transactions(&TxnFilter {
                account: Some(acc.clone()),
                ..TxnFilter::default()
            })
            .unwrap();
        assert_eq!(rows.iter().map(|r| &r.id).collect::<Vec<_>>(), [&t]);
    }
    assert!(
        s.transactions(&TxnFilter {
            account: Some(w.bourso.clone()),
            ..TxnFilter::default()
        })
        .unwrap()
        .is_empty()
    );
}

// ---- budget ---------------------------------------------------------------------------------

#[test]
fn budget_counts_spending_refunds_income_and_unsorted() {
    let w = world();
    let s = &w.s;
    let (courses, resto, revenus) = (cat(s, "Courses"), cat(s, "Resto"), cat(s, "Revenus"));
    s.set_category_limit(&courses, TODAY, 40_000).unwrap();
    s.set_category_limit(&resto, TODAY, 15_000).unwrap();
    s.add_txn(&entry(&w.sg, "2026-11-02", -8_000, Some(&courses)))
        .unwrap();
    s.add_txn(&entry(&w.cash, "2026-11-05", -2_000, Some(&courses)))
        .unwrap();
    s.add_txn(&entry(&w.sg, "2026-11-06", 1_000, Some(&courses)))
        .unwrap(); // refund
    s.add_txn(&entry(&w.sg, "2026-11-07", -16_000, Some(&resto)))
        .unwrap();
    s.add_txn(&entry(&w.sg, "2026-11-08", -3_000, None)).unwrap();
    s.add_txn(&entry(&w.sg, "2026-11-01", 240_000, Some(&revenus)))
        .unwrap();
    // Another period.
    s.add_txn(&entry(&w.sg, "2026-10-31", -99_900, Some(&courses)))
        .unwrap();

    let b = s.budget(TODAY).unwrap();
    assert_eq!(
        (b.start.as_str(), b.end.as_str(), b.days_left),
        ("2026-11-01", "2026-11-30", 18)
    );
    let c = line(&b, "Courses");
    assert_eq!((c.limit, c.spent, c.left, c.count), (40_000, 9_000, 31_000, 3));
    let r = line(&b, "Resto");
    assert_eq!((r.spent, r.left), (16_000, -1_000));
    assert_eq!(line(&b, "Transport").spent, 0);
    assert_eq!(line(&b, "Revenus").spent, 240_000);
    assert_eq!(b.unsorted_spent, 3_000);
    assert_eq!(b.to_review, 1);
    assert_eq!(b.available, 55_000);
    assert_eq!(b.spent, 9_000 + 16_000 + 3_000);
    assert_eq!(b.left, 55_000 - 28_000);
    assert_eq!(b.income_total, 240_000);
    assert_eq!(b.net, 240_000 - 28_000);

    // A day of another period: nothing left to count down.
    assert_eq!(s.budget("2026-10-31").unwrap().days_left, 1);
    assert_eq!(s.budget("2026-10-31").unwrap().spent, 99_900);
}

#[test]
fn periods_follow_the_start_day() {
    let w = world();
    let s = &w.s;
    s.set_period_start_day(25).unwrap();
    let courses = cat(s, "Courses");
    s.add_txn(&entry(&w.sg, "2026-10-25", -1_000, Some(&courses)))
        .unwrap();
    s.add_txn(&entry(&w.sg, "2026-11-24", -2_000, Some(&courses)))
        .unwrap();
    s.add_txn(&entry(&w.sg, "2026-11-25", -4_000, Some(&courses)))
        .unwrap();
    let b = s.budget(TODAY).unwrap();
    assert_eq!((b.start.as_str(), b.end.as_str()), ("2026-10-25", "2026-11-24"));
    assert_eq!(b.spent, 3_000);
    assert_eq!(b.days_left, 12);
    assert_eq!(s.budget("2026-11-25").unwrap().spent, 4_000);
    assert_eq!(
        s.money_period(TODAY).unwrap(),
        ("2026-10-25".into(), "2026-11-24".into())
    );
}

#[test]
fn a_new_limit_applies_from_its_period_on() {
    let w = world();
    let s = &w.s;
    let resto = cat(s, "Resto");
    s.set_category_limit(&resto, "2026-09-10", 10_000).unwrap();
    s.set_category_limit(&resto, "2026-12-03", 30_000).unwrap();
    let limit = |date| line(&s.budget(date).unwrap(), "Resto").limit;
    assert_eq!(limit("2026-08-15"), 0);
    assert_eq!(limit("2026-09-01"), 10_000);
    assert_eq!(limit("2026-11-30"), 10_000);
    assert_eq!(limit("2027-03-01"), 30_000);
    // Changing it in November drops the December one: "from now on" means from now on.
    s.set_category_limit(&resto, TODAY, 20_000).unwrap();
    assert_eq!(limit("2026-10-01"), 10_000);
    assert_eq!(limit("2026-11-01"), 20_000);
    assert_eq!(limit("2027-03-01"), 20_000);
    // 0 removes the limit from here on.
    s.set_category_limit(&resto, "2027-01-01", 0).unwrap();
    assert_eq!(limit("2027-03-01"), 0);
    assert_eq!(limit("2026-12-15"), 20_000);
}

#[test]
fn rollover_modes() {
    let w = world();
    let s = &w.s;
    let id = cat(s, "Loisirs");
    s.set_category_limit(&id, "2026-09-01", 10_000).unwrap();
    s.add_txn(&entry(&w.sg, "2026-09-10", -4_000, Some(&id))).unwrap(); // 60 left
    s.add_txn(&entry(&w.sg, "2026-10-10", -25_000, Some(&id)))
        .unwrap(); // over
    let set = |r| {
        s.update_category(&id, "Loisirs", "🎉", CategoryKind::Expense, r)
            .unwrap()
    };
    let nov = |s: &Store| {
        let b = s.budget(TODAY).unwrap();
        let l = line(&b, "Loisirs").clone();
        (l.carried, l.left)
    };
    assert_eq!(nov(s), (0, 10_000));
    set(Rollover::Leftover);
    // Sep: 100 → 60 carried into Oct; Oct: 160 − 250 = −90, overspend not carried.
    assert_eq!(line(&s.budget("2026-10-05").unwrap(), "Loisirs").carried, 6_000);
    assert_eq!(nov(s), (0, 10_000));
    set(Rollover::Both);
    // Oct: 100 + 60 − 250 = −90 → Nov starts at 10.
    assert_eq!(nov(s), (-9_000, 1_000));
    assert_eq!(s.budget(TODAY).unwrap().available, 1_000);
    // A refund in October changes what carries.
    s.add_txn(&entry(&w.sg, "2026-10-20", 9_000, Some(&id))).unwrap();
    assert_eq!(nov(s), (0, 10_000));
}

#[test]
fn rollover_without_a_limit_carries_leftover_from_the_first_row() {
    let w = world();
    let s = &w.s;
    let id = cat(s, "Santé");
    s.update_category(&id, "Santé", "💊", CategoryKind::Expense, Rollover::Both)
        .unwrap();
    s.add_txn(&entry(&w.sg, "2026-10-03", -3_000, Some(&id))).unwrap();
    s.set_category_limit(&id, TODAY, 5_000).unwrap();
    let l = line(&s.budget(TODAY).unwrap(), "Santé").clone();
    assert_eq!((l.carried, l.limit, l.left), (-3_000, 5_000, 2_000));
}

#[test]
fn splits_count_per_part_and_must_add_up() {
    let w = world();
    let s = &w.s;
    let (courses, shopping) = (cat(s, "Courses"), cat(s, "Shopping"));
    let split = |parts: &[(&str, i64)], total| TxnInput {
        splits: parts
            .iter()
            .map(|(c, a)| Split {
                category: (*c).into(),
                amount: *a,
            })
            .collect(),
        ..entry(&w.sg, TODAY, total, Some(&courses))
    };
    assert!(
        s.add_txn(&split(&[(&courses, -9_000), (&shopping, -2_000)], -12_000))
            .is_err()
    );
    assert!(
        s.add_txn(&split(&[(&courses, -12_000)], -12_000)).is_err(),
        "one part"
    );
    assert!(
        s.add_txn(&split(&[(&courses, -13_000), (&shopping, 1_000)], -12_000))
            .is_err()
    );
    let id = s
        .add_txn(&split(&[(&courses, -9_000), (&shopping, -3_000)], -12_000))
        .unwrap();
    let t = s.txn(&id).unwrap();
    assert_eq!(t.category, None, "a split has no single category");
    assert_eq!(t.splits.len(), 2);
    let b = s.budget(TODAY).unwrap();
    assert_eq!(line(&b, "Courses").spent, 9_000);
    assert_eq!(line(&b, "Shopping").spent, 3_000);
    assert_eq!(b.to_review, 0);
    let in_shopping = s
        .transactions(&TxnFilter {
            category: Some(shopping.clone()),
            ..TxnFilter::default()
        })
        .unwrap();
    assert_eq!(in_shopping.len(), 1);
    // Back to one category.
    s.set_txn_category(&id, Some(&shopping)).unwrap();
    let t = s.txn(&id).unwrap();
    assert!(t.splits.is_empty());
    assert_eq!(line(&s.budget(TODAY).unwrap(), "Shopping").spent, 12_000);
}

#[test]
fn deleting_a_category_makes_its_rows_unsorted_until_undone() {
    let w = world();
    let s = &w.s;
    let resto = cat(s, "Resto");
    s.add_txn(&entry(&w.sg, TODAY, -1_500, Some(&resto))).unwrap();
    s.delete_category(&resto).unwrap();
    let b = s.budget(TODAY).unwrap();
    assert_eq!(b.unsorted_spent, 1_500);
    assert_eq!(b.spent, 1_500);
    assert!(s.undo().unwrap());
    let b = s.budget(TODAY).unwrap();
    assert_eq!((b.unsorted_spent, line(&b, "Resto").spent), (0, 1_500));
}

#[test]
fn reports_cover_the_periods_asked_for() {
    let w = world();
    let s = &w.s;
    let (courses, revenus) = (cat(s, "Courses"), cat(s, "Revenus"));
    s.add_txn(&entry(&w.sg, "2026-09-03", -1_000, Some(&courses)))
        .unwrap();
    s.add_txn(&entry(&w.sg, "2026-11-03", -2_000, Some(&courses)))
        .unwrap();
    s.add_txn(&entry(&w.sg, "2026-11-04", -500, None)).unwrap();
    s.add_txn(&entry(&w.sg, "2026-11-01", 200_000, Some(&revenus)))
        .unwrap();
    let r = s.money_reports(TODAY, 3).unwrap();
    let starts: Vec<_> = r.iter().map(|p| p.start.as_str()).collect();
    assert_eq!(starts, ["2026-09-01", "2026-10-01", "2026-11-01"]);
    assert_eq!((r[0].spent, r[0].income), (1_000, 0));
    assert_eq!((r[1].spent, r[1].categories.len()), (0, 0));
    assert_eq!((r[2].spent, r[2].income, r[2].net), (2_500, 200_000, 197_500));
    assert_eq!(r[2].categories.len(), 1);
    assert_eq!(r[2].end, "2026-11-30");
}

// ---- transactions ---------------------------------------------------------------------------

#[test]
fn transactions_are_newest_first_and_filter() {
    let w = world();
    let s = &w.s;
    let resto = cat(s, "Resto");
    let a = s
        .add_txn(&entry(&w.sg, "2026-11-10", -100, Some(&resto)))
        .unwrap();
    let b = s.add_txn(&entry(&w.cash, "2026-11-12", -200, None)).unwrap();
    let c = s
        .add_txn(&entry(&w.sg, "2026-11-12", -300, Some(&resto)))
        .unwrap();
    let ids = |f: TxnFilter| {
        s.transactions(&f)
            .unwrap()
            .into_iter()
            .map(|t| t.id)
            .collect::<Vec<_>>()
    };
    assert_eq!(ids(TxnFilter::default()), [c.clone(), b.clone(), a.clone()]);
    assert_eq!(
        ids(TxnFilter {
            from: Some("2026-11-11".into()),
            ..TxnFilter::default()
        }),
        [c.clone(), b.clone()]
    );
    assert_eq!(
        ids(TxnFilter {
            to_review: true,
            ..TxnFilter::default()
        }),
        [b.as_str()]
    );
    assert_eq!(
        ids(TxnFilter {
            category: Some(resto),
            to: Some("2026-11-11".into()),
            ..TxnFilter::default()
        }),
        [a.as_str()]
    );
    s.delete_txn(&c).unwrap();
    assert_eq!(ids(TxnFilter::default()), [b, a]);
    assert!(s.txn(&c).is_err());
}

#[test]
fn concurrent_edits_of_different_fields_both_survive() {
    let w = world();
    let (a, b) = (&w.s, Store::open_in_memory().unwrap());
    let mut log = Log::default();
    let id = a.add_txn(&entry(&w.sg, TODAY, -1_000, None)).unwrap();
    log.sync(a);
    log.sync(&b);
    let t = b.txn(&id).unwrap();
    let input = |t: &not_core::Txn| TxnInput {
        kind: t.kind,
        account: t.account.clone(),
        to_account: t.to_account.clone(),
        date: t.date.clone(),
        amount: t.amount,
        payee: t.payee.clone(),
        note: t.note.clone(),
        category: t.category.clone(),
        splits: t.splits.clone(),
    };
    a.update_txn(
        &id,
        &TxnInput {
            amount: -1_250,
            ..input(&t)
        },
    )
    .unwrap();
    b.set_txn_category(&id, Some(&cat(&b, "Resto"))).unwrap();
    log.sync(a);
    log.sync(&b);
    log.sync(a);
    for s in [a, &b] {
        let t = s.txn(&id).unwrap();
        assert_eq!((t.amount, t.category.clone()), (-1_250, Some(cat(s, "Resto"))));
    }
}

#[test]
fn undo_and_redo_keep_ids_so_rows_keep_their_account() {
    let s = Store::open_in_memory().unwrap();
    let acc = s.create_account("Cash", AccountKind::Cash, 0).unwrap();
    assert!(s.undo().unwrap());
    assert!(s.accounts(TODAY).unwrap().is_empty());
    assert!(s.redo().unwrap());
    assert_eq!(s.accounts(TODAY).unwrap()[0].id, acc);
    let t = s.add_txn(&entry(&acc, TODAY, -500, None)).unwrap();
    assert!(s.undo().unwrap());
    assert!(s.txn(&t).is_err());
    assert!(s.redo().unwrap());
    assert_eq!(s.txn(&t).unwrap().amount, -500);
    assert_eq!(balance(&s, &acc, TODAY), -500);
}

// ---- quick entry ----------------------------------------------------------------------------

#[test]
fn parses_the_basics() {
    let w = world();
    let s = &w.s;
    let p = s.parse_money("12.50 Starbucks", TODAY).unwrap();
    assert_eq!(
        (p.amount, p.payee.as_str(), p.date.as_str()),
        (Some(-1_250), "Starbucks", TODAY)
    );
    assert_eq!(p.kind, TxnKind::Entry);
    assert_eq!(
        p.account.as_deref(),
        Some(w.sg.as_str()),
        "first account until one is used"
    );
    assert_eq!(p.problem, None);

    let p = s.parse_money("45,80 courses #cour hier @cash", TODAY).unwrap();
    assert_eq!(p.amount, Some(-4_580));
    assert_eq!(p.payee, "courses");
    assert_eq!(p.category, Some(cat(s, "Courses")));
    assert!(!p.category_guessed);
    assert_eq!(p.account.as_deref(), Some(w.cash.as_str()));
    assert_eq!(p.date, "2026-11-12");
    let kinds: Vec<_> = p.tokens.iter().map(|t| t.kind).collect();
    assert_eq!(
        kinds,
        [
            MoneyTokenKind::Amount,
            MoneyTokenKind::Category,
            MoneyTokenKind::Date,
            MoneyTokenKind::Account
        ]
    );
    assert_eq!(
        (p.tokens[1].start, p.tokens[1].end, p.tokens[1].text.as_str()),
        (14, 19, "#cour")
    );

    // Accents and case don't matter; the payee keeps how it was typed.
    let p = s.parse_money("8 Pharmacie du Centre #SANTE", TODAY).unwrap();
    assert_eq!(
        (p.category, p.payee.as_str()),
        (Some(cat(s, "Santé")), "Pharmacie du Centre")
    );
}

#[test]
fn signs_come_from_the_input_or_an_income_category() {
    let w = world();
    let s = &w.s;
    assert_eq!(
        s.parse_money("+2400 salaire", TODAY).unwrap().amount,
        Some(240_000)
    );
    assert_eq!(
        s.parse_money("2400 salaire #revenus", TODAY).unwrap().amount,
        Some(240_000)
    );
    assert_eq!(
        s.parse_money("-20 remboursement #revenus", TODAY).unwrap().amount,
        Some(-2_000)
    );
    assert_eq!(
        s.parse_money("+15 refund #resto", TODAY).unwrap().amount,
        Some(1_500)
    );
}

#[test]
fn dates_look_back() {
    let w = world();
    let date = |input: &str| w.s.parse_money(input, TODAY).unwrap().date;
    assert_eq!(date("5 pain"), TODAY);
    assert_eq!(date("5 pain hier"), "2026-11-12");
    assert_eq!(date("5 pain yesterday"), "2026-11-12");
    assert_eq!(date("5 pain avant-hier"), "2026-11-11");
    assert_eq!(date("5 pain avant hier"), "2026-11-11");
    assert_eq!(date("5 pain lundi"), "2026-11-09");
    assert_eq!(date("5 pain vendredi"), TODAY, "today's weekday is today");
    assert_eq!(date("5 pain vendredi dernier"), "2026-11-06");
    assert_eq!(date("5 pain last fri"), "2026-11-06");
    assert_eq!(date("5 pain 12/11"), "2026-11-12");
    assert_eq!(date("5 pain 20/11"), "2025-11-20", "not ahead");
    assert_eq!(date("5 pain 3 nov"), "2026-11-03");
    assert_eq!(date("5 pain 24 dec"), "2025-12-24");
    assert_eq!(date("5 pain 24 dec 2026"), "2026-12-24", "an explicit year wins");
    assert_eq!(date("5 pain le 20"), "2026-10-20");
    assert_eq!(date("5 pain le 2"), "2026-11-02");
    assert_eq!(date("5 pain 2026-10-01"), "2026-10-01");
    assert_eq!(
        date("5 pain 12.11"),
        "2026-11-12",
        "after the amount, 12.11 is a date"
    );
}

#[test]
fn transfers_parse_in_several_spellings() {
    let w = world();
    let s = &w.s;
    for input in [
        "500 @sg > @bourso",
        "500 @sg>@bourso",
        "500 @sg >@bour",
        "500 @sg → @bourso",
        "500 @sg @bourso",
    ] {
        let p = s.parse_money(input, TODAY).unwrap();
        assert_eq!(p.kind, TxnKind::Transfer, "{input}");
        assert_eq!(p.amount, Some(50_000), "{input}");
        assert_eq!(p.account.as_deref(), Some(w.sg.as_str()), "{input}");
        assert_eq!(p.to_account.as_deref(), Some(w.bourso.as_str()), "{input}");
        assert_eq!(p.problem, None, "{input}");
    }
    let p = s.parse_money("500 @sg >", TODAY).unwrap();
    assert!(p.problem.is_some(), "no destination");
    let id = s.quick_add_money("100 retrait @sg > @cash", TODAY).unwrap();
    assert_eq!(balance(s, &w.cash, TODAY), 10_000);
    assert_eq!(s.txn(&id).unwrap().payee, "retrait");
}

#[test]
fn splits_parse_with_one_part_taking_the_rest() {
    let w = world();
    let s = &w.s;
    let (courses, shopping) = (cat(s, "Courses"), cat(s, "Shopping"));
    let p = s
        .parse_money("120 carrefour #courses 90 / #shopping", TODAY)
        .unwrap();
    assert_eq!(p.problem, None);
    assert_eq!(p.payee, "carrefour");
    assert_eq!(p.category, None);
    assert_eq!(
        p.splits,
        [
            Split {
                category: courses.clone(),
                amount: -9_000
            },
            Split {
                category: shopping.clone(),
                amount: -3_000
            }
        ]
    );
    let p = s
        .parse_money("120 carrefour #courses / #shopping 30", TODAY)
        .unwrap();
    assert_eq!(p.splits[0].amount, -9_000);
    let p = s
        .parse_money("120 carrefour #courses 90 / #shopping 20", TODAY)
        .unwrap();
    assert!(p.problem.unwrap().contains("add up"));
    let p = s
        .parse_money("120 carrefour #courses / #shopping", TODAY)
        .unwrap();
    assert!(p.problem.is_some(), "two parts without amounts");
    let p = s.parse_money("120 carrefour #courses / chips", TODAY).unwrap();
    assert!(p.problem.is_some(), "a part without a category");
    let id = s
        .quick_add_money("120 carrefour #courses 90 / #shopping", TODAY)
        .unwrap();
    assert_eq!(s.txn(&id).unwrap().splits.len(), 2);
}

#[test]
fn defaults_come_from_history() {
    let w = world();
    let s = &w.s;
    s.quick_add_money("4 Boulangerie Paul #courses @cash", "2026-11-10")
        .unwrap();
    s.quick_add_money("6 boulangerie paul #resto @cash", "2026-11-11")
        .unwrap();
    let p = s.parse_money("3.20 Boulangerie  Paul", TODAY).unwrap();
    assert_eq!(p.category, Some(cat(s, "Resto")), "the latest one");
    assert!(p.category_guessed);
    assert_eq!(
        p.account.as_deref(),
        Some(w.cash.as_str()),
        "the last used account"
    );
    let p = s.parse_money("3.20 Boulangerie Paul #courses", TODAY).unwrap();
    assert_eq!(p.category, Some(cat(s, "Courses")), "a tag beats history");
    assert!(s.parse_money("3 nouveau", TODAY).unwrap().category.is_none());
}

#[test]
fn incomplete_input_explains_itself() {
    let s = Store::open_in_memory().unwrap();
    s.seed_money().unwrap();
    assert_eq!(
        s.parse_money("coffee", TODAY).unwrap().problem.as_deref(),
        Some("Type an amount")
    );
    assert_eq!(
        s.parse_money("3 coffee", TODAY).unwrap().problem.as_deref(),
        Some("Add an account first")
    );
    assert!(s.quick_add_money("3 coffee", TODAY).is_err());
}

// ---- recurring ------------------------------------------------------------------------------

#[test]
fn recurring_becomes_due_and_enters_in_one_step() {
    let w = world();
    let s = &w.s;
    let logement = cat(s, "Logement");
    let rent = s
        .create_recurring(&not_core::RecurringInput {
            name: "Loyer".into(),
            amount: -85_000,
            account: w.sg.clone(),
            category: Some(logement.clone()),
            repeat: "FREQ=MONTHLY".into(),
            next_due: "2026-11-05".into(),
        })
        .unwrap();
    let netflix = s
        .create_recurring(&not_core::RecurringInput {
            name: "Netflix".into(),
            amount: -1_399,
            account: w.bourso.clone(),
            category: Some(cat(s, "Abonnements")),
            repeat: "FREQ=MONTHLY".into(),
            next_due: "2026-11-20".into(),
        })
        .unwrap();
    let due: Vec<_> = s
        .due_recurring(TODAY)
        .unwrap()
        .into_iter()
        .map(|r| r.id)
        .collect();
    assert_eq!(due, [rent.as_str()]);

    // The amount can differ this time.
    let tx = s.enter_recurring(&rent, Some(-86_000)).unwrap();
    let t = s.txn(&tx).unwrap();
    assert_eq!(
        (t.date.as_str(), t.amount, t.payee.as_str()),
        ("2026-11-05", -86_000, "Loyer")
    );
    assert_eq!(t.recurring.as_deref(), Some(rent.as_str()));
    assert_eq!(t.category, Some(logement));
    assert!(s.due_recurring(TODAY).unwrap().is_empty());
    let next = |id: &str| s.recurring().into_iter().find(|r| r.id == id).unwrap().next_due;
    assert_eq!(next(&rent), "2026-12-05");
    assert_eq!(s.recurring()[0].id, netflix, "soonest first");

    // One undo takes back both the row and the move.
    assert!(s.undo().unwrap());
    assert!(s.txn(&tx).is_err());
    assert_eq!(s.due_recurring(TODAY).unwrap().len(), 1);

    s.skip_recurring(&rent).unwrap();
    assert!(s.due_recurring(TODAY).unwrap().is_empty());
    assert_eq!(s.due_recurring("2026-12-31").unwrap().len(), 2);
    s.delete_recurring(&netflix).unwrap();
    assert_eq!(s.recurring().len(), 1);
    assert!(
        s.create_recurring(&not_core::RecurringInput {
            name: "Bad".into(),
            amount: -1,
            account: w.sg.clone(),
            category: None,
            repeat: "FREQ=SOMETIMES".into(),
            next_due: TODAY.into(),
        })
        .is_err()
    );
}

#[test]
fn export_carries_budgeting() {
    let w = world();
    let s = &w.s;
    s.set_category_limit(&cat(s, "Resto"), TODAY, 15_000).unwrap();
    s.quick_add_money("12.50 Starbucks #resto", TODAY).unwrap();
    let json: serde_json::Value =
        serde_json::from_str(&s.export_json(not_core::date::Date::parse(TODAY).unwrap()).unwrap()).unwrap();
    let money = &json["money"];
    assert_eq!(money["currency"], "EUR");
    assert_eq!(money["accounts"][0]["balance"], "987.50");
    assert_eq!(money["transactions"][0]["amount"], "-12.50");
    assert_eq!(money["transactions"][0]["payee"], "Starbucks");
    let resto = money["categories"].as_array().unwrap().iter().find(|c| c["name"] == "Resto").unwrap();
    assert_eq!(resto["limit"], "150.00");
}
