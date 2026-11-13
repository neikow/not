# Budgeting — design

Status: design agreed 2026-11-13; core built (`crates/not-core/src/money.rs`, `money_parse.rs`), apps not yet. Entry is fully manual, plus file import. Bank sync was
considered and rejected ([ADR 0014](adr/0014-bank-sync-on-the-server.md)).

## Scope (decisions)
| # | Decision |
|---|----------|
| Model | Limit per category per period, with spent and left. Income tracked; net = in − out. |
| Period | Starts on a chosen day of the month (payday). Clamped to the last day in short months. |
| Currency | One currency (EUR). Amounts are `i64` cents, negative = money out. |
| Accounts | Several (bank, cash, savings) with balances. **Set balance** adds an adjustment row that stays out of the budget. |
| Entry | Quick-add shorthand parsed in core. CSV/OFX import from bank exports, for backfill and catching up. |
| Categories | Starter set: Courses, Resto, Transport, Logement, Abonnements, Loisirs, Santé, Shopping, Revenus, Autre. Editable, each with an emoji. A payee defaults to the category it had last. |
| Rollover | Per category: `none` · `leftover` (carry unspent only) · `both` (carry unspent and overspend). |
| Transfers | Entered as one row (`500 @sg > @bourso`). It moves both balances and is never spending. |
| Recurring | Templates become **Due** on their date. One tap enters them (amount editable). |
| Splits | One row can be split across categories. |
| Alerts | 80% / 100% of a category, toggled per device like reminders. |
| Place | New **More** page (replaces the Workouts tab) holding Workouts and Budget. |
| Capture | QS tile, widget +, and the Mac quick-entry panel all use the same parser. |
| Web | Nothing for now. |

## Data model (workspace doc, root containers)
All of these are `LoroMap`s keyed by id. Each value is a nested map, so fields merge per field
(last write wins). Deletes are soft (`deleted_at`, per 0007). Maps rather than trees, even for the
ordered accounts and categories: rows point at them by id, and a Loro redo of a tree create
gives the node a new id. Order is an `order` number (new ones last, a move takes the midpoint).
Nothing is projected into SQLite; figures are computed from the doc on each call.

The starter categories have fixed ids (`cat-courses`, …) and are created as mergeable maps, so
two devices seeding offline end up with one set. `seeded` in settings stops it happening again,
even after the user deletes them all.

- `money_settings`: `period_start_day` (1–31), `currency` ("EUR").
- `money_accounts`: id → `name, kind (bank|cash|savings), opening_balance, order, deleted_at`.
- `money_categories`: id → `name, emoji, kind (expense|income), rollover (none|leftover|both),
  limits: map<period_start "YYYY-MM-DD", cents>, order, deleted_at`. A period's limit is the latest
  entry at or before the period's start, so changing a limit doesn't rewrite the past and rollover
  stays correct.
- `money_txns`: id → `kind (entry|transfer|adjustment), account, to_account? (transfer), date,
  amount, payee, label? (raw text from an import), note, category?, splits?: list<{category,
  amount}>, source (manual|import), import_ref?, merged_into?, recurring?, deleted_at`.
  - A transfer stores the amount moved as positive, from `account` to `to_account`.
  - An adjustment is written by Set balance: amount = real balance − computed balance.
  - Ids: manual = random; import = `imp:<account>:<FITID or hash(date, amount, label, n-th
    duplicate)>`, so importing the same file twice adds nothing.
  - `merged_into` hides an imported row that matched a row already entered by hand.
- `money_recurring`: id → `name, amount (usual, signed), account, category?, repeat (RRULE subset,
  as tasks), next_due, deleted_at`. Entering it writes a row dated `next_due` and moves `next_due`
  on, in one undo step; Skip moves it on with no row.

Size: about 2–3k rows a year. That stays small in one doc, so no separate doc is needed.

