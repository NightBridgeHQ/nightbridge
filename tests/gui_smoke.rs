use std::env;
use std::process::{Command, Stdio};

#[test]
fn gui_smoke_is_gated() {
    if env::var("LSI_RUN_GUI_SMOKE").as_deref() != Ok("1") {
        eprintln!("skipping GUI smoke: set LSI_RUN_GUI_SMOKE=1 to run");
        return;
    }
    if !command_exists("tauri-driver") {
        eprintln!("skipping GUI smoke: tauri-driver is not available");
        return;
    }

    let status = Command::new("bash")
        .arg("scripts/gui-smoke.sh")
        .stdin(Stdio::null())
        .status()
        .expect("failed to start scripts/gui-smoke.sh");
    assert!(status.success(), "gui-smoke.sh failed with {status}");
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
