//! Native chunked transfer data path.

use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use tokio::fs::{self, OpenOptions};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncSeekExt, AsyncWrite, AsyncWriteExt};
use uuid::Uuid;

use crate::chunk::{blake3_hex, DEFAULT_CHUNK_SIZE};
use crate::dto::{AcceptTransfer, FileOffer, NativePeerInfo, RequestTransfer};
use crate::manifest::{ManifestStore, ResumeRange, TransferDirection, TransferState};
use crate::{NativeError, Result};

/// Native transfer sender.
#[derive(Clone, Debug)]
pub struct NativeTransferSender {
    chunk_size: u64,
}

impl Default for NativeTransferSender {
    fn default() -> Self {
        Self::new(DEFAULT_CHUNK_SIZE)
    }
}

impl NativeTransferSender {
    /// Create a sender with a fixed chunk size.
    pub fn new(chunk_size: u64) -> Self {
        Self { chunk_size }
    }

    /// Send local files to an already-connected receiver abstraction.
    pub async fn send_files_to_receiver(
        &self,
        receiver: &NativeTransferReceiver,
        peer: NativePeerInfo,
        paths: Vec<PathBuf>,
    ) -> Result<String> {
        if self.chunk_size == 0 {
            return Err(transfer_error("chunk size must be greater than zero"));
        }
        if paths.is_empty() {
            return Err(transfer_error("at least one path is required"));
        }

        let request = self.prepare_request(&paths).await?;
        let accepted = receiver.accept_transfer(&peer, &request).await?;
        self.send_ranges(receiver, &request, &paths, accepted.chunk_size, None).await?;
        receiver.finish_transfer(&request.transfer_id).await?;
        Ok(request.transfer_id)
    }

    /// Prepare a transfer request without sending bytes.
    pub async fn prepare_files(&self, paths: &[PathBuf]) -> Result<RequestTransfer> {
        self.prepare_request(paths).await
    }

    /// Resume an existing transfer by sending only ranges requested by the receiver.
    pub async fn resume_files_to_receiver(
        &self,
        receiver: &NativeTransferReceiver,
        request: &RequestTransfer,
        paths: Vec<PathBuf>,
    ) -> Result<()> {
        if paths.len() != request.files.len() {
            return Err(transfer_error("resume paths must match request files"));
        }
        let plan = receiver.resume_plan(&request.transfer_id, self.chunk_size)?;
        self.send_ranges(receiver, request, &paths, self.chunk_size, Some(&plan.missing)).await?;
        receiver.finish_transfer(&request.transfer_id).await
    }

    async fn prepare_request(&self, paths: &[PathBuf]) -> Result<RequestTransfer> {
        let mut files = Vec::with_capacity(paths.len());
        for (index, path) in paths.iter().enumerate() {
            let bytes = fs::read(path).await?;
            files.push(FileOffer {
                file_id: format!("file-{index}"),
                file_name: file_name(path)?,
                size: bytes.len() as u64,
                blake3: Some(blake3_hex(&bytes)),
            });
        }

        Ok(RequestTransfer { transfer_id: Uuid::new_v4().to_string(), files })
    }

    async fn send_ranges(
        &self,
        receiver: &NativeTransferReceiver,
        request: &RequestTransfer,
        paths: &[PathBuf],
        chunk_size: u64,
        resume_plan: Option<&BTreeMap<String, Vec<crate::manifest::ResumeRange>>>,
    ) -> Result<()> {
        for (file, path) in request.files.iter().zip(paths.iter()) {
            let ranges = match resume_plan.and_then(|plan| plan.get(&file.file_id)) {
                Some(ranges) => ranges.clone(),
                None => receiver.missing_ranges(&request.transfer_id, &file.file_id, chunk_size)?,
            };
            let mut input = fs::File::open(path).await?;
            for range in ranges {
                input.seek(std::io::SeekFrom::Start(range.offset)).await?;
                let mut bytes = vec![0_u8; range.length as usize];
                input.read_exact(&mut bytes).await?;
                receiver
                    .receive_chunk_with_hash(
                        &request.transfer_id,
                        &file.file_id,
                        range.offset,
                        &bytes,
                        &blake3_hex(&bytes),
                    )
                    .await?;
            }
        }
        Ok(())
    }
}

