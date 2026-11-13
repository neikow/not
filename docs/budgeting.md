# Budgeting — design

Status: design agreed 2026-11-13; not built. Bank sync: [ADR 0014](adr/0014-bank-sync-on-the-server.md).

## Scope (decisions)
| # | Decision |
|---|----------|
| Model | Limit per category per period, with spent and left. Income tracked; net = in − out. |
| Period | Starts on a chosen day of the month (payday). Clamped to the last day in short months. |
| Currency | One currency (EUR). Amounts are `i64` cents, negative = money out. |
| Accounts | Several (bank, cash, savings) with balances. Transfers between them are kept out of spending. |
| Entry | Quick-add shorthand parsed in core. Bank sync on the server (Enable Banking). CSV/OFX import. |
| Bank vs manual | The bank is the truth. A manual entry on a bank account stays *pending* until a bank row matches it. Cash accounts are manual only. |
| Categories | Starter set: Courses, Resto, Transport, Logement, Abonnements, Loisirs, Santé, Shopping, Revenus, Autre. Editable, each with an emoji. |
| Rollover | Per category: `none` · `leftover` (carry unspent only) · `both` (carry unspent and overspend). |
| Rules | Learned from corrections, confirmed once ("Always put CARREFOUR in Courses?"). Rows with no category go to **To review**. |
| Transfers | Paired automatically (same amount, opposite sign, two own accounts, ≤3 days apart). Can be unpaired. |
| Recurring | Templates become **Due** on their date. One tap creates a pending entry, which the bank row later matches. |
| Splits | One row can be split across categories. |
| Pending bank rows | Shown greyed and counted in spent. |
| Alerts | 80% / 100% of a category, toggled per device like reminders. |
| Place | New **More** page (replaces the Workouts tab) holding Workouts and Budget. |
| Capture | QS tile, widget +, and the Mac quick-entry panel all use the same parser. |
| Web | Nothing for now. |

## Data model (workspace doc, root containers)
All of these are `LoroMap`s keyed by id. Each value is a nested map, so fields merge per field
(last write wins). Deletes are soft (`deleted_at`, per 0007).

- `money_settings`: `period_start_day` (1–31), `currency` ("EUR").
- `money_accounts`: id → `name, kind (bank|cash|savings), opening_balance, opening_date, order,
  bank? { provider: "enablebanking", account_uid, bank }, bank_status? { consent_until,
  last_fetch, error }, deleted_at`.
- `money_categories`: id → `name, emoji, kind (expense|income), rollover (none|leftover|both),
  limits: map<period_start "YYYY-MM-DD", cents>, order, deleted_at`. A period's limit is the latest
  entry at or before the period's start, so changing a limit doesn't rewrite the past and rollover
  stays correct.
- `money_txns`: id → `account, date, amount, payee, label (raw bank text), note, category?,
  category_source (user|rule|recurring), splits?: list<{category, amount}>, status
  (pending|booked), source (manual|bank|import), bank_ref?, merged_into?, transfer?, recurring?,
  deleted_at`.
  - Ids: manual = random; bank = `eb:<account uid>:<tx id>`; import = `imp:<account>:<FITID or
    hash(date, amount, label, n-th duplicate)>`.
  - `merged_into` hides a manual or imported row that was matched to a bank row.
- `money_rules`: id → `contains (normalised label), category, payee?, created_from_txn`.
- `money_recurring`: id → `name, amount (estimate), account, category, schedule (reuses
  schedule.rs), match? (label text), last_paid_period, deleted_at`.

Size: about 2–3k rows a year. That stays small in one doc, so no separate doc is needed.

## Shorthand parser (`money::parse`)
```
[+|-]amount  [text…]  [#category]  [@account]  [date words]  [/ split …]
12.50 coffee                → −12.50, payee "coffee", category from rules or last use of the payee
+2400 salaire @sg           → income on SG
45,80 courses #courses hier → comma decimal, yesterday (reuses parse.rs date words)
120 carrefour #courses 90 / #shopping 30   → split
```
- No sign means money out. `#` and `@` match by prefix and ignore accents ("#sant" → Santé).
- Without `@`, the default account is the last one used (per device).