## Shorthand parser (`Store::parse_money`, `quick_add_money`)
```
[+|-]amount  [text…]  [#category]  [@account [> @account]]  [date words]  [/ split …]
12.50 coffee                → −12.50, payee "coffee", category from the payee's last use
+2400 salaire @sg           → income on SG
45,80 courses #courses hier → comma decimal, yesterday
120 carrefour #courses 90 / #shopping   → split; the part without an amount takes the rest
500 @sg > @bourso           → transfer
```
- No sign means money out, unless the category is an income one. `#` and `@` match by prefix and
  ignore accents ("#sant" → Santé).
- Dates look back: "lundi" = the last Monday (today if it is Monday), "lundi dernier" / "last mon"
  the one before today, "12/11" and "3 nov" the last such day, "le 20" this month or last.
- Without `@`, the account is that of the last row entered by hand (synced, so the same on every
  device). Without `#`, the category is the one the payee's latest entry had.
- The result carries `problem` ("Type an amount", "The parts add up to 110.00, not 120.00", …)
  for the live preview; Enter is refused until it's `None`.

## Core computations (pure, tested)
- `period_of(date, start_day)` → `[start, end)`.
- `budget(period)` → per category `limit, carried, spent, left`, plus totals. `spent` counts entries
  (splits by their parts) and leaves out transfers, adjustments and merged rows. `carried` follows
  the rollover mode back through earlier periods, starting at the first period with a limit or a
  row. The header's `left` = Σ (limit + carried) over expense categories − all money out, unsorted
  and unlimited categories included. Rows dated later in the period count already.
- `balance(account, date)` = opening + entries + adjustments ± transfers.
- `due_recurring(today)` → templates whose `next_due` is today or earlier.
- `import_preview(account, file)` → each row marked **new**, **already imported** (same
  `import_ref`), or **matches** an existing manual row. A match needs the same account, the same
  amount (or a transfer leg touching that account) and a date within ±3 days. With several
  candidates, pick the nearest date, then the older row; a tie that remains is shown as new so you
  can decide. Applying the preview is one undo step. For a match, the manual row keeps its category,
  payee and note, takes the bank date and label, and the imported row gets `merged_into`. A new row
  takes the category its normalised label had last. A row with no category goes to **To review**.
  Normalising drops `CB`, `PRLV`, `VIR`, card dates (`12/11`), card numbers and city suffixes, then
  upper-cases.
- `reports(n periods)` → per category spent per period, and in/out/net per period.

## Screens
- **Budget** (More → Budget): header "25 Nov → 24 Dec · 312 € left · 9 days". Then **Due (n)** and,
  after an import, **To review (n)** chips, then category rows with a spent/limit bar (carried
  amount shown small). Tap a row to open its transactions. Swipe the header to step through periods.
- **Transactions**: all accounts, grouped by day, with filter chips (account, category). Transfers
  show ⇄ and adjustments show ±. Tap opens the edit sheet (amount, payee, category, split, note,
  date, account).
- **Accounts sheet**: balances, Set balance, Import file.
- **Import preview**: new, matched and already-imported rows. Untick a row to skip it, then Import.
- **Categories editor**: name, emoji, limit (from this period on), rollover mode, order.
- **Reports**: per-category bars for 12 periods, and an in/out/net chart.
- **Widget** (Android): left this period, top 3 categories, + button (quick-add dialog).
- **QS tile / Mac panel**: same shorthand, live preview of the parsed result.

## Import formats
- OFX/QFX: generic. FITID gives `import_ref`.
- CSV: presets for Boursobank and SG exports (columns, `;` separator, comma decimal, date format),
  which are detected from the header row. Unknown files open a column-mapping sheet.

## Build order
1. **Core**: model, periods, parser, budget/rollover, splits, transfers, balances, recurring,
   reports. Plenty of tests (`crates/not-core/tests/money_*.rs`).
2. **Android UI**: More page, Budget, Transactions, edit sheet, Accounts, Categories, quick-add.
3. **Import**: parsers and preview in core, then the preview sheet.
4. **Mac UI** and quick-entry panel.
5. **Widget, QS tile, alerts, reports.**