/// Missing range plan for an interrupted transfer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResumePlan {
    /// Transfer id being resumed.
    pub transfer_id: String,
    /// Missing ranges per file id.
    pub missing: BTreeMap<String, Vec<ResumeRange>>,
}

/// One native data chunk transported after a transfer is accepted.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransferChunk {
    /// Sender-scoped file id.
    pub file_id: String,
    /// Byte offset inside the file.
    pub offset: u64,
    /// BLAKE3 hex digest for `bytes`.
    pub blake3: String,
    /// Chunk bytes.
    pub bytes: Vec<u8>,
}

/// Write one binary chunk frame.
pub async fn write_chunk_frame<W>(writer: &mut W, chunk: &TransferChunk) -> Result<()>
where
    W: AsyncWrite + Unpin,
{
    if chunk.file_id.trim().is_empty() {
        return Err(transfer_error("file id is required"));
    }
    if chunk.blake3.len() != 64 || !chunk.blake3.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return Err(transfer_error("chunk blake3 must be 64 hex characters"));
    }
    if chunk.bytes.is_empty() {
        return Err(transfer_error("chunk bytes must not be empty"));
    }
    if blake3_hex(&chunk.bytes) != chunk.blake3 {
        return Err(transfer_error("chunk hash mismatch"));
    }

    let file_id = chunk.file_id.as_bytes();
    let file_id_len: u16 =
        file_id.len().try_into().map_err(|_| transfer_error("file id is too long"))?;
    let bytes_len: u32 =
        chunk.bytes.len().try_into().map_err(|_| transfer_error("chunk is too large"))?;
    let frame_len = 2_u32
        .checked_add(file_id_len as u32)
        .and_then(|len| len.checked_add(8))
        .and_then(|len| len.checked_add(64))
        .and_then(|len| len.checked_add(4))
        .and_then(|len| len.checked_add(bytes_len))
        .ok_or_else(|| transfer_error("chunk frame is too large"))?;

    writer.write_all(&frame_len.to_be_bytes()).await?;
    writer.write_all(&file_id_len.to_be_bytes()).await?;
    writer.write_all(file_id).await?;
    writer.write_all(&chunk.offset.to_be_bytes()).await?;
    writer.write_all(chunk.blake3.as_bytes()).await?;
    writer.write_all(&bytes_len.to_be_bytes()).await?;
    writer.write_all(&chunk.bytes).await?;
    writer.flush().await?;
    Ok(())
}

/// Read one binary chunk frame.
pub async fn read_chunk_frame<R>(reader: &mut R) -> Result<TransferChunk>
where
    R: AsyncRead + Unpin,
{
    let frame_len = reader.read_u32().await? as usize;
    if frame_len < 2 + 8 + 64 + 4 {
        return Err(transfer_error("chunk frame is too small"));
    }

    let file_id_len = reader.read_u16().await? as usize;
    if file_id_len == 0 || file_id_len > frame_len {
        return Err(transfer_error("invalid file id length"));
    }
    let mut file_id = vec![0_u8; file_id_len];
    reader.read_exact(&mut file_id).await?;
    let file_id =
        String::from_utf8(file_id).map_err(|_| transfer_error("file id must be valid UTF-8"))?;

    let offset = reader.read_u64().await?;
    let mut blake3 = vec![0_u8; 64];
    reader.read_exact(&mut blake3).await?;
    let blake3 = String::from_utf8(blake3)
        .map_err(|_| transfer_error("chunk blake3 must be valid UTF-8"))?;
    let bytes_len = reader.read_u32().await? as usize;
    let expected_len = 2 + file_id_len + 8 + 64 + 4 + bytes_len;
    if expected_len != frame_len {
        return Err(transfer_error("chunk frame length mismatch"));
    }
    let mut bytes = vec![0_u8; bytes_len];
    reader.read_exact(&mut bytes).await?;

    let chunk = TransferChunk { file_id, offset, blake3, bytes };
    if blake3_hex(&chunk.bytes) != chunk.blake3 {
        return Err(transfer_error("chunk hash mismatch"));
    }
    Ok(chunk)
}

/// Native transfer receiver.
pub struct NativeTransferReceiver {
    inbox_dir: PathBuf,
    manifest: ManifestStore,
    trusted_peer_fingerprint: Option<String>,
    blocked: bool,
    chunk_size: u64,
}

