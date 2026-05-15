use std::path::Path;
use std::sync::Arc;
use std::time::UNIX_EPOCH;

use lsi_proto::inbox::v1::{
    inbox_service_server::InboxService, InboxEntry, ListInboxRequest, ListInboxResponse,
};
use tonic::{Request, Response, Status};

use crate::state::DaemonState;

#[derive(Clone)]
pub(crate) struct InboxApi {
    state: Arc<DaemonState>,
}

impl InboxApi {
    pub(crate) fn new(state: Arc<DaemonState>) -> Self {
        Self { state }
    }
}

#[tonic::async_trait]
impl InboxService for InboxApi {
    async fn list_inbox(
        &self,
        _request: Request<ListInboxRequest>,
    ) -> Result<Response<ListInboxResponse>, Status> {
        let entries = list_inbox_entries(&self.state.inbox_dir).await?;
        Ok(Response::new(ListInboxResponse { entries }))
    }
}

async fn list_inbox_entries(inbox_dir: &Path) -> Result<Vec<InboxEntry>, Status> {
    if tokio::fs::metadata(inbox_dir).await.is_err() {
        return Ok(Vec::new());
    }

    let mut dir = tokio::fs::read_dir(inbox_dir).await.map_err(internal_status)?;
    let mut entries = Vec::new();

    while let Some(entry) = dir.next_entry().await.map_err(internal_status)? {
        let file_name = entry.file_name().to_string_lossy().into_owned();
        if file_name.ends_with(".incoming") {
            continue;
        }

        let metadata = entry.metadata().await.map_err(internal_status)?;
        if !metadata.is_file() {
            continue;
        }

        let path = entry.path().canonicalize().map_err(internal_status)?;
        let modified_unix_seconds = metadata
            .modified()
            .ok()
            .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
            .map(|duration| duration.as_secs() as i64)
            .unwrap_or(0);

        entries.push(InboxEntry {
            file_name,
            path: path.display().to_string(),
            size: metadata.len(),
            modified_unix_seconds,
        });
    }

    entries.sort_by(|left, right| left.file_name.cmp(&right.file_name));
    Ok(entries)
}

fn internal_status(error: impl std::fmt::Display) -> Status {
    Status::internal(error.to_string())
}
