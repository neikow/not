# 0006 — Device pairing with per-device tokens kept in the local database

Status: accepted (2026-11-02)

## Context
The spike authenticated every device with one static token from the server's environment, pasted into
each app and stored in UserDefaults / SharedPreferences. A lost device could only be locked out by
rotating the secret everywhere, and the server's environment held a long-lived credential.

## Decision
- **Per-device tokens.** Each device gets a 256-bit random token at pairing. The server stores only its
  SHA-256 and the device's name; deleting the row revokes it immediately (open WebSockets close at the
  next poke).
- **One-time pairing codes.** A code is 10 Crockford base32 symbols (50 bits, shown as `7KQ2M-X9CHD`),
  single use, valid 10 minutes. `POST /v1/pair` trades a code for a token and is the only unauthenticated
  endpoint. 50 bits put online guessing out of reach within the lifetime, so there is no rate limiting.
- **Bootstrap through the server CLI.** `not-server pair` prints a code (`docker compose exec not
  not-server pair`). Access to the Docker host is the admin credential, so the server needs no secret
  in its environment. Paired devices mint codes too (Settings → Add device), list devices and revoke any
  of them. All devices are equal: single user ([0004](0004-single-user.md)).
- **QR codes** carry `not://pair?server=<url>&code=<code>` (format in `not-protocol`), so scanning fills
  in both fields. Shown by `not-server pair <public URL>`, the Mac's and Android's Add Device; scanned in
  the Android app with CameraX + ZXing rather than Google's code scanner, which needs Play services and
  ships usage telemetry. The camera permission is asked for only when the scanner opens.
- **The server identifies the pusher from the token.** The client no longer sends a device id; pulls
  skip the caller's own updates by its paired device id.
- **Credentials live in the core's SQLite file**, next to the replica they authenticate, not in the
  Keychain / Android Keystore. Pairing resets the sync cursor and the server version vector, so pairing
  with a new or reset server re-pushes all history (Loro skips ops it already has).

## Why the database and not Keychain / Keystore
- The token guards the same data that sits unencrypted in that database; anyone who can read the file
  already has everything the token would give them, except writing to the server.
- Credentials and replica stay bound: wiping the database also unpairs, so a fresh replica can never
  reuse an old device id and have the server hide "its own" updates from it.
- One implementation in Rust instead of two platform ones.
- The macOS app is ad-hoc signed ([0005](0005-macos-without-paid-developer-account.md)): every rebuild
  changes its code signature, and the login keychain would ask for the user's password on each update.

Android backups are disabled (`allowBackup="false"`) so the token isn't copied off the device; the
server is the backup.

## Consequences
- A stolen unlocked Mac (or a process running as the user) can read the token; revoke the device from
  another one. Revisit when the app is signed with a stable identity (paid Apple account), where Keychain
  access is prompt-free.
- `pair` / `unpair` / `devices` / `revoke_device` are async in the FFI (Swift `async`, Kotlin `suspend`).
- Pairing over plain `http://` sends the code and token in clear; use HTTPS or Tailscale.
- `not://` URLs and Android intents never pair a device, since any web page or app could trigger them
  and point sync at another server. The debug-only intent extras are the exception, for scripted tests.
