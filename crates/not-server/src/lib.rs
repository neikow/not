//! Sync relay: an append-only log of opaque Loro updates with a global sequence number.
//! See `docs/adr/0003-server-as-relay.md`.

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{DefaultBodyLimit, Query, Request, State};
use axum::http::{StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use not_protocol::{API_PREFIX, LoggedUpdate, Poke, PullQuery, PullResponse, PushRequest, PushResponse, Seq};
use rusqlite::{Connection, params};
use tokio::sync::broadcast;

const DEFAULT_PULL_LIMIT: u32 = 500;
const MAX_PULL_LIMIT: u32 = 5_000;
/// Snapshots of large docs can be several MB.
const MAX_BODY_BYTES: usize = 64 * 1024 * 1024;

#[derive(Clone)]
pub struct AppState {
    db: Arc<Mutex<Connection>>,
    token: Arc<str>,
    pokes: broadcast::Sender<Seq>,
}

impl AppState {
    pub fn open(db_path: impl AsRef<Path>, token: &str) -> rusqlite::Result<Self> {
        Self::new(Connection::open(db_path)?, token)
    }

    pub fn in_memory(token: &str) -> rusqlite::Result<Self> {
        Self::new(Connection::open_in_memory()?, token)
    }

    fn new(db: Connection, token: &str) -> rusqlite::Result<Self> {
        db.execute_batch(
            "
            PRAGMA journal_mode = WAL;
            PRAGMA synchronous = NORMAL;
            CREATE TABLE IF NOT EXISTS update_log (
                seq        INTEGER PRIMARY KEY AUTOINCREMENT,
                doc        TEXT NOT NULL,
                device     TEXT NOT NULL,
                data       BLOB NOT NULL,
                created_at INTEGER NOT NULL
            );
            ",
        )?;
        Ok(Self {
            db: Arc::new(Mutex::new(db)),
            token: token.into(),
            pokes: broadcast::channel(64).0,
        })
    }

    fn head(&self) -> rusqlite::Result<Seq> {
        let db = self.db.lock().unwrap();
        db.query_row("SELECT coalesce(max(seq), 0) FROM update_log", [], |r| r.get(0))
    }
}

pub fn router(state: AppState) -> Router {
    let api = Router::new()
        .route("/push", post(push))
        .route("/pull", get(pull))
        .route("/ws", get(ws))
        .layer(middleware::from_fn_with_state(state.clone(), require_token))
        .layer(DefaultBodyLimit::max(MAX_BODY_BYTES));
    Router::new()
        .nest(API_PREFIX, api)
        .route("/healthz", get(|| async { "ok" }))
        .with_state(state)
}

async fn require_token(State(state): State<AppState>, request: Request, next: Next) -> Response {
    let presented = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));
    match presented {
        Some(token) if constant_time_eq(token.as_bytes(), state.token.as_bytes()) => next.run(request).await,
        _ => StatusCode::UNAUTHORIZED.into_response(),
    }
}

async fn push(
    State(state): State<AppState>,
    Json(request): Json<PushRequest>,
) -> Result<Json<PushResponse>, AppError> {
    let seq = {
        let mut db = state.db.lock().unwrap();
        let tx = db.transaction()?;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_millis() as i64);
        for update in &request.updates {
            tx.execute(
                "INSERT INTO update_log (doc, device, data, created_at) VALUES (?1, ?2, ?3, ?4)",
                params![update.doc, request.device, update.data, now],
            )?;
        }
        let seq: Seq = tx.query_row("SELECT coalesce(max(seq), 0) FROM update_log", [], |r| r.get(0))?;
        tx.commit()?;
        seq
    };
    if !request.updates.is_empty() {
        tracing::debug!(device = %request.device, count = request.updates.len(), seq, "push");
        // No listeners is fine.
        let _ = state.pokes.send(seq);
    }
    Ok(Json(PushResponse { seq }))
}

async fn pull(
    State(state): State<AppState>,
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
    // The cursor advances past this device's own updates even though they aren't returned.
    let seq = rows.last().map_or(query.since, |(u, _)| u.seq);
    let updates = rows
        .into_iter()
        .filter(|(_, device)| query.device.as_deref() != Some(device))
        .map(|(u, _)| u)
        .collect();
    Ok(Json(PullResponse { updates, seq, more }))
}

async fn ws(State(state): State<AppState>, upgrade: WebSocketUpgrade) -> Response {
    upgrade.on_upgrade(move |socket| pokes(socket, state))
}

async fn pokes(mut socket: WebSocket, state: AppState) {
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

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
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
