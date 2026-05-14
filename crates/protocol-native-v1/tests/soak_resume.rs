use std::path::Path;

use lsi_protocol_native_v1::chunk::blake3_hex;
use lsi_protocol_native_v1::dto::{default_extensions, NativePeerInfo};
use lsi_protocol_native_v1::manifest::ManifestStore;
use lsi_protocol_native_v1::transfer::{NativeTransferReceiver, NativeTransferSender};
use tempfile::TempDir;
use tokio::fs;
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};

const CHUNK_SIZE: u64 = 1024 * 1024;
const DEFAULT_SOAK_SIZE: u64 = 16 * 1024 * 1024;
const EXTENDED_SOAK_SIZE: u64 = 128 * 1024 * 1024;
const FORCED_RECONNECTS: u64 = 10;

fn peer() -> NativePeerInfo {
    NativePeerInfo {
        alias: "soak-sender".to_string(),
        fingerprint: "soak-peer-fingerprint".to_string(),
        pubkey: [17; 32],
        quic_port: 53_401,
        extensions: default_extensions(),
    }
}

#[tokio::test]
#[ignore = "soak test; run manually before release"]
async fn repeated_interruptions_resume_to_matching_file_hash() {
    let temp = TempDir::new().unwrap();
    let source = temp.path().join("native-soak.bin");
    let inbox = temp.path().join("inbox");
    let manifest_path = temp.path().join("manifest.db");
    let file_size = soak_size();

    let source_hash = write_patterned_file(&source, file_size).await.unwrap();
    let sender = NativeTransferSender::new(CHUNK_SIZE);
    let request = sender.prepare_files(&[source.clone()]).await.unwrap();

    for reconnect_index in 0..FORCED_RECONNECTS {
        let receiver = NativeTransferReceiver::trusted(
            inbox.clone(),
            ManifestStore::open(&manifest_path).unwrap(),
            peer().fingerprint,
        );
        receiver.accept_transfer(&peer(), &request).await.unwrap();

        let offset = reconnect_index * CHUNK_SIZE;
        if offset >= file_size {
            break;
        }
        let bytes =
            read_range(&source, offset, (file_size - offset).min(CHUNK_SIZE)).await.unwrap();
        receiver.receive_chunk(&request.transfer_id, "file-0", offset, &bytes).await.unwrap();
    }

    let receiver = NativeTransferReceiver::trusted(
        inbox.clone(),
        ManifestStore::open(&manifest_path).unwrap(),
        peer().fingerprint,
    );

    let plan = receiver.resume_plan(&request.transfer_id, CHUNK_SIZE).unwrap();
    let missing_before_resume = plan.missing.get("file-0").unwrap();
    assert!(!missing_before_resume.is_empty(), "soak setup must leave ranges to resume");

    // Until QUIC/daemon wiring lands, this harness simulates reconnects by recreating the
    // receiver over a persistent manifest. The final call exercises the public resume path.
    sender.resume_files_to_receiver(&receiver, &request, vec![source.clone()]).await.unwrap();

    let target = inbox.join("native-soak.bin");
    let final_hash = hash_file(&target).await.unwrap();
    assert_eq!(final_hash, source_hash);
}

fn soak_size() -> u64 {
    if std::env::var_os("LSI_SOAK").is_some() {
        EXTENDED_SOAK_SIZE
    } else {
        DEFAULT_SOAK_SIZE
    }
}

async fn write_patterned_file(path: &Path, size: u64) -> std::io::Result<String> {
    let mut file = fs::File::create(path).await?;
    let mut hasher = blake3::Hasher::new();
    let mut written = 0_u64;

    while written < size {
        let len = (size - written).min(CHUNK_SIZE) as usize;
        let bytes = patterned_bytes(written, len);
        file.write_all(&bytes).await?;
        hasher.update(&bytes);
        written += len as u64;
    }
    file.flush().await?;

    Ok(hasher.finalize().to_hex().to_string())
}

async fn read_range(path: &Path, offset: u64, length: u64) -> std::io::Result<Vec<u8>> {
    let mut file = fs::File::open(path).await?;
    file.seek(std::io::SeekFrom::Start(offset)).await?;
    let mut bytes = vec![0_u8; length as usize];
    file.read_exact(&mut bytes).await?;
    Ok(bytes)
}

async fn hash_file(path: &Path) -> std::io::Result<String> {
    let mut file = fs::File::open(path).await?;
    let mut hasher = blake3::Hasher::new();
    let mut buffer = vec![0_u8; CHUNK_SIZE as usize];

    loop {
        let read = file.read(&mut buffer).await?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }

    Ok(hasher.finalize().to_hex().to_string())
}

fn patterned_bytes(offset: u64, len: usize) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(len);
    for index in 0..len {
        let position = offset + index as u64;
        bytes.push(((position.wrapping_mul(31) ^ (position >> 7) ^ 0xa5) & 0xff) as u8);
    }
    bytes
}

#[test]
fn patterned_bytes_have_stable_hash() {
    let bytes = patterned_bytes(4096, 1024);

    assert_eq!(
        blake3_hex(bytes),
        "206e6997faef0f33f3e624cd7f38a9ecf09a92c326324a2f32a0c5ac9eaba923"
    );
}
