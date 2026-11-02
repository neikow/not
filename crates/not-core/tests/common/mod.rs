#![allow(dead_code)]

use not_core::Store;
use not_protocol::{LoggedUpdate, PullResponse};

/// In-memory stand-in for the server log. Unlike the real server it also returns a store's own
/// updates, which must be harmless.
#[derive(Default)]
pub struct Log(Vec<LoggedUpdate>);

impl Log {
    pub fn push(&mut self, store: &Store) {
        if let Some(out) = store.outgoing().unwrap() {
            for u in &out.request.updates {
                let seq = self.0.len() as u64 + 1;
                self.0.push(LoggedUpdate {
                    seq,
                    doc: u.doc.clone(),
                    data: u.data.clone(),
                });
            }
            store.ack_push(out).unwrap();
        }
    }

    pub fn pull(&self, store: &Store) {
        let since = store.pull_cursor().unwrap();
        let updates = self.0.iter().filter(|u| u.seq > since).cloned().collect();
        store
            .apply_pull(&PullResponse {
                updates,
                seq: self.0.len() as u64,
                more: false,
            })
            .unwrap();
    }

    pub fn sync(&mut self, store: &Store) {
        self.push(store);
        self.pull(store);
    }
}
