use std::env;
use std::process::{Command, Stdio};

#[test]
fn gui_smoke_is_gated() {
    if env::var("NBRG_RUN_GUI_SMOKE").as_deref() != Ok("1") {
        eprintln!("skipping GUI smoke: set NBRG_RUN_GUI_SMOKE=1 to run");
        return;
    }

    let status = Command::new("bash")
        .arg("scripts/gui-smoke.sh")
        .stdin(Stdio::null())
        .status()
        .expect("failed to start scripts/gui-smoke.sh");
    assert!(status.success(), "gui-smoke.sh failed with {status}");
}
