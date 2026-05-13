//! LocalSend v2 JSON data transfer objects.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// LocalSend protocol identifier used in device information payloads.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Protocol(String);

impl Protocol {
    /// Returns the protocol string as received from the peer.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for Protocol {
    fn from(value: &str) -> Self {
        Self(value.to_string())
    }
}

impl From<String> for Protocol {
    fn from(value: String) -> Self {
        Self(value)
    }
}

/// Device information exchanged by LocalSend peers.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceInfo {
    /// Human-readable peer alias.
    pub alias: String,
    /// LocalSend protocol version string.
    pub version: String,
    /// Optional device model.
    pub device_model: Option<String>,
    /// Device class as a string so unknown values do not fail parsing.
    #[serde(rename = "deviceType", default)]
    pub device_type: Option<String>,
    /// TLS certificate fingerprint advertised by HTTPS peers.
    pub fingerprint: String,
    /// HTTP(S) port used by the peer.
    pub port: u16,
    /// Transport protocol, usually `http` or `https`.
    pub protocol: Protocol,
    /// Whether the peer accepts downloads/uploads through the protocol API.
    pub download: bool,
}

/// Arbitrary LocalSend file metadata.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(transparent)]
pub struct FileMetadata(pub BTreeMap<String, serde_json::Value>);

/// Metadata for a single file in a prepare-upload request.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileMeta {
    /// Sender-scoped file identifier.
    pub id: String,
    /// Original file name.
    #[serde(rename = "fileName")]
    pub file_name: String,
    /// File size in bytes.
    pub size: u64,
    /// MIME type or LocalSend file type string.
    #[serde(rename = "fileType")]
    pub file_type: Option<String>,
    /// Optional SHA-256 digest supplied by the sender.
    pub sha256: Option<String>,
    /// Optional preview payload supplied by the sender.
    pub preview: Option<String>,
    /// Optional sender metadata.
    pub metadata: Option<FileMetadata>,
}

/// Request body for `/api/localsend/v2/prepare-upload`.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct PrepareUploadRequest {
    /// Sender device information.
    pub info: DeviceInfo,
    /// Files keyed by LocalSend file id.
    pub files: BTreeMap<String, FileMeta>,
}

/// Response body for `/api/localsend/v2/prepare-upload`.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrepareUploadResponse {
    /// Receiver-generated upload session id.
    #[serde(rename = "sessionId")]
    pub session_id: String,
    /// Per-file upload tokens keyed by file id.
    pub files: BTreeMap<String, String>,
}

/// Request body for `/api/localsend/v2/register`.
pub type RegisterRequest = DeviceInfo;

/// Response body for `/api/localsend/v2/register`.
pub type RegisterResponse = DeviceInfo;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_info_matches_official_shape() {
        let json = r#"{
          "alias":"NAS",
          "version":"2.0",
          "deviceModel":"Raspberry Pi",
          "deviceType":"server",
          "fingerprint":"abc",
          "port":53317,
          "protocol":"https",
          "download":true
        }"#;

        let info: DeviceInfo = serde_json::from_str(json).unwrap();

        assert_eq!(info.alias, "NAS");
        assert_eq!(info.device_type.as_deref(), Some("server"));
        assert!(info.download);
    }

    #[test]
    fn prepare_upload_request_parses_files_map() {
        let json = r#"{
          "info":{"alias":"Phone","version":"2.0","deviceType":"mobile","fingerprint":"abc","port":53317,"protocol":"https","download":true},
          "files":{"file-1":{"id":"file-1","fileName":"photo.jpg","size":12,"fileType":"image/jpeg","sha256":null,"preview":null,"metadata":null}}
        }"#;

        let request: PrepareUploadRequest = serde_json::from_str(json).unwrap();

        assert_eq!(request.files["file-1"].file_name, "photo.jpg");
    }

    #[test]
    fn prepare_upload_response_serializes_session_and_tokens() {
        let mut files = std::collections::BTreeMap::new();
        files.insert("file-1".to_string(), "token-1".to_string());
        let response = PrepareUploadResponse { session_id: "session".into(), files };

        let json = serde_json::to_string(&response).unwrap();

        assert!(json.contains("\"sessionId\":\"session\""));
    }
}
