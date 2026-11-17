# not

Self-hosted, local-first tasks, notes and habits for macOS and Android.
Architecture: [docs/architecture.md](docs/architecture.md) · decisions: [docs/adr/](docs/adr/).

## Layout

| Path | What |
|---|---|
| `crates/not-core` | Shared Rust core: Loro docs, SQLite read model, commands, sync client (`net` feature) |
| `crates/not-protocol` | Wire types shared by core and server |
| `crates/not-server` | Sync relay (axum + SQLite) |
| `crates/not-ffi` | uniffi bindings for the apps |
| `apple/` | macOS app (SwiftUI); `NotCore` Swift package wraps the Rust core |
| `android/` | Android app (Compose) |
| `scripts/` | Build the Rust core for each platform and generate bindings |

## Develop

```sh
cargo test --workspace --all-features
```

## Run the apps

`make` lists dev shortcuts (`make server`, `make pair URL=…`, `make mac-run`, `make phone`, …); the
steps below are what they run.

Prerequisites: Xcode, xcodegen, Android SDK + NDK, `rustup target add aarch64-linux-android`,
`cargo install cargo-ndk`.

```sh
# macOS
./scripts/build-apple.sh
cd apple && xcodegen generate && open Not.xcodeproj      # run the NotMac scheme

# Android (Android Studio's JDK works: export JAVA_HOME="/Applications/Android Studio.app/Contents/jbr/Contents/Home")
./scripts/build-android.sh
cd android && ./gradlew installDebug
```

Pair each app with the server (below) in its sync settings: macOS Settings (⌘,), Android "Sync". For
local testing the emulator reaches the host at `http://10.0.2.2:8080`. Dev shortcuts:

```sh
open "not://add?title=Milk"                                       # macOS: quick add to the Inbox
open not.app --args -serverURL http://127.0.0.1:8080 -pairingCode CODE   # macOS: pair on launch
adb shell am start -n dev.lysen.not/.MainActivity --es server http://10.0.2.2:8080 --es code CODE  # debug builds only
adb shell am start -n dev.lysen.not/.MainActivity --ez pinWidget true   # debug: ask the launcher to place the widget
adb shell am start -n dev.lysen.not/.MainActivity --ez addTile true     # debug: offer the Quick Settings tile
```

## Run the server

```sh
docker compose up -d --build
curl localhost:8080/healthz
docker compose exec not not-server pair https://not.example.ts.net   # one-time code + QR (valid 10 min)
```

Scan the QR code with the Android app (Sync → Scan QR code), or enter the server URL and code by hand.
Set `NOT_PUBLIC_URL` in the environment to skip the URL argument. Further devices get codes from a paired one (Add device) or
from `not-server pair` again. `not-server devices` and `not-server revoke <id>` manage them from the host.

Data lives in the `not-data` volume (`/data/not.db`). Set `NOT_DB_KEY` (e.g. in `.env`) to encrypt it at
rest; an existing plain database is encrypted on the next start. Keep the key safe: without it the
server's copy can't be read (devices still hold everything and can re-upload to a fresh server).
The sync log is compacted into a snapshot as it grows (`not-server compact` does it on demand). Put the server behind your reverse proxy or Tailscale
for TLS: the apps only trust publicly trusted certificates.
