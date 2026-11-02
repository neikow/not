# 0004 — Single user, sideloaded Android

Status: accepted (2026-11-02)

## Decision
One user per server. No sharing, no tenancy. Android app is sideloaded (no Play Store policy limits:
exact alarms, UnifiedPush are fine). Device auth via pairing tokens.

## Consequences
SQLite on the server is enough. Revisit if family sharing is ever wanted (would need per-doc ACLs).
