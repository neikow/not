# 0016 — Encryption at rest, not end to end (for now)

Status: accepted (2026-11-17)

## Context
End-to-end encryption would keep the server from ever reading the data. But the server keeps a
readable replica of the workspace (ADR 0009) to serve the web companion and shared list links,
and it uses that replica to compact the sync log. Both would have to go, or shared lists be
re-published readable, for the server to be truly blind.

## Decision
- No end-to-end encryption yet; the web companion and share links stay.
- The server's databases (the log and the replica) are encrypted at rest with SQLCipher when
  `NOT_DB_KEY` (or `NOT_DB_KEY_FILE`) is set. A plain log is encrypted in place on the first start
  with a key; the replica is simply rebuilt from the log. Only the server links SQLCipher; the
  apps keep plain SQLite (their data is protected by the device).
- The server compacts the workspace log into a snapshot from its replica as it grows: the last
  compacted row keeps its seq, so devices behind it import the snapshot (a merge) and the sync
  protocol is unchanged.

## Consequences
- A stolen disk or backup of the server volume is unreadable without the key; the running server
  can still read everything (it has the key in its environment).
- Losing the key loses the server's copy; devices hold everything and can re-upload to a fresh
  server by pairing again.
- Revisit end to end if the server ever runs somewhere less trusted; it would mean dropping or
  reworking web pages, shares and server-side compaction (clients would upload snapshots instead).
