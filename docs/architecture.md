# not — architecture

Personal, self-hosted replacement for TickTick (tasks, habits) and Samsung Notes (markdown notes).
Clients: native macOS app (+ Raycast), native Android app (+ widgets), light web companion.
Single user. Bundle/package prefix: `dev.lysen.not`.

The product bet is **speed and system integration**: every action must feel instant, work offline,
and be reachable without opening the app (hotkey panel, Raycast, widgets, notifications).

## Principles

1. **Local-first.** Every client holds the full dataset locally. UI reads and writes local state only;
   the network is never on the interaction path.
2. **CRDT everywhere.** All user data lives in [Loro](https://loro.dev) documents. Merges are automatic
   and deterministic; there is no conflict UI and no server-side merge logic.
3. **One core, native shells.** Domain logic, storage, sync, parsing and recurrence live in one Rust
   crate. UIs are fully native (SwiftUI, Jetpack Compose) and stay thin.
4. **Dumb server.** The server relays and stores opaque CRDT update blobs. It never interprets data,
   which keeps it tiny and leaves end-to-end encryption open.

## Components

```
crates/
  not-core       Rust: Loro docs, SQLite read model, commands, quick-add parser, rrule, sync client
  not-protocol   Rust: wire types shared by core and server
  not-server     Rust (axum): sync relay, device auth, serves the web app
  not-ffi        uniffi bindings → Swift / Kotlin (`NotStore`, `StoreObserver` callback)
  uniffi-bindgen bindings generator used by scripts/
apple/           SwiftUI macOS app (xcodegen `project.yml`) + `NotCore` SwiftPM wrapper; later widgets
android/         Compose app; later Glance widgets, QS tile, share target
scripts/         build-apple.sh (xcframework + Swift bindings), build-android.sh (cargo-ndk + Kotlin bindings)
raycast/         (planned) Raycast extension (TypeScript)
web/             (planned) light web companion (TypeScript + core compiled to wasm)
```

```
           ┌────────────── Docker host ──────────────┐
           │ not-server  ── SQLite (updates log)      │
           │   ├ /v1/push, /v1/pull, /v1/ws           │
           │   └ static web app                       │
           │ ntfy (optional, UnifiedPush for Android) │
           └───────────────────▲─────────────────────┘
                               │ HTTPS + WebSocket pokes
        ┌──────────────────────┼──────────────────────┐
   macOS app              Android app              web
   not-core (Swift)       not-core (Kotlin)        not-core (wasm)
   ├ menubar, hotkey panel ├ Glance widgets
   ├ notifications         ├ QS tile, share target
   └ unix socket ◄─ Raycast└ notification actions
```

## Data model

All data lives in Loro documents. Two kinds of doc:

**`workspace`** — one per user, holds everything small and structural.

| Container | Type | Contents |
|---|---|---|
| `tasks` | `LoroTree` | Nodes are lists and tasks. List → task → subtask. Tasks at the root are in the **Inbox**, which is built in (id `inbox`) rather than a node, so two fresh devices can't each create one. Sibling order via Loro fractional index; move indexes count siblings of the same kind. Each node's meta map holds its fields (`kind`, `title`, `notes` as a mergeable `LoroText`, `completed_at`, `deleted_at`, later `priority`, `due`, `rrule`, …). Deletes are soft (`deleted_at`) so undo restores the same nodes ([ADR 0007](adr/0007-soft-delete-and-undo.md)). |
| `notes` | `LoroTree` | Folders (nesting) and notes. Note meta: `body` (mergeable `LoroText`, markdown), `pinned`, `created_at`, `updated_at`, `deleted_at`; folder meta: `title`. Title and preview are derived from the body ([ADR 0008](adr/0008-notes-in-the-workspace-doc.md)). |
| `habits` | `LoroTree` | Flat; the tree only gives habits a mergeable order. Meta: `title`, `goal` (check-ins per day), `days` (weekday bit mask, bit 0 = Monday), `deleted_at`. |
| `habit_log` | `LoroMap` of `LoroCounter` | Key `habitId:YYYY-MM-DD` (local calendar date from the app, never a timestamp). Counters sum concurrent check-ins; streaks are computed from the read model. |
| `completions` | `LoroMap` (planned) | Recurring-task history keyed `taskId:occurrenceDate`, so completing the same occurrence twice offline is idempotent. |

**`note:<id>`** — per-note docs are deferred: bodies live in the workspace doc for now
([ADR 0008](adr/0008-notes-in-the-workspace-doc.md)).

Moves (task between lists, subtask re-parenting, folder reorganisation) are tree moves: concurrent moves
never duplicate or orphan a node and never create cycles.

## Local storage and the read model

Each client keeps one SQLite file containing:

- **Doc storage** — Loro snapshot + appended update blobs per doc, compacted periodically.
- **Read model** — plain tables (`list`, `task`, later `note_meta`, `habit`, FTS5 indexes) projected from
  the docs. UI queries, widgets and Raycast read only these tables.
- **Sync state** — pull cursor, per-doc version vector the server is known to have, and this device's
  credentials (server URL, device id, token; see [ADR 0006](adr/0006-device-pairing.md)).

The read model is a disposable cache: it can always be rebuilt from the docs. `Store::outline(list)` shapes
it for display (open tasks with their subtrees, then completed ones, latest first) so both apps, the
widget and the menu bar panel share one implementation.

Undo/redo is a per-session `UndoManager` in the core covering this device's own changes only.

Write path:

```
UI action → core command → Loro transaction commit
          → Loro events → dirty node ids → read-model upsert/delete → UI observers
          → update blob appended to doc storage → pushed on next sync
```

Every step is local; target latency for an action is single-digit milliseconds.

### Peer ids

Loro peer ids are random per session (Loro's default). Two processes on one device therefore never share
a peer id. Only one process per device writes to the SQLite file; other processes (Raycast, macOS
extensions) read the read model and send writes to the app over IPC.

## Sync protocol

The server is an append-only log of opaque updates with a global sequence number. Every endpoint but
`/pair` needs `Authorization: Bearer <device token>`, which also tells the server who is calling.

| Endpoint | Purpose |
|---|---|
| `POST /v1/push` | Client sends `{updates: [{doc, data}]}`. Server appends each, returns last `seq`, pokes WebSocket listeners. |
| `GET /v1/pull?since=<seq>` | Returns other devices' updates with `seq > since`, paged. Client imports them; importing a known update is a no-op. |
| `GET /v1/ws` | Server sends `{"seq": n}` after every push so clients pull immediately. |
| `POST /v1/pair` | No token. Trades a one-time pairing code and a device name for a device token. |
| `POST /v1/pairings` | Mints a pairing code for another device. |
| `GET /v1/devices`, `DELETE /v1/devices/{id}` | List paired devices; revoke one. |

Client sync loop (`LiveSync` in `not-core`): push (export updates since the server-known version vector)
→ pull (import since cursor) → advance cursor. Triggered by local commits (150 ms debounce), WebSocket
pokes, app foreground (`sync_now`), and every 60 s as a safety net. Failures back off from 1 s to 30 s;
a 401 (device revoked) pauses sync until the user pairs again. The server sends its head on WebSocket
connect, so a reconnect always catches up. Pairing (again) resets the cursor and server version vector,
so a new or reset server receives the full history.

Client transport: `reqwest` + `tokio-tungstenite` over rustls with the ring provider and Mozilla's root
store (`webpki-roots`). No platform verifier means no JNI setup on Android and no aws-lc cross-compile;
the trade-off is that the server needs a publicly trusted certificate (Tailscale/Caddy/Let's Encrypt are
fine; a private CA is not, yet).

Compaction (planned): a client uploads a snapshot `PUT /v1/docs/<doc>/snapshot?upto=<seq>`; the server
drops older updates for that doc and serves the snapshot to clients whose cursor is behind it.

Encoding: JSON with base64 payloads for now (easy to debug, web-friendly); can move to a binary framing
without changing the model.

## Server

- One container: `not-server` binary + SQLite volume. Litestream optional for continuous backup.
- Single user, no secrets in the environment. Devices pair with one-time codes and get revocable
  per-device tokens (stored hashed). The first code comes from the CLI on the host
  (`not-server pair`); paired devices mint more. See [ADR 0006](adr/0006-device-pairing.md).
- CLI: `not-server [serve]`, `pair [public URL]` (prints a QR code when the URL is known),
  `devices`, `revoke <id>`.
- Runs behind the user's reverse proxy or Tailscale; TLS is terminated there.

## Reminders and notifications

Reminders are scheduled **on each device** from synced data, never pushed by the server, so they fire
offline and on time. Completing a task anywhere cancels its reminder everywhere on the next sync.

- macOS: `UNUserNotificationCenter` with Complete / Snooze actions. The app keeps running after its
  window closes (menu bar extra, live sync over the WebSocket); `not://` links are handled without
  opening the window.
- Android (sideloaded, no Play policy limits): `AlarmManager` exact alarms (`USE_EXACT_ALARM`), WorkManager
  for periodic sync, optional UnifiedPush via self-hosted ntfy for instant wake-ups.

## Background sync on Android

Live sync runs whenever the process is up (the `Application` opens the store and starts it, for the
activity, widgets and workers alike). `SyncWorker` (WorkManager, network-constrained) covers the rest:
every 15 minutes, so widgets stay fresh, and right after edits made from a widget or quick add, so they
reach the server even if the app is never opened. It calls the FFI's one-shot `sync()`; a pull cursor
never moves backwards, so a worker and live sync running at once are harmless.

## System integration

| macOS | Android |
|---|---|
| Menu bar extra ✅ (quick add, open tasks of a chosen list); global hotkey → quick-add panel with natural-language parsing | Glance tasks widget ✅ (any list, checkboxes, + opens a quick-add dialog); later today list, habit grid |
| Raycast extension over a local unix socket (add, search, today, check habit, new note) | Quick Settings tile ✅ and app-icon shortcut ✅ → quick-add dialog |
| Local notifications with actions | Share target → task or note |
| `not://` URL scheme, App Intents (Shortcuts), Core Spotlight indexing | Notification actions, app shortcuts |
| Later (needs paid Apple Developer account): WidgetKit widgets, share extension, notarized builds | |

## Web companion

Light: view, search, quick edits, quick add. Same sync protocol; runs `not-core` compiled to wasm with
an IndexedDB cache. Online-first; no parity goal.

## Roadmap

| Phase | Scope |
|---|---|
| P0 spike ✅ | core (tasks in Loro + read model), relay server, live sync, uniffi → minimal SwiftUI + Compose apps, verified Mac ⇄ Android end to end incl. offline edits |
| P0.5 ✅ | Built-in Inbox, device pairing with per-device tokens, revocation |
| P1 (in progress) | ✅ Android tasks widget (with picker preview), quick add from widget / QS tile / app shortcut, app icons (Mac: Icon Composer / Liquid Glass), Mac menu bar extra, background sync, QR pairing; core UX: notes, subtasks, rename, reorder, completed section, undo, soft delete, keyboard flow (Mac), swipe actions + drawer + settings screen (Android). Next: quick-add parser, due dates, Today, reminders, recurrence, hotkey panel, Raycast |
| P1 | Quick-add parser, recurrence, reminders, hotkey panel, Raycast, Android widgets |
| P2 ✅ | Notes: folders, pinning, search, live-styled markdown editors (core span highlighter) on Mac + Android |
| P3 ✅ | Habits: check-ins, goals, weekday schedules, streaks, history grid (Mac + Android) |
| P4 | Web companion, TickTick CSV import, compaction |

## Decision records

See [`adr/`](adr/).
