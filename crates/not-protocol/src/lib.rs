//! Wire types shared by `not-core` (client) and `not-server`.
//!
//! The server treats `data` as opaque bytes: it never decodes Loro updates.

use serde::{Deserialize, Serialize};

/// Server-assigned position in the global update log. Strictly increasing.
pub type Seq = u64;

pub const API_PREFIX: &str = "/v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PushRequest {
    /// Stable id of the pushing device; lets the server skip echoing updates back.
    pub device: String,
    pub updates: Vec<DocUpdate>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocUpdate {
    /// Document id, e.g. `workspace` or `note:<id>`.
    pub doc: String,
    #[serde(with = "b64")]
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PushResponse {
    /// Seq of the last appended update (or the current head if nothing was appended).
    pub seq: Seq,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PullQuery {
    pub since: Seq,
    /// Updates pushed by this device are omitted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PullResponse {
    pub updates: Vec<LoggedUpdate>,
    /// Cursor to resume from. Can be ahead of the last returned update when updates were skipped.
    pub seq: Seq,
    pub more: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoggedUpdate {
    pub seq: Seq,
    pub doc: String,
    #[serde(with = "b64")]
    pub data: Vec<u8>,
}

/// Sent by the server over `/v1/ws` after every push.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Poke {
    pub seq: Seq,
}

mod b64 {
    use base64::{Engine, engine::general_purpose::STANDARD};
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(bytes: &[u8], s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&STANDARD.encode(bytes))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        let s = String::deserialize(d)?;
        STANDARD.decode(s).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn update_data_is_base64_on_the_wire() {
        let u = DocUpdate {
            doc: "workspace".into(),
            data: vec![0, 1, 254, 255],
        };
        let json = serde_json::to_string(&u).unwrap();
        assert_eq!(json, r#"{"doc":"workspace","data":"AAH+/w=="}"#);
        assert_eq!(serde_json::from_str::<DocUpdate>(&json).unwrap(), u);
    }
}
