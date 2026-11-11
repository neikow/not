//! Transport-agnostic sync state on [`Store`], plus an HTTP client behind the `net` feature.

use std::fmt;

use loro::{ExportMode, VersionVector};
use not_protocol::{DocUpdate, PullResponse, PushRequest, Seq};
use serde::{Deserialize, Serialize};

use crate::store::{ChangeOrigin, Inner, SERVER_VV_KEY, WORKSPACE};
use crate::{Error, Result, Store, storage};

const PULL_SEQ_KEY: &str = "pull_seq";
const CREDENTIALS_KEY: &str = "sync_credentials";

/// What a paired device needs to reach its server. Stored in the local database next to the replica
/// it belongs to, so wiping the database also unpairs (see `docs/adr/0006-device-pairing.md`).
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Credentials {
    pub server_url: String,
    pub token: String,
    /// Server-assigned id, as listed by `GET /devices`.
    pub device_id: i64,
    pub device_name: String,
}

impl fmt::Debug for Credentials {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Credentials")
            .field("server_url", &self.server_url)
            .field("token", &"<redacted>")
            .field("device_id", &self.device_id)
            .field("device_name", &self.device_name)
            .finish()
    }
}

/// Updates ready to push. Pass back to [`Store::ack_push`] once the server accepted them.
#[derive(Debug)]
pub struct Outgoing {
    pub request: PushRequest,
    version: VersionVector,
}

impl Store {
    pub fn credentials(&self) -> Result<Option<Credentials>> {
        let inner = self.lock();
        storage::kv_get(&inner.db, CREDENTIALS_KEY)?
            .map(|json| serde_json::from_slice(&json).map_err(|e| Error::Invalid(e.to_string())))
            .transpose()
    }

    /// Saves credentials from a new pairing and forgets what the previous server had: it may be a
    /// different or reset server, so the next sync pushes all history and pulls from the start.
    /// Re-sending ops a server already has is harmless. Stop any [`LiveSync`] on this store first.
    pub fn set_credentials(&self, credentials: &Credentials) -> Result<()> {
        let json = serde_json::to_vec(credentials).map_err(|e| Error::Invalid(e.to_string()))?;
        let mut guard = self.lock();
        let inner = &mut *guard;
        let tx = inner.db.transaction()?;
        storage::kv_set(&tx, CREDENTIALS_KEY, &json)?;
        storage::kv_delete(&tx, SERVER_VV_KEY)?;
        storage::kv_delete(&tx, PULL_SEQ_KEY)?;
        tx.commit()?;
        inner.server_vv = VersionVector::default();
        Ok(())
    }

    pub fn clear_credentials(&self) -> Result<()> {
        storage::kv_delete(&self.lock().db, CREDENTIALS_KEY)
    }

    /// Everything the server doesn't have yet, or `None` when it is up to date.
    pub fn outgoing(&self) -> Result<Option<Outgoing>> {
        let inner = self.lock();
        let version = self.doc.oplog_vv();
        if covers(&inner.server_vv, &version) {
            return Ok(None);
        }
        let data = self.doc.export(ExportMode::updates(&inner.server_vv))?;
        Ok(Some(Outgoing {
            request: PushRequest {
                updates: vec![DocUpdate {
                    doc: WORKSPACE.into(),
                    data,
                }],
            },
            version,
        }))
    }

    pub fn ack_push(&self, pushed: Outgoing) -> Result<()> {
        let mut inner = self.lock();
        inner.server_vv.merge(&pushed.version);
        storage::kv_set(&inner.db, SERVER_VV_KEY, &inner.server_vv.encode())
    }

    pub fn pull_cursor(&self) -> Result<Seq> {
        read_cursor(&self.lock())
    }

    /// Imports pulled updates, refreshes the read model and advances the cursor.
    pub fn apply_pull(&self, response: &PullResponse) -> Result<()> {
        let mut inner = self.lock();
        let result = self.import_pulled(&mut inner, response);
        // Flush even after a failed import so whatever did apply is persisted and visible.
        let changed = self.flush(&mut inner)?;
        if result.is_ok() {
            storage::kv_set(&inner.db, SERVER_VV_KEY, &inner.server_vv.encode())?;
            // A concurrent sync round (e.g. a background worker) may already have moved it further.
            if response.seq > read_cursor(&inner)? {
                storage::kv_set(&inner.db, PULL_SEQ_KEY, &response.seq.to_le_bytes())?;
            }
        }
        drop(inner);
        if changed {
            self.notify(ChangeOrigin::Remote);
        }
        result
    }

