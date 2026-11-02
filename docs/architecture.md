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
| `tasks` | `LoroTree` | Nodes are lists and tasks. List → task → subtask. Sibling order via Loro fractional index. Each node's meta map holds its fields (`kind`, `title`, `completed_at`, `priority`, `due`, `rrule`, …). |
| `notes` | `LoroTree` (planned) | Folder → note-ref nodes. Meta holds `title`, `pinned`, `preview`, so note lists render without loading bodies. |
| `habits` | `LoroMap` (planned) | Habit definitions: schedule, goal, unit. |
| `habit_log` | `LoroMap` of `LoroCounter` (planned) | Key `habitId:YYYY-MM-DD` (local calendar date, never a timestamp). Counters sum concurrent check-ins. |
| `completions` | `LoroMap` (planned) | Recurring-task history keyed `taskId:occurrenceDate`, so completing the same occurrence twice offline is idempotent. |

**`note:<id>`** — one per note (planned). `LoroText` body holding markdown. Loaded lazily; carries its own
history (version history and undo come for free).

Moves (task between lists, subtask re-parenting, folder reorganisation) are tree moves: concurrent moves
never duplicate or orphan a node and never create cycles.

## Local storage and the read model

Each client keeps one SQLite file containing:

- **Doc storage** — Loro snapshot + appended update blobs per doc, compacted periodically.
- **Read model** — plain tables (`list`, `task`, later `note_meta`, `habit`, FTS5 indexes) projected from
  the docs. UI queries, widgets and Raycast read only these tables.
- **Sync state** — pull cursor, per-doc version vector the server is known to have.

The read model is a disposable cache: it can always be rebuilt from the docs.

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

The server is an append-only log of opaque updates with a global sequence number.

| Endpoint | Purpose |
|---|---|
| `POST /v1/push` | Client sends `{device, updates: [{doc, data}]}`. Server appends each, returns last `seq`, pokes WebSocket listeners. |
| `GET /v1/pull?since=<seq>` | Returns updates with `seq > since`, paged. Client imports them; importing a known update is a no-op. |
| `GET /v1/ws` | Server sends `{"seq": n}` after every push so clients pull immediately. |

Client sync loop (`LiveSync` in `not-core`): push (export updates since the server-known version vector)
→ pull (import since cursor) → advance cursor. Triggered by local commits (150 ms debounce), WebSocket
pokes, app foreground (`sync_now`), and every 60 s as a safety net. Failures back off from 1 s to 30 s.
The server sends its head on WebSocket connect, so a reconnect always catches up.

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
- Single user. Auth: admin secret from the environment; devices pair via QR code and get revocable
  per-device tokens (spike uses one static bearer token).
- Runs behind the user's reverse proxy or Tailscale; TLS is terminated there.

## Reminders and notifications

Reminders are scheduled **on each device** from synced data, never pushed by the server, so they fire
offline and on time. Completing a task anywhere cancels its reminder everywhere on the next sync.

- macOS: `UNUserNotificationCenter` with Complete / Snooze actions. The menu-bar app stays resident and
  keeps the WebSocket open.
- Android (sideloaded, no Play policy limits): `AlarmManager` exact alarms (`USE_EXACT_ALARM`), WorkManager
  for periodic sync, optional UnifiedPush via self-hosted ntfy for instant wake-ups.

## System integration

| macOS | Android |
|---|---|
| Menu-bar app, global hotkey → quick-add panel with natural-language parsing | Glance widgets: today list with checkboxes, habit grid, quick add |
| Raycast extension over a local unix socket (add, search, today, check habit, new note) | Quick Settings tile → quick-add sheet |
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
| P1 | Quick-add parser, recurrence, reminders, hotkey panel, Raycast, Android widgets |
| P2 | Notes: per-note docs, markdown editors (biggest UX effort) |
| P3 | Habits |
| P4 | Web companion, TickTick CSV import, compaction, device pairing |

## Decision records

See [`adr/`](adr/).
