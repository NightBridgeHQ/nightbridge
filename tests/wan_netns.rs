use std::env;
use std::process::{Command, Stdio};

#[test]
fn wan_netns_smoke_is_gated_and_runs_when_requested() {
    if !cfg!(target_os = "linux") {
        eprintln!("skipping WAN netns smoke: Linux network namespaces are required");
        return;
    }
    if env::var("LSI_RUN_NETNS_TESTS").as_deref() != Ok("1") {
        eprintln!("skipping WAN netns smoke: set LSI_RUN_NETNS_TESTS=1 to run");
        return;
    }
    if !command_exists("ip") {
        eprintln!("skipping WAN netns smoke: ip command is not available");
        return;
    }
    if !has_netns_privileges() {
        eprintln!("skipping WAN netns smoke: root or CAP_NET_ADMIN privileges are required");
        return;
    }

    let status = Command::new("bash")
        .arg("scripts/wan-netns-smoke.sh")
        .stdin(Stdio::null())
        .status()
        .expect("failed to start scripts/wan-netns-smoke.sh");
    assert!(status.success(), "wan-netns-smoke.sh failed with {status}");
}

fn command_exists(command: &str) -> bool {
    Command::new("sh")
        .arg("-c")
        .arg(format!("command -v {command} >/dev/null 2>&1"))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn has_netns_privileges() -> bool {
    Command::new("sh")
        .arg("-c")
        .arg("id -u | grep -qx 0")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}
