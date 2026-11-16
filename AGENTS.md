# AGENTS.md

Guide for coding agents (and people) working in this repo. Read it before changing code.

**not** is a single-user, self-hosted, local-first tasks / notes / habits / workouts app: a native
macOS app, a native Android app (sideloaded, installed and updated with Obtainium), and a small
sync server that also serves a light web companion. Product bet: speed and system integration.
Background and reasoning: [docs/architecture.md](docs/architecture.md) and the decision records in
[docs/adr/](docs/adr/).

## Layout

```text
crates/
  not-core/        All domain logic. Loro CRDT docs + SQLite read model, commands, quick-add parser,
                   schedules/repeats, views (smart lists), notes, habits, workouts, export/backup,
                   sync client (`net` feature).
    src/store.rs       Store: opening, transactions, task commands, undo
    src/views.rs       Built-in views (Inbox/Today/Upcoming/Someday), saved filters, per-view order
    src/filter.rs      Smart-list query language
    src/parse.rs       Quick-add parser (English + French dates, times, repeats, #tags, !priority)
    src/schedule.rs    Start/deadline/someday, RRULE subset, due labels
    src/projection.rs  Loro → SQLite read model
    src/notes.rs, checklist.rs, markdown.rs, images.rs   Notes, checklists, editor spans, blobs
    src/habits.rs, workouts.rs, catalog.rs               Habits, workouts, exercise catalogue
    src/export.rs      JSON export, backup/restore
    src/sync.rs        SyncClient, LiveSync (push/pull/WebSocket)
    tests/             Integration tests, one file per area; tests/common = in-memory server log;
                       tests/fixtures = backups made by older versions (migration tests)
  not-protocol/    Wire types shared by core and server (also pairing links)
  not-server/      axum + SQLite relay: /v1 sync API, pairing, devices, blobs, shares; embeds a
                   not-core replica for the web pages (src/web.rs). Binary: serve | pair | devices | revoke
  not-ffi/         uniffi surface (`NotStore`, `StoreObserver`, records) for Swift and Kotlin.
                   Strings for dates (`YYYY-MM-DD`), UTF-16 offsets for text ranges.
  uniffi-bindgen/  Bindings generator used by scripts/
apple/             macOS SwiftUI app (xcodegen `project.yml`); NotCore = SwiftPM wrapper of the
                   generated xcframework + Swift bindings
android/           Compose app. Own design system in ui/ (no Material 3). widget/ = Glance widgets.
                   core/ (generated Kotlin bindings) and jniLibs/ are build outputs, not sources
scripts/           build-apple.sh, build-android.sh (core + bindings), fetch-exercises.sh
docs/              architecture.md, adr/NNNN-*.md
.github/workflows/ CI: tests on every push/PR, signed APK release on main
```

## Where code goes

- **Logic lives in not-core.** Parsing, sorting, filtering, dates, recurrence, merging rules, labels
  shared by several screens: Rust, tested in Rust. The apps stay thin: render, gestures, platform
  integrations (notifications, widgets, calendar, location).
- A new core command is exposed through `crates/not-ffi/src/lib.rs`, then used from Swift and Kotlin.
  Regenerate bindings with the build scripts (`make android`, `make mac`) — never edit generated files.
- All user data goes into the Loro workspace doc (see the data model table in
  docs/architecture.md). Never store user data only in SQLite: the read model is a disposable cache
  rebuilt from the docs. Per-device settings (reminder times, calendar overlay, places) stay in
  SharedPreferences / UserDefaults.
- Deletes are soft (`deleted_at`) so undo and concurrent edits work (ADR 0007). Write only the fields
  that changed, so concurrent edits of different fields both survive.
- The server never interprets synced data, except through its replica for the web pages and shares.
- A decision that changes the model or a user-visible behaviour gets an ADR in docs/adr/ (next
  number, short: context, decision, consequences), and docs/architecture.md is updated.

## Build, run, test

`make` lists every shortcut. Prerequisites: Rust stable, Xcode + xcodegen (Mac), Android SDK + NDK,
`rustup target add aarch64-linux-android`, `cargo install cargo-ndk`, JDK 17+ (Android Studio's:
`/Applications/Android Studio.app/Contents/jbr/Contents/Home`).

