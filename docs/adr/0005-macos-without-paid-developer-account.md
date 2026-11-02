# 0005 — macOS integrations ordered by what works without a paid Apple account

Status: accepted (2026-11-02)

## Context
No paid Apple Developer account yet. App Groups for widget/extension data sharing, notarization and
distribution need it; macOS 15+ is strict about group containers without proper provisioning.

## Decision
Build first: menu-bar app, global hotkey quick-add panel, local notifications, Raycast via unix socket
(app not sandboxed), URL scheme, App Intents, Core Spotlight. Defer WidgetKit widgets and the share
extension until the app earns the account. Keep the database location behind one function so moving it
into an App Group container is a single change plus a file move.
