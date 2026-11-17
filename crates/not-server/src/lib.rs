//! Sync relay: an append-only log of opaque Loro updates with a global sequence number.
//! See `docs/adr/0003-server-as-relay.md`; device pairing: `docs/adr/0006-device-pairing.md`.
//! It also keeps a replica of the workspace for the web companion and shared lists: `web`,
//! `docs/adr/0009-server-replica-for-web-and-sharing.md`.

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{DefaultBodyLimit, Path as UrlPath, Query, Request, State};
use axum::http::{StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use axum::{Extension, Json, Router};
use not_protocol::{
    API_PREFIX, CreateShare, DeviceInfo, LoggedUpdate, PairRequest, PairResponse, PairingCode, Poke,
    PullQuery, PullResponse, PushRequest, PushResponse, Seq, ShareInfo,
};
use rusqlite::{Connection, OptionalExtension, params};
use sha2::{Digest, Sha256};
use tokio::sync::broadcast;

mod web;

/// What the server's own replica pushes as: never a paired device's id, so every device pulls it.
const REPLICA_PUSHER: &str = "web";
/// Pusher of a compacted log row: no device, so every device pulls it.
const COMPACTED_PUSHER: &str = "compacted";
/// Workspace log rows past which a push sets off a compaction.
pub const COMPACT_AFTER_ROWS: i64 = 2_000;

const DEFAULT_PULL_LIMIT: u32 = 500;
const MAX_PULL_LIMIT: u32 = 5_000;
/// Snapshots of large docs can be several MB.
const MAX_BODY_BYTES: usize = 64 * 1024 * 1024;

pub const PAIRING_CODE_TTL: Duration = Duration::from_secs(10 * 60);
/// Crockford base32: no I, L, O, U, so codes survive being read aloud or retyped.
const CODE_ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
/// 10 symbols = 50 bits: out of reach of online guessing within the code's lifetime.
const CODE_LEN: usize = 10;
const MAX_DEVICE_NAME: usize = 64;
const LAST_SEEN_GRANULARITY_MS: i64 = 60_000;

/// The paired device a request is authenticated as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DeviceId(i64);

/// What a compaction did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Compacted {
    /// Log rows folded into one.
    pub rows: u64,
    pub bytes_before: u64,
    pub bytes_after: u64,
    /// The seq the snapshot row kept.
    pub seq: Seq,
}

#[derive(Clone)]
pub struct AppState {
    db: Arc<Mutex<Connection>>,
    pokes: broadcast::Sender<Seq>,
    /// The server's own copy of the workspace, for the web pages and shared lists (ADR 0009).
    /// `None` if it couldn't be opened; syncing still works without it.
    replica: Option<Arc<not_core::Store>>,
}

impl AppState {
    /// The replica lives next to the log, in `<db>-replica`.
    pub fn open(db_path: impl AsRef<Path>) -> rusqlite::Result<Self> {
        Self::open_with_key(db_path, None)
    }

    /// [`AppState::open`], with both files encrypted at rest under `key` (SQLCipher) when one is
    /// given. A log that is still plain is encrypted in place the first time; the replica, being
    /// a copy of the log, is rebuilt instead.
    pub fn open_with_key(db_path: impl AsRef<Path>, key: Option<&str>) -> rusqlite::Result<Self> {
        let path = db_path.as_ref();
        let replica_path = path.with_file_name(format!(
            "{}-replica",
            path.file_name().map_or("not.db".into(), |n| n.to_string_lossy())
        ));
        let db = match key {
            Some(key) => open_encrypted(path, key)?,
            None => Connection::open(path)?,
        };
        let replica = match key {
            Some(key) => not_core::Store::open_with_key(&replica_path, key).or_else(|_| {
                // Plain (from before the key) or unreadable: it's only a copy, start it over.
                tracing::info!(path = %replica_path.display(), "rebuilding the web replica, encrypted");
                remove_db_files(&replica_path);
                not_core::Store::open_with_key(&replica_path, key)
            }),
            None => not_core::Store::open(&replica_path),
        }
        .map_err(
            |e| tracing::error!(error = %e, path = %replica_path.display(), "can't open the web replica"),
        )
        .ok();
        Self::new(db, replica)
    }

    pub fn in_memory() -> rusqlite::Result<Self> {
        Self::new(
            Connection::open_in_memory()?,
            not_core::Store::open_in_memory().ok(),
        )
    }

