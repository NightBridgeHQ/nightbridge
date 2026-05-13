//! In-memory upload session management for LocalSend v2 transfers.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use uuid::Uuid;

use crate::dto::{FileMeta, PrepareUploadRequest};
use crate::{LocalSendError, Result};

/// Prepared upload session returned after validating a prepare-upload request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreparedSession {
    /// Receiver-generated session id.
    pub session_id: String,
    /// Per-file upload tokens keyed by file id.
    pub files: BTreeMap<String, String>,
}

/// Authorized upload details for a single file transfer.
#[derive(Clone, Debug, PartialEq)]
pub struct AuthorizedUpload {
    /// Receiver-generated session id.
    pub session_id: String,
    /// Sender-scoped file id.
    pub file_id: String,
    /// Upload token that authorized the transfer.
    pub token: String,
    /// Metadata supplied in the original prepare-upload request.
    pub meta: FileMeta,
}

/// In-memory LocalSend v2 upload session manager.
#[derive(Clone, Debug)]
pub struct SessionManager {
    ttl: Duration,
    sessions: Arc<Mutex<BTreeMap<String, Session>>>,
}

#[derive(Clone, Debug)]
struct Session {
    expires_at: Instant,
    files: BTreeMap<String, SessionFile>,
}

#[derive(Clone, Debug)]
struct SessionFile {
    token: String,
    meta: FileMeta,
    uploaded: bool,
}

impl SessionManager {
    /// Creates a new in-memory session manager with the given session TTL.
    pub fn new(ttl: Duration) -> Self {
        Self { ttl, sessions: Arc::new(Mutex::new(BTreeMap::new())) }
    }

    /// Validates a prepare-upload request and stores a new upload session.
    pub fn prepare(&self, request: PrepareUploadRequest) -> Result<PreparedSession> {
        validate_file_ids(&request)?;

        let session_id = Uuid::new_v4().to_string();
        let mut prepared_files = BTreeMap::new();
        let mut session_files = BTreeMap::new();

        for (file_id, meta) in request.files {
            let token = Uuid::new_v4().to_string();
            prepared_files.insert(file_id.clone(), token.clone());
            session_files.insert(file_id, SessionFile { token, meta, uploaded: false });
        }

        let mut sessions = self.lock_sessions()?;
        sessions.insert(
            session_id.clone(),
            Session { expires_at: Instant::now() + self.ttl, files: session_files },
        );

        Ok(PreparedSession { session_id, files: prepared_files })
    }

    /// Authorizes an upload token for a pending file in a live session.
    pub fn authorize_upload(
        &self,
        session_id: &str,
        file_id: &str,
        token: &str,
    ) -> Result<AuthorizedUpload> {
        let mut sessions = self.lock_sessions()?;
        prune_expired_locked(&mut sessions);

        let session = sessions.get(session_id).ok_or_else(|| session_error("unknown session"))?;
        let file = session.files.get(file_id).ok_or_else(|| session_error("unknown file"))?;

        if file.uploaded {
            return Err(session_error("file already uploaded"));
        }

        if file.token != token {
            return Err(session_error("invalid upload token"));
        }

        Ok(AuthorizedUpload {
            session_id: session_id.to_string(),
            file_id: file_id.to_string(),
            token: token.to_string(),
            meta: file.meta.clone(),
        })
    }

    /// Marks a file in a live session as uploaded.
    pub fn mark_uploaded(&self, session_id: &str, file_id: &str) -> Result<()> {
        let mut sessions = self.lock_sessions()?;
        prune_expired_locked(&mut sessions);

        let session =
            sessions.get_mut(session_id).ok_or_else(|| session_error("unknown session"))?;
        let file = session.files.get_mut(file_id).ok_or_else(|| session_error("unknown file"))?;

        file.uploaded = true;
        Ok(())
    }

    /// Cancels a session, returning whether a live session was removed.
    pub fn cancel(&self, session_id: &str) -> Result<bool> {
        let mut sessions = self.lock_sessions()?;
        prune_expired_locked(&mut sessions);

        Ok(sessions.remove(session_id).is_some())
    }

    /// Removes expired sessions and returns the number of sessions pruned.
    pub fn prune_expired(&self) -> usize {
        self.lock_sessions().map(|mut sessions| prune_expired_locked(&mut sessions)).unwrap_or(0)
    }

    fn lock_sessions(&self) -> Result<std::sync::MutexGuard<'_, BTreeMap<String, Session>>> {
        self.sessions.lock().map_err(|_| session_error("session manager lock poisoned"))
    }
}

fn validate_file_ids(request: &PrepareUploadRequest) -> Result<()> {
    let mut seen = BTreeSet::new();

    for (file_id, meta) in &request.files {
        if file_id != &meta.id {
            return Err(session_error("file id does not match map key"));
        }

        if !seen.insert(meta.id.as_str()) {
            return Err(session_error("duplicate file id"));
        }
    }

    Ok(())
}

fn prune_expired_locked(sessions: &mut BTreeMap<String, Session>) -> usize {
    let now = Instant::now();
    let original_len = sessions.len();
    sessions.retain(|_, session| session.expires_at > now);
    original_len - sessions.len()
}

