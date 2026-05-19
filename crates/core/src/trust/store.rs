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

/// LocalSend-compatible peer approval state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalSendPeerStatus {
    /// Seen on a rejected receive attempt and waiting for admin review.
    Pending,
    /// Approved for future LocalSend-compatible receives.
    Trusted,
    /// Explicitly denied.
    Blocked,
}

impl LocalSendPeerStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Trusted => "trusted",
            Self::Blocked => "blocked",
        }
    }

    fn from_str(value: &str) -> Result<Self> {
        match value {
            "pending" => Ok(Self::Pending),
            "trusted" => Ok(Self::Trusted),
            "blocked" => Ok(Self::Blocked),
            other => Err(CoreError::TrustStore(format!("unknown LocalSend peer status {other:?}"))),
        }
    }
}

/// A persisted official LocalSend compatibility peer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalSendPeer {
    /// Normalized LocalSend fingerprint.
    pub fingerprint: String,
    /// Last advertised LocalSend alias.
    pub alias: String,
    /// Admin-assigned label.
    pub label: Option<String>,
    /// Pending, trusted, or blocked.
    pub status: LocalSendPeerStatus,
    /// Unix timestamp when first seen.
    pub first_seen: i64,
    /// Unix timestamp when most recently seen or updated.
    pub last_seen: i64,
    /// Number of rejected attempts observed.
    pub attempt_count: u64,
    /// Last known source IP, when available.
    pub source_ip: Option<String>,
}

