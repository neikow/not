# not

Self-hosted, local-first tasks, notes and habits for macOS and Android.
Architecture: [docs/architecture.md](docs/architecture.md) · decisions: [docs/adr/](docs/adr/).

## Layout

| Path | What |
|---|---|
| `crates/not-core` | Shared Rust core: Loro docs, SQLite read model, commands, sync client (`net` feature) |
| `crates/not-protocol` | Wire types shared by core and server |
| `crates/not-server` | Sync relay (axum + SQLite) |

## Develop

```sh
cargo test --workspace --all-features
```

## Run the server

```sh
cp .env.example .env              # set NOT_TOKEN (openssl rand -hex 32)
docker compose up -d --build
curl localhost:8080/healthz
```

Data lives in the `not-data` volume (`/data/not.db`). Put the server behind your reverse proxy or Tailscale
for TLS.
