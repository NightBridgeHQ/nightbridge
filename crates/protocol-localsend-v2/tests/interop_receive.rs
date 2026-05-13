use std::time::Duration;

use lsi_protocol_localsend_v2::client::LocalSendClient;
use lsi_protocol_localsend_v2::dto::{DeviceInfo, Protocol};
use lsi_protocol_localsend_v2::server::{LocalSendServer, LocalSendServerConfig};

fn device_info(alias: &str, port: u16) -> DeviceInfo {
    DeviceInfo {
        alias: alias.to_string(),
        version: "2.0".to_string(),
        device_model: Some("interop-test".to_string()),
        device_type: Some("desktop".to_string()),
        fingerprint: format!("{alias}-fingerprint"),
        port,
        protocol: Protocol::from("http"),
        download: true,
    }
}

#[tokio::test]
async fn local_client_uploads_file_to_local_receiver() {
    let temp = tempfile::tempdir().unwrap();
    let inbox = temp.path().join("inbox");
    let source = temp.path().join("interop.txt");
    tokio::fs::write(&source, b"interop body").await.unwrap();
    let config = LocalSendServerConfig {
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        info: device_info("Receiver", 0),
        inbox_dir: inbox.clone(),
        session_ttl: Duration::from_secs(60),
        tls_identity: None,
    };
    let server = LocalSendServer::bind(config).await.unwrap();
    let addr = server.local_addr();
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
    let task = tokio::spawn(server.serve_until_shutdown(async {
        let _ = shutdown_rx.await;
    }));

    LocalSendClient::new()
        .unwrap()
        .send_files_to_url(&format!("http://{addr}"), vec![source], device_info("Sender", 53317))
        .await
        .unwrap();

    let written = tokio::fs::read_to_string(inbox.join("interop.txt")).await.unwrap();
    assert_eq!(written, "interop body");
    assert!(!tokio::fs::try_exists(inbox.join(".incoming/interop.txt")).await.unwrap());

    let _ = shutdown_tx.send(());
    task.await.unwrap().unwrap();
}
