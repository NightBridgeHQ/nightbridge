use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;

use anyhow::{Context, Result};
use lsi_proto::{
    common::v1::Empty,
    daemon::v1::{
        daemon_service_server::{DaemonService, DaemonServiceServer},
        DaemonStatus,
    },
    events::v1::{
        events_service_server::{EventsService, EventsServiceServer},
        DaemonEvent as ProtoDaemonEvent, WatchEventsRequest,
    },
};
use std::pin::Pin;
use tokio::task::JoinHandle;
use tokio_stream::wrappers::TcpListenerStream;
use tonic::codegen::tokio_stream::Stream;
use tonic::{transport::Server, Request, Response, Status};

use crate::api::auth::BearerAuth;
use crate::state::DaemonState;

pub(crate) struct GrpcApiRuntime {
    local_addr: SocketAddr,
    shutdown_tx: tokio::sync::watch::Sender<bool>,
    server_task: JoinHandle<Result<()>>,
}

impl GrpcApiRuntime {
    pub(crate) fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }
}

#[derive(Clone)]
struct DaemonApi {
    state: Arc<DaemonState>,
}

impl DaemonApi {
    fn new(state: Arc<DaemonState>) -> Self {
        Self { state }
    }
}

#[tonic::async_trait]
impl DaemonService for DaemonApi {
    async fn get_status(&self, _request: Request<Empty>) -> Result<Response<DaemonStatus>, Status> {
        Ok(Response::new(DaemonStatus {
            alias: self.state.alias.clone(),
            fingerprint: self.state.fingerprint.clone(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            inbox_dir: self.state.inbox_dir.display().to_string(),
            localsend_port: u32::from(self.state.localsend_port),
            native_port: u32::from(self.state.native_port),
        }))
    }
}

#[tonic::async_trait]
impl EventsService for DaemonApi {
    type WatchStream = Pin<Box<dyn Stream<Item = Result<ProtoDaemonEvent, Status>> + Send>>;