## Core computations (pure, tested)
- `period_of(date, start_day)` → `[start, end)`.
- `budget(period)` → per category `limit, carried, spent, left`, plus totals. `spent` counts booked
  and pending rows, splits by their parts, and leaves out transfers and merged rows. `carried`
  follows the rollover mode back through earlier periods.
- `reconcile(changed ids)`, run after every bank batch, import or manual add (idempotent):
  1. **Pending → booked**: a booked bank row with the same account and amount within 5 days, whose
     label is similar to a pending bank row's, takes over that row's user edits (category, note,
     splits). The pending row is soft-deleted.
  2. **Manual match**: an unmerged manual or imported row on a bank account, same amount, ±3 days →
     the bank row takes the manual row's category, payee and note where it has none, and the manual
     row gets `merged_into`. With several candidates, pick the nearest date, then the older row. A
     tie that remains is left alone and shown in To review.
  3. **Transfers**: an unpaired pair across two own accounts with opposite amounts, ≤3 days apart
     (a `VIR` label boosts the score) → `transfer` is set on both. The user can unpair, and an
     unpaired pair is never paired again.
  4. **Rules**: an uncategorised bank or imported row whose normalised label contains `contains`
     gets `category_source=rule`. Normalising drops `CB`, `PRLV`, `VIR`, card dates (`12/11`), card
     numbers and city suffixes, then upper-cases.
  5. **Recurring**: a row matching a template's `match` text and amount within ±20% marks that
     period paid.
- `reports(n periods)` → per category spent per period, and in/out/net per period.

## Screens
- **Budget** (More → Budget): header "25 Nov → 24 Dec · 312 € left · 9 days". Then **To review (n)**
  and **Due (n)** chips, then category rows with a spent/limit bar (carried amount shown small).
  Tap a row to open its transactions. Swipe the header to step through periods.
- **Transactions**: all accounts, grouped by day, with filter chips (account, category). Pending
  rows are greyed and transfers show ⇄. Tap opens the edit sheet (amount, payee, category, split,
  note, unpair). Change a bank row's category → toast "Always put CARREFOUR in Courses? · Yes".
- **Accounts sheet**: balances, bank status, Reconnect, Import file.
- **Categories editor**: name, emoji, limit (from this period on), rollover mode, order.
- **Reports**: per-category bars for 12 periods, and an in/out/net chart.
- **Widget** (Android): left this period, top 3 categories, + button (quick-add dialog).
- **QS tile / Mac panel**: same shorthand, live preview of the parsed result.

## Import
- OFX/QFX: generic. FITID gives the dedupe id.
- CSV: presets for Boursobank and SG exports (columns, `;` separator, comma decimal, date format),
  which are detected from the header row. Unknown files open a column-mapping sheet.
- Imported rows on an account that is also bank-linked go through reconcile step 2, so history from
  before the link doesn't double up.

## Build order
1. **Core**: model, periods, parser, budget/rollover, splits, reconcile, rules, recurring, reports.
   Plenty of tests (`crates/not-core/tests/money_*.rs`).
2. **Import (core) + Android UI**: More page, Budget, Transactions, edit sheet, Accounts,
   Categories, quick-add. Real data then comes in through CSV right away.
3. **Server bank sync**: Enable Banking client, connect/callback, scheduler, refresh, status, ADR
   0014 accepted.
4. **Mac UI** and quick-entry panel.
5. **Widget, QS tile, alerts, reports.**

## Settled questions
- Boursobank is listed in Enable Banking, so both banks sync.
- Bank redirects go to `NOT_PUBLIC_URL`: the Tailscale hostname for now, a public domain later.
- **More** replaces the Workouts tab.
