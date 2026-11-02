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

Point both at a server in their sync settings. For local testing the emulator reaches the host at
`http://10.0.2.2:8080`. Dev shortcuts:

```sh
open "not://add?title=Milk"                                  # macOS: quick add via URL scheme
open not.app --args -serverURL http://127.0.0.1:8080 -token T  # macOS: sync settings as launch args
adb shell am start -n dev.lysen.not/.MainActivity --es server http://10.0.2.2:8080 --es token T  # debug builds only
```

## Run the server

```sh
cp .env.example .env              # set NOT_TOKEN (openssl rand -hex 32)
docker compose up -d --build
curl localhost:8080/healthz
```

Data lives in the `not-data` volume (`/data/not.db`). Put the server behind your reverse proxy or Tailscale
for TLS.