    async fn watch(
        &self,
        _request: Request<WatchEventsRequest>,
    ) -> Result<Response<Self::WatchStream>, Status> {
        let mut rx = self.state.events.subscribe();
        let stream = async_stream::try_stream! {
            loop {
                match rx.recv().await {
                    Ok(event) => yield event.to_proto(),
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        };

        Ok(Response::new(Box::pin(stream) as Self::WatchStream))
    }
}

pub(crate) async fn start_grpc_runtime(
    state: Arc<DaemonState>,
    port: u16,
) -> Result<GrpcApiRuntime> {
    let bind_addr = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    let listener = tokio::net::TcpListener::bind(bind_addr)
        .await
        .with_context(|| format!("failed to bind gRPC API on {bind_addr}"))?;
    let local_addr = listener.local_addr().context("failed to read gRPC API listener address")?;
    let incoming = TcpListenerStream::new(listener);
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    let auth = BearerAuth::new(state.api_token.expose_secret());
    let daemon_api = DaemonApi::new(Arc::clone(&state));
    let events_api = DaemonApi::new(state);

    let server_task = tokio::spawn(async move {
        Server::builder()
            .add_service(DaemonServiceServer::with_interceptor(daemon_api, auth.clone()))
            .add_service(EventsServiceServer::with_interceptor(events_api, auth))
            .serve_with_incoming_shutdown(incoming, wait_for_shutdown_signal(shutdown_rx))
            .await
            .map_err(Into::into)
    });

    Ok(GrpcApiRuntime { local_addr, shutdown_tx, server_task })
}

pub(crate) async fn stop_grpc_runtime(runtime: GrpcApiRuntime) -> Result<()> {
    let _ = runtime.shutdown_tx.send(true);
    runtime.server_task.await.context("gRPC API server task panicked")??;
    Ok(())
}

async fn wait_for_shutdown_signal(mut shutdown_rx: tokio::sync::watch::Receiver<bool>) {
    while !*shutdown_rx.borrow_and_update() {
        if shutdown_rx.changed().await.is_err() {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use clap::Parser;
    use lsi_core::{
        api_token::ApiToken,
        identity::{Fingerprint, Keypair},
    };
    use lsi_proto::{
        common::v1::Empty,
        daemon::v1::daemon_service_client::DaemonServiceClient,
        events::v1::{
            daemon_event, events_service_client::EventsServiceClient, DaemonEventType,
            WatchEventsRequest,
        },
    };
    use tonic::metadata::MetadataValue;

    use super::*;
    use crate::Args;

    #[tokio::test]
    async fn grpc_status_returns_daemon_state() {
        let fixture = ApiFixture::start().await;
        let mut client = fixture.client().await;
        let status = client
            .get_status(authenticated_status_request("test-token"))
            .await
            .unwrap()
            .into_inner();

        assert_eq!(status.alias, "api-test");
        assert_eq!(status.fingerprint, fixture.fingerprint);
        assert_eq!(status.inbox_dir, fixture.state.inbox_dir.display().to_string());
        assert_eq!(status.localsend_port, 4444);
        assert_eq!(status.native_port, 4445);

        fixture.stop().await;
    }

    #[tokio::test]
    async fn grpc_rejects_missing_bearer_token() {
        let fixture = ApiFixture::start().await;
        let mut client = fixture.client().await;

        let error = client.get_status(Empty {}).await.unwrap_err();

        assert_eq!(error.code(), tonic::Code::Unauthenticated);
        fixture.stop().await;
    }

    #[tokio::test]
    async fn grpc_rejects_bad_bearer_token() {
        let fixture = ApiFixture::start().await;
        let mut client = fixture.client().await;

        let error =
            client.get_status(authenticated_status_request("wrong-token")).await.unwrap_err();

        assert_eq!(error.code(), tonic::Code::Unauthenticated);
        fixture.stop().await;
    }

    #[tokio::test]
    async fn grpc_watch_streams_daemon_events() {
        let fixture = ApiFixture::start().await;
        let mut client = fixture.events_client().await;
        let state = Arc::clone(&fixture.state);
        let emit_task = tokio::spawn(async move {
            wait_for_event_subscriber(&state).await;
            state.events.emit(crate::events::DaemonEvent::InboxChanged);
        });

        let mut stream = tokio::time::timeout(
            Duration::from_secs(1),
            client.watch(authenticated_events_request("test-token")),
        )
        .await
        .unwrap()
        .unwrap()
        .into_inner();

        let event = tokio::time::timeout(Duration::from_secs(1), stream.message())
            .await
            .unwrap()
            .unwrap()
            .unwrap();

        assert_eq!(event.r#type, DaemonEventType::InboxChanged as i32);
        assert!(matches!(event.payload, Some(daemon_event::Payload::InboxChanged(_))));
        emit_task.await.unwrap();
        drop(stream);
        drop(client);
        fixture.stop().await;
    }

    struct ApiFixture {
        runtime: GrpcApiRuntime,
        state: Arc<DaemonState>,
        fingerprint: String,
    }

    impl ApiFixture {
        async fn start() -> Self {
            let temp = tempfile::TempDir::new().unwrap();
            let args = Args::parse_from([
                "daemon",
                "--alias",
                "api-test",
                "--inbox",
                temp.path().join("inbox").to_str().unwrap(),
                "--localsend-port",
                "4444",
                "--native-port",
                "4445",
            ]);
            let identity = Keypair::generate();
            let fingerprint = Fingerprint::from_pubkey(&identity.public_bytes()).to_string();
            let state = Arc::new(DaemonState::from_args(
                &args,
                identity,
                fingerprint.clone(),
                ApiToken::new("test-token").unwrap(),
            ));
            let runtime = start_grpc_runtime(Arc::clone(&state), 0).await.unwrap();

            Self { runtime, state, fingerprint }
        }

        async fn client(&self) -> DaemonServiceClient<tonic::transport::Channel> {
            DaemonServiceClient::connect(format!("http://{}", self.runtime.local_addr()))
                .await
                .unwrap()
        }

        async fn events_client(&self) -> EventsServiceClient<tonic::transport::Channel> {
            tokio::time::timeout(
                Duration::from_secs(1),
                EventsServiceClient::connect(format!("http://{}", self.runtime.local_addr())),
            )
            .await
            .unwrap()
            .unwrap()
        }

        async fn stop(self) {
            stop_grpc_runtime(self.runtime).await.unwrap();
        }
    }

    fn authenticated_status_request(token: &str) -> Request<Empty> {
        let mut request = Request::new(Empty {});
        let value = format!("Bearer {token}");
        request.metadata_mut().insert("authorization", MetadataValue::try_from(value).unwrap());
        request
    }

    fn authenticated_events_request(token: &str) -> Request<WatchEventsRequest> {
        let mut request = Request::new(WatchEventsRequest {});
        let value = format!("Bearer {token}");
        request.metadata_mut().insert("authorization", MetadataValue::try_from(value).unwrap());
        request
    }

    async fn wait_for_event_subscriber(state: &DaemonState) {
        for _ in 0..20 {
            if state.events.subscriber_count() > 0 {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("event stream did not subscribe");
    }
}
