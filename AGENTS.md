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
    src/habits.rs, timed.rs, workouts.rs, catalog.rs     Habits (check-ins and timed), workouts, exercise catalogue
    src/trash.rs       Deleted items: list, restore, delete for good, 30-day purge
    src/export.rs      JSON export, backup/restore
    src/sync.rs        SyncClient, LiveSync (push/pull/WebSocket)
    tests/             Integration tests, one file per area; tests/common = in-memory server log;
                       tests/fixtures = backups made by older versions (migration tests)
  not-protocol/    Wire types shared by core and server (also pairing links)
  not-server/      axum + SQLite relay: /v1 sync API, pairing, devices, blobs, shares; embeds a
                   not-core replica for the web pages (src/web.rs) and log compaction; SQLCipher at
                   rest with NOT_DB_KEY. Binary: serve | pair | devices | revoke | compact
  not-ffi/         uniffi surface (`NotStore`, `StoreObserver`, records) for Swift and Kotlin.
                   Strings for dates (`YYYY-MM-DD`), UTF-16 offsets for text ranges.
  uniffi-bindgen/  Bindings generator used by scripts/
apple/             macOS SwiftUI app (xcodegen `project.yml`); NotCore = SwiftPM wrapper of the
                   generated xcframework + Swift bindings
android/           Compose app. Own design system in ui/ (no Material 3). widget/ = Glance widgets
                   (one setup activity for all, looks and options, layouts by size).
                   core/ (generated Kotlin bindings) and jniLibs/ are build outputs, not sources
scripts/           build-apple.sh, build-android.sh (core + bindings), fetch-exercises.sh
docs/              architecture.md, adr/NNNN-*.md
deploy/            portainer-stack.yml: production server stack (ghcr image, `nginx` network)
.github/workflows/ ci.yml: tests on every push/PR; a version tag releases (APK, Mac installer,
                   server image);
                   mac.yml: rebuild a release's Mac installer (.dmg), run by hand
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
- The server never interprets synced data, except through its replica for the web pages, shares and
  log compaction. Only the server links SQLCipher (`bundled-sqlcipher-vendored-openssl`); keep it
  out of not-core/not-ffi so the apps stay on plain SQLite.
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
| `make dmg` | Release build as a drag-and-drop installer, `build/not.dmg` (`DMG_LAYOUT=0` skips the Finder layout step) |
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
- Android logic: JVM tests in `android/app/src/test` for Kotlin rules that don't need the core.
- Android screens: `android/app/src/test/.../screens` run the whole app under Robolectric with
  Compose UI tests, on the **real Rust core** (its host build in `target/release`, which
  `scripts/build-android.sh` makes; JNA's desktop jar loads it). Use `TestApp` (WorkManager's test
  version), `NotAppUi` with real view models, and fresh data per test. Add one for every new screen
  flow: a click path and what the store holds afterwards.
- Widgets: debug builds render one widget at every common size, from 1×1 to 4×3:
  `adb shell am start -S -n dev.lysen.not/.widget.WidgetSetupActivity --es sizes Timed` (any
  `WidgetKind` name). Glance rows and columns hold at most 10 children, and widget picker
  previews (res/layout/widget_*_preview.xml) may only use RemoteViews classes (a test checks).
- UI changes: verify on the emulator (screenshots) and in a separate Mac instance with a scratch data
  dir (`-dataDir <dir> -seedDemo YES -show <page>` in DEBUG builds). Don't automate the user's own
  Mac or phone.

## Releases (tags)

A release is a version tag on `main`: `git tag v0.2.3 && git push origin v0.2.3`. Pushes to `main`
and pull requests only run the tests (`.github/workflows/ci.yml`: Rust fmt, clippy, tests; Android
core build and unit tests). A `vMAJOR.MINOR.PATCH` tag, once its tests pass, runs:

- **android**: the release APK signed with the release key, `versionName` = the tag's version,
  `versionCode` = MAJOR·10000 + MINOR·100 + PATCH (so minor and patch stay under 100, and each
  release installs over the last);
- **mac**: the Release app, ad-hoc signed, packed as the drag-and-drop installer
  (`scripts/make-dmg.sh`);
- **release**: GitHub release `vX.Y.Z` with `not-X.Y.Z.apk` and `not-mac-X.Y.Z.dmg`, notes listing
  every commit since the previous tag. Obtainium picks up the APK;
- **server-image**: see below.

A tag that isn't on `main`, or isn't `vMAJOR.MINOR.PATCH`, fails before anything is built.
`mac.yml` (run by hand) rebuilds the installer for an existing release.

- Signing secrets (repo settings → Secrets → Actions): `NOT_KEYSTORE_BASE64`, `NOT_KEYSTORE_PASSWORD`,
  `NOT_KEY_ALIAS`, `NOT_KEY_PASSWORD`. The keystore itself lives outside the repo
  (`~/.android/not-release.jks` + `not-release.env`). Losing it means users must uninstall to update.
- Obtainium: add the GitHub repo URL as the source; the repo is private, so give Obtainium a GitHub
  token with read access to it (Obtainium settings → GitHub personal access token).
- Versions only go up: never reuse or lower a tag (the last push-based release was v0.1.11).
- Local builds keep versionCode 1 and the debug key, so they don't install over a release build.

## Server deploy (ghcr, Portainer)

With a release (after it's published), CI's `server-image` job builds the Dockerfile and pushes
`ghcr.io/neikow/not-server:latest` and `:X.Y.Z`, then POSTs the Portainer stack webhook
(`PORTAINER_WEBHOOK_URL` secret; skipped with a warning if unset). Pushes to `main` don't deploy.
The stack is `deploy/portainer-stack.yml`: no published ports, joins the external `nginx` network,
and Nginx Proxy Manager forwards `not.lysen.dev` to `http://not-server:8080` with WebSockets on.
`NOT_DB_KEY` lives in the stack's environment in Portainer; keep a copy with the backups.

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
