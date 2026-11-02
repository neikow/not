# 0001 — Local-first with CRDTs (Loro) for all data

Status: accepted (2026-11-02)

## Context
UX speed and offline use are the core value. Data is edited on a Mac, an Android phone and occasionally the
web, often offline. Options considered: per-field last-write-wins with hybrid logical clocks, PowerSync
self-hosted, CRDT everywhere.

## Decision
All user data lives in Loro documents on every client. Lists/tasks/subtasks are nodes of a `LoroTree`;
note bodies are `LoroText`; habit check-ins are `LoroCounter`s keyed by local date.

## Consequences
- No conflict UI and no server-side merge logic; moves can't duplicate or orphan nodes.
- Undo/redo and note version history come from the CRDT.
- Loro is Rust, which drives ADR 0002.
- Docs only grow; we need compaction (snapshots, possibly shallow snapshots) later.