    fn new(db: Connection, replica: Option<not_core::Store>) -> rusqlite::Result<Self> {
        db.execute_batch(
            "
            PRAGMA journal_mode = WAL;
            PRAGMA synchronous = NORMAL;
            CREATE TABLE IF NOT EXISTS update_log (
                seq        INTEGER PRIMARY KEY AUTOINCREMENT,
                doc        TEXT NOT NULL,
                device     TEXT NOT NULL,  -- device.id of the pusher
                data       BLOB NOT NULL,
                created_at INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS device (
                id           INTEGER PRIMARY KEY AUTOINCREMENT,
                name         TEXT NOT NULL,
                token_hash   BLOB NOT NULL UNIQUE,
                created_at   INTEGER NOT NULL,
                last_seen_at INTEGER
            );
            CREATE TABLE IF NOT EXISTS pairing_code (
                code_hash  BLOB PRIMARY KEY,
                expires_at INTEGER NOT NULL
            );
            -- Kept in the clear: the owner's apps show the link again. It is a capability URL to
            -- data this server already holds in the clear.
            -- Images in notes, by content id (sha256.ext); checked against their bytes on upload.
            CREATE TABLE IF NOT EXISTS blob (
                id         TEXT PRIMARY KEY,
                data       BLOB NOT NULL,
                created_at INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS share (
                id         INTEGER PRIMARY KEY AUTOINCREMENT,
                token      TEXT NOT NULL UNIQUE,
                list_id    TEXT NOT NULL,
                can_edit   INTEGER NOT NULL,
                created_at INTEGER NOT NULL
            );
            ",
        )?;
        Ok(Self {
            db: Arc::new(Mutex::new(db)),
            pokes: broadcast::channel(64).0,
            replica: replica.map(Arc::new),
        })
    }

    // ---- replica ----

    /// The replica, brought up to date with the log first when `catch_up`.
    fn replica(&self, catch_up: bool) -> Option<Arc<not_core::Store>> {
        let store = self.replica.clone()?;
        if catch_up && let Err(e) = self.catch_up(&store) {
            tracing::error!(error = %e, "web replica fell behind");
        }
        Some(store)
    }

    fn catch_up(&self, store: &not_core::Store) -> Result<(), Box<dyn std::error::Error>> {
        loop {
            let since = store.pull_cursor()?;
            let (updates, seq, more) = {
                let db = self.db.lock().unwrap();
                let mut stmt = db.prepare_cached(
                    "SELECT seq, doc, data FROM update_log WHERE seq > ?1 ORDER BY seq LIMIT ?2",
                )?;
                let rows = stmt
                    .query_map(params![since, MAX_PULL_LIMIT + 1], |r| {
                        Ok(LoggedUpdate {
                            seq: r.get(0)?,
                            doc: r.get(1)?,
                            data: r.get(2)?,
                        })
                    })?
                    .collect::<Result<Vec<_>, _>>()?;
                let more = rows.len() > MAX_PULL_LIMIT as usize;
                let rows: Vec<_> = rows.into_iter().take(MAX_PULL_LIMIT as usize).collect();
                let seq = rows.last().map_or(since, |u| u.seq);
                (rows, seq, more)
            };
            if updates.is_empty() {
                return Ok(());
            }
            store.apply_pull(&PullResponse { updates, seq, more })?;
            if !more {
                return Ok(());
            }
        }
    }

    /// Replaces the workspace's log, up to what the replica has merged, with one snapshot row
    /// taking the last of those rows' place (and seq). Devices behind it import the snapshot,
    /// which merges like any update; devices past it never see it. Returns `None` when there's
    /// nothing to compact or no replica.
    pub fn compact(&self) -> Result<Option<Compacted>, Box<dyn std::error::Error>> {
        let Some(store) = self.replica(true) else {
            return Ok(None);
        };
        let cursor = store.pull_cursor()?;
        let snapshot = store.snapshot()?;
        let mut db = self.db.lock().unwrap();
        let tx = db.transaction()?;
        let (rows, bytes, last): (u64, u64, Option<Seq>) = tx.query_row(
            "SELECT count(*), coalesce(sum(length(data)), 0), max(seq) FROM update_log WHERE doc = ?1 AND seq <= ?2",
            params![not_core::WORKSPACE, cursor],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?;
        let Some(last) = last.filter(|_| rows > 1) else {
            return Ok(None);
        };
        tx.execute(
            "DELETE FROM update_log WHERE doc = ?1 AND seq < ?2",
            params![not_core::WORKSPACE, last],
        )?;
        tx.execute(
            "UPDATE update_log SET data = ?1, device = ?2 WHERE seq = ?3",
            params![snapshot, COMPACTED_PUSHER, last],
        )?;
        tx.commit()?;
        let report = Compacted {
            rows,
            bytes_before: bytes,
            bytes_after: snapshot.len() as u64,
            seq: last,
        };
        tracing::info!(?report, "compacted the log");
        Ok(Some(report))
    }

    /// Compacts in the background once the workspace log has grown past [COMPACT_AFTER_ROWS].
    fn compact_if_long(&self) {
        let rows: i64 = {
            let db = self.db.lock().unwrap();
            db.query_row(
                "SELECT count(*) FROM update_log WHERE doc = ?1",
                [not_core::WORKSPACE],
                |r| r.get(0),
            )
            .unwrap_or(0)
        };
        if rows <= COMPACT_AFTER_ROWS {
            return;
        }
        let state = self.clone();
        tokio::task::spawn_blocking(move || {
            if let Err(e) = state.compact() {
                tracing::error!(error = %e, "compaction failed");
            }
        });
    }

    /// Appends what the replica changed (a web edit) to the log, and pokes the devices.
    fn publish_replica(&self) {
        let Some(store) = self.replica.as_ref() else {
            return;
        };
        let outgoing = match store.outgoing() {
            Ok(Some(outgoing)) => outgoing,
            Ok(None) => return,
            Err(e) => return tracing::error!(error = %e, "web replica: nothing to push"),
        };
        let seq = {
            let mut db = self.db.lock().unwrap();
            let result = (|| -> rusqlite::Result<Seq> {
                let tx = db.transaction()?;
                let now = now_millis();
                for update in &outgoing.request.updates {
                    tx.execute(
                        "INSERT INTO update_log (doc, device, data, created_at) VALUES (?1, ?2, ?3, ?4)",
                        params![update.doc, REPLICA_PUSHER, update.data, now],
                    )?;
                }
                let seq = tx.query_row("SELECT coalesce(max(seq), 0) FROM update_log", [], |r| r.get(0))?;
                tx.commit()?;
                Ok(seq)
            })();
            match result {
                Ok(seq) => seq,
                Err(e) => return tracing::error!(error = %e, "web replica: push failed"),
            }
        };
        if let Err(e) = store.ack_push(outgoing) {
            tracing::error!(error = %e, "web replica: ack failed");
        }
        let _ = self.pokes.send(seq);
    }

    // ---- images ----

    /// Keeps an image unless it is there already; false if `bytes` aren't what `id` names.
    fn put_blob(&self, id: &str, bytes: &[u8]) -> rusqlite::Result<bool> {
        if not_protocol::image_id(bytes).as_deref() != Some(id) {
            return Ok(false);
        }
        self.db.lock().unwrap().execute(
            "INSERT OR IGNORE INTO blob (id, data, created_at) VALUES (?1, ?2, ?3)",
            params![id, bytes, now_millis()],
        )?;
        Ok(true)
    }

    fn blob(&self, id: &str) -> rusqlite::Result<Option<Vec<u8>>> {
        self.db
            .lock()
            .unwrap()
            .query_row("SELECT data FROM blob WHERE id = ?1", [id], |r| r.get(0))
            .optional()
    }

    // ---- shares ----

    pub fn shares(&self) -> rusqlite::Result<Vec<ShareInfo>> {
        let db = self.db.lock().unwrap();
        let mut stmt =
            db.prepare_cached("SELECT id, list_id, can_edit, token, created_at FROM share ORDER BY id")?;
        stmt.query_map([], share_row)?.collect()
    }

    fn create_share(&self, list_id: &str, can_edit: bool) -> rusqlite::Result<ShareInfo> {
        let token = random_token()[..32].to_owned();
        let now = now_millis();
        let db = self.db.lock().unwrap();
        db.execute(
            "INSERT INTO share (token, list_id, can_edit, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![token, list_id, can_edit, now],
        )?;
        Ok(ShareInfo {
            id: db.last_insert_rowid(),
            list_id: list_id.to_owned(),
            can_edit,
            token,
            created_at: now,
        })
    }

    fn revoke_share(&self, id: i64) -> rusqlite::Result<bool> {
        Ok(self
            .db
            .lock()
            .unwrap()
            .execute("DELETE FROM share WHERE id = ?1", [id])?
            > 0)
    }

    fn share_by_token(&self, token: &str) -> Option<ShareInfo> {
        let db = self.db.lock().unwrap();
        db.query_row(
            "SELECT id, list_id, can_edit, token, created_at FROM share WHERE token = ?1",
            [token],
            share_row,
        )
        .optional()
        .ok()
        .flatten()
    }

    /// A single-use code a new device trades for a token via `POST /pair`.
    pub fn create_pairing_code(&self) -> rusqlite::Result<PairingCode> {
        let code = random_code();
        let now = now_millis();
        let expires_at = now + PAIRING_CODE_TTL.as_millis() as i64;
        let db = self.db.lock().unwrap();
        db.execute("DELETE FROM pairing_code WHERE expires_at <= ?1", [now])?;
        db.execute(
            "INSERT INTO pairing_code (code_hash, expires_at) VALUES (?1, ?2)",
            params![hash(&code), expires_at],
        )?;
        Ok(PairingCode {
            code: format!("{}-{}", &code[..CODE_LEN / 2], &code[CODE_LEN / 2..]),
            expires_at,
        })
    }

    pub fn devices(&self) -> rusqlite::Result<Vec<DeviceInfo>> {
        let db = self.db.lock().unwrap();
        let mut stmt =
            db.prepare_cached("SELECT id, name, created_at, last_seen_at FROM device ORDER BY id")?;
        let rows = stmt.query_map([], |r| {
            Ok(DeviceInfo {
                id: r.get(0)?,
                name: r.get(1)?,
                created_at: r.get(2)?,
                last_seen_at: r.get(3)?,
            })
        })?;
        rows.collect()
    }

    /// Removes a device; its token stops working immediately. Returns false if there was none.
    pub fn revoke(&self, id: i64) -> rusqlite::Result<bool> {
        let db = self.db.lock().unwrap();
        Ok(db.execute("DELETE FROM device WHERE id = ?1", [id])? > 0)
    }

    /// Consumes `code` and registers a device. `None` if the code is unknown, used or expired.
    fn pair(&self, code: &str, name: &str) -> rusqlite::Result<Option<PairResponse>> {
        let Some(code) = normalize_code(code) else {
            return Ok(None);
        };
        let name = device_name(name);
        let now = now_millis();
        let mut db = self.db.lock().unwrap();
        let tx = db.transaction()?;
        let consumed = tx.execute(
            "DELETE FROM pairing_code WHERE code_hash = ?1 AND expires_at > ?2",
            params![hash(&code), now],
        )?;
        if consumed == 0 {
            return Ok(None);
        }
        let token = random_token();
        tx.execute(
            "INSERT INTO device (name, token_hash, created_at) VALUES (?1, ?2, ?3)",
            params![name, hash(&token), now],
        )?;
        let id = tx.last_insert_rowid();
        tx.commit()?;
        Ok(Some(PairResponse {
            device: DeviceInfo {
                id,
                name,
                created_at: now,
                last_seen_at: None,
            },
            token,
        }))
    }

    fn authenticate(&self, token: &str) -> rusqlite::Result<Option<DeviceId>> {
        let db = self.db.lock().unwrap();
        let id: Option<i64> = db
            .query_row(
                "SELECT id FROM device WHERE token_hash = ?1",
                [hash(token)],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(id) = id {
            let now = now_millis();
            db.execute(
                "UPDATE device SET last_seen_at = ?2
                 WHERE id = ?1 AND coalesce(last_seen_at, 0) < ?2 - ?3",
                params![id, now, LAST_SEEN_GRANULARITY_MS],
            )?;
        }
        Ok(id.map(DeviceId))
    }

    fn is_paired(&self, device: DeviceId) -> rusqlite::Result<bool> {
        let db = self.db.lock().unwrap();
        db.query_row("SELECT 1 FROM device WHERE id = ?1", [device.0], |_| Ok(()))
            .optional()
            .map(|found| found.is_some())
    }

    fn head(&self) -> rusqlite::Result<Seq> {
        let db = self.db.lock().unwrap();
        db.query_row("SELECT coalesce(max(seq), 0) FROM update_log", [], |r| r.get(0))
    }
}

pub fn router(state: AppState) -> Router {
    let authenticated = Router::new()
        .route("/push", post(push))
        .route("/pull", get(pull))
        .route("/ws", get(ws))
        .route("/pairings", post(create_pairing))
        .route("/devices", get(list_devices))
        .route("/devices/{id}", delete(revoke_device))
        .route("/shares", get(list_shares).post(new_share))
        .route("/shares/{id}", delete(delete_share))
        .route("/blobs/{id}", get(get_blob).put(put_blob))
        .layer(middleware::from_fn_with_state(state.clone(), require_device));
    let api = Router::new()
        .route("/pair", post(pair))
        .merge(authenticated)
        .layer(DefaultBodyLimit::max(MAX_BODY_BYTES));
    Router::new()
        .nest(API_PREFIX, api)
        .merge(web::routes())
        .route("/healthz", get(|| async { "ok" }))
        .with_state(state)
}

async fn require_device(State(state): State<AppState>, mut request: Request, next: Next) -> Response {
    let presented = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));
    // Tokens are 256-bit random values looked up by hash, so the lookup's timing reveals nothing useful.
    match presented.map(|token| state.authenticate(token)) {
        Some(Ok(Some(device))) => {
            request.extensions_mut().insert(device);
            next.run(request).await
        }
        Some(Err(e)) => AppError(e).into_response(),
        _ => StatusCode::UNAUTHORIZED.into_response(),
    }
}

async fn pair(State(state): State<AppState>, Json(request): Json<PairRequest>) -> Result<Response, AppError> {
    Ok(match state.pair(&request.code, &request.name)? {
        Some(paired) => {
            tracing::info!(id = paired.device.id, name = %paired.device.name, "device paired");
            Json(paired).into_response()
        }
        None => {
            tracing::warn!("pairing attempt with an invalid or expired code");
            StatusCode::UNAUTHORIZED.into_response()
        }
    })
}

async fn create_pairing(State(state): State<AppState>) -> Result<Json<PairingCode>, AppError> {
    Ok(Json(state.create_pairing_code()?))
}

async fn list_devices(State(state): State<AppState>) -> Result<Json<Vec<DeviceInfo>>, AppError> {
    Ok(Json(state.devices()?))
}

async fn revoke_device(
    State(state): State<AppState>,
    Extension(caller): Extension<DeviceId>,
    UrlPath(id): UrlPath<i64>,
) -> Result<StatusCode, AppError> {
    if !state.revoke(id)? {
        return Ok(StatusCode::NOT_FOUND);
    }
    tracing::info!(id, by = caller.0, "device revoked");
    Ok(StatusCode::NO_CONTENT)
}

async fn list_shares(State(state): State<AppState>) -> Result<Json<Vec<ShareInfo>>, AppError> {
    Ok(Json(state.shares()?))
}

async fn new_share(
    State(state): State<AppState>,
    Json(request): Json<CreateShare>,
) -> Result<Json<ShareInfo>, AppError> {
    let share = state.create_share(&request.list_id, request.can_edit)?;
    tracing::info!(id = share.id, can_edit = share.can_edit, "list shared");
    Ok(Json(share))
}

async fn delete_share(
    State(state): State<AppState>,
    UrlPath(id): UrlPath<i64>,
) -> Result<StatusCode, AppError> {
    Ok(if state.revoke_share(id)? {
        StatusCode::NO_CONTENT
    } else {
        StatusCode::NOT_FOUND
    })
}

async fn put_blob(
    State(state): State<AppState>,
    UrlPath(id): UrlPath<String>,
    body: axum::body::Bytes,
) -> Result<StatusCode, AppError> {
    if !not_protocol::is_image_id(&id) || !state.put_blob(&id, &body)? {
        return Ok(StatusCode::UNPROCESSABLE_ENTITY);
    }
    tracing::debug!(%id, bytes = body.len(), "image stored");
    Ok(StatusCode::NO_CONTENT)
}

async fn get_blob(State(state): State<AppState>, UrlPath(id): UrlPath<String>) -> Result<Response, AppError> {
    Ok(image_response(&state, &id)?)
}

/// An image's bytes with a long cache lifetime (ids are content hashes, so they never change).
fn image_response(state: &AppState, id: &str) -> rusqlite::Result<Response> {
    if !not_protocol::is_image_id(id) {
        return Ok(StatusCode::NOT_FOUND.into_response());
    }
    Ok(match state.blob(id)? {
        Some(bytes) => (
            [
                (header::CONTENT_TYPE, not_protocol::image_mime(id)),
                (header::CACHE_CONTROL, "private, max-age=31536000, immutable"),
            ],
            bytes,
        )
            .into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    })
}

fn share_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<ShareInfo> {
    Ok(ShareInfo {
        id: r.get(0)?,
        list_id: r.get(1)?,
        can_edit: r.get(2)?,
        token: r.get(3)?,
        created_at: r.get(4)?,
    })
}

async fn push(
    State(state): State<AppState>,
    Extension(device): Extension<DeviceId>,
    Json(request): Json<PushRequest>,
) -> Result<Json<PushResponse>, AppError> {
    let seq = {
        let mut db = state.db.lock().unwrap();
        let tx = db.transaction()?;
        let now = now_millis();
        for update in &request.updates {
            tx.execute(
                "INSERT INTO update_log (doc, device, data, created_at) VALUES (?1, ?2, ?3, ?4)",
                params![update.doc, device.0.to_string(), update.data, now],
            )?;
        }
        let seq: Seq = tx.query_row("SELECT coalesce(max(seq), 0) FROM update_log", [], |r| r.get(0))?;
        tx.commit()?;
        seq
    };
    if !request.updates.is_empty() {
        tracing::debug!(device = device.0, count = request.updates.len(), seq, "push");
        // No listeners is fine.
        let _ = state.pokes.send(seq);
        state.compact_if_long();
    }
    Ok(Json(PushResponse { seq }))
}

async fn pull(
    State(state): State<AppState>,
    Extension(device): Extension<DeviceId>,
    Query(query): Query<PullQuery>,
) -> Result<Json<PullResponse>, AppError> {
    let limit = query.limit.unwrap_or(DEFAULT_PULL_LIMIT).clamp(1, MAX_PULL_LIMIT);
    let db = state.db.lock().unwrap();
    let mut stmt = db.prepare_cached(
        "SELECT seq, doc, device, data FROM update_log WHERE seq > ?1 ORDER BY seq LIMIT ?2",
    )?;
    let rows = stmt.query_map(params![query.since, limit + 1], |r| {
        Ok((
            LoggedUpdate {
                seq: r.get(0)?,
                doc: r.get(1)?,
                data: r.get(3)?,
            },
            r.get::<_, String>(2)?,
        ))
    })?;
    let mut rows = rows.collect::<Result<Vec<_>, _>>()?;
    let more = rows.len() > limit as usize;
    rows.truncate(limit as usize);
    // The cursor advances past the caller's own updates even though they aren't returned.
    let seq = rows.last().map_or(query.since, |(u, _)| u.seq);
    let own = device.0.to_string();
    let updates = rows
        .into_iter()
        .filter(|(_, pusher)| *pusher != own)
        .map(|(u, _)| u)
        .collect();
    Ok(Json(PullResponse { updates, seq, more }))
}

async fn ws(
    State(state): State<AppState>,
    Extension(device): Extension<DeviceId>,
    upgrade: WebSocketUpgrade,
) -> Response {
    upgrade.on_upgrade(move |socket| pokes(socket, state, device))
}

async fn pokes(mut socket: WebSocket, state: AppState, device: DeviceId) {
    let mut rx = state.pokes.subscribe();
    let Ok(head) = state.head() else { return };
    if send_poke(&mut socket, head).await.is_err() {
        return;
    }
    loop {
        tokio::select! {
            poke = rx.recv() => {
                let seq = match poke {
                    Ok(seq) => seq,
                    // Missed some pokes: the latest head is all a client needs.
                    Err(broadcast::error::RecvError::Lagged(_)) => match state.head() {
                        Ok(seq) => seq,
                        Err(_) => return,
                    },
                    Err(broadcast::error::RecvError::Closed) => return,
                };
                // A revoked device loses its open socket at the next poke.
                if !matches!(state.is_paired(device), Ok(true)) {
                    let _ = socket.send(Message::Close(None)).await;
                    return;
                }
                if send_poke(&mut socket, seq).await.is_err() {
                    return;
                }
            }
            incoming = socket.recv() => match incoming {
                Some(Ok(Message::Close(_))) | None | Some(Err(_)) => return,
                Some(Ok(_)) => {}
            },
        }
    }
}

async fn send_poke(socket: &mut WebSocket, seq: Seq) -> Result<(), axum::Error> {
    let text = serde_json::to_string(&Poke { seq }).expect("poke serializes");
    socket.send(Message::Text(text.into())).await
}

/// Uppercases and strips separators; accepts O for 0 and I/L for 1. `None` if it can't be a code.
fn normalize_code(input: &str) -> Option<String> {
    let code: String = input
        .chars()
        .filter(|c| !matches!(c, '-' | ' '))
        .map(|c| match c.to_ascii_uppercase() {
            'O' => '0',
            'I' | 'L' => '1',
            c => c,
        })
        .collect();
    (code.len() == CODE_LEN && code.bytes().all(|b| CODE_ALPHABET.contains(&b))).then_some(code)
}

fn device_name(name: &str) -> String {
    let name: String = name.trim().chars().take(MAX_DEVICE_NAME).collect();
    if name.is_empty() {
        "Unnamed device".into()
    } else {
        name
    }
}

fn random_code() -> String {
    let mut bytes = [0u8; CODE_LEN];
    getrandom::fill(&mut bytes).expect("system random number generator");
    // 256 is a multiple of 32, so taking the low 5 bits keeps every symbol equally likely.
    bytes
        .iter()
        .map(|b| CODE_ALPHABET[usize::from(b & 31)] as char)
        .collect()
}

fn random_token() -> String {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).expect("system random number generator");
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn hash(secret: &str) -> Vec<u8> {
    Sha256::digest(secret.as_bytes()).to_vec()
}

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

struct AppError(rusqlite::Error);

impl From<rusqlite::Error> for AppError {
    fn from(e: rusqlite::Error) -> Self {
        Self(e)
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        tracing::error!(error = %self.0, "storage error");
        StatusCode::INTERNAL_SERVER_ERROR.into_response()
    }
}

/// Opens the log encrypted with `key`; a log that is still plain SQLite is encrypted in place
/// first (into a new file that then takes its place, so a crash halfway leaves the old one).
fn open_encrypted(path: &Path, key: &str) -> rusqlite::Result<Connection> {
    let readable = |db: &Connection| db.query_row("SELECT count(*) FROM sqlite_master", [], |_| Ok(()));
    let db = Connection::open(path)?;
    db.pragma_update(None, "key", key)?;
    if readable(&db).is_ok() {
        return Ok(db);
    }
    drop(db);
    // Plain? Then it opens without a key.
    let plain = Connection::open(path)?;
    if readable(&plain).is_err() {
        return Err(rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_NOTADB),
            Some(format!("{} doesn't open with this key", path.display())),
        ));
    }
    tracing::info!(path = %path.display(), "encrypting the database");
    plain.pragma_update(None, "wal_checkpoint", "TRUNCATE")?;
    let encrypted = path.with_extension("encrypting");
    remove_db_files(&encrypted);
    plain.execute(
        "ATTACH DATABASE ?1 AS encrypted KEY ?2",
        rusqlite::params![encrypted.to_string_lossy(), key],
    )?;
    plain.query_row("SELECT sqlcipher_export('encrypted')", [], |_| Ok(()))?;
    plain.execute("DETACH DATABASE encrypted", [])?;
    drop(plain);
    remove_db_files(path);
    std::fs::rename(&encrypted, path).map_err(|e| {
        rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_CANTOPEN),
            Some(e.to_string()),
        )
    })?;
    let db = Connection::open(path)?;
    db.pragma_update(None, "key", key)?;
    readable(&db)?;
    Ok(db)
}

/// A database file and its WAL companions.
fn remove_db_files(path: &Path) {
    for suffix in ["", "-wal", "-shm", "-journal"] {
        let _ = std::fs::remove_file(format!("{}{suffix}", path.display()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_tolerate_typos_and_formatting() {
        assert_eq!(normalize_code("7kq2m-x9chd").as_deref(), Some("7KQ2MX9CHD"));
        assert_eq!(normalize_code(" 7KQ2M X9CHD ").as_deref(), Some("7KQ2MX9CHD"));
        assert_eq!(normalize_code("OIL00-11111").as_deref(), Some("0110011111"));
        assert_eq!(normalize_code("7KQ2M-X9CH"), None, "too short");
        assert_eq!(normalize_code("7KQ2M-X9CHU"), None, "U is not in the alphabet");
    }

    #[test]
    fn generated_codes_round_trip() {
        let code = random_code();
        assert_eq!(code.len(), CODE_LEN);
        assert_eq!(normalize_code(&code), Some(code));
    }
}