    fn import_pulled(&self, inner: &mut Inner, response: &PullResponse) -> Result<()> {
        for update in &response.updates {
            if update.doc != WORKSPACE {
                continue; // per-note docs are not implemented yet
            }
            let status = self.doc.import(&update.data)?;
            if status.pending.is_some() {
                // Never advance the cursor past ops Loro couldn't apply: they would be lost.
                return Err(Error::Invalid(format!(
                    "update {} depends on history this device doesn't have",
                    update.seq
                )));
            }
            // Ops that came from the server are by definition on the server: don't push them back.
            for (peer, &(start, end)) in status.success.iter() {
                let known = inner.server_vv.get(peer).copied().unwrap_or(0);
                if known >= start && end > known {
                    inner.server_vv.insert(*peer, end);
                }
            }
        }
        Ok(())
    }
}

fn read_cursor(inner: &Inner) -> Result<Seq> {
    Ok(storage::kv_get(&inner.db, PULL_SEQ_KEY)?
        .and_then(|b| b.try_into().ok().map(u64::from_le_bytes))
        .unwrap_or(0))
}

/// True if `a` contains every op in `b`.
fn covers(a: &VersionVector, b: &VersionVector) -> bool {
    b.iter()
        .all(|(peer, &end)| a.get(peer).copied().unwrap_or(0) >= end)
}

#[cfg(feature = "net")]
pub use client::{LiveSync, SyncClient, SyncReport, SyncStatus};
#[cfg(feature = "net")]
pub use not_protocol::{DeviceInfo, PairingCode};

#[cfg(feature = "net")]
mod client {
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use futures_util::StreamExt;
    use not_protocol::{
        API_PREFIX, DeviceInfo, PairRequest, PairResponse, PairingCode, PullQuery, PullResponse, PushResponse,
    };
    use reqwest::StatusCode;
    use tokio::sync::Notify;
    use tokio::task::JoinHandle;
    use tokio_tungstenite::Connector;
    use tokio_tungstenite::tungstenite::Message;
    use tokio_tungstenite::tungstenite::client::IntoClientRequest;
    use tokio_tungstenite::tungstenite::http::HeaderValue;

    use super::Credentials;
    use crate::{ChangeOrigin, Error, ListenerId, Result, Store};

    /// Coalesces bursts of local edits (typing, drag reorders) into one push.
    const PUSH_DEBOUNCE: Duration = Duration::from_millis(150);
    /// Safety net in case a poke is missed.
    const PERIODIC_SYNC: Duration = Duration::from_secs(60);
    const MAX_BACKOFF: Duration = Duration::from_secs(30);
    /// A socket that stayed up this long resets the reconnect backoff.
    const STABLE_CONNECTION: Duration = Duration::from_secs(10);

    #[derive(Debug, Default, Clone, PartialEq, Eq)]
    pub struct SyncReport {
        pub pushed: bool,
        pub pulled: usize,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum SyncStatus {
        Syncing,
        Synced,
        Offline(String),
        /// The server no longer accepts this device's token (revoked, or the server was reset).
        /// Sync pauses until the next local change or [`LiveSync::sync_now`]; pair again to fix.
        Unauthorized,
    }

    #[derive(Clone)]
    pub struct SyncClient {
        http: reqwest::Client,
        tls: Arc<rustls::ClientConfig>,
        base: String,
        token: String,
    }

    impl SyncClient {
        pub fn new(base_url: &str, token: &str) -> Self {
            let tls = Arc::new(tls_config());
            let http = reqwest::Client::builder()
                .tls_backend_preconfigured((*tls).clone())
                .connect_timeout(Duration::from_secs(10))
                .timeout(Duration::from_secs(60))
                .build()
                .expect("static HTTP client config is valid");
            Self {
                http,
                tls,
                base: format!("{}{API_PREFIX}", base_url.trim_end_matches('/')),
                token: token.to_owned(),
            }
        }

        pub fn from_credentials(credentials: &Credentials) -> Self {
            Self::new(&credentials.server_url, &credentials.token)
        }

