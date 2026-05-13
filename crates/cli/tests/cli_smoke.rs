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
fn peers_list_empty_on_fresh_install() {
    let dir = TempDir::new().unwrap();
    let mut cmd = Command::cargo_bin("localsend-improved").unwrap();
    set_isolated_dirs(&mut cmd, &dir);

    cmd.args(["peers", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("no trusted peers"));
}
