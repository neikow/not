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
    #[error("network: {}", with_causes(.0))]
    Net(#[from] reqwest::Error),
    #[cfg(feature = "net")]
    #[error("server responded {0}")]
    Status(u16),
    #[cfg(feature = "net")]
    #[error("the server doesn't accept this device anymore; pair it again")]
    Unauthorized,
    #[cfg(feature = "net")]
    #[error("websocket: {0}")]
    WebSocket(String),
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

/// reqwest's own message ("error sending request for url …") hides the useful part, e.g.
/// "connection refused" or a certificate problem, in its source chain.
#[cfg(feature = "net")]
fn with_causes(e: &dyn std::error::Error) -> String {
    let mut message = e.to_string();
    let mut source = e.source();
    while let Some(cause) = source {
        message.push_str(": ");
        message.push_str(&cause.to_string());
        source = cause.source();
    }
    message
}
