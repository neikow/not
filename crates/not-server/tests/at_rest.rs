//! The server's databases encrypted at rest (SQLCipher), and moving a plain one over.

use not_core::{INBOX, Store, SyncClient};
use not_server::AppState;

fn day() -> not_core::date::Date {
    not_core::date::Date::parse("2026-11-16").unwrap()
}

/// A fresh directory with the server database path in it, removed when dropped.
struct Dir(std::path::PathBuf);

impl Dir {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("not-at-rest-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }
    fn db(&self) -> std::path::PathBuf {
        self.0.join("not.db")
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A phone pushes a task titled `secret` through a server opened from `state`.
async fn push_secret(state: AppState) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let router = not_server::router(state.clone());
    let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let code = state.create_pairing_code().unwrap();
    let client = SyncClient::from_credentials(&SyncClient::pair(&base, &code.code, "Phone").await.unwrap());
    let phone = Store::open_in_memory().unwrap();
    phone.add_task(INBOX, "the-secret-title", day()).unwrap();
    client.sync(&phone).await.unwrap();
    phone.add_task(INBOX, "another", day()).unwrap();
    client.sync(&phone).await.unwrap();
    server.abort();
}

fn file_contains(path: &std::path::Path, needle: &str) -> bool {
    let bytes = std::fs::read(path).unwrap_or_default();
    bytes.windows(needle.len()).any(|w| w == needle.as_bytes())
}

#[tokio::test]
async fn with_a_key_nothing_readable_reaches_the_disk() {
    let dir = Dir::new("key");
    let state = AppState::open_with_key(dir.db(), Some("correct horse")).unwrap();
    push_secret(state.clone()).await;
    state.compact().unwrap();
    drop(state);

    let replica = dir.0.join("not.db-replica");
    for path in [dir.db(), replica.clone()] {
        assert!(
            !file_contains(&path, "SQLite format 3"),
            "{} is encrypted",
            path.display()
        );
    }
    // Compressed Loro data wouldn't show the title anyway; the device name sits in plain columns.
    assert!(!file_contains(&dir.db(), "Phone"));

    // Same key: everything's there. No key or another key: refused.
    let again = AppState::open_with_key(dir.db(), Some("correct horse")).unwrap();
    assert_eq!(again.devices().unwrap().len(), 1);
    drop(again);
    assert!(AppState::open_with_key(dir.db(), Some("wrong")).is_err());
    let plain = AppState::open(dir.db());
    assert!(
        plain.is_err() || plain.unwrap().devices().is_err(),
        "unreadable without the key"
    );
}

#[tokio::test]
async fn a_plain_database_is_encrypted_when_a_key_is_first_given() {
    let dir = Dir::new("migrate");
    let state = AppState::open(dir.db()).unwrap();
    push_secret(state.clone()).await;
    drop(state);
    assert!(file_contains(&dir.db(), "SQLite format 3"), "plain to begin with");

    let state = AppState::open_with_key(dir.db(), Some("k3y")).unwrap();
    assert_eq!(state.devices().unwrap().len(), 1, "the paired phone survived");
    // The replica was rebuilt from the log, encrypted: compacting needs it caught up with both pushes.
    assert_eq!(state.compact().unwrap().map(|c| c.rows), Some(2));
    drop(state);
    assert!(!file_contains(&dir.db(), "SQLite format 3"));
    assert!(!file_contains(&dir.0.join("not.db-replica"), "SQLite format 3"));
    assert!(!dir.0.join("not.encrypting").exists(), "no leftovers");
    assert!(AppState::open_with_key(dir.db(), Some("k3y")).is_ok());
}