/// A recently discovered official LocalSend LAN peer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalSendLanPeer {
    /// Normalized LocalSend fingerprint.
    pub fingerprint: String,
    /// Last advertised LocalSend alias.
    pub alias: String,
    /// Last known source IP.
    pub source_ip: String,
    /// Advertised LocalSend API port.
    pub port: u16,
    /// Advertised protocol, usually `https`.
    pub protocol: String,
    /// Advertised device model, when available.
    pub device_model: Option<String>,
    /// Advertised device type, when available.
    pub device_type: Option<String>,
    /// Whether the peer advertises download support.
    pub download: bool,
    /// Unix timestamp when first seen.
    pub first_seen: i64,
    /// Unix timestamp when most recently seen.
    pub last_seen: i64,
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

    /// Record an untrusted incoming LocalSend peer attempt.
    pub fn record_pending_localsend_peer(
        &self,
        fingerprint: &str,
        alias: &str,
        source_ip: Option<&str>,
    ) -> Result<LocalSendPeer> {
        let fingerprint = normalize_localsend_fingerprint(fingerprint)?;
        require_nonblank("LocalSend alias", alias)?;
        let now = now_unix();
        let conn = self.conn.lock().expect("trust store mutex poisoned");
        conn.execute(
            "INSERT INTO localsend_peers
               (fingerprint, alias, status, first_seen, last_seen, attempt_count, source_ip)
             VALUES (?, ?, 'pending', ?, ?, 1, ?)
             ON CONFLICT(fingerprint) DO UPDATE SET
               alias = excluded.alias,
               last_seen = excluded.last_seen,
               attempt_count = localsend_peers.attempt_count + 1,
               source_ip = COALESCE(excluded.source_ip, localsend_peers.source_ip)",
            params![fingerprint, alias, now, now, source_ip],
        )?;
        drop(conn);
        self.get_localsend_peer(&fingerprint)?
            .ok_or_else(|| CoreError::TrustStore("LocalSend peer disappeared after insert".into()))
    }

    /// List LocalSend peers waiting for admin approval.
    pub fn list_pending_localsend_peers(&self) -> Result<Vec<LocalSendPeer>> {
        self.list_localsend_peers_by_status(LocalSendPeerStatus::Pending)
    }

    /// List LocalSend peers approved for future receives.
    pub fn list_trusted_localsend_peers(&self) -> Result<Vec<LocalSendPeer>> {
        self.list_localsend_peers_by_status(LocalSendPeerStatus::Trusted)
    }

    /// Record an approved incoming LocalSend peer sighting.
    pub fn record_trusted_localsend_peer(
        &self,
        fingerprint: &str,
        alias: &str,
        source_ip: Option<&str>,
    ) -> Result<LocalSendPeer> {
        let fingerprint = normalize_localsend_fingerprint(fingerprint)?;
        require_nonblank("LocalSend alias", alias)?;
        let now = now_unix();
        let conn = self.conn.lock().expect("trust store mutex poisoned");
        conn.execute(
            "INSERT INTO localsend_peers
               (fingerprint, alias, status, first_seen, last_seen, attempt_count, source_ip)
             VALUES (?, ?, 'trusted', ?, ?, 0, ?)
             ON CONFLICT(fingerprint) DO UPDATE SET
               alias = excluded.alias,
               last_seen = excluded.last_seen,
               source_ip = COALESCE(excluded.source_ip, localsend_peers.source_ip)",
            params![fingerprint, alias, now, now, source_ip],
        )?;
        drop(conn);
        self.get_localsend_peer(&fingerprint)?
            .ok_or_else(|| CoreError::TrustStore("LocalSend peer disappeared after insert".into()))
    }

    /// Record a LocalSend peer observed through LAN discovery registration.
    pub fn record_localsend_lan_peer(&self, peer: LocalSendLanPeer) -> Result<LocalSendLanPeer> {
        let fingerprint = normalize_localsend_fingerprint(&peer.fingerprint)?;
        require_nonblank("LocalSend alias", &peer.alias)?;
        require_nonblank("LocalSend source IP", &peer.source_ip)?;
        require_nonblank("LocalSend protocol", &peer.protocol)?;
        let now = now_unix();
        let conn = self.conn.lock().expect("trust store mutex poisoned");
        conn.execute(
            "INSERT INTO localsend_lan_peers
               (fingerprint, alias, source_ip, port, protocol, device_model, device_type,
                download, first_seen, last_seen)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(fingerprint) DO UPDATE SET
               alias = excluded.alias,
               source_ip = excluded.source_ip,
               port = excluded.port,
               protocol = excluded.protocol,
               device_model = excluded.device_model,
               device_type = excluded.device_type,
               download = excluded.download,
               last_seen = excluded.last_seen",
            params![
                fingerprint,
                peer.alias,
                peer.source_ip,
                i64::from(peer.port),
                peer.protocol,
                peer.device_model,
                peer.device_type,
                if peer.download { 1 } else { 0 },
                now,
                now
            ],
        )?;
        drop(conn);
        self.get_localsend_lan_peer(&fingerprint)?.ok_or_else(|| {
            CoreError::TrustStore("LocalSend LAN peer disappeared after insert".into())
        })
    }

    /// List recently discovered LocalSend LAN peers.
    pub fn list_localsend_lan_peers(&self) -> Result<Vec<LocalSendLanPeer>> {
        let conn = self.conn.lock().expect("trust store mutex poisoned");
        let mut stmt = conn.prepare(
            "SELECT fingerprint, alias, source_ip, port, protocol, device_model, device_type,
                    download, first_seen, last_seen
             FROM localsend_lan_peers
             ORDER BY last_seen DESC, first_seen DESC",
        )?;
        let mut rows = stmt.query([])?;
        let mut peers = Vec::new();
        while let Some(row) = rows.next()? {
            peers.push(row_to_localsend_lan_peer(row)?);
        }
        Ok(peers)
    }

    fn get_localsend_lan_peer(&self, fingerprint: &str) -> Result<Option<LocalSendLanPeer>> {
        let fingerprint = normalize_localsend_fingerprint(fingerprint)?;
        let conn = self.conn.lock().expect("trust store mutex poisoned");
        let mut stmt = conn.prepare(
            "SELECT fingerprint, alias, source_ip, port, protocol, device_model, device_type,
                    download, first_seen, last_seen
             FROM localsend_lan_peers WHERE fingerprint = ?",
        )?;
        let mut rows = stmt.query(params![fingerprint])?;
        match rows.next()? {
            Some(row) => Ok(Some(row_to_localsend_lan_peer(row)?)),
            None => Ok(None),
        }
    }

    /// Approve a LocalSend peer fingerprint for future receives.
    pub fn approve_localsend_peer(
        &self,
        fingerprint: &str,
        label: Option<&str>,
    ) -> Result<LocalSendPeer> {
        self.set_localsend_peer_status(fingerprint, LocalSendPeerStatus::Trusted, label)
    }

    /// Deny a LocalSend peer fingerprint.
    pub fn deny_localsend_peer(&self, fingerprint: &str) -> Result<LocalSendPeer> {
        self.set_localsend_peer_status(fingerprint, LocalSendPeerStatus::Blocked, None)
    }

    /// Return whether a LocalSend peer is explicitly blocked.
    pub fn is_localsend_peer_blocked(&self, fingerprint: &str) -> Result<bool> {
        Ok(self
            .get_localsend_peer(fingerprint)?
            .is_some_and(|peer| peer.status == LocalSendPeerStatus::Blocked))
    }

    /// List normalized LocalSend fingerprints approved through the store.
    pub fn trusted_localsend_fingerprints(&self) -> Result<Vec<String>> {
        Ok(self
            .list_localsend_peers_by_status(LocalSendPeerStatus::Trusted)?
            .into_iter()
            .map(|peer| peer.fingerprint)
            .collect())
    }

    /// Fetch a LocalSend peer by fingerprint.
    pub fn get_localsend_peer(&self, fingerprint: &str) -> Result<Option<LocalSendPeer>> {
        let fingerprint = normalize_localsend_fingerprint(fingerprint)?;
        let conn = self.conn.lock().expect("trust store mutex poisoned");
        let mut stmt = conn.prepare(
            "SELECT fingerprint, alias, label, status, first_seen, last_seen,
                    attempt_count, source_ip
             FROM localsend_peers WHERE fingerprint = ?",
        )?;
        let mut rows = stmt.query(params![fingerprint])?;
        match rows.next()? {
            Some(row) => Ok(Some(row_to_localsend_peer(row)?)),
            None => Ok(None),
        }
    }

    fn list_localsend_peers_by_status(
        &self,
        status: LocalSendPeerStatus,
    ) -> Result<Vec<LocalSendPeer>> {
        let conn = self.conn.lock().expect("trust store mutex poisoned");
        let mut stmt = conn.prepare(
            "SELECT fingerprint, alias, label, status, first_seen, last_seen,
                    attempt_count, source_ip
             FROM localsend_peers
             WHERE status = ?
             ORDER BY last_seen DESC, first_seen DESC",
        )?;
        let mut rows = stmt.query(params![status.as_str()])?;
        let mut peers = Vec::new();
        while let Some(row) = rows.next()? {
            peers.push(row_to_localsend_peer(row)?);
        }
        Ok(peers)
    }

    fn set_localsend_peer_status(
        &self,
        fingerprint: &str,
        status: LocalSendPeerStatus,
        label: Option<&str>,
    ) -> Result<LocalSendPeer> {
        let fingerprint = normalize_localsend_fingerprint(fingerprint)?;
        let label = label.map(str::trim).filter(|value| !value.is_empty());
        let now = now_unix();
        let conn = self.conn.lock().expect("trust store mutex poisoned");
        let updated = conn.execute(
            "UPDATE localsend_peers
             SET status = ?, label = COALESCE(?, label), last_seen = ?
             WHERE fingerprint = ?",
            params![status.as_str(), label, now, fingerprint],
        )?;
        if updated == 0 {
            conn.execute(
                "INSERT INTO localsend_peers
                   (fingerprint, alias, label, status, first_seen, last_seen, attempt_count)
                 VALUES (?, '', ?, ?, ?, ?, 0)",
                params![fingerprint, label, status.as_str(), now, now],
            )?;
        }
        drop(conn);
        self.get_localsend_peer(&fingerprint)?
            .ok_or_else(|| CoreError::TrustStore("LocalSend peer disappeared after update".into()))
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

fn normalize_localsend_fingerprint(fingerprint: &str) -> Result<String> {
    let normalized = fingerprint
        .chars()
        .filter(|ch| *ch != ':' && *ch != '-')
        .flat_map(char::to_lowercase)
        .collect::<String>();
    if normalized.is_empty() {
        return Err(CoreError::TrustStore("LocalSend fingerprint is required".into()));
    }
    Ok(normalized)
}

fn require_nonblank(label: &str, value: &str) -> Result<()> {
    if value.trim().is_empty() {
        Err(CoreError::TrustStore(format!("{label} is required")))
    } else {
        Ok(())
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

fn row_to_localsend_peer(row: &rusqlite::Row<'_>) -> Result<LocalSendPeer> {
    let status: String = row.get("status")?;
    let attempt_count: i64 = row.get("attempt_count")?;
    Ok(LocalSendPeer {
        fingerprint: row.get("fingerprint")?,
        alias: row.get("alias")?,
        label: row.get("label")?,
        status: LocalSendPeerStatus::from_str(&status)?,
        first_seen: row.get("first_seen")?,
        last_seen: row.get("last_seen")?,
        attempt_count: u64::try_from(attempt_count)
            .map_err(|_| CoreError::TrustStore("LocalSend attempt_count is negative".into()))?,
        source_ip: row.get("source_ip")?,
    })
}

fn row_to_localsend_lan_peer(row: &rusqlite::Row<'_>) -> Result<LocalSendLanPeer> {
    let port: i64 = row.get(3)?;
    if !(0..=u16::MAX as i64).contains(&port) {
        return Err(CoreError::TrustStore("LocalSend LAN peer port is out of range".into()));
    }
    let download: i64 = row.get(7)?;
    Ok(LocalSendLanPeer {
        fingerprint: row.get(0)?,
        alias: row.get(1)?,
        source_ip: row.get(2)?,
        port: port as u16,
        protocol: row.get(4)?,
        device_model: row.get(5)?,
        device_type: row.get(6)?,
        download: download != 0,
        first_seen: row.get(8)?,
        last_seen: row.get(9)?,
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
    fn records_pending_localsend_peer_attempts() {
        let store = TrustStore::open_in_memory().unwrap();

        let first = store
            .record_pending_localsend_peer("AA:BB", "Diego iPhone", Some("10.16.20.53"))
            .unwrap();
        let second =
            store.record_pending_localsend_peer("aa-bb", "Diego Personal iPhone", None).unwrap();

        assert_eq!(first.fingerprint, "aabb");
        assert_eq!(second.fingerprint, "aabb");
        assert_eq!(second.alias, "Diego Personal iPhone");
        assert_eq!(second.attempt_count, 2);
        assert_eq!(second.source_ip.as_deref(), Some("10.16.20.53"));
        assert!(second.last_seen >= first.first_seen);
    }

    #[test]
    fn approve_and_deny_localsend_pending_peer() {
        let store = TrustStore::open_in_memory().unwrap();
        store.record_pending_localsend_peer("AA:BB", "Diego iPhone", None).unwrap();

        let approved = store.approve_localsend_peer("aa-bb", Some("personal phone")).unwrap();

        assert_eq!(approved.status, LocalSendPeerStatus::Trusted);
        assert_eq!(approved.label.as_deref(), Some("personal phone"));
        assert!(store.trusted_localsend_fingerprints().unwrap().contains(&"aabb".to_string()));

        let denied = store.deny_localsend_peer("aa-bb").unwrap();
        assert_eq!(denied.status, LocalSendPeerStatus::Blocked);
        assert!(store.trusted_localsend_fingerprints().unwrap().is_empty());
    }

    #[test]
    fn lists_only_pending_localsend_peers() {
        let store = TrustStore::open_in_memory().unwrap();
        store.record_pending_localsend_peer("aa", "pending", None).unwrap();
        store.record_pending_localsend_peer("bb", "trusted", None).unwrap();
        store.approve_localsend_peer("bb", None).unwrap();

        let pending = store.list_pending_localsend_peers().unwrap();

        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].fingerprint, "aa");
    }

    #[test]
    fn lists_trusted_localsend_peers_with_source_ip() {
        let store = TrustStore::open_in_memory().unwrap();
        store.record_pending_localsend_peer("AA:BB", "Android", Some("10.16.20.81")).unwrap();
        store.approve_localsend_peer("aa-bb", None).unwrap();

        let trusted = store.list_trusted_localsend_peers().unwrap();

        assert_eq!(trusted.len(), 1);
        assert_eq!(trusted[0].fingerprint, "aabb");
        assert_eq!(trusted[0].source_ip.as_deref(), Some("10.16.20.81"));
    }

    #[test]
    fn records_trusted_localsend_peer_sighting_without_counting_rejected_attempt() {
        let store = TrustStore::open_in_memory().unwrap();
        store.approve_localsend_peer("AA:BB", Some("Android")).unwrap();

        let seen =
            store.record_trusted_localsend_peer("aa-bb", "Rich Pear", Some("10.16.20.81")).unwrap();

        assert_eq!(seen.status, LocalSendPeerStatus::Trusted);
        assert_eq!(seen.alias, "Rich Pear");
        assert_eq!(seen.label.as_deref(), Some("Android"));
        assert_eq!(seen.attempt_count, 0);
        assert_eq!(seen.source_ip.as_deref(), Some("10.16.20.81"));
    }

    #[test]
    fn records_localsend_lan_peer_without_changing_receive_approval_state() {
        let store = TrustStore::open_in_memory().unwrap();

        let seen = store
            .record_localsend_lan_peer(LocalSendLanPeer {
                fingerprint: "AA:BB".to_string(),
                alias: "Rich Pear".to_string(),
                source_ip: "10.16.20.81".to_string(),
                port: 53317,
                protocol: "https".to_string(),
                device_model: Some("Pixel".to_string()),
                device_type: Some("mobile".to_string()),
                download: true,
                first_seen: 0,
                last_seen: 0,
            })
            .unwrap();

        let lan = store.list_localsend_lan_peers().unwrap();

        assert_eq!(seen.fingerprint, "aabb");
        assert_eq!(lan.len(), 1);
        assert_eq!(lan[0].alias, "Rich Pear");
        assert_eq!(lan[0].source_ip, "10.16.20.81");
        assert_eq!(lan[0].port, 53317);
        assert_eq!(lan[0].device_model.as_deref(), Some("Pixel"));
        assert!(store.list_pending_localsend_peers().unwrap().is_empty());
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
