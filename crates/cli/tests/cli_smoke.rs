use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

fn set_isolated_dirs(cmd: &mut Command, dir: &TempDir) {
    let root = dir.path().to_path_buf();
    cmd.env("XDG_CONFIG_HOME", root.join("config"));
    cmd.env("XDG_DATA_HOME", root.join("data"));
    cmd.env("HOME", &root);
    cmd.env("USERPROFILE", &root);
    cmd.env("APPDATA", root.join("AppData/Roaming"));
    cmd.env("LOCALAPPDATA", root.join("AppData/Local"));
}

#[test]
fn identity_show_prints_a_fingerprint() {
    let dir = TempDir::new().unwrap();
    let mut cmd = Command::cargo_bin("localsend-improved").unwrap();
    set_isolated_dirs(&mut cmd, &dir);

    cmd.args(["identity", "show"]).assert().success().stdout(
        predicate::str::is_match(r"fingerprint: [0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}")
            .unwrap(),
    );
}

#[test]
fn identity_show_twice_is_stable() {
    let dir = TempDir::new().unwrap();
    let mut first = Command::cargo_bin("localsend-improved").unwrap();
    set_isolated_dirs(&mut first, &dir);
    let before = first.args(["identity", "show"]).output().unwrap();

    let mut second = Command::cargo_bin("localsend-improved").unwrap();
    set_isolated_dirs(&mut second, &dir);
    let after = second.args(["identity", "show"]).output().unwrap();

    assert!(before.status.success());
    assert!(after.status.success());
    assert_eq!(before.stdout, after.stdout);
}

#[test]
fn identity_rotate_without_yes_is_a_noop() {
    let dir = TempDir::new().unwrap();
    let mut show_before = Command::cargo_bin("localsend-improved").unwrap();
    set_isolated_dirs(&mut show_before, &dir);
    let before = show_before.args(["identity", "show"]).output().unwrap();

    let mut rotate = Command::cargo_bin("localsend-improved").unwrap();
    set_isolated_dirs(&mut rotate, &dir);
    rotate
        .args(["identity", "rotate"])
        .assert()
        .success()
        .stderr(predicate::str::contains("Re-run with --yes to confirm"));

    let mut show_after = Command::cargo_bin("localsend-improved").unwrap();
    set_isolated_dirs(&mut show_after, &dir);
    let after = show_after.args(["identity", "show"]).output().unwrap();

    assert!(before.status.success());
    assert!(after.status.success());
    assert_eq!(before.stdout, after.stdout);
}

#[test]
fn identity_rotate_with_yes_changes_fingerprint() {
    let dir = TempDir::new().unwrap();
    let mut show_before = Command::cargo_bin("localsend-improved").unwrap();
    set_isolated_dirs(&mut show_before, &dir);
    let before = show_before.args(["identity", "show"]).output().unwrap();

    let mut rotate = Command::cargo_bin("localsend-improved").unwrap();
    set_isolated_dirs(&mut rotate, &dir);
    rotate
        .args(["identity", "rotate", "--yes"])
        .assert()
        .success()
        .stdout(predicate::str::contains("new fingerprint: "));

    let mut show_after = Command::cargo_bin("localsend-improved").unwrap();
    set_isolated_dirs(&mut show_after, &dir);
    let after = show_after.args(["identity", "show"]).output().unwrap();

    assert!(before.status.success());
    assert!(after.status.success());
    assert_ne!(before.stdout, after.stdout);
}

#[test]
fn peers_list_requires_daemon() {
    let dir = TempDir::new().unwrap();
    let mut cmd = Command::cargo_bin("localsend-improved").unwrap();
    set_isolated_dirs(&mut cmd, &dir);

    cmd.args(["--daemon-grpc", "http://127.0.0.1:9", "--api-token", "test-token", "peers", "list"])
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("daemon API unavailable")
                .and(predicate::str::contains("http://127.0.0.1:9")),
        );
}

#[test]
fn peers_list_lan_empty_with_short_timeout() {
    let dir = TempDir::new().unwrap();
    let mut cmd = Command::cargo_bin("localsend-improved").unwrap();
    set_isolated_dirs(&mut cmd, &dir);

    cmd.args(["peers", "list-lan", "--timeout-ms", "1"])
        .assert()
        .success()
        .stdout(predicate::str::contains("no LocalSend peers found"));
}

