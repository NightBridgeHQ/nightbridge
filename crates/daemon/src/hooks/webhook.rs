//! Webhook hook sink.

use hmac::{Hmac, Mac};
use lsi_core::hooks::HookEvent;
use reqwest::{
    header::{HeaderMap, HeaderValue, CONTENT_TYPE},
    StatusCode,
};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

const SIGNATURE_HEADER: &str = "X-LSI-Signature";
const EVENT_HEADER: &str = "X-LSI-Event";
const JSON_CONTENT_TYPE: &str = "application/json";

/// HTTP webhook sink for delivering daemon hook events.
#[derive(Clone, Debug)]
pub(crate) struct WebhookSink {
    url: String,
    secret: String,
    max_attempts: u8,
    client: reqwest::Client,
}

impl WebhookSink {
    pub(crate) fn new(url: String, secret: String, max_attempts: u8) -> Self {
        Self { url, secret, max_attempts: max_attempts.max(1), client: reqwest::Client::new() }
    }

    pub(crate) async fn deliver(&self, event: &HookEvent) -> anyhow::Result<()> {
        let body = serde_json::to_vec(event)?;
        let signature = format!("sha256={}", sign_payload(&self.secret, &body));
        let mut last_retryable_error = None;

        for attempt in 1..=self.max_attempts {
            let result = self.send_once(event, body.clone(), &signature).await;
            match result {
                Ok(()) => return Ok(()),
                Err(error) if error.is_retryable() && attempt < self.max_attempts => {
                    last_retryable_error = Some(error);
                }
                Err(error) => return Err(error.into()),
            }
        }

        Err(last_retryable_error
            .unwrap_or(WebhookDeliveryError::RetryableStatus(StatusCode::INTERNAL_SERVER_ERROR))
            .into())
    }

    async fn send_once(
        &self,
        event: &HookEvent,
        body: Vec<u8>,
        signature: &str,
    ) -> Result<(), WebhookDeliveryError> {
        let response = self
            .client
            .post(&self.url)
            .headers(webhook_headers(event, signature).map_err(WebhookDeliveryError::Header)?)
            .body(body)
            .send()
            .await
            .map_err(WebhookDeliveryError::Network)?;

        let status = response.status();
        if status.is_success() {
            Ok(())
        } else if status.is_server_error() {
            Err(WebhookDeliveryError::RetryableStatus(status))
        } else {
            Err(WebhookDeliveryError::Status(status))
        }
    }
}

#[derive(Debug)]
enum WebhookDeliveryError {
    Header(reqwest::header::InvalidHeaderValue),
    Network(reqwest::Error),
    RetryableStatus(StatusCode),
    Status(StatusCode),
}

impl WebhookDeliveryError {
    fn is_retryable(&self) -> bool {
        matches!(self, Self::Network(_) | Self::RetryableStatus(_))
    }
}

impl std::fmt::Display for WebhookDeliveryError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Header(error) => write!(formatter, "invalid webhook header: {error}"),
            Self::Network(error) => write!(formatter, "webhook delivery failed: {error}"),
            Self::RetryableStatus(status) => {
                write!(formatter, "webhook delivery failed with retryable status {status}")
            }
            Self::Status(status) => {
                write!(formatter, "webhook delivery failed with status {status}")
            }
        }
    }
}

impl std::error::Error for WebhookDeliveryError {}

fn webhook_headers(
    event: &HookEvent,
    signature: &str,
) -> Result<HeaderMap, reqwest::header::InvalidHeaderValue> {
    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static(JSON_CONTENT_TYPE));
    headers.insert(EVENT_HEADER, HeaderValue::from_static(event.event_type.as_str()));
    headers.insert(SIGNATURE_HEADER, HeaderValue::from_str(signature)?);
    Ok(headers)
}

fn sign_payload(secret: &str, payload: &[u8]) -> String {
    let mut mac =
        HmacSha256::new_from_slice(secret.as_bytes()).expect("HMAC accepts keys of any length");
    mac.update(payload);
    encode_hex(&mac.finalize().into_bytes())
}

