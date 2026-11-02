#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("storage: {0}")]
    Storage(#[from] rusqlite::Error),
    #[error("crdt: {0}")]
    Crdt(#[from] loro::LoroError),
    #[error("crdt encode: {0}")]
    CrdtEncode(#[from] loro::LoroEncodeError),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("invalid: {0}")]
    Invalid(String),
    #[cfg(feature = "net")]
    #[error("network: {0}")]
    Net(#[from] reqwest::Error),
    #[cfg(feature = "net")]
    #[error("server responded {0}")]
    Status(u16),
    #[cfg(feature = "net")]
    #[error("websocket: {0}")]
    WebSocket(String),
}

pub type Result<T, E = Error> = std::result::Result<T, E>;
