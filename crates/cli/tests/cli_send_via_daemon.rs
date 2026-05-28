use std::io::Read;
use std::net::{SocketAddr, TcpListener};
use std::path::{Path, PathBuf};
use std::process::{Child, Stdio};
use std::time::{Duration, Instant};

use assert_cmd::Command;
use lsi_protocol_localsend_v2::{
    dto::{DeviceInfo, Protocol},
    server::{LocalSendReceivePolicy, LocalSendServer, LocalSendServerConfig},
};
use predicates::prelude::*;
use tempfile::TempDir;

fn set_isolated_dirs_assert(cmd: &mut Command, dir: &TempDir) {
    let root = dir.path().to_path_buf();
    create_isolated_dirs(&root);
    cmd.env("XDG_CONFIG_HOME", root.join("config"));
    cmd.env("XDG_DATA_HOME", root.join("data"));
    cmd.env("HOME", &root);
    cmd.env("USERPROFILE", &root);
    cmd.env("APPDATA", root.join("AppData/Roaming"));
    cmd.env("LOCALAPPDATA", root.join("AppData/Local"));
}

fn set_isolated_dirs_process(cmd: &mut std::process::Command, dir: &TempDir) {
    let root = dir.path().to_path_buf();
    create_isolated_dirs(&root);
    cmd.env("XDG_CONFIG_HOME", root.join("config"));
    cmd.env("XDG_DATA_HOME", root.join("data"));
    cmd.env("HOME", &root);
    cmd.env("USERPROFILE", &root);
    cmd.env("APPDATA", root.join("AppData/Roaming"));
    cmd.env("LOCALAPPDATA", root.join("AppData/Local"));
}

fn create_isolated_dirs(root: &Path) {
    let _ = std::fs::create_dir_all(root.join("config"));
    let _ = std::fs::create_dir_all(root.join("data"));
    let _ = std::fs::create_dir_all(root.join("AppData/Roaming"));
    let _ = std::fs::create_dir_all(root.join("AppData/Local"));
}

#[cfg_attr(windows, ignore = "daemon send E2E is not stable on Windows CI")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn send_uploads_file_through_daemon_api() {
    let dir = TempDir::new().unwrap();
    let receiver_dir = TempDir::new().unwrap();
    let source = dir.path().join("daemon-send.txt");
    std::fs::write(&source, b"hello via daemon").unwrap();
    let (receiver_addr, receiver_shutdown, receiver_task) =
        spawn_localsend_receiver(receiver_dir.path()).await;
    let grpc_port = unused_port();
    let http_port = unused_port();
    let mut daemon = spawn_daemon(&dir, grpc_port, http_port);
    let token = wait_for_api_token(&dir, &mut daemon);

    let mut cmd = Command::cargo_bin("night-bridge").unwrap();
    set_isolated_dirs_assert(&mut cmd, &dir);
    let assert = cmd
        .timeout(Duration::from_secs(10))
        .args([
            "--daemon-grpc",
            &format!("http://127.0.0.1:{grpc_port}"),
            "--api-token",
            &token,
            "send",
            "--url",
            &format!("http://{receiver_addr}"),
            source.to_str().unwrap(),
        ])
        .assert();

    let uploaded = std::fs::read(receiver_dir.path().join("daemon-send.txt"));

    let _ = receiver_shutdown.send(());
    receiver_task.abort();
    let _ = daemon.kill();
    let _ = daemon.wait();

    assert.success().stdout(predicate::str::contains("transfer: "));
    assert_eq!(uploaded.unwrap(), b"hello via daemon");
}

fn spawn_daemon(dir: &TempDir, grpc_port: u16, http_port: u16) -> Child {
    let mut daemon = std::process::Command::new(daemon_bin_path());
    set_isolated_dirs_process(&mut daemon, dir);
    daemon
        .args([
            "--disable-localsend-v2",
            "--disable-native",
            "--api-grpc-port",
            &grpc_port.to_string(),
            "--api-http-port",
            &http_port.to_string(),
            "--inbox",
            dir.path().join("daemon-inbox").to_str().unwrap(),
        ])
        .env("RUST_LOG", "info")
        .stderr(Stdio::piped())
        .stdout(Stdio::null())
        .spawn()
        .expect("daemon spawn")
}

fn wait_for_api_token(dir: &TempDir, daemon: &mut Child) -> String {
    let deadline = Instant::now() + Duration::from_secs(20);
    while Instant::now() < deadline {
        if let Some(path) = find_api_token(dir.path()) {
            if let Ok(token) = std::fs::read_to_string(path) {
                let token = token.trim().to_string();
                if !token.is_empty() {
                    return token;
                }
            }
        }
        if let Some(status) = daemon.try_wait().expect("daemon status") {
            let mut stderr = String::new();
            if let Some(mut pipe) = daemon.stderr.take() {
                let _ = pipe.read_to_string(&mut stderr);
            }
            panic!("daemon exited before api.token was written: {status}\n{stderr}");
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let _ = daemon.kill();
    let _ = daemon.wait();
    let mut stderr = String::new();
    if let Some(mut pipe) = daemon.stderr.take() {
        let _ = pipe.read_to_string(&mut stderr);
    }
    panic!("daemon did not write api.token within 20s\n{stderr}");
}

fn find_api_token(root: &Path) -> Option<PathBuf> {
    let entries = std::fs::read_dir(root).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.file_name().is_some_and(|name| name == "api.token") {
            return Some(path);
        }
        if path.is_dir() {
            if let Some(found) = find_api_token(&path) {
                return Some(found);
            }
        }
    }
    None
}

async fn spawn_localsend_receiver(
    inbox_dir: &Path,
) -> (
    SocketAddr,
    tokio::sync::oneshot::Sender<()>,
    tokio::task::JoinHandle<lsi_protocol_localsend_v2::Result<()>>,
) {
    let config = LocalSendServerConfig {
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        info: DeviceInfo {
            alias: "receiver".to_string(),
            version: "2.0".to_string(),
            device_model: None,
            device_type: Some("desktop".to_string()),
            fingerprint: "receiver-fingerprint".to_string(),
            port: 0,
            protocol: Protocol::from("http"),
            download: true,
        },
        inbox_dir: inbox_dir.to_path_buf(),
        session_ttl: Duration::from_secs(60),
        receive_policy: LocalSendReceivePolicy::Auto,
        trusted_fingerprints: Default::default(),
        trusted_fingerprints_file: None,
        trust_db_path: None,
        tls_identity: None,
    };
    let server = LocalSendServer::bind(config).await.unwrap();
    let addr = server.local_addr();
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
    let task = tokio::spawn(server.serve_until_shutdown(async {
        let _ = shutdown_rx.await;
    }));

    (addr, shutdown_tx, task)
}

fn unused_port() -> u16 {
    TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
}

fn daemon_bin_path() -> std::path::PathBuf {
    let path = assert_cmd::cargo::cargo_bin("night-bridge-daemon");
    let status = std::process::Command::new(env!("CARGO"))
        .args(["build", "-p", "lsi-daemon"])
        .status()
        .expect("build daemon binary");
    assert!(status.success(), "failed to build daemon binary for E2E test");
    path
}
