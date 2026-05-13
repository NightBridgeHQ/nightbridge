//! Safe inbox file writer for authorized LocalSend v2 uploads.

use std::ffi::OsStr;
use std::path::{Component, Path, PathBuf};

use sha2::{Digest, Sha256};
use tokio::fs::{self, File};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use uuid::Uuid;

use crate::session::AuthorizedUpload;
use crate::{LocalSendError, Result};

/// Writes authorized uploads into a local inbox directory.
#[derive(Clone, Debug)]
pub struct InboxWriter {
    inbox_dir: PathBuf,
}

/// Result of a completed inbox write.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WrittenUpload {
    /// Final published path in the inbox directory.
    pub path: PathBuf,
    /// Number of bytes written from the upload stream.
    pub bytes_written: u64,
    /// Hex-encoded SHA-256 digest of the written bytes.
    pub sha256: String,
}

impl InboxWriter {
    /// Creates an inbox writer rooted at `inbox_dir`.
    pub fn new(inbox_dir: impl Into<PathBuf>) -> Self {
        Self { inbox_dir: inbox_dir.into() }
    }

    /// Streams an authorized upload into `.incoming` and publishes it after verification.
    pub async fn write_upload<R>(
        &self,
        upload: &AuthorizedUpload,
        mut reader: R,
    ) -> Result<WrittenUpload>
    where
        R: AsyncRead + Unpin,
    {
        let file_name = validate_file_name(&upload.meta.file_name)?;
        let incoming_dir = self.inbox_dir.join(".incoming");
        fs::create_dir_all(&incoming_dir).await?;
        fs::create_dir_all(&self.inbox_dir).await?;

        let incoming_path = incoming_dir.join(incoming_file_name(upload));
        let mut file = File::create(&incoming_path).await?;
        let mut hasher = Sha256::new();
        let mut bytes_written = 0_u64;
        let mut buffer = [0_u8; 8192];

        loop {
            let bytes_read = reader.read(&mut buffer).await?;
            if bytes_read == 0 {
                break;
            }

            file.write_all(&buffer[..bytes_read]).await?;
            hasher.update(&buffer[..bytes_read]);
            bytes_written += bytes_read as u64;
        }

        file.flush().await?;
        drop(file);

        if bytes_written != upload.meta.size {
            return Err(inbox_error(format!(
                "upload size mismatch: expected {}, got {bytes_written}",
                upload.meta.size
            )));
        }

        let sha256 = hex_digest(hasher.finalize().as_slice());
        if let Some(expected) = &upload.meta.sha256 {
            if !expected.eq_ignore_ascii_case(&sha256) {
                return Err(LocalSendError::Crypto("upload sha256 mismatch".to_string()));
            }
        }

        let target_path = next_available_target(&self.inbox_dir, file_name).await?;
        fs::rename(&incoming_path, &target_path).await?;

        Ok(WrittenUpload { path: target_path, bytes_written, sha256 })
    }
}

fn validate_file_name(file_name: &str) -> Result<&str> {
    if file_name.is_empty() || file_name.contains(['/', '\\']) {
        return Err(inbox_error("invalid upload filename"));
    }

    let path = Path::new(file_name);
    if path.is_absolute() {
        return Err(inbox_error("invalid upload filename"));
    }

    let mut components = path.components();
    match (components.next(), components.next()) {
        (Some(Component::Normal(name)), None) if !name.is_empty() => Ok(file_name),
        _ => Err(inbox_error("invalid upload filename")),
    }
}

fn incoming_file_name(upload: &AuthorizedUpload) -> String {
    format!("{}-{}.part", safe_identifier(&upload.session_id), safe_identifier(&upload.file_id))
}

fn safe_identifier(value: &str) -> String {
    let sanitized =
        value
            .chars()
            .map(|char| {
                if char.is_ascii_alphanumeric() || char == '-' || char == '_' {
                    char
                } else {
                    '-'
                }
            })
            .collect::<String>();

    if sanitized.is_empty() {
        Uuid::new_v4().to_string()
    } else {
        sanitized
    }
}

async fn next_available_target(inbox_dir: &Path, file_name: &str) -> Result<PathBuf> {
    let first = inbox_dir.join(file_name);
    if !path_exists(&first).await? {
        return Ok(first);
    }

    let path = Path::new(file_name);
    let stem = path.file_stem().and_then(OsStr::to_str).unwrap_or(file_name);
    let extension = path.extension().and_then(OsStr::to_str);

    for suffix in 1_u32.. {
        let candidate_name = match extension {
            Some(extension) => format!("{stem} ({suffix}).{extension}"),
            None => format!("{stem} ({suffix})"),
        };
        let candidate = inbox_dir.join(candidate_name);

        if !path_exists(&candidate).await? {
            return Ok(candidate);
        }
    }

    unreachable!("unbounded suffix loop always returns or awaits an I/O error")
}

