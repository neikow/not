# 0002 — Shared Rust core, native UIs via uniffi

Status: accepted (2026-11-02)

## Context
Initial preference was Kotlin Multiplatform. Loro has official Swift and JS bindings but no official
Kotlin/KMP binding; KMP would need the community Gobley generator on top of loro-ffi, i.e. two binding
layers, with Rust in the build anyway. The `rrule` crate exists in Rust; KMP has no common-code RRULE library.

## Decision
`not-core` is a Rust crate: Loro docs, SQLite read model, commands, quick-add parser, recurrence, sync client.
Exposed to Swift and Kotlin via uniffi (`not-ffi`) and to the web via wasm. UIs stay fully native
(SwiftUI, Jetpack Compose).

## Consequences
- One implementation of the bug-prone logic for all platforms.
- Build needs cargo cross-compilation for Android ABIs and an xcframework for macOS.
- Reactive UI updates cross FFI via callback interfaces → Swift `AsyncStream` / Kotlin `Flow` adapters.
