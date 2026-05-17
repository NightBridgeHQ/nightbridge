//! SQLite-backed trust store.

use std::path::Path;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection};

use crate::error::{CoreError, Result};
use crate::identity::Fingerprint;

const SCHEMA: &str = include_str!("schema.sql");

/// Policy applied when this peer initiates a transfer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PeerPolicy {
    /// Accept incoming transfers without prompt.
    AutoAccept,
    /// Prompt the user/admin per transfer.
    Prompt,
    /// Refuse all transfers from this peer.
    Block,
}

impl PeerPolicy {
    fn as_str(self) -> &'static str {
        match self {
            Self::AutoAccept => "auto_accept",
            Self::Prompt => "prompt",
            Self::Block => "block",
        }
    }

    fn from_str(value: &str) -> Result<Self> {
        match value {
            "auto_accept" => Ok(Self::AutoAccept),
            "prompt" => Ok(Self::Prompt),
            "block" => Ok(Self::Block),
            other => Err(CoreError::TrustStore(format!("unknown policy {other:?}"))),
        }
    }
}

/// A persisted peer record.
#[derive(Debug, Clone)]
pub struct Peer {
    /// Human-readable fingerprint.
    pub fingerprint: Fingerprint,
    /// Raw 32-byte Ed25519 public key.
    pub pubkey: [u8; 32],
    /// User-assigned label, which may be empty.
    pub label: String,
    /// Unix timestamp when first trusted.
    pub trusted_at: i64,
    /// Unix timestamp of most recent sighting; `None` until seen.
    pub last_seen: Option<i64>,
    /// Pinned native QUIC server certificate fingerprint, if known.
    pub native_certificate_fingerprint: Option<String>,
    /// Auto-accept, prompt, or block.
    pub policy: PeerPolicy,
}

/// Thread-safe handle to the on-disk trust store.
pub struct TrustStore {
    conn: Mutex<Connection>,
}

