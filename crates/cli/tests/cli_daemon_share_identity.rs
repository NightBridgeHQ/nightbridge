//! E2E: CLI and daemon see the same identity fingerprint.

use std::process::Stdio;
use std::time::Duration;

use assert_cmd::Command;
use tempfile::TempDir;

fn set_isolated_dirs<C>(cmd: &mut C, dir: &TempDir)
where
    C: EnvLike,
{
    let root = dir.path().to_path_buf();
    cmd.env("XDG_CONFIG_HOME", root.join("config"));
    cmd.env("XDG_DATA_HOME", root.join("data"));
    cmd.env("HOME", &root);
    cmd.env("USERPROFILE", &root);
    cmd.env("APPDATA", root.join("AppData/Roaming"));
    cmd.env("LOCALAPPDATA", root.join("AppData/Local"));
}

trait EnvLike {
    fn env(&mut self, key: &str, value: impl AsRef<std::ffi::OsStr>) -> &mut Self;
}

impl EnvLike for assert_cmd::Command {
    fn env(&mut self, key: &str, value: impl AsRef<std::ffi::OsStr>) -> &mut Self {
        assert_cmd::Command::env(self, key, value)
    }
}

impl EnvLike for std::process::Command {
    fn env(&mut self, key: &str, value: impl AsRef<std::ffi::OsStr>) -> &mut Self {
        std::process::Command::env(self, key, value)
    }
}

#[test]
fn cli_and_daemon_see_same_fingerprint() {
    let dir = TempDir::new().unwrap();

    let mut cli_show = Command::cargo_bin("localsend-improved").unwrap();
    set_isolated_dirs(&mut cli_show, &dir);
    let cli_output = cli_show.args(["identity", "show"]).output().unwrap();
    assert!(cli_output.status.success());
    let cli_stdout = String::from_utf8(cli_output.stdout).unwrap();
    let cli_fingerprint = parse_fingerprint(&cli_stdout);

    let daemon_bin = assert_cmd::cargo::cargo_bin("localsend-improved-daemon");
    let mut daemon = std::process::Command::new(daemon_bin);
    set_isolated_dirs(&mut daemon, &dir);
    let mut child = daemon
        .env("RUST_LOG", "info")
        .stderr(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("daemon spawn");

    use std::io::{BufRead, BufReader};
    let stderr = child.stderr.take().expect("stderr");
    let mut reader = BufReader::new(stderr);
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    let mut line = String::new();
    let mut daemon_fingerprint = None;

    while std::time::Instant::now() < deadline {
        line.clear();
        if reader.read_line(&mut line).unwrap_or(0) == 0 {
            std::thread::sleep(Duration::from_millis(50));
            continue;
        }
        if line.contains("identity loaded") {
            daemon_fingerprint = parse_json_string_field(&line, "fp");
            if daemon_fingerprint.is_some() {
                break;
            }
        }
    }

    let _ = child.kill();
    let _ = child.wait();

    let daemon_fingerprint = daemon_fingerprint.expect("daemon did not log identity within 5s");
    assert_eq!(cli_fingerprint, daemon_fingerprint);
}

fn parse_fingerprint(stdout: &str) -> String {
    for line in stdout.lines() {
        if let Some(rest) = line.strip_prefix("fingerprint: ") {
            return rest.trim().to_string();
        }
    }
    panic!("no fingerprint line in CLI output:\n{stdout}");
}

fn parse_json_string_field(line: &str, field: &str) -> Option<String> {
    let needle = format!("\"{field}\":\"");
    let start = line.find(&needle)?;
    let rest = &line[start + needle.len()..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}