fn session_error(message: impl Into<String>) -> LocalSendError {
    LocalSendError::Session(message.into())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::thread::sleep;
    use std::time::Duration;

    use crate::dto::{DeviceInfo, FileMeta, PrepareUploadRequest, Protocol};
    use crate::session::SessionManager;

    fn device_info() -> DeviceInfo {
        DeviceInfo {
            alias: "Phone".to_string(),
            version: "2.0".to_string(),
            device_model: None,
            device_type: Some("mobile".to_string()),
            fingerprint: "abc".to_string(),
            port: 53317,
            protocol: Protocol::from("https"),
            download: true,
        }
    }

    fn file_meta(id: &str) -> FileMeta {
        FileMeta {
            id: id.to_string(),
            file_name: format!("{id}.txt"),
            size: 12,
            file_type: Some("text/plain".to_string()),
            sha256: None,
            preview: None,
            metadata: None,
        }
    }

    fn prepare_request(
        files: impl IntoIterator<Item = (&'static str, FileMeta)>,
    ) -> PrepareUploadRequest {
        PrepareUploadRequest {
            info: device_info(),
            files: files.into_iter().map(|(id, meta)| (id.to_string(), meta)).collect(),
        }
    }

    #[test]
    fn prepare_creates_session_with_token_per_file() {
        let manager = SessionManager::new(Duration::from_secs(60));

        let prepared = manager
            .prepare(prepare_request([
                ("file-1", file_meta("file-1")),
                ("file-2", file_meta("file-2")),
            ]))
            .unwrap();

        assert!(!prepared.session_id.is_empty());
        assert_eq!(prepared.files.len(), 2);
        assert!(prepared.files.contains_key("file-1"));
        assert!(prepared.files.contains_key("file-2"));
        assert_ne!(prepared.files["file-1"], prepared.files["file-2"]);
    }

    #[test]
    fn authorize_upload_returns_matching_file_metadata() {
        let manager = SessionManager::new(Duration::from_secs(60));
        let prepared = manager.prepare(prepare_request([("file-1", file_meta("file-1"))])).unwrap();

        let authorized = manager
            .authorize_upload(&prepared.session_id, "file-1", &prepared.files["file-1"])
            .unwrap();

        assert_eq!(authorized.session_id, prepared.session_id);
        assert_eq!(authorized.file_id, "file-1");
        assert_eq!(authorized.token, prepared.files["file-1"]);
        assert_eq!(authorized.meta.id, "file-1");
        assert_eq!(authorized.meta.file_name, "file-1.txt");
    }

    #[test]
    fn authorize_upload_rejects_invalid_session_file_or_token() {
        let manager = SessionManager::new(Duration::from_secs(60));
        let prepared = manager.prepare(prepare_request([("file-1", file_meta("file-1"))])).unwrap();

        assert!(manager.authorize_upload("missing", "file-1", &prepared.files["file-1"]).is_err());
        assert!(manager
            .authorize_upload(&prepared.session_id, "missing", &prepared.files["file-1"])
            .is_err());
        assert!(manager.authorize_upload(&prepared.session_id, "file-1", "wrong-token").is_err());
    }

    #[test]
    fn mark_uploaded_and_cancel_end_authorization() {
        let manager = SessionManager::new(Duration::from_secs(60));
        let prepared = manager
            .prepare(prepare_request([
                ("file-1", file_meta("file-1")),
                ("file-2", file_meta("file-2")),
            ]))
            .unwrap();

        manager.mark_uploaded(&prepared.session_id, "file-1").unwrap();

        assert!(manager
            .authorize_upload(&prepared.session_id, "file-1", &prepared.files["file-1"])
            .is_err());
        assert!(manager
            .authorize_upload(&prepared.session_id, "file-2", &prepared.files["file-2"])
            .is_ok());

        assert!(manager.cancel(&prepared.session_id).unwrap());
        assert!(!manager.cancel("missing").unwrap());

        assert!(manager
            .authorize_upload(&prepared.session_id, "file-2", &prepared.files["file-2"])
            .is_err());
    }

    #[test]
    fn prune_expired_removes_old_sessions() {
        let manager = SessionManager::new(Duration::from_millis(5));
        let prepared = manager.prepare(prepare_request([("file-1", file_meta("file-1"))])).unwrap();

        sleep(Duration::from_millis(10));

        assert_eq!(manager.prune_expired(), 1);
        assert!(manager
            .authorize_upload(&prepared.session_id, "file-1", &prepared.files["file-1"])
            .is_err());
    }

    #[test]
    fn prepare_rejects_mismatched_or_duplicate_file_ids() {
        let manager = SessionManager::new(Duration::from_secs(60));

        assert!(manager.prepare(prepare_request([("file-1", file_meta("other-id"))])).is_err());

        let duplicate_id_request = PrepareUploadRequest {
            info: device_info(),
            files: BTreeMap::from([
                ("map-key-1".to_string(), file_meta("same-id")),
                ("map-key-2".to_string(), file_meta("same-id")),
            ]),
        };

        assert!(manager.prepare(duplicate_id_request).is_err());
    }
}
