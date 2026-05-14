//! Persistent transfer manifest store for native resume.

use std::path::Path;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection, OptionalExtension};

use crate::dto::FileOffer;
use crate::{NativeError, Result};

const SCHEMA: &str = include_str!("manifest_schema.sql");

/// Transfer direction.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TransferDirection {
    /// This node sends bytes.
    Send,
    /// This node receives bytes.
    Receive,
}

impl TransferDirection {
    fn as_str(self) -> &'static str {
        match self {
            Self::Send => "send",
            Self::Receive => "receive",
        }
    }
}

/// Persisted transfer state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TransferState {
    /// Transfer exists but has not started.
    Pending,
    /// Transfer is currently active.
    Active,
    /// Transfer was interrupted and can be resumed.
    Interrupted,
    /// Transfer completed and all files were verified.
    Completed,
    /// Transfer was explicitly cancelled.
    Cancelled,
    /// Transfer failed.
    Failed,
}

impl TransferState {
    fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Active => "active",
            Self::Interrupted => "interrupted",
            Self::Completed => "completed",
            Self::Cancelled => "cancelled",
            Self::Failed => "failed",
        }
    }

    fn from_str(value: &str) -> Result<Self> {
        match value {
            "pending" => Ok(Self::Pending),
            "active" => Ok(Self::Active),
            "interrupted" => Ok(Self::Interrupted),
            "completed" => Ok(Self::Completed),
            "cancelled" => Ok(Self::Cancelled),
            "failed" => Ok(Self::Failed),
            other => Err(manifest_error(format!("unknown transfer state {other:?}"))),
        }
    }
}

/// A missing byte range needed to resume a file.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResumeRange {
    /// Byte offset.
    pub offset: u64,
    /// Range length.
    pub length: u64,
}

/// SQLite-backed transfer manifest store.
pub struct ManifestStore {
    conn: Mutex<Connection>,
}