impl TrustStore {
    /// Open or create the store at `path`.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let conn = Connection::open(path.as_ref())?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.execute_batch(SCHEMA)?;
        migrate_schema(&conn)?;
        Ok(Self { conn: Mutex::new(conn) })
    }

    /// Open an in-memory store for tests.
    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(SCHEMA)?;
        migrate_schema(&conn)?;
        Ok(Self { conn: Mutex::new(conn) })
    }

    /// Insert or update a peer with the given public key, label, and policy.
    pub fn trust(&self, pubkey: [u8; 32], label: &str, policy: PeerPolicy) -> Result<Peer> {
        let fingerprint = Fingerprint::from_pubkey(&pubkey);
        let now = now_unix();
        let conn = self.conn.lock().expect("trust store mutex poisoned");
        conn.execute(
            "INSERT INTO peers (fingerprint, pubkey, label, trusted_at, policy)
             VALUES (?, ?, ?, ?, ?)
             ON CONFLICT(fingerprint) DO UPDATE SET
               label = excluded.label,
               policy = excluded.policy",
            params![fingerprint.to_string(), pubkey.as_slice(), label, now, policy.as_str()],
        )?;
        drop(conn);
        self.get(&fingerprint)?
            .ok_or_else(|| CoreError::TrustStore("peer disappeared after insert".into()))
    }

    /// Insert or update a peer and pin its native QUIC certificate fingerprint.
    pub fn trust_with_native_certificate(
        &self,
        pubkey: [u8; 32],
        label: &str,
        policy: PeerPolicy,
        native_certificate_fingerprint: &str,
    ) -> Result<Peer> {
        validate_certificate_fingerprint(native_certificate_fingerprint)?;
        let fingerprint = Fingerprint::from_pubkey(&pubkey);
        let now = now_unix();
        let conn = self.conn.lock().expect("trust store mutex poisoned");
        conn.execute(
            "INSERT INTO peers
               (fingerprint, pubkey, label, trusted_at, policy, native_certificate_fingerprint)
             VALUES (?, ?, ?, ?, ?, ?)
             ON CONFLICT(fingerprint) DO UPDATE SET
               label = excluded.label,
               policy = excluded.policy,
               native_certificate_fingerprint = excluded.native_certificate_fingerprint",
            params![
                fingerprint.to_string(),
                pubkey.as_slice(),
                label,
                now,
                policy.as_str(),
                native_certificate_fingerprint
            ],
        )?;
        drop(conn);
        self.get(&fingerprint)?
            .ok_or_else(|| CoreError::TrustStore("peer disappeared after insert".into()))
    }

    /// Remove a peer by fingerprint. Returns `true` when a row was removed.
    pub fn untrust(&self, fingerprint: &Fingerprint) -> Result<bool> {
        let conn = self.conn.lock().expect("trust store mutex poisoned");
        let removed = conn
            .execute("DELETE FROM peers WHERE fingerprint = ?", params![fingerprint.to_string()])?;
        Ok(removed > 0)
    }

    /// Fetch a peer by fingerprint.
    pub fn get(&self, fingerprint: &Fingerprint) -> Result<Option<Peer>> {
        let conn = self.conn.lock().expect("trust store mutex poisoned");
        let mut stmt = conn.prepare(
            "SELECT fingerprint, pubkey, label, trusted_at, last_seen,
                    native_certificate_fingerprint, policy
             FROM peers WHERE fingerprint = ?",
        )?;
        let mut rows = stmt.query(params![fingerprint.to_string()])?;
        match rows.next()? {
            Some(row) => Ok(Some(row_to_peer(row)?)),
            None => Ok(None),
        }
    }

    /// List all peers in insertion order.
    pub fn list(&self) -> Result<Vec<Peer>> {
        let conn = self.conn.lock().expect("trust store mutex poisoned");
        let mut stmt = conn.prepare(
            "SELECT fingerprint, pubkey, label, trusted_at, last_seen,
                    native_certificate_fingerprint, policy
             FROM peers ORDER BY trusted_at ASC",
        )?;
        let mut rows = stmt.query([])?;
        let mut peers = Vec::new();
        while let Some(row) = rows.next()? {
            peers.push(row_to_peer(row)?);
        }
        Ok(peers)
    }

    /// Update `last_seen` for a peer to the current time.
    pub fn touch(&self, fingerprint: &Fingerprint) -> Result<()> {
        let conn = self.conn.lock().expect("trust store mutex poisoned");
        conn.execute(
            "UPDATE peers SET last_seen = ? WHERE fingerprint = ?",
            params![now_unix(), fingerprint.to_string()],
        )?;
        Ok(())
    }
}

fn migrate_schema(conn: &Connection) -> Result<()> {
    if !column_exists(conn, "peers", "native_certificate_fingerprint")? {
        conn.execute("ALTER TABLE peers ADD COLUMN native_certificate_fingerprint TEXT", [])?;
    }
    Ok(())
}

fn column_exists(conn: &Connection, table: &str, column: &str) -> Result<bool> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let name: String = row.get("name")?;
        if name == column {
            return Ok(true);
        }
    }
    Ok(false)
}

fn validate_certificate_fingerprint(value: &str) -> Result<()> {
    let valid = value.len() == 64
        && value.chars().all(|ch| ch.is_ascii_hexdigit() && !ch.is_ascii_uppercase());
    if valid {
        Ok(())
    } else {
        Err(CoreError::TrustStore(
            "native certificate fingerprint must be 64 lowercase hex characters".into(),
        ))
    }
}

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}

