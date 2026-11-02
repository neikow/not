//! Wire types shared by `not-core` (client) and `not-server`.
//!
//! The server treats `data` as opaque bytes: it never decodes Loro updates. Every endpoint except
//! `/pair` needs `Authorization: Bearer <device token>`; the server knows which device is calling.

use std::fmt;

use serde::{Deserialize, Serialize};

/// Server-assigned position in the global update log. Strictly increasing.
pub type Seq = u64;

pub const API_PREFIX: &str = "/v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PushRequest {
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PullResponse {
    /// Updates pushed by the calling device are omitted.
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

/// `POST /pair` (no token): trades a one-time pairing code for a device token.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PairRequest {
    /// As shown to the user, e.g. `7KQ2M-X9CHD`. Case, dashes and spaces don't matter.
    pub code: String,
    /// Shown in the device list, e.g. "MacBook Air".
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PairResponse {
    pub device: DeviceInfo,
    /// Secret bearer token for this device. The server only keeps its hash.
    pub token: String,
}

/// `POST /pairings`: a fresh code another device can pair with.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PairingCode {
    pub code: String,
    /// Unix millis.
    pub expires_at: i64,
}

/// `GET /devices` returns all paired devices; `DELETE /devices/{id}` revokes one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceInfo {
    pub id: i64,
    pub name: String,
    /// Unix millis.
    pub created_at: i64,
    /// Unix millis; updated at most once a minute.
    pub last_seen_at: Option<i64>,
}

/// What a pairing QR code holds: `not://pair?server=<url>&code=<code>`. Scanning one fills in both
/// fields a new device needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PairingLink {
    pub server_url: String,
    pub code: String,
}

impl PairingLink {
    const PREFIX: &str = "not://pair?";

    /// `None` unless `text` is a pairing link with both fields.
    pub fn parse(text: &str) -> Option<Self> {
        let query = text.trim().strip_prefix(Self::PREFIX)?;
        let (mut server_url, mut code) = (None, None);
        for (key, value) in form_urlencoded::parse(query.as_bytes()) {
            match &*key {
                "server" => server_url = Some(value.into_owned()),
                "code" => code = Some(value.into_owned()),
                _ => {}
            }
        }
        Some(Self {
            server_url: server_url.filter(|s| !s.is_empty())?,
            code: code.filter(|c| !c.is_empty())?,
        })
    }
}

impl fmt::Display for PairingLink {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let query = form_urlencoded::Serializer::new(String::new())
            .append_pair("server", &self.server_url)
            .append_pair("code", &self.code)
            .finish();
        write!(f, "{}{query}", Self::PREFIX)
    }
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

    #[test]
    fn pairing_links_round_trip() {
        let link = PairingLink {
            server_url: "https://not.example.ts.net:8443/x?y=1".into(),
            code: "7KQ2M-X9CHD".into(),
        };
        let text = link.to_string();
        assert_eq!(
            text,
            "not://pair?server=https%3A%2F%2Fnot.example.ts.net%3A8443%2Fx%3Fy%3D1&code=7KQ2M-X9CHD"
        );
        assert_eq!(PairingLink::parse(&format!(" {text}\n")), Some(link));
        assert_eq!(
            PairingLink::parse("not://pair?server=https%3A%2F%2Fa&code="),
            None
        );
        assert_eq!(PairingLink::parse("https://example.com/?server=a&code=b"), None);
    }
}