        /// Trades a pairing code (from `not-server pair` or another device) for credentials.
        /// Doesn't touch any store: save them with [`Store::set_credentials`].
        pub async fn pair(server_url: &str, code: &str, device_name: &str) -> Result<Credentials> {
            let server_url = server_url.trim().trim_end_matches('/');
            if !(server_url.starts_with("https://") || server_url.starts_with("http://")) {
                return Err(Error::Invalid(
                    "server URL must start with https:// or http://".into(),
                ));
            }
            let client = Self::new(server_url, "");
            let response = client
                .http
                .post(format!("{}/pair", client.base))
                .json(&PairRequest {
                    code: code.to_owned(),
                    name: device_name.to_owned(),
                })
                .send()
                .await?;
            if response.status() == StatusCode::UNAUTHORIZED {
                return Err(Error::Invalid("pairing code is wrong, used or expired".into()));
            }
            let paired: PairResponse = checked(response)?.json().await?;
            Ok(Credentials {
                server_url: server_url.to_owned(),
                token: paired.token,
                device_id: paired.device.id,
                device_name: paired.device.name,
            })
        }

        /// A one-time code for pairing another device with this server.
        pub async fn create_pairing_code(&self) -> Result<PairingCode> {
            let response = self
                .http
                .post(format!("{}/pairings", self.base))
                .bearer_auth(&self.token)
                .send()
                .await?;
            Ok(checked(response)?.json().await?)
        }

        pub async fn devices(&self) -> Result<Vec<DeviceInfo>> {
            let response = self
                .http
                .get(format!("{}/devices", self.base))
                .bearer_auth(&self.token)
                .send()
                .await?;
            Ok(checked(response)?.json().await?)
        }

        /// Unpairs a device (possibly this one); its token stops working immediately.
        pub async fn revoke(&self, device_id: i64) -> Result<()> {
            let response = self
                .http
                .delete(format!("{}/devices/{device_id}", self.base))
                .bearer_auth(&self.token)
                .send()
                .await?;
            if response.status() == StatusCode::NOT_FOUND {
                return Err(Error::NotFound(format!("device {device_id}")));
            }
            checked(response)?;
            Ok(())
        }

        /// Links that show a list in a browser; see [`not_protocol::ShareInfo`].
        pub async fn shares(&self) -> Result<Vec<not_protocol::ShareInfo>> {
            let response = self
                .http
                .get(format!("{}/shares", self.base))
                .bearer_auth(&self.token)
                .send()
                .await?;
            Ok(checked(response)?.json().await?)
        }

        pub async fn create_share(&self, list_id: &str, can_edit: bool) -> Result<not_protocol::ShareInfo> {
            let response = self
                .http
                .post(format!("{}/shares", self.base))
                .bearer_auth(&self.token)
                .json(&not_protocol::CreateShare {
                    list_id: list_id.to_owned(),
                    can_edit,
                })
                .send()
                .await?;
            Ok(checked(response)?.json().await?)
        }

        /// The link stops working at once.
        pub async fn revoke_share(&self, id: i64) -> Result<()> {
            let response = self
                .http
                .delete(format!("{}/shares/{id}", self.base))
                .bearer_auth(&self.token)
                .send()
                .await?;
            if response.status() == StatusCode::NOT_FOUND {
                return Err(Error::NotFound(format!("share {id}")));
            }
            checked(response)?;
            Ok(())
        }

        /// One round: push local changes, then pull until caught up.
        pub async fn sync(&self, store: &Store) -> Result<SyncReport> {
            let mut report = SyncReport::default();
            if let Some(outgoing) = store.outgoing()? {
                let response = self
                    .http
                    .post(format!("{}/push", self.base))
                    .bearer_auth(&self.token)
                    .json(&outgoing.request)
                    .send()
                    .await?;
                let _: PushResponse = checked(response)?.json().await?;
                store.ack_push(outgoing)?;
                report.pushed = true;
            }
            loop {
                let query = PullQuery {
                    since: store.pull_cursor()?,
                    limit: None,
                };
                let response = self
                    .http
                    .get(format!("{}/pull", self.base))
                    .bearer_auth(&self.token)
                    .query(&query)
                    .send()
                    .await?;
                let pulled: PullResponse = checked(response)?.json().await?;
                report.pulled += pulled.updates.len();
                store.apply_pull(&pulled)?;
                if !pulled.more {
                    return Ok(report);
                }
            }
        }

        /// Holds the poke socket open, calling `on_poke` per message, until it closes or fails.
        async fn watch_pokes(&self, on_poke: impl Fn()) -> Result<()> {
            // http → ws, https → wss
            let url = format!("{}/ws", self.base).replacen("http", "ws", 1);
            let mut request = url.into_client_request().map_err(ws_error)?;
            let auth = HeaderValue::from_str(&format!("Bearer {}", self.token))
                .map_err(|e| Error::Invalid(e.to_string()))?;
            request.headers_mut().insert("authorization", auth);
            let connector = Connector::Rustls(self.tls.clone());
            let (mut socket, _) =
                tokio_tungstenite::connect_async_tls_with_config(request, None, true, Some(connector))
                    .await
                    .map_err(ws_error)?;
            while let Some(message) = socket.next().await {
                match message.map_err(ws_error)? {
                    Message::Text(_) => on_poke(),
                    Message::Close(_) => break,
                    _ => {}
                }
            }
            Ok(())
        }
    }