fn row_to_peer(row: &rusqlite::Row<'_>) -> Result<Peer> {
    let fingerprint_text: String = row.get("fingerprint")?;
    let fingerprint = fingerprint_text.parse()?;
    let pubkey_vec: Vec<u8> = row.get("pubkey")?;
    if pubkey_vec.len() != 32 {
        return Err(CoreError::TrustStore(format!("pubkey wrong length: {}", pubkey_vec.len())));
    }
    let mut pubkey = [0u8; 32];
    pubkey.copy_from_slice(&pubkey_vec);

    let policy_text: String = row.get("policy")?;
    Ok(Peer {
        fingerprint,
        pubkey,
        label: row.get("label")?,
        trusted_at: row.get("trusted_at")?,
        last_seen: row.get("last_seen")?,
        native_certificate_fingerprint: row.get("native_certificate_fingerprint")?,
        policy: PeerPolicy::from_str(&policy_text)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_pubkey(seed: u8) -> [u8; 32] {
        [seed; 32]
    }

    #[test]
    fn trust_and_get_roundtrip() {
        let store = TrustStore::open_in_memory().unwrap();
        let pk = fixture_pubkey(7);
        let peer = store.trust(pk, "NAS de casa", PeerPolicy::AutoAccept).unwrap();
        assert_eq!(peer.label, "NAS de casa");
        assert_eq!(peer.policy, PeerPolicy::AutoAccept);
        let fetched = store.get(&peer.fingerprint).unwrap().unwrap();
        assert_eq!(fetched.pubkey, pk);
        assert!(fetched.native_certificate_fingerprint.is_none());
    }

    #[test]
    fn trust_is_idempotent_and_updates_label() {
        let store = TrustStore::open_in_memory().unwrap();
        let pk = fixture_pubkey(8);
        store.trust(pk, "before", PeerPolicy::Prompt).unwrap();
        let updated = store.trust(pk, "after", PeerPolicy::AutoAccept).unwrap();
        assert_eq!(updated.label, "after");
        assert_eq!(updated.policy, PeerPolicy::AutoAccept);
    }

    #[test]
    fn untrust_removes_peer() {
        let store = TrustStore::open_in_memory().unwrap();
        let pk = fixture_pubkey(9);
        let peer = store.trust(pk, "tmp", PeerPolicy::Prompt).unwrap();
        assert!(store.untrust(&peer.fingerprint).unwrap());
        assert!(store.get(&peer.fingerprint).unwrap().is_none());
        assert!(!store.untrust(&peer.fingerprint).unwrap());
    }

    #[test]
    fn list_returns_peers_in_insertion_order() {
        let store = TrustStore::open_in_memory().unwrap();
        let _ = store.trust(fixture_pubkey(1), "a", PeerPolicy::Prompt).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(1100));
        let _ = store.trust(fixture_pubkey(2), "b", PeerPolicy::Prompt).unwrap();
        let list = store.list().unwrap();
        assert_eq!(list.len(), 2);
        assert!(list[0].trusted_at <= list[1].trusted_at);
    }

    #[test]
    fn touch_updates_last_seen() {
        let store = TrustStore::open_in_memory().unwrap();
        let peer = store.trust(fixture_pubkey(3), "x", PeerPolicy::Prompt).unwrap();
        assert!(peer.last_seen.is_none());
        store.touch(&peer.fingerprint).unwrap();
        let updated = store.get(&peer.fingerprint).unwrap().unwrap();
        assert!(updated.last_seen.is_some());
    }

    #[test]
    fn trust_with_native_certificate_roundtrip() {
        let store = TrustStore::open_in_memory().unwrap();
        let cert = "a".repeat(64);
        let peer = store
            .trust_with_native_certificate(
                fixture_pubkey(12),
                "native",
                PeerPolicy::AutoAccept,
                &cert,
            )
            .unwrap();

        assert_eq!(peer.native_certificate_fingerprint.as_deref(), Some(cert.as_str()));
        let fetched = store.get(&peer.fingerprint).unwrap().unwrap();
        assert_eq!(fetched.native_certificate_fingerprint.as_deref(), Some(cert.as_str()));
    }

    #[test]
    fn native_certificate_fingerprint_must_be_lowercase_sha256_hex() {
        let store = TrustStore::open_in_memory().unwrap();
        let error = store
            .trust_with_native_certificate(fixture_pubkey(13), "bad", PeerPolicy::AutoAccept, "ABC")
            .unwrap_err();

        assert!(error
            .to_string()
            .contains("native certificate fingerprint must be 64 lowercase hex"));
    }

    #[test]
    fn persists_to_disk_across_open() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("trust.db");
        let pk = fixture_pubkey(4);
        {
            let store = TrustStore::open(&path).unwrap();
            store.trust(pk, "kept", PeerPolicy::AutoAccept).unwrap();
        }
        let store2 = TrustStore::open(&path).unwrap();
        assert_eq!(store2.list().unwrap().len(), 1);
    }
}
