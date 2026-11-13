# 0014 — The server fetches bank transactions and writes them into the workspace

Status: **rejected** (2026-11-13). Would have amended [0003](0003-server-as-relay.md) and built on
[0009](0009-server-replica-for-web-and-sharing.md).

**Why rejected:** the owner decided against a bank integration that may end up needing a paid plan,
and against the extra moving parts (a server secret, a scheduler, consent renewal). Budgeting is
manual entry plus CSV/OFX import from bank exports instead; see `docs/budgeting.md`. The
proposal is kept below for reference.

## Context
Budgeting needs the transactions from the owner's two banks, Boursobank and Société Générale,
without typing them in. The EU's open banking rules (PSD2) let an account information service read
accounts the owner consents to. GoCardless Bank Account Data, the usual free route, stopped taking
new sign-ups in July 2025. Enable Banking offers a free "restricted production" mode that returns
data only for accounts the application owner links themselves, which fits a single-user app (0004).
Its French docs list Société Générale, where consent is approved in L'Appli SG, and the owner
confirmed Boursobank is listed in the control panel too.

Consent expires, and the bank must renew it after a while (up to 180 days). Unattended fetches are
capped at about 4 a day per account. Fetches the owner starts while present are not capped.

## Decision
- **Bank sync runs on the server**, not in the apps. It works while every device is off, and only
  the server holds the Enable Banking key.
- **Secrets:** the Enable Banking application id and RSA private key are passed to the server as a
  mounted file (`NOT_BANK_KEY`, mode 600) plus an env var for the app id. This is the first secret the
  server needs; 0006 kept pairing secret-free, and that does not change. Without the key the server
  runs as before and the apps hide the bank options.
- **Linking** starts in an app: `POST /v1/bank/connect {bank}` returns the bank's authorisation
  URL, which the app opens in the browser. The bank redirects to `/bank/callback` on the server,
  which opens the session, stores it in a server DB table `bank_sessions` (session id, bank,
  valid until, the account uids), and shows "Connected, return to the app". The redirect URL must be
  registered with Enable Banking, so it is built from `NOT_PUBLIC_URL`, which the phone's browser
  must reach. For now that is the server's Tailscale hostname; a public domain comes later, and
  switching means registering the new redirect URL with Enable Banking. Existing sessions keep
  working, because the redirect is only used while linking.
- **Fetching**: a scheduler fetches every linked account 4 times a day. `POST /v1/bank/refresh`
  (pull to refresh in an app) fetches with the owner's PSU headers so it counts as owner-present.
  Each fetch asks for transactions from `last_booked_date − 5 days`, which catches late bookings.
- **Writing**: new and changed transactions are written into the replica, like a web edit (0009),
  as pusher `bank`, then the devices are poked. Each bank transaction gets a **deterministic id**
  (`eb:<account uid>:<transaction id or entry reference>`), so a refetch, or a replica rebuilt from
  the log, never duplicates anything.
- **Reconciliation runs in the core** (`money::reconcile`): pending→booked replacement, matching
  manual entries, pairing transfers, applying category rules. The server calls it right after
  writing a batch, and the apps call it after a manual entry. It is deterministic and idempotent, so
  two writers that run it reach the same state.
- **Status for the apps**: each linked money account carries `bank_status` in the doc
  (`consent_until`, `last_fetch`, `error`). Apps show a quiet "Reconnect SG" row 14 days before
  consent ends, or straight away after an auth error. Settings → Banks lists sessions and can
  disconnect (`DELETE /v1/bank/sessions/{id}`, which also revokes the session at Enable Banking).

## Consequences
- The server is no longer just a relay with a replica: it now **creates data** and holds a
  credential that reads bank accounts. Anyone who takes over the server can read account history,
  but cannot move money (account information access only).
- End-to-end encryption, already made unlikely by 0009, is effectively off the table.
- If Enable Banking drops the free mode or a bank breaks, CSV/OFX import in the apps still works;
  imported and fetched rows for the same payment are matched by amount, date and label (see
  `docs/budgeting.md`).
- One more moving part to keep running: consent renewal is manual about twice a year per bank.