fn encode_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

#[cfg(test)]
mod tests {
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };

    use axum::{
        body::Bytes,
        extract::State,
        http::{HeaderMap, StatusCode},
        routing::post,
        Router,
    };
    use lsi_core::hooks::{HookEvent, HookEventType};
    use tokio::{net::TcpListener, sync::Mutex};

    #[tokio::test]
    async fn webhook_posts_signed_json_event() {
        let captured = Arc::new(Mutex::new(None));
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let app =
            Router::new().route("/hook", post(capture_request)).with_state(Arc::clone(&captured));

        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        let event = sample_event();
        let sink = super::WebhookSink::new(
            format!("http://{address}/hook"),
            "shared-secret".to_string(),
            1,
        );

        sink.deliver(&event).await.unwrap();

        let request = captured.lock().await.take().unwrap();
        assert_eq!(request.event_header, "transfer_completed");
        assert_eq!(request.content_type, "application/json");
        let parsed: HookEvent = serde_json::from_slice(&request.body).unwrap();
        assert_eq!(parsed, event);
        assert_eq!(
            request.signature,
            format!("sha256={}", super::sign_payload("shared-secret", &request.body))
        );
    }

    #[tokio::test]
    async fn webhook_retries_5xx_then_succeeds() {
        let state =
            Arc::new(RetryState { attempts: AtomicUsize::new(0), captured: Mutex::new(None) });
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let app =
            Router::new().route("/hook", post(capture_after_retry)).with_state(Arc::clone(&state));

        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        let event = sample_event();
        let sink = super::WebhookSink::new(
            format!("http://{address}/hook"),
            "shared-secret".to_string(),
            2,
        );

        sink.deliver(&event).await.unwrap();

        assert_eq!(state.attempts.load(Ordering::SeqCst), 2);
        assert!(state.captured.lock().await.is_some());
    }

    #[test]
    fn hmac_signature_is_deterministic() {
        let signature = super::sign_payload("secret", br#"{"event_id":"event-1"}"#);

        assert_eq!(signature, "56e353682d14cac3f804afabbd60c8d53691778dc424c405ae1860fdb5aab542");
    }

    fn sample_event() -> HookEvent {
        HookEvent {
            event_id: "event-1".to_string(),
            event_type: HookEventType::TransferCompleted,
            occurred_at_unix_seconds: 42,
            payload: serde_json::json!({ "transfer_id": "tx-1" }),
        }
    }

    #[derive(Debug)]
    struct CapturedRequest {
        signature: String,
        event_header: String,
        content_type: String,
        body: Vec<u8>,
    }

    struct RetryState {
        attempts: AtomicUsize,
        captured: Mutex<Option<CapturedRequest>>,
    }

    async fn capture_request(
        State(captured): State<Arc<Mutex<Option<CapturedRequest>>>>,
        headers: HeaderMap,
        body: Bytes,
    ) -> StatusCode {
        *captured.lock().await = Some(CapturedRequest {
            signature: header_value(&headers, "X-LSI-Signature"),
            event_header: header_value(&headers, "X-LSI-Event"),
            content_type: header_value(&headers, "content-type"),
            body: body.to_vec(),
        });
        StatusCode::NO_CONTENT
    }

    async fn capture_after_retry(
        State(state): State<Arc<RetryState>>,
        headers: HeaderMap,
        body: Bytes,
    ) -> StatusCode {
        if state.attempts.fetch_add(1, Ordering::SeqCst) == 0 {
            return StatusCode::INTERNAL_SERVER_ERROR;
        }

        *state.captured.lock().await = Some(CapturedRequest {
            signature: header_value(&headers, "X-LSI-Signature"),
            event_header: header_value(&headers, "X-LSI-Event"),
            content_type: header_value(&headers, "content-type"),
            body: body.to_vec(),
        });
        StatusCode::NO_CONTENT
    }

    fn header_value(headers: &HeaderMap, name: &str) -> String {
        headers.get(name).and_then(|value| value.to_str().ok()).unwrap_or_default().to_string()
    }
}
