//! Transport-agnostic sync state on [`Store`], plus an HTTP client behind the `net` feature.

use loro::{ExportMode, VersionVector};
use not_protocol::{DocUpdate, PullResponse, PushRequest, Seq};

use crate::store::{SERVER_VV_KEY, WORKSPACE};
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
        for update in &response.updates {
            if update.doc != WORKSPACE {
                continue; // per-note docs are not implemented yet
            }
            let status = self.doc.import(&update.data)?;
            if status.pending.is_some() {
                // Never advance the cursor past ops Loro couldn't apply: they would be lost.
                self.flush(&mut inner)?;
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
        self.flush(&mut inner)?;
        storage::kv_set(&inner.db, SERVER_VV_KEY, &inner.server_vv.encode())?;
        storage::kv_set(&inner.db, PULL_SEQ_KEY, &response.seq.to_le_bytes())?;
        Ok(())
    }
}

/// True if `a` contains every op in `b`.
fn covers(a: &VersionVector, b: &VersionVector) -> bool {
    b.iter()
        .all(|(peer, &end)| a.get(peer).copied().unwrap_or(0) >= end)
}

#[cfg(feature = "net")]
pub use client::{SyncClient, SyncReport};

#[cfg(feature = "net")]
mod client {
    use not_protocol::{API_PREFIX, PullQuery, PullResponse, PushResponse};

    use crate::{Error, Result, Store};

    #[derive(Debug, Default, Clone, PartialEq, Eq)]
    pub struct SyncReport {
        pub pushed: bool,
        pub pulled: usize,
    }

    pub struct SyncClient {
        http: reqwest::Client,
        base: String,
        token: String,
    }

    impl SyncClient {
        pub fn new(base_url: &str, token: &str) -> Self {
            Self {
                http: reqwest::Client::new(),
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
    }

    fn checked(response: reqwest::Response) -> Result<reqwest::Response> {
        match response.status() {
            s if s.is_success() => Ok(response),
            s => Err(Error::Status(s.as_u16())),
        }
    }
}
