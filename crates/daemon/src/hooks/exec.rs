//! Exec hook sink.

use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use lsi_core::hooks::HookEvent;
use tokio::{process::Command, time::timeout};

const OUTPUT_ERROR_CAP_BYTES: usize = 4096;

#[derive(Clone, Debug)]
pub(crate) struct ExecSink {
    command: String,
    timeout: Duration,
}

impl ExecSink {
    pub(crate) fn new(command: String, timeout: Duration) -> Self {
        Self { command, timeout }
    }

    pub(crate) async fn deliver(&self, event: &HookEvent) -> Result<()> {
        let mut parts = self.command.split_whitespace();
        let program = parts.next().ok_or_else(|| anyhow!("exec hook command must not be blank"))?;

        let event_json = serde_json::to_string(event).context("serialize hook event")?;
        let mut command = Command::new(program);
        command
            .args(parts)
            .env("LSI_EVENT_ID", &event.event_id)
            .env("LSI_EVENT_TYPE", event.event_type.as_str())
            .env("LSI_EVENT_JSON", event_json)
            .kill_on_drop(true);

        let output = timeout(self.timeout, command.output())
            .await
            .map_err(|_| anyhow!("exec hook command timed out after {:?}", self.timeout))?
            .with_context(|| format!("spawn exec hook command `{program}`"))?;

        if !output.status.success() {
            return Err(anyhow!(
                "exec hook command exited with {}; stdout: {}; stderr: {}",
                output.status,
                capped_output(&output.stdout),
                capped_output(&output.stderr)
            ));
        }

        Ok(())
    }
}

fn capped_output(output: &[u8]) -> String {
    let truncated = output.len() > OUTPUT_ERROR_CAP_BYTES;
    let cap = output.len().min(OUTPUT_ERROR_CAP_BYTES);
    let mut text = String::from_utf8_lossy(&output[..cap]).into_owned();
    if truncated {
        text.push_str("...[truncated]");
    }
    text
}

#[cfg(test)]
mod tests {
    use std::{fs, os::unix::fs::PermissionsExt, time::Duration};

    use lsi_core::hooks::{HookEvent, HookEventType};
    use tempfile::tempdir;

    use super::*;

    #[tokio::test]
    async fn exec_hook_times_out_when_command_exceeds_timeout() {
        let dir = tempdir().unwrap();
        let script = dir.path().join("slow-hook");
        fs::write(&script, "#!/bin/sh\nsleep 2\n").unwrap();
        make_executable(&script);

        let sink = ExecSink::new(script.display().to_string(), Duration::from_millis(50));

        let error = sink.deliver(&hook_event()).await.unwrap_err();

        assert!(error.to_string().contains("timed out"), "{error}");
    }

    #[tokio::test]
    async fn exec_hook_sets_event_environment_variables() {
        let dir = tempdir().unwrap();
        let script = dir.path().join("env-hook");
        let output = dir.path().join("env.out");
        fs::write(
            &script,
            "#!/bin/sh\nprintf '%s\\n%s\\n%s\\n' \"$LSI_EVENT_ID\" \"$LSI_EVENT_TYPE\" \"$LSI_EVENT_JSON\" > \"$1\"\n",
        )
        .unwrap();
        make_executable(&script);

        let sink = ExecSink::new(
            format!("{} {}", script.display(), output.display()),
            Duration::from_secs(2),
        );

        sink.deliver(&hook_event()).await.unwrap();

        let contents = fs::read_to_string(output).unwrap();
        let mut lines = contents.lines();
        assert_eq!(lines.next(), Some("event-1"));
        assert_eq!(lines.next(), Some("transfer_started"));
        assert_eq!(
            lines.next(),
            Some(
                r#"{"event_id":"event-1","event_type":"transfer_started","occurred_at_unix_seconds":42,"payload":{"transfer_id":"tx-1"}}"#
            )
        );
    }

    fn make_executable(path: &std::path::Path) {
        let mut permissions = fs::metadata(path).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(path, permissions).unwrap();
    }

    fn hook_event() -> HookEvent {
        HookEvent {
            event_id: "event-1".to_string(),
            event_type: HookEventType::TransferStarted,
            occurred_at_unix_seconds: 42,
            payload: serde_json::json!({ "transfer_id": "tx-1" }),
        }
    }
}
