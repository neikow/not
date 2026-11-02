//! Transport-agnostic sync state on [`Store`], plus an HTTP client behind the `net` feature.

use loro::{ExportMode, VersionVector};
use not_protocol::{DocUpdate, PullResponse, PushRequest, Seq};

use crate::store::{ChangeOrigin, Inner, SERVER_VV_KEY, WORKSPACE};
use crate::{Error, Result, Store, storage};

const PULL_SEQ_KEY: &str = "pull_seq";

/// Updates ready to push. Pass back to [`Store::ack_push`] once the server accepted them.
#[derive(Debug)]
pub struct Outgoing {
    pub request: PushRequest,
    version: VersionVector,
}

impl Store {
    pub fn device_id(&self) -> &str {
        &self.device_id
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
                device: self.device_id.clone(),
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
        let inner = self.lock();
        Ok(storage::kv_get(&inner.db, PULL_SEQ_KEY)?
            .and_then(|b| b.try_into().ok().map(u64::from_le_bytes))
            .unwrap_or(0))
    }

    /// Imports pulled updates, refreshes the read model and advances the cursor.
    pub fn apply_pull(&self, response: &PullResponse) -> Result<()> {
        let mut inner = self.lock();
        let result = self.import_pulled(&mut inner, response);
        // Flush even after a failed import so whatever did apply is persisted and visible.
        let changed = self.flush(&mut inner)?;
        if result.is_ok() {
            storage::kv_set(&inner.db, SERVER_VV_KEY, &inner.server_vv.encode())?;
            storage::kv_set(&inner.db, PULL_SEQ_KEY, &response.seq.to_le_bytes())?;
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

/// True if `a` contains every op in `b`.
fn covers(a: &VersionVector, b: &VersionVector) -> bool {
    b.iter()
        .all(|(peer, &end)| a.get(peer).copied().unwrap_or(0) >= end)
}

#[cfg(feature = "net")]
pub use client::{LiveSync, SyncClient, SyncReport, SyncStatus};

#[cfg(feature = "net")]
mod client {
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use futures_util::StreamExt;
    use not_protocol::{API_PREFIX, PullQuery, PullResponse, PushResponse};
    use tokio::sync::Notify;
    use tokio::task::JoinHandle;
    use tokio_tungstenite::Connector;
    use tokio_tungstenite::tungstenite::Message;
    use tokio_tungstenite::tungstenite::client::IntoClientRequest;
    use tokio_tungstenite::tungstenite::http::HeaderValue;

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
                    device: Some(store.device_id().to_owned()),
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
                                PERIODIC_SYNC
                            }
                            Err(e) => {
                                on_status(SyncStatus::Offline(e.to_string()));
                                backoff.next()
                            }
                        };
                        tokio::select! {
                            _ = trigger.notified() => tokio::time::sleep(PUSH_DEBOUNCE).await,
                            _ = tokio::time::sleep(wait) => {}
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
            s => Err(Error::Status(s.as_u16())),
        }
    }

    fn ws_error(e: impl std::fmt::Display) -> Error {
        Error::WebSocket(e.to_string())
    }
}