impl ManifestStore {
    /// Open or create a manifest store at `path`.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        if let Some(parent) = path.as_ref().parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path).map_err(sqlite_error)?;
        init_connection(conn)
    }

    /// Open an in-memory manifest store for tests.
    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory().map_err(sqlite_error)?;
        init_connection(conn)
    }

    /// Create a transfer and persist its file offers.
    pub fn create_transfer(
        &self,
        transfer_id: &str,
        peer_fingerprint: &str,
        direction: TransferDirection,
        files: &[FileOffer],
    ) -> Result<()> {
        if transfer_id.trim().is_empty() {
            return Err(manifest_error("transfer id is required"));
        }
        if peer_fingerprint.trim().is_empty() {
            return Err(manifest_error("peer fingerprint is required"));
        }
        if files.is_empty() {
            return Err(manifest_error("at least one file is required"));
        }

        let now = now_unix();
        let mut conn = self.conn.lock().expect("manifest store mutex poisoned");
        let tx = conn.transaction().map_err(sqlite_error)?;
        tx.execute(
            "INSERT INTO transfers (transfer_id, peer_fingerprint, direction, state, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?)",
            params![
                transfer_id,
                peer_fingerprint,
                direction.as_str(),
                TransferState::Pending.as_str(),
                now,
                now
            ],
        )
        .map_err(sqlite_error)?;

        for file in files {
            tx.execute(
                "INSERT INTO transfer_files (transfer_id, file_id, file_name, size, blake3)
                 VALUES (?, ?, ?, ?, ?)",
                params![transfer_id, file.file_id, file.file_name, file.size as i64, file.blake3],
            )
            .map_err(sqlite_error)?;
        }

        tx.commit().map_err(sqlite_error)?;
        Ok(())
    }

    /// Record a chunk that has been written and verified.
    pub fn record_verified_chunk(
        &self,
        transfer_id: &str,
        file_id: &str,
        offset: u64,
        length: u64,
        blake3: &str,
    ) -> Result<()> {
        if length == 0 {
            return Err(manifest_error("chunk length must be greater than zero"));
        }
        let file_size = self.file_size(transfer_id, file_id)?;
        let end = offset.checked_add(length).ok_or_else(|| manifest_error("chunk overflow"))?;
        if end > file_size {
            return Err(manifest_error("chunk extends past file size"));
        }

        let conn = self.conn.lock().expect("manifest store mutex poisoned");
        conn.execute(
            "INSERT OR REPLACE INTO verified_chunks (transfer_id, file_id, offset, length, blake3)
             VALUES (?, ?, ?, ?, ?)",
            params![transfer_id, file_id, offset as i64, length as i64, blake3],
        )
        .map_err(sqlite_error)?;
        conn.execute(
            "UPDATE transfers SET updated_at = ? WHERE transfer_id = ?",
            params![now_unix(), transfer_id],
        )
        .map_err(sqlite_error)?;
        Ok(())
    }

    /// Returns missing ranges for `file_id`, using `chunk_size` boundaries.
    pub fn missing_ranges(
        &self,
        transfer_id: &str,
        file_id: &str,
        chunk_size: u64,
    ) -> Result<Vec<ResumeRange>> {
        if chunk_size == 0 {
            return Err(manifest_error("chunk size must be greater than zero"));
        }
        let file_size = self.file_size(transfer_id, file_id)?;
        let verified = self.verified_ranges(transfer_id, file_id)?;
        Ok(missing_ranges_for(file_size, chunk_size, &verified))
    }

    /// Set transfer state.
    pub fn set_state(&self, transfer_id: &str, state: TransferState) -> Result<()> {
        if state == TransferState::Completed && !self.transfer_complete(transfer_id)? {
            return Err(manifest_error("cannot complete transfer with missing bytes"));
        }

        let conn = self.conn.lock().expect("manifest store mutex poisoned");
        let updated = conn
            .execute(
                "UPDATE transfers SET state = ?, updated_at = ? WHERE transfer_id = ?",
                params![state.as_str(), now_unix(), transfer_id],
            )
            .map_err(sqlite_error)?;
        if updated == 0 {
            return Err(manifest_error("unknown transfer"));
        }
        Ok(())
    }

    /// Fetch transfer state.
    pub fn state(&self, transfer_id: &str) -> Result<Option<TransferState>> {
        let conn = self.conn.lock().expect("manifest store mutex poisoned");
        conn.query_row(
            "SELECT state FROM transfers WHERE transfer_id = ?",
            params![transfer_id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(sqlite_error)?
        .map(|state| TransferState::from_str(&state))
        .transpose()
    }

    /// Returns whether all files in a transfer have complete verified coverage.
    pub fn transfer_complete(&self, transfer_id: &str) -> Result<bool> {
        let conn = self.conn.lock().expect("manifest store mutex poisoned");
        let mut stmt = conn
            .prepare("SELECT file_id, size FROM transfer_files WHERE transfer_id = ?")
            .map_err(sqlite_error)?;
        let mut rows = stmt.query(params![transfer_id]).map_err(sqlite_error)?;
        let mut saw_file = false;
        while let Some(row) = rows.next().map_err(sqlite_error)? {
            saw_file = true;
            let file_id: String = row.get(0).map_err(sqlite_error)?;
            let size: i64 = row.get(1).map_err(sqlite_error)?;
            let ranges = verified_ranges_locked(&conn, transfer_id, &file_id)?;
            if !ranges_cover(size as u64, &ranges) {
                return Ok(false);
            }
        }
        Ok(saw_file)
    }

    fn file_size(&self, transfer_id: &str, file_id: &str) -> Result<u64> {
        let conn = self.conn.lock().expect("manifest store mutex poisoned");
        file_size_locked(&conn, transfer_id, file_id)
    }

    fn verified_ranges(&self, transfer_id: &str, file_id: &str) -> Result<Vec<ResumeRange>> {
        let conn = self.conn.lock().expect("manifest store mutex poisoned");
        verified_ranges_locked(&conn, transfer_id, file_id)
    }
}

fn init_connection(conn: Connection) -> Result<ManifestStore> {
    conn.pragma_update(None, "foreign_keys", "ON").map_err(sqlite_error)?;
    conn.execute_batch(SCHEMA).map_err(sqlite_error)?;
    Ok(ManifestStore { conn: Mutex::new(conn) })
}

fn file_size_locked(conn: &Connection, transfer_id: &str, file_id: &str) -> Result<u64> {
    let size = conn
        .query_row(
            "SELECT size FROM transfer_files WHERE transfer_id = ? AND file_id = ?",
            params![transfer_id, file_id],
            |row| row.get::<_, i64>(0),
        )
        .optional()
        .map_err(sqlite_error)?
        .ok_or_else(|| manifest_error("unknown transfer file"))?;
    Ok(size as u64)
}

fn verified_ranges_locked(
    conn: &Connection,
    transfer_id: &str,
    file_id: &str,
) -> Result<Vec<ResumeRange>> {
    let mut stmt = conn
        .prepare(
            "SELECT offset, length FROM verified_chunks
             WHERE transfer_id = ? AND file_id = ?
             ORDER BY offset ASC",
        )
        .map_err(sqlite_error)?;
    let mut rows = stmt.query(params![transfer_id, file_id]).map_err(sqlite_error)?;
    let mut ranges = Vec::new();
    while let Some(row) = rows.next().map_err(sqlite_error)? {
        let offset: i64 = row.get(0).map_err(sqlite_error)?;
        let length: i64 = row.get(1).map_err(sqlite_error)?;
        ranges.push(ResumeRange { offset: offset as u64, length: length as u64 });
    }
    Ok(merge_ranges(ranges))
}

fn missing_ranges_for(
    file_size: u64,
    chunk_size: u64,
    verified: &[ResumeRange],
) -> Vec<ResumeRange> {
    let verified = merge_ranges(verified.to_vec());
    let mut missing = Vec::new();
    let mut offset = 0_u64;

    while offset < file_size {
        let length = chunk_size.min(file_size - offset);
        let chunk = ResumeRange { offset, length };
        if !range_fully_covered(chunk, &verified) {
            missing.push(chunk);
        }
        offset += length;
    }

    missing
}

fn merge_ranges(mut ranges: Vec<ResumeRange>) -> Vec<ResumeRange> {
    ranges.sort_by_key(|range| range.offset);
    let mut merged: Vec<ResumeRange> = Vec::new();

    for range in ranges {
        if range.length == 0 {
            continue;
        }
        let Some(last) = merged.last_mut() else {
            merged.push(range);
            continue;
        };
        let last_end = last.offset + last.length;
        let range_end = range.offset + range.length;
        if range.offset <= last_end {
            last.length = range_end.max(last_end) - last.offset;
        } else {
            merged.push(range);
        }
    }

    merged
}

fn range_fully_covered(range: ResumeRange, verified: &[ResumeRange]) -> bool {
    let range_end = range.offset + range.length;
    verified.iter().any(|verified| {
        verified.offset <= range.offset && verified.offset + verified.length >= range_end
    })
}

fn ranges_cover(size: u64, ranges: &[ResumeRange]) -> bool {
    if size == 0 {
        return true;
    }
    merge_ranges(ranges.to_vec()) == vec![ResumeRange { offset: 0, length: size }]
}

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}