    /// Background sync for one store: pushes local edits (debounced), pulls when the server pokes,
    /// re-syncs periodically and retries with backoff while offline. Stops when dropped.
    pub struct LiveSync {
        store: Arc<Store>,
        trigger: Arc<Notify>,
        listener: ListenerId,
        tasks: [JoinHandle<()>; 2],
    }

    impl LiveSync {
        pub fn start(
            runtime: &tokio::runtime::Handle,
            store: Arc<Store>,
            client: SyncClient,
            on_status: impl Fn(SyncStatus) + Send + Sync + 'static,
        ) -> Self {
            let trigger = Arc::new(Notify::new());
            let listener = store.add_listener({
                let trigger = trigger.clone();
                move |origin| {
                    if origin == ChangeOrigin::Local {
                        trigger.notify_one();
                    }
                }
            });

            let pokes = runtime.spawn({
                let client = client.clone();
                let trigger = trigger.clone();
                async move {
                    let mut backoff = Backoff::default();
                    loop {
                        let started = Instant::now();
                        // The server sends its head right after connecting, which triggers a catch-up sync.
                        let _ = client.watch_pokes(|| trigger.notify_one()).await;
                        if started.elapsed() > STABLE_CONNECTION {
                            backoff.reset();
                        }
                        tokio::time::sleep(backoff.next()).await;
                    }
                }
            });

            let syncer = runtime.spawn({
                let store = store.clone();
                let trigger = trigger.clone();
                async move {
                    let mut backoff = Backoff::default();
                    loop {
                        on_status(SyncStatus::Syncing);
                        let wait = match client.sync(&store).await {
                            Ok(_) => {
                                backoff.reset();
                                on_status(SyncStatus::Synced);
                                Some(PERIODIC_SYNC)
                            }
                            // Retrying on a timer won't help; only re-pairing will.
                            Err(Error::Unauthorized) => {
                                on_status(SyncStatus::Unauthorized);
                                None
                            }
                            Err(e) => {
                                on_status(SyncStatus::Offline(e.to_string()));
                                Some(backoff.next())
                            }
                        };
                        let timer = async {
                            match wait {
                                Some(wait) => tokio::time::sleep(wait).await,
                                None => std::future::pending().await,
                            }
                        };
                        tokio::select! {
                            _ = trigger.notified() => tokio::time::sleep(PUSH_DEBOUNCE).await,
                            _ = timer => {}
                        }
                    }
                }
            });

            Self {
                store,
                trigger,
                listener,
                tasks: [pokes, syncer],
            }
        }

        /// Sync as soon as possible, e.g. when the app returns to the foreground.
        pub fn sync_now(&self) {
            self.trigger.notify_one();
        }
    }

    impl Drop for LiveSync {
        fn drop(&mut self) {
            self.store.remove_listener(self.listener);
            for task in &self.tasks {
                task.abort();
            }
        }
    }

    #[derive(Default)]
    struct Backoff(Duration);

    impl Backoff {
        fn next(&mut self) -> Duration {
            self.0 = (self.0 * 2).clamp(Duration::from_secs(1), MAX_BACKOFF);
            self.0
        }

        fn reset(&mut self) {
            self.0 = Duration::ZERO;
        }
    }

    /// rustls with ring and Mozilla's root store: no platform verifier, so no JNI setup on Android
    /// and no aws-lc cross-compilation. Servers need a publicly trusted certificate.
    fn tls_config() -> rustls::ClientConfig {
        let roots = rustls::RootCertStore {
            roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
        };
        rustls::ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
            .with_safe_default_protocol_versions()
            .expect("ring supports the default protocol versions")
            .with_root_certificates(roots)
            .with_no_client_auth()
    }

    fn checked(response: reqwest::Response) -> Result<reqwest::Response> {
        match response.status() {
            s if s.is_success() => Ok(response),
            StatusCode::UNAUTHORIZED => Err(Error::Unauthorized),
            s => Err(Error::Status(s.as_u16())),
        }
    }

    fn ws_error(e: impl std::fmt::Display) -> Error {
        Error::WebSocket(e.to_string())
    }
}