#[test]
fn lookup_wan_command_is_available() {
    let dir = TempDir::new().unwrap();
    let mut cmd = Command::cargo_bin("localsend-improved").unwrap();
    set_isolated_dirs(&mut cmd, &dir);

    cmd.args(["peers", "lookup-wan", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--rendezvous"));
}

#[test]
fn send_rejects_missing_file() {
    let dir = TempDir::new().unwrap();
    let mut cmd = Command::cargo_bin("localsend-improved").unwrap();
    set_isolated_dirs(&mut cmd, &dir);

    cmd.args(["send", "--direct", "--url", "http://127.0.0.1:9", "missing.txt"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("sending files"));
}

#[test]
fn send_defaults_to_daemon_api() {
    let dir = TempDir::new().unwrap();
    let file = dir.path().join("file.bin");
    std::fs::write(&file, b"payload").unwrap();
    let mut cmd = Command::cargo_bin("localsend-improved").unwrap();
    set_isolated_dirs(&mut cmd, &dir);

    cmd.args([
        "--daemon-grpc",
        "http://127.0.0.1:9",
        "--api-token",
        "test-token",
        "send",
        "--url",
        "http://127.0.0.1:9",
        file.to_str().unwrap(),
    ])
    .assert()
    .failure()
    .stderr(predicate::str::contains("daemon API unavailable"));
}

#[test]
fn send_native_requires_url_or_peer() {
    let dir = TempDir::new().unwrap();
    let file = dir.path().join("file.bin");
    std::fs::write(&file, b"native payload").unwrap();

    let mut cmd = Command::cargo_bin("localsend-improved").unwrap();
    set_isolated_dirs(&mut cmd, &dir);

    cmd.args(["send", "--native", file.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("native send requires --url or --wan --peer"));
}

#[test]
fn direct_native_requires_certificate_fingerprint() {
    let dir = TempDir::new().unwrap();
    let file = dir.path().join("file.bin");
    std::fs::write(&file, b"native payload").unwrap();

    let mut cmd = Command::cargo_bin("localsend-improved").unwrap();
    set_isolated_dirs(&mut cmd, &dir);

    cmd.args([
        "send",
        "--direct",
        "--native",
        "--url",
        "quic://127.0.0.1:53400",
        file.to_str().unwrap(),
    ])
    .assert()
    .failure()
    .stderr(predicate::str::contains("direct native send requires --native-cert-fingerprint"));
}

#[test]
fn send_wan_requires_peer() {
    let dir = TempDir::new().unwrap();
    let file = dir.path().join("file.bin");
    std::fs::write(&file, b"native payload").unwrap();

    let mut cmd = Command::cargo_bin("localsend-improved").unwrap();
    set_isolated_dirs(&mut cmd, &dir);

    cmd.args(["send", "--wan", file.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("--wan requires --peer"));
}

#[test]
fn send_wan_rejects_url() {
    let dir = TempDir::new().unwrap();
    let file = dir.path().join("file.bin");
    std::fs::write(&file, b"native payload").unwrap();

    let mut cmd = Command::cargo_bin("localsend-improved").unwrap();
    set_isolated_dirs(&mut cmd, &dir);

    cmd.args([
        "send",
        "--wan",
        "--peer",
        "abcd-1234-abcd-1234",
        "--url",
        "quic://127.0.0.1:53400",
        file.to_str().unwrap(),
    ])
    .assert()
    .failure()
    .stderr(predicate::str::contains("--wan cannot be used with --url"));
}

#[test]
fn send_wan_uses_daemon_api() {
    let dir = TempDir::new().unwrap();
    let file = dir.path().join("file.bin");
    std::fs::write(&file, b"native payload").unwrap();

    let mut cmd = Command::cargo_bin("localsend-improved").unwrap();
    set_isolated_dirs(&mut cmd, &dir);

    cmd.args([
        "--daemon-grpc",
        "http://127.0.0.1:9",
        "--api-token",
        "test-token",
        "send",
        "--wan",
        "--peer",
        "abcd-1234-abcd-1234",
        file.to_str().unwrap(),
    ])
    .assert()
    .failure()
    .stderr(predicate::str::contains("daemon API unavailable"));
}

#[test]
fn transfers_list_active_requires_daemon() {
    let dir = TempDir::new().unwrap();
    let mut cmd = Command::cargo_bin("localsend-improved").unwrap();
    set_isolated_dirs(&mut cmd, &dir);

    cmd.args([
        "--daemon-grpc",
        "http://127.0.0.1:9",
        "--api-token",
        "test-token",
        "transfers",
        "list-active",
    ])
    .assert()
    .failure()
    .stderr(
        predicate::str::contains("daemon API unavailable")
            .and(predicate::str::contains("http://127.0.0.1:9")),
    );
}

#[test]
fn transfers_resume_requires_transfer_id() {
    let dir = TempDir::new().unwrap();
    let mut cmd = Command::cargo_bin("localsend-improved").unwrap();
    set_isolated_dirs(&mut cmd, &dir);

    cmd.args(["transfers", "resume"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("<TRANSFER_ID>"));
}

#[test]
fn transfers_resume_requires_daemon() {
    let dir = TempDir::new().unwrap();
    let mut cmd = Command::cargo_bin("localsend-improved").unwrap();
    set_isolated_dirs(&mut cmd, &dir);

    cmd.args([
        "--daemon-grpc",
        "http://127.0.0.1:9",
        "--api-token",
        "test-token",
        "transfers",
        "resume",
        "transfer-123",
    ])
    .assert()
    .failure()
    .stderr(
        predicate::str::contains("daemon API unavailable")
            .and(predicate::str::contains("http://127.0.0.1:9")),
    );
}

#[test]
fn daemon_status_rejects_missing_api_token_file() {
    let dir = TempDir::new().unwrap();
    let mut cmd = Command::cargo_bin("localsend-improved").unwrap();
    set_isolated_dirs(&mut cmd, &dir);

    cmd.args(["daemon", "status"]).assert().failure().stderr(
        predicate::str::contains("api token not found")
            .and(predicate::str::contains("--api-token"))
            .and(predicate::str::contains("api.token")),
    );
}

#[test]
fn daemon_status_fails_clearly_when_daemon_unavailable() {
    let dir = TempDir::new().unwrap();
    let mut cmd = Command::cargo_bin("localsend-improved").unwrap();
    set_isolated_dirs(&mut cmd, &dir);

    cmd.args([
        "--daemon-grpc",
        "http://127.0.0.1:9",
        "--api-token",
        "test-token",
        "daemon",
        "status",
    ])
    .assert()
    .failure()
    .stderr(
        predicate::str::contains("daemon API unavailable")
            .and(predicate::str::contains("http://127.0.0.1:9")),
    );
}