| Command | What |
| --- | --- |
| `make check` | Everything CI runs: fmt check, clippy `-D warnings`, Rust tests, Android unit tests |
| `make test` | Rust tests (`cargo test --workspace --all-features`) |
| `make android-test` | Android JVM unit tests (android/app/src/test) |
| `make coverage` | Per-file Rust line coverage (cargo-llvm-cov) |
| `make android` / `make emu` | Debug APK / install on the emulator |
| `make mac` / `make mac-run` | Build / run the Mac app |
| `make serve`, `make serve-pair` | Run the server locally on :8081, get a pairing code |
| `make release-apk` | Signed release APK locally (needs `~/.android/not-release.env`) |

## Testing rules

Every feature and every bug fix comes with tests; nothing merges with `make check` failing.

- Core behaviour: integration tests in `crates/not-core/tests/<area>.rs` (tasks, notes, checklists,
  habits, workouts, store). Pure helpers may have unit tests in the module. Use fixed dates
  (`Date::parse("2026-11-08")`), never the clock.
- Sync and merging: test two or more stores exchanging updates through `tests/common::Log`, including
  concurrent edits and undo after sync.
- Data migrations: keep a backup fixture made by the *previous* version in `tests/fixtures/` and test
  that it opens correctly (see `lists_become_tags_and_saved_views`). Fixtures are never regenerated.
- Server and web: `crates/not-server/tests/` drive the router in-process (`tower::ServiceExt::oneshot`)
  and pair real `SyncClient`s against a server on a random port. Check escaping of user text.
- FFI: `crates/not-ffi/tests/ffi.rs` covers the app-facing surface (string dates, error mapping,
  UTF-16 offsets, observers, a store on disk).
- Android: JVM tests in `android/app/src/test` for Kotlin logic that doesn't call the core (the native
  library isn't loaded there). Anything else worth testing belongs in the core.
- UI changes: verify on the emulator (screenshots) and in a separate Mac instance with a scratch data
  dir (`-dataDir <dir> -seedDemo YES -show <page>` in DEBUG builds). Don't automate the user's own
  Mac or phone.

## Releases (Android, Obtainium)

`.github/workflows/ci.yml`: every push and PR runs the Rust job (fmt, clippy, tests), then the Android
job (core build for arm64-v8a, unit tests). On a push to `main` that passes, it also builds a release
APK signed with the release key, and publishes GitHub release `v0.1.<run number>` with
`not-0.1.<run number>.apk` attached. `versionCode` = the workflow run number, so each release installs
over the previous one.

- Signing secrets (repo settings → Secrets → Actions): `NOT_KEYSTORE_BASE64`, `NOT_KEYSTORE_PASSWORD`,
  `NOT_KEY_ALIAS`, `NOT_KEY_PASSWORD`. The keystore itself lives outside the repo
  (`~/.android/not-release.jks` + `not-release.env`). Losing it means users must uninstall to update.
- Obtainium: add the GitHub repo URL as the source; the repo is private, so give Obtainium a GitHub
  token with read access to it (Obtainium settings → GitHub personal access token).
- Renaming the workflow file resets the run number: bump versions past the last release if you do.
- Local builds keep versionCode 1 and the debug key, so they don't install over a release build.

## Gotchas

- Latest AndroidX needs `compileSdk = 37`. Release builds don't minify (R8 would need JNA/uniffi
  keep rules).
- uniffi: an error field can't be named `message` (clashes with Kotlin's `Throwable.message`).
- TLS is rustls + ring + webpki-roots: servers need publicly trusted certificates.
- Text offsets crossing the FFI are UTF-16 code units (what Kotlin and NSString use), not bytes.
- Loro: a redo of tree creates makes nodes with new ids — don't key anything durable on an id
  that undo/redo may recreate.
- Mac editor uses TextKit 1 (`usingTextLayoutManager: false`): asking a TextKit 2 view for
  `layoutManager` blanks the text.
- Compose `OutputTransformation` paragraph styles must include the trailing `\n` and need
  `LineHeightStyle.Trim.None`.
- Dev hooks — Android intent extras (pairing, demo data, pinning widgets) and Mac launch args
  (`-dataDir`, `-seedDemo`) — are behind `BuildConfig.DEBUG` / `#if DEBUG`; keep new ones there too,
  since release APKs go to a real phone.

## Style

- Rust: `cargo fmt` (max width 110), clippy clean with `-D warnings`. Doc comments say what and why,
  in plain words.
- Kotlin/Swift: follow the surrounding code; Android UI uses `ui/` components and `Not.colors` /
  `Not.type`, never Material 3.
- UX: undo instead of confirmation dialogs; no layout shift; offline first, the network never sits
  on the interaction path.
- Commits: imperative summary line describing the user-visible change, body for the why.