impl NativeTransferReceiver {
    /// Create a receiver that accepts one trusted peer fingerprint.
    pub fn trusted(inbox_dir: PathBuf, manifest: ManifestStore, peer_fingerprint: String) -> Self {
        Self {
            inbox_dir,
            manifest,
            trusted_peer_fingerprint: Some(peer_fingerprint),
            blocked: false,
            chunk_size: DEFAULT_CHUNK_SIZE,
        }
    }

    /// Create a receiver that rejects transfers.
    pub fn blocked(inbox_dir: PathBuf, manifest: ManifestStore) -> Self {
        Self {
            inbox_dir,
            manifest,
            trusted_peer_fingerprint: None,
            blocked: true,
            chunk_size: DEFAULT_CHUNK_SIZE,
        }
    }

    /// Accept a transfer request and allocate manifest state.
    pub async fn accept_transfer(
        &self,
        peer: &NativePeerInfo,
        request: &RequestTransfer,
    ) -> Result<AcceptTransfer> {
        if self.blocked {
            return Err(transfer_error("peer is blocked"));
        }
        if self.trusted_peer_fingerprint.as_deref() != Some(peer.fingerprint.as_str()) {
            return Err(transfer_error("peer is not trusted"));
        }
        crate::dto::validate_transfer_request(request)?;

        fs::create_dir_all(self.incoming_dir(&request.transfer_id)).await?;
        if self.manifest.state(&request.transfer_id)?.is_none() {
            self.manifest.create_transfer(
                &request.transfer_id,
                &peer.fingerprint,
                TransferDirection::Receive,
                &request.files,
            )?;
        }
        self.manifest.set_state(&request.transfer_id, TransferState::Active)?;

        Ok(AcceptTransfer { transfer_id: request.transfer_id.clone(), chunk_size: self.chunk_size })
    }

    /// Receive, verify, and persist one chunk.
    pub async fn receive_chunk(
        &self,
        transfer_id: &str,
        file_id: &str,
        offset: u64,
        bytes: &[u8],
    ) -> Result<()> {
        self.receive_chunk_with_hash(transfer_id, file_id, offset, bytes, &blake3_hex(bytes)).await
    }

    /// Receive a chunk after validating its advertised BLAKE3 digest.
    pub async fn receive_chunk_with_hash(
        &self,
        transfer_id: &str,
        file_id: &str,
        offset: u64,
        bytes: &[u8],
        expected_blake3: &str,
    ) -> Result<()> {
        if bytes.is_empty() {
            return Err(transfer_error("chunk bytes must not be empty"));
        }
        let actual_blake3 = blake3_hex(bytes);
        if actual_blake3 != expected_blake3 {
            return Err(transfer_error("chunk hash mismatch"));
        }

        let path = self.incoming_file_path(transfer_id, file_id);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await?;
        }
        let mut file =
            OpenOptions::new().create(true).truncate(false).write(true).open(&path).await?;
        file.seek(std::io::SeekFrom::Start(offset)).await?;
        file.write_all(bytes).await?;
        file.flush().await?;

        self.manifest.record_verified_chunk(
            transfer_id,
            file_id,
            offset,
            bytes.len() as u64,
            &actual_blake3,
        )?;
        Ok(())
    }

    /// Return missing ranges for a transfer file.
    pub fn missing_ranges(
        &self,
        transfer_id: &str,
        file_id: &str,
        chunk_size: u64,
    ) -> Result<Vec<crate::manifest::ResumeRange>> {
        self.manifest.missing_ranges(transfer_id, file_id, chunk_size)
    }

    /// Build a resume plan for all files in a transfer.
    pub fn resume_plan(&self, transfer_id: &str, chunk_size: u64) -> Result<ResumePlan> {
        let mut missing = BTreeMap::new();
        for file in self.manifest.files(transfer_id)? {
            missing.insert(
                file.file_id.clone(),
                self.manifest.missing_ranges(transfer_id, &file.file_id, chunk_size)?,
            );
        }
        Ok(ResumePlan { transfer_id: transfer_id.to_string(), missing })
    }

    /// Verify all files and publish complete files to the inbox.
    pub async fn finish_transfer(&self, transfer_id: &str) -> Result<()> {
        if !self.manifest.transfer_complete(transfer_id)? {
            self.manifest.set_state(transfer_id, TransferState::Interrupted)?;
            return Err(transfer_error("transfer has missing bytes"));
        }

        let files = self.manifest.files(transfer_id)?;
        for file in &files {
            let incoming = self.incoming_file_path(transfer_id, &file.file_id);
            let bytes = fs::read(&incoming).await?;
            if bytes.len() as u64 != file.size {
                self.manifest.set_state(transfer_id, TransferState::Failed)?;
                return Err(transfer_error("file size mismatch"));
            }
            if let Some(expected) = &file.blake3 {
                let actual = blake3_hex(&bytes);
                if &actual != expected {
                    self.manifest.set_state(transfer_id, TransferState::Failed)?;
                    return Err(transfer_error("file hash mismatch"));
                }
            }
        }

        fs::create_dir_all(&self.inbox_dir).await?;
        for file in files {
            let incoming = self.incoming_file_path(transfer_id, &file.file_id);
            let target = available_target_path(&self.inbox_dir, &file.file_name).await?;
            fs::rename(incoming, target).await?;
        }
        self.manifest.set_state(transfer_id, TransferState::Completed)?;
        Ok(())
    }

    fn incoming_dir(&self, transfer_id: &str) -> PathBuf {
        self.inbox_dir.join(".incoming/native").join(safe_component(transfer_id))
    }

    fn incoming_file_path(&self, transfer_id: &str, file_id: &str) -> PathBuf {
        self.incoming_dir(transfer_id).join(format!("{}.part", safe_component(file_id)))
    }
}

