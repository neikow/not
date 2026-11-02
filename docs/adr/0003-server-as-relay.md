# 0003 — Server is a dumb relay of opaque updates

Status: accepted (2026-11-02)

## Context
With CRDTs the server does not need to merge. A self-hosted single-user server should be trivial to run.

## Decision
`not-server` (Rust, axum) stores an append-only log of `(seq, doc, device, bytes)` in SQLite, serves
`push`/`pull`/`ws` (plus device pairing, [0006](0006-device-pairing.md)), and hosts the web app. It
never decodes the bytes. Compaction is driven by clients uploading snapshots.

## Consequences
- One small container + one SQLite file to back up.
- End-to-end encryption can be added later by encrypting blobs client-side.
- Server-side features that need to understand data (email digests, server search) are out of scope
  unless the server embeds `not-core`.