async fn path_exists(path: &Path) -> Result<bool> {
    match fs::try_exists(path).await {
        Ok(exists) => Ok(exists),
        Err(error) => Err(error.into()),
    }
}

fn hex_digest(bytes: &[u8]) -> String {
    bytes.iter().fold(String::with_capacity(bytes.len() * 2), |mut hex, byte| {
        use std::fmt::Write as _;
        write!(&mut hex, "{byte:02x}").expect("writing to string cannot fail");
        hex
    })
}

fn inbox_error(message: impl Into<String>) -> LocalSendError {
    LocalSendError::Http(format!("inbox: {}", message.into()))
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use sha2::{Digest, Sha256};
    use tempfile::TempDir;
    use tokio::fs;

    use crate::dto::FileMeta;
    use crate::inbox::InboxWriter;
    use crate::session::AuthorizedUpload;

    fn sha256_hex(bytes: &[u8]) -> String {
        let digest = Sha256::digest(bytes);
        digest.iter().fold(String::with_capacity(64), |mut hex, byte| {
            use std::fmt::Write as _;
            write!(&mut hex, "{byte:02x}").expect("writing to string cannot fail");
            hex
        })
    }

    fn upload(file_name: &str, bytes: &[u8]) -> AuthorizedUpload {
        AuthorizedUpload {
            session_id: "session-1".to_string(),
            file_id: "file-1".to_string(),
            token: "token-1".to_string(),
            meta: FileMeta {
                id: "file-1".to_string(),
                file_name: file_name.to_string(),
                size: bytes.len() as u64,
                file_type: Some("text/plain".to_string()),
                sha256: Some(sha256_hex(bytes)),
                preview: None,
                metadata: None,
            },
        }
    }

    async fn read(path: impl AsRef<Path>) -> Vec<u8> {
        fs::read(path).await.unwrap()
    }

    #[tokio::test]
    async fn completed_upload_moves_from_incoming_to_inbox() {
        let temp = TempDir::new().unwrap();
        let writer = InboxWriter::new(temp.path());
        let body = b"hello inbox";
        let upload = upload("hello.txt", body);

        let written = writer.write_upload(&upload, &body[..]).await.unwrap();

        assert_eq!(written.path, temp.path().join("hello.txt"));
        assert_eq!(written.bytes_written, body.len() as u64);
        assert_eq!(read(temp.path().join("hello.txt")).await, body);
        assert!(!temp.path().join(".incoming").join("session-1-file-1.part").exists());
    }

    #[tokio::test]
    async fn cancelled_upload_stays_in_incoming() {
        let temp = TempDir::new().unwrap();
        let writer = InboxWriter::new(temp.path());
        let body = b"short";
        let mut upload = upload("cancelled.txt", body);
        upload.meta.size = 10;

        let result = writer.write_upload(&upload, &body[..]).await;

        assert!(result.is_err());
        assert!(!temp.path().join("cancelled.txt").exists());
        assert_eq!(read(temp.path().join(".incoming").join("session-1-file-1.part")).await, body);
    }

    #[tokio::test]
    async fn sha256_mismatch_fails_and_does_not_publish() {
        let temp = TempDir::new().unwrap();
        let writer = InboxWriter::new(temp.path());
        let body = b"trusted content";
        let mut upload = upload("digest.txt", body);
        upload.meta.sha256 = Some("00".repeat(32));

        let result = writer.write_upload(&upload, &body[..]).await;

        assert!(result.is_err());
        assert!(!temp.path().join("digest.txt").exists());
        assert_eq!(read(temp.path().join(".incoming").join("session-1-file-1.part")).await, body);
    }

    #[tokio::test]
    async fn filename_path_traversal_is_rejected() {
        let temp = TempDir::new().unwrap();
        let writer = InboxWriter::new(temp.path());
        let body = b"blocked";

        for file_name in ["", "../escape.txt", "nested/file.txt", "nested\\file.txt", "/abs.txt"] {
            let upload = upload(file_name, body);

            let result = writer.write_upload(&upload, &body[..]).await;

            assert!(result.is_err(), "{file_name:?} should be rejected");
        }

        assert!(!temp.path().join("escape.txt").exists());
        assert!(!temp.path().join("nested").exists());
        assert!(!temp.path().join("abs.txt").exists());
    }

    #[tokio::test]
    async fn duplicate_target_filename_gets_suffix() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("photo.jpg"), b"existing").await.unwrap();
        let writer = InboxWriter::new(temp.path());
        let body = b"new photo";
        let upload = upload("photo.jpg", body);

        let written = writer.write_upload(&upload, &body[..]).await.unwrap();

        assert_eq!(written.path, temp.path().join("photo (1).jpg"));
        assert_eq!(read(temp.path().join("photo.jpg")).await, b"existing");
        assert_eq!(read(temp.path().join("photo (1).jpg")).await, body);
    }
}