async fn available_target_path(inbox_dir: &Path, file_name: &str) -> Result<PathBuf> {
    let safe_name = safe_file_name(file_name)?;
    let candidate = inbox_dir.join(&safe_name);
    if fs::metadata(&candidate).await.is_err() {
        return Ok(candidate);
    }

    let path = Path::new(&safe_name);
    let stem = path.file_stem().and_then(OsStr::to_str).unwrap_or("file");
    let extension = path.extension().and_then(OsStr::to_str);
    for index in 1..=9999 {
        let name = match extension {
            Some(extension) => format!("{stem} ({index}).{extension}"),
            None => format!("{stem} ({index})"),
        };
        let candidate = inbox_dir.join(name);
        if fs::metadata(&candidate).await.is_err() {
            return Ok(candidate);
        }
    }

    Err(transfer_error("could not allocate unique target filename"))
}

fn file_name(path: &Path) -> Result<String> {
    path.file_name()
        .and_then(OsStr::to_str)
        .map(str::to_string)
        .ok_or_else(|| transfer_error("path does not have a valid file name"))
}

fn safe_file_name(file_name: &str) -> Result<String> {
    if file_name.contains('/') || file_name.contains('\\') || file_name == "." || file_name == ".."
    {
        return Err(transfer_error("unsafe file name"));
    }
    let trimmed = file_name.trim();
    if trimmed.is_empty() {
        return Err(transfer_error("file name is required"));
    }
    Ok(trimmed.to_string())
}

fn safe_component(value: &str) -> String {
    let cleaned = value
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_'))
        .collect::<String>();
    if cleaned.is_empty() {
        Uuid::new_v4().to_string()
    } else {
        cleaned
    }
}

