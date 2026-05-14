use tempfile::TempDir;
use tokio::fs;

use lsi_protocol_native_v1::dto::{default_extensions, NativePeerInfo};
use lsi_protocol_native_v1::manifest::ManifestStore;
use lsi_protocol_native_v1::transfer::{NativeTransferReceiver, NativeTransferSender};

fn peer() -> NativePeerInfo {
    NativePeerInfo {
        alias: "sender".to_string(),
        fingerprint: "peer-fingerprint".to_string(),
        pubkey: [7; 32],
        quic_port: 53_400,
        extensions: default_extensions(),
    }
}

#[tokio::test]
async fn interrupted_transfer_resumes_missing_ranges() {
    let temp = TempDir::new().unwrap();
    let source = temp.path().join("large.bin");
    let bytes = (0..8192).map(|i| (i % 251) as u8).collect::<Vec<_>>();
    fs::write(&source, &bytes).await.unwrap();

    let manifest_path = temp.path().join("manifests.db");
    let sender = NativeTransferSender::new(1024);
    let request = sender.prepare_files(&[source.clone()]).await.unwrap();

    {
        let receiver = NativeTransferReceiver::trusted(
            temp.path().join("inbox"),
            ManifestStore::open(&manifest_path).unwrap(),
            peer().fingerprint,
        );
        receiver.accept_transfer(&peer(), &request).await.unwrap();
        receiver.receive_chunk(&request.transfer_id, "file-0", 0, &bytes[..1024]).await.unwrap();
    }

    let receiver = NativeTransferReceiver::trusted(
        temp.path().join("inbox"),
        ManifestStore::open(&manifest_path).unwrap(),
        peer().fingerprint,
    );
    sender.resume_files_to_receiver(&receiver, &request, vec![source]).await.unwrap();

    let written = fs::read(temp.path().join("inbox/large.bin")).await.unwrap();
    assert_eq!(written, bytes);
}