fn sqlite_error(error: rusqlite::Error) -> NativeError {
    NativeError::Manifest(error.to_string())
}

fn manifest_error(message: impl Into<String>) -> NativeError {
    NativeError::Manifest(message.into())
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use crate::dto::FileOffer;
    use crate::manifest::{ManifestStore, ResumeRange, TransferDirection, TransferState};

    fn offer(file_id: &str, size: u64) -> FileOffer {
        FileOffer {
            file_id: file_id.to_string(),
            file_name: format!("{file_id}.bin"),
            size,
            blake3: None,
        }
    }

    #[test]
    fn create_transfer_persists_files() {
        let store = ManifestStore::open_in_memory().unwrap();
        store
            .create_transfer("tx", "peer", TransferDirection::Receive, &[offer("file-1", 10)])
            .unwrap();

        assert_eq!(store.state("tx").unwrap(), Some(TransferState::Pending));
        assert_eq!(
            store.missing_ranges("tx", "file-1", 4).unwrap(),
            vec![
                ResumeRange { offset: 0, length: 4 },
                ResumeRange { offset: 4, length: 4 },
                ResumeRange { offset: 8, length: 2 },
            ]
        );
    }

    #[test]
    fn record_chunk_marks_range_verified() {
        let store = ManifestStore::open_in_memory().unwrap();
        store
            .create_transfer("tx", "peer", TransferDirection::Receive, &[offer("file-1", 10)])
            .unwrap();

        store.record_verified_chunk("tx", "file-1", 0, 4, "abc").unwrap();

        assert_eq!(
            store.missing_ranges("tx", "file-1", 4).unwrap(),
            vec![ResumeRange { offset: 4, length: 4 }, ResumeRange { offset: 8, length: 2 }]
        );
    }

    #[test]
    fn completed_transfer_requires_all_bytes_verified() {
        let store = ManifestStore::open_in_memory().unwrap();
        store
            .create_transfer("tx", "peer", TransferDirection::Receive, &[offer("file-1", 10)])
            .unwrap();

        assert!(store.set_state("tx", TransferState::Completed).is_err());

        store.record_verified_chunk("tx", "file-1", 0, 10, "abc").unwrap();
        store.set_state("tx", TransferState::Completed).unwrap();

        assert_eq!(store.state("tx").unwrap(), Some(TransferState::Completed));
    }

    #[test]
    fn resume_plan_returns_missing_ranges() {
        let store = ManifestStore::open_in_memory().unwrap();
        store
            .create_transfer("tx", "peer", TransferDirection::Receive, &[offer("file-1", 12)])
            .unwrap();
        store.record_verified_chunk("tx", "file-1", 4, 4, "abc").unwrap();

        assert_eq!(
            store.missing_ranges("tx", "file-1", 4).unwrap(),
            vec![ResumeRange { offset: 0, length: 4 }, ResumeRange { offset: 8, length: 4 }]
        );
    }

    #[test]
    fn interrupted_transfer_survives_reopen() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("manifests.db");
        {
            let store = ManifestStore::open(&path).unwrap();
            store
                .create_transfer("tx", "peer", TransferDirection::Receive, &[offer("file-1", 10)])
                .unwrap();
            store.record_verified_chunk("tx", "file-1", 0, 4, "abc").unwrap();
            store.set_state("tx", TransferState::Interrupted).unwrap();
        }

        let reopened = ManifestStore::open(&path).unwrap();

        assert_eq!(reopened.state("tx").unwrap(), Some(TransferState::Interrupted));
        assert_eq!(
            reopened.missing_ranges("tx", "file-1", 4).unwrap(),
            vec![ResumeRange { offset: 4, length: 4 }, ResumeRange { offset: 8, length: 2 }]
        );
    }
}