fn transfer_error(message: impl Into<String>) -> NativeError {
    NativeError::Protocol(message.into())
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use tempfile::TempDir;
    use tokio::fs;

    use crate::chunk::DEFAULT_CHUNK_SIZE;
    use crate::dto::{default_extensions, FileOffer, NativePeerInfo, RequestTransfer};
    use crate::manifest::ManifestStore;
    use crate::transfer::{NativeTransferReceiver, NativeTransferSender};

    fn peer() -> NativePeerInfo {
        NativePeerInfo {
            alias: "sender".to_string(),
            fingerprint: "peer-fingerprint".to_string(),
            pubkey: [7; 32],
            quic_port: 53_400,
            extensions: default_extensions(),
        }
    }

    async fn write_file(path: &Path, bytes: &[u8]) {
        fs::write(path, bytes).await.unwrap();
    }

    #[tokio::test]
    async fn send_file_writes_to_receiver_inbox() {
        let temp = TempDir::new().unwrap();
        let source = temp.path().join("source.txt");
        write_file(&source, b"hello native").await;
        let receiver = NativeTransferReceiver::trusted(
            temp.path().join("inbox"),
            ManifestStore::open_in_memory().unwrap(),
            peer().fingerprint,
        );
        let sender = NativeTransferSender::new(DEFAULT_CHUNK_SIZE);

        sender.send_files_to_receiver(&receiver, peer(), vec![source]).await.unwrap();

        let written = fs::read_to_string(temp.path().join("inbox/source.txt")).await.unwrap();
        assert_eq!(written, "hello native");
    }

    #[tokio::test]
    async fn send_multiple_files_writes_all_files() {
        let temp = TempDir::new().unwrap();
        let first = temp.path().join("a.txt");
        let second = temp.path().join("b.txt");
        write_file(&first, b"alpha").await;
        write_file(&second, b"beta").await;
        let receiver = NativeTransferReceiver::trusted(
            temp.path().join("inbox"),
            ManifestStore::open_in_memory().unwrap(),
            peer().fingerprint,
        );
        let sender = NativeTransferSender::new(2);

        sender.send_files_to_receiver(&receiver, peer(), vec![first, second]).await.unwrap();

        assert_eq!(fs::read_to_string(temp.path().join("inbox/a.txt")).await.unwrap(), "alpha");
        assert_eq!(fs::read_to_string(temp.path().join("inbox/b.txt")).await.unwrap(), "beta");
    }

    #[tokio::test]
    async fn receiver_rejects_untrusted_peer() {
        let temp = TempDir::new().unwrap();
        let receiver = NativeTransferReceiver::blocked(
            temp.path().join("inbox"),
            ManifestStore::open_in_memory().unwrap(),
        );
        let request = RequestTransfer {
            transfer_id: "tx".to_string(),
            files: vec![FileOffer {
                file_id: "file-1".to_string(),
                file_name: "blocked.txt".to_string(),
                size: 4,
                blake3: None,
            }],
        };

        let error = receiver.accept_transfer(&peer(), &request).await.unwrap_err();

        assert!(error.to_string().contains("blocked"));
    }

    #[tokio::test]
    async fn file_hash_mismatch_fails_transfer() {
        let temp = TempDir::new().unwrap();
        let receiver = NativeTransferReceiver::trusted(
            temp.path().join("inbox"),
            ManifestStore::open_in_memory().unwrap(),
            peer().fingerprint,
        );
        let request = RequestTransfer {
            transfer_id: "tx".to_string(),
            files: vec![FileOffer {
                file_id: "file-1".to_string(),
                file_name: "bad.txt".to_string(),
                size: 5,
                blake3: Some(crate::chunk::blake3_hex(b"other")),
            }],
        };

        receiver.accept_transfer(&peer(), &request).await.unwrap();
        receiver.receive_chunk("tx", "file-1", 0, b"hello").await.unwrap();
        let error = receiver.finish_transfer("tx").await.unwrap_err();

        assert!(error.to_string().contains("hash mismatch"));
        assert!(!temp.path().join("inbox/bad.txt").exists());
    }

    #[tokio::test]
    async fn chunk_hash_mismatch_rejects_before_write() {
        let temp = TempDir::new().unwrap();
        let receiver = NativeTransferReceiver::trusted(
            temp.path().join("inbox"),
            ManifestStore::open_in_memory().unwrap(),
            peer().fingerprint,
        );
        let request = RequestTransfer {
            transfer_id: "tx".to_string(),
            files: vec![FileOffer {
                file_id: "file-1".to_string(),
                file_name: "bad-chunk.txt".to_string(),
                size: 5,
                blake3: None,
            }],
        };

        receiver.accept_transfer(&peer(), &request).await.unwrap();
        let error = receiver
            .receive_chunk_with_hash(
                "tx",
                "file-1",
                0,
                b"hello",
                &crate::chunk::blake3_hex(b"other"),
            )
            .await
            .unwrap_err();

        assert!(error.to_string().contains("chunk hash mismatch"));
        assert!(!temp.path().join("inbox/.incoming/native/tx/file-1.part").exists());
    }

    #[tokio::test]
    async fn chunk_frame_roundtrip_preserves_metadata_and_bytes() {
        let chunk = crate::transfer::TransferChunk {
            file_id: "file-1".to_string(),
            offset: 1024,
            blake3: crate::chunk::blake3_hex(b"chunk"),
            bytes: b"chunk".to_vec(),
        };
        let mut encoded = Vec::new();

        crate::transfer::write_chunk_frame(&mut encoded, &chunk).await.unwrap();
        let decoded = crate::transfer::read_chunk_frame(&mut encoded.as_slice()).await.unwrap();

        assert_eq!(decoded, chunk);
    }
}
