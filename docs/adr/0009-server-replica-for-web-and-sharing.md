# 0009 — The server keeps a replica for the web companion and shared lists

Status: accepted (2026-11-11). Amends [0003](0003-server-as-relay.md).

## Context
The light web companion and shared lists ("send my partner the groceries list") need something
that can read the workspace and write to it from a browser. A guest has no app, no pairing and must
only ever see the one list shared with them.

## Decision
- **The server embeds `not-core` and keeps its own replica** (`<NOT_DB>-replica`), fed from the
  update log it already stores. Before each web request it applies whatever is new; edits made on a
  page are exported like a device's push and appended to the log as pusher `web`, then the devices
  are poked. Sync itself is unchanged: devices still see opaque updates.
- **The owner signs a browser in with a pairing code.** The browser becomes a device (cookie holds
  its token), shows in Devices and is revoked the same way. No password or env secret.
- **Shared lists are capability URLs** (`/s/<token>`), stored in the server DB with the list id and
  whether visitors can edit. The sharer picks read-only or edit per link; editing means ticking off
  and adding tasks (no delete or rename, and `#list` in a guest's text can't move a task out). Apps
  create and revoke links through `/v1/shares`.
- Pages are plain server-rendered HTML forms; the only script reports the browser's UTC offset so
  "tomorrow" means the visitor's tomorrow.

## Consequences
- The server holds a readable copy of the data. End-to-end encryption (0003's open door) would now
  mean dropping the web companion and sharing, or moving them to the client.
- Losing the replica costs nothing: it is rebuilt from the log on the next request.
- Share tokens are kept in the clear so apps can show a link again; they grant access to one list.
