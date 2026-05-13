# Sprint 1 LocalSend v2 Compatibility Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use `superpowers:executing-plans` or `superpowers:subagent-driven-development` to implement this plan task-by-task.

**Goal:** Make the daemon wire-compatible with LocalSend Protocol v2.1 for LAN discovery and inbound file receive from the official LocalSend app, with an internal seam for outbound send.

**Architecture:** Implement the compatibility protocol inside `lsi-protocol-localsend-v2` as a small library with DTOs, TLS identity, session management, inbox writing, HTTP handlers, and UDP multicast discovery. Keep daemon wiring thin: the daemon creates the core identity/trust paths, starts the LocalSend listener, and waits for shutdown. Keep CLI changes limited to LAN peer listing and a first outbound-send surface only after discovery and receive are stable.

**Tech Stack:** Rust 1.78, `tokio`, `serde`, `axum`, `axum-server` or `hyper`+`rustls`, `rcgen`, `rustls`, `sha2`, `blake3`, `uuid`, `rand`, `tokio-util`, `reqwest`, `if-addrs` or `socket2`, existing `rusqlite` trust store and `directories` paths.

**Protocol references:**
- Local protocol spec in this repo: `docs/superpowers/specs/2026-05-10-localsend-improved-design.md`.
- Official LocalSend protocol: `https://github.com/localsend/protocol` (observed as "LocalSend Protocol v2.1" on 2026-05-13).
- Required v2 upload endpoints: `GET /api/localsend/v2/info`, `POST /api/localsend/v2/register`, `POST /api/localsend/v2/prepare-upload`, `POST /api/localsend/v2/upload?sessionId=&fileId=&token=`, `POST /api/localsend/v2/cancel?sessionId=`.
- Discovery defaults: UDP multicast address `224.0.0.167`, UDP port `53317`, TCP port `53317`.

---

## Sprint 1 Scope Decision

Sprint 1 is split into two checkpoints:

- **Checkpoint A (must-have demo):** Official LocalSend Android can discover this daemon and send files into the configured inbox. This satisfies the original Sprint 1 demo's first half.
- **Checkpoint B (stretch, but planned):** Our CLI can discover a LocalSend peer and send one or more files to it using the v2 Upload API.

Do not start native protocol, WAN, gRPC, TUI, WebUI, hooks, manifests DB, or resume work in this sprint.

---

## Task 1.1: Dependency and Crate Setup

**Files:**
- Modify: `Cargo.toml`
- Modify: `crates/protocol-localsend-v2/Cargo.toml`
- Modify: `crates/daemon/Cargo.toml`
- Modify: `crates/cli/Cargo.toml`

**Step 1: Add workspace dependencies**

Add exact or MSRV-checked versions to `[workspace.dependencies]`.

```toml
axum = "=0.7.5"
axum-server = { version = "=0.6.0", features = ["tls-rustls"] }
blake3 = "=1.5.1"
futures-util = "=0.3.30"
http = "=1.1.0"
mime_guess = "=2.0.4"
rcgen = "=0.13.1"
reqwest = { version = "=0.12.4", default-features = false, features = ["json", "stream", "rustls-tls"] }
rustls = "=0.23.5"
socket2 = "=0.5.7"
tokio-util = { version = "=0.7.10", features = ["io"] }
uuid = { version = "=1.8.0", features = ["v4", "serde"] }
```

If any dependency requires Rust newer than `1.78.0`, stop and pin the newest compatible version with `cargo update -p <crate> --precise <version>`.

**Step 2: Wire protocol crate dependencies**

In `crates/protocol-localsend-v2/Cargo.toml`, add:

```toml
[dependencies]
lsi-core = { path = "../core" }
anyhow.workspace = true
axum.workspace = true
axum-server.workspace = true
blake3.workspace = true
futures-util.workspace = true
http.workspace = true
mime_guess.workspace = true
rand.workspace = true
rcgen.workspace = true
reqwest.workspace = true
rustls.workspace = true
serde.workspace = true
serde_json.workspace = true
sha2.workspace = true
socket2.workspace = true
thiserror.workspace = true
tokio.workspace = true
tokio-util.workspace = true
tracing.workspace = true
uuid.workspace = true

[dev-dependencies]
tempfile.workspace = true
```

**Step 3: Wire daemon and CLI dependencies**

Add `lsi-protocol-localsend-v2 = { path = "../protocol-localsend-v2" }` to `crates/daemon/Cargo.toml` and `crates/cli/Cargo.toml`.

**Step 4: Verify metadata resolves**

Run: `cargo metadata --format-version 1 --no-deps`

Expected: success.

**Step 5: Commit**

```bash
git add Cargo.toml Cargo.lock crates/protocol-localsend-v2/Cargo.toml crates/daemon/Cargo.toml crates/cli/Cargo.toml
git commit -s -m "build(localsend-v2): add protocol dependencies"
```

---

## Task 1.2: LocalSend v2 DTOs

**Files:**
- Replace: `crates/protocol-localsend-v2/src/lib.rs`
- Create: `crates/protocol-localsend-v2/src/dto.rs`
- Create: `crates/protocol-localsend-v2/src/error.rs`

**Step 1: Write DTO tests first**

Create `dto.rs` with tests for the official JSON shapes:

```rust
#[test]
fn device_info_matches_official_shape() {
    let json = r#"{
      "alias":"NAS",
      "version":"2.0",
      "deviceModel":"Raspberry Pi",
      "deviceType":"server",
      "fingerprint":"abc",
      "port":53317,
      "protocol":"https",
      "download":true
    }"#;
    let info: DeviceInfo = serde_json::from_str(json).unwrap();
    assert_eq!(info.alias, "NAS");
    assert_eq!(info.device_type.as_deref(), Some("server"));
    assert!(info.download);
}

#[test]
fn prepare_upload_request_parses_files_map() {
    let json = r#"{
      "info":{"alias":"Phone","version":"2.0","deviceType":"mobile","fingerprint":"abc","port":53317,"protocol":"https","download":true},
      "files":{"file-1":{"id":"file-1","fileName":"photo.jpg","size":12,"fileType":"image/jpeg","sha256":null,"preview":null,"metadata":null}}
    }"#;
    let request: PrepareUploadRequest = serde_json::from_str(json).unwrap();
    assert_eq!(request.files["file-1"].file_name, "photo.jpg");
}

#[test]
fn prepare_upload_response_serializes_session_and_tokens() {
    let mut files = std::collections::BTreeMap::new();
    files.insert("file-1".to_string(), "token-1".to_string());
    let response = PrepareUploadResponse { session_id: "session".into(), files };
    let json = serde_json::to_string(&response).unwrap();
    assert!(json.contains("\"sessionId\":\"session\""));
}
```

**Step 2: Implement DTOs**

Use `serde(rename_all = "camelCase")` where possible, but explicitly rename `fileName`, `fileType`, `sessionId`, and `deviceType`.

Required public types:

- `DeviceInfo`
- `FileMeta`
- `FileMetadata`
- `PrepareUploadRequest`
- `PrepareUploadResponse`
- `RegisterRequest`
- `RegisterResponse`
- `Protocol` as a string enum or string wrapper accepting `http`/`https`

Unknown `deviceType` values must deserialize as strings rather than failing.

**Step 3: Add error type**

Create `error.rs`:

```rust
#[derive(Debug, thiserror::Error)]
pub enum LocalSendError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("http: {0}")]
    Http(String),
    #[error("session: {0}")]
    Session(String),
    #[error("crypto: {0}")]
    Crypto(String),
}

pub type Result<T> = std::result::Result<T, LocalSendError>;
```

**Step 4: Export modules**

In `lib.rs`:

```rust
#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod dto;
pub mod error;

pub use error::{LocalSendError, Result};
```

**Step 5: Run tests**

Run: `cargo test -p lsi-protocol-localsend-v2 dto`

Expected: DTO tests pass.

**Step 6: Commit**

```bash
git add crates/protocol-localsend-v2/src
git commit -s -m "feat(localsend-v2): add protocol DTOs"
```

---

## Task 1.3: TLS Certificate Vault

**Files:**
- Create: `crates/protocol-localsend-v2/src/tls.rs`
- Modify: `crates/protocol-localsend-v2/src/lib.rs`

**Step 1: Write failing tests**

Tests:

- `generate_cert_has_stable_sha256_fingerprint`
- `save_then_load_roundtrip`
- `load_returns_none_when_missing`

The fingerprint must be lowercase hex SHA-256 of DER certificate bytes. This is the value LocalSend expects for HTTPS mode discovery.

**Step 2: Implement `TlsIdentity`**

Public API:

```rust
pub struct TlsIdentity {
    pub cert_der: Vec<u8>,
    pub key_der: Vec<u8>,
}

impl TlsIdentity {
    pub fn generate(alias: &str) -> Result<Self>;
    pub fn fingerprint_sha256_hex(&self) -> String;
}
```

Use `rcgen` ECDSA P-256 or the closest Rustls-compatible default. Keep the key material inside the protocol crate; do not mix this with `lsi-core` Ed25519 identity.

**Step 3: Implement `TlsIdentityVault`**

Public API:

```rust
pub struct TlsIdentityVault { /* path prefix or cert/key paths */ }

impl TlsIdentityVault {
    pub fn new(config_dir: impl Into<PathBuf>) -> Self;
    pub fn load(&self) -> Result<Option<TlsIdentity>>;
    pub fn save(&self, identity: &TlsIdentity) -> Result<()>;
    pub fn load_or_generate(&self, alias: &str) -> Result<TlsIdentity>;
}
```

Store files as:

- `localsend-v2.cert.der`
- `localsend-v2.key.der`

Set key file permissions to `0600` on Unix.

**Step 4: Export module**

Add `pub mod tls;` to `lib.rs`.

**Step 5: Run tests**

Run: `cargo test -p lsi-protocol-localsend-v2 tls`

Expected: all TLS vault tests pass.

**Step 6: Commit**

```bash
git add crates/protocol-localsend-v2/src/tls.rs crates/protocol-localsend-v2/src/lib.rs
git commit -s -m "feat(localsend-v2): add TLS certificate vault"
```

---

## Task 1.4: Session Manager

**Files:**
- Create: `crates/protocol-localsend-v2/src/session.rs`
- Modify: `crates/protocol-localsend-v2/src/lib.rs`

**Step 1: Write failing tests**

Tests:

- `prepare_creates_session_and_file_tokens`
- `tokens_are_unique_per_file`
- `duplicate_file_id_is_rejected`
- `upload_authorization_requires_matching_session_file_and_token`
- `cancel_removes_session`
- `expired_sessions_are_pruned`

**Step 2: Implement types**

Public API:

```rust
pub struct SessionManager;

pub struct PreparedSession {
    pub session_id: String,
    pub files: BTreeMap<String, String>,
}

pub struct AuthorizedUpload {
    pub session_id: String,
    pub file_id: String,
    pub token: String,
    pub meta: FileMeta,
}
```

Methods:

```rust
impl SessionManager {
    pub fn new(ttl: Duration) -> Self;
    pub fn prepare(&self, request: PrepareUploadRequest) -> Result<PreparedSession>;
    pub fn authorize_upload(&self, session_id: &str, file_id: &str, token: &str) -> Result<AuthorizedUpload>;
    pub fn mark_uploaded(&self, session_id: &str, file_id: &str) -> Result<()>;
    pub fn cancel(&self, session_id: &str) -> Result<bool>;
    pub fn prune_expired(&self) -> usize;
}
```

Use `Arc<Mutex<...>>` or `tokio::sync::RwLock`; keep this crate simple until concurrency pressure is real.

**Step 3: Run tests**

Run: `cargo test -p lsi-protocol-localsend-v2 session`

Expected: all session tests pass.

**Step 4: Commit**

```bash
git add crates/protocol-localsend-v2/src/session.rs crates/protocol-localsend-v2/src/lib.rs
git commit -s -m "feat(localsend-v2): add upload session manager"
```

---

## Task 1.5: Inbox Writer

**Files:**
- Create: `crates/protocol-localsend-v2/src/inbox.rs`
- Modify: `crates/protocol-localsend-v2/src/lib.rs`

**Step 1: Write failing tests**

Tests:

- `completed_upload_moves_from_incoming_to_inbox`
- `cancelled_upload_stays_in_incoming`
- `sha256_mismatch_fails_and_does_not_publish`
- `filename_path_traversal_is_rejected`
- `duplicate_target_filename_gets_suffix`

**Step 2: Implement safe filename handling**

Reject or sanitize:

- absolute paths
- `..`
- path separators
- empty names

Prefer reject for Sprint 1 so behavior is explicit.

**Step 3: Implement `InboxWriter`**

Public API:

```rust
pub struct InboxWriter;

impl InboxWriter {
    pub fn new(inbox_dir: impl Into<PathBuf>) -> Self;
    pub async fn write_upload<S>(&self, upload: &AuthorizedUpload, body: S) -> Result<PathBuf>
    where
        S: futures_util::Stream<Item = std::result::Result<bytes::Bytes, axum::Error>> + Unpin;
}
```

If stream type friction is high, introduce an internal `AsyncRead` API and adapt Axum bodies in Task 1.6.

Write to:

- temporary: `inbox/incoming/<sessionId>/<fileId>/<fileName>`
- completed: `inbox/<fileName>` for Sprint 1

Use atomic rename on the same filesystem after size/hash verification.

**Step 4: Run tests**

Run: `cargo test -p lsi-protocol-localsend-v2 inbox`

Expected: all inbox tests pass.

**Step 5: Commit**

```bash
git add crates/protocol-localsend-v2/src/inbox.rs crates/protocol-localsend-v2/src/lib.rs
git commit -s -m "feat(localsend-v2): add safe inbox writer"
```

---

## Task 1.6: HTTP Upload API

**Files:**
- Create: `crates/protocol-localsend-v2/src/server.rs`
- Modify: `crates/protocol-localsend-v2/src/lib.rs`

**Step 1: Write handler tests**

Use `tower::ServiceExt` if added, or start an ephemeral listener with `tokio::net::TcpListener`.

Tests:

- `info_returns_device_info`
- `register_returns_device_info`
- `prepare_upload_returns_session_and_tokens`
- `upload_with_valid_token_writes_file`
- `upload_with_invalid_token_returns_403`
- `cancel_removes_session`

**Step 2: Define server config**

```rust
pub struct LocalSendServerConfig {
    pub alias: String,
    pub device_model: Option<String>,
    pub device_type: Option<String>,
    pub port: u16,
    pub inbox_dir: PathBuf,
    pub tls_config_dir: PathBuf,
    pub session_ttl: Duration,
}
```

**Step 3: Build Axum router**

Routes:

- `GET /api/localsend/v2/info`
- `POST /api/localsend/v2/register`
- `POST /api/localsend/v2/prepare-upload`
- `POST /api/localsend/v2/upload`
- `POST /api/localsend/v2/cancel`

Return the status codes from the official protocol: `400`, `403`, `409`, `500` at minimum.

**Step 4: Add `LocalSendServer`**

```rust
pub struct LocalSendServer;

impl LocalSendServer {
    pub async fn bind(config: LocalSendServerConfig) -> Result<Self>;
    pub async fn serve_until_shutdown(self, shutdown: impl Future<Output = ()>) -> Result<()>;
}
```

Use HTTPS with the persisted TLS identity. If TLS binding blocks progress, keep HTTP-only behind an explicit `test_only_http` constructor and stop before daemon wiring.

**Step 5: Run tests**

Run: `cargo test -p lsi-protocol-localsend-v2 server`

Expected: handler tests pass.

**Step 6: Commit**

```bash
git add crates/protocol-localsend-v2/src/server.rs crates/protocol-localsend-v2/src/lib.rs
git commit -s -m "feat(localsend-v2): add HTTP upload API"
```

---

## Task 1.7: UDP Multicast Discovery

**Files:**
- Create: `crates/protocol-localsend-v2/src/discovery.rs`
- Modify: `crates/protocol-localsend-v2/src/lib.rs`

**Step 1: Write serialization tests**

Tests:

- `announcement_matches_official_fields`
- `response_sets_announce_false`
- `self_fingerprint_is_ignored_by_browser`

**Step 2: Implement UDP announcement DTO**

Use the same `DeviceInfo` fields plus `announce`.

```rust
pub struct Announcement {
    pub alias: String,
    pub version: String,
    pub device_model: Option<String>,
    pub device_type: Option<String>,
    pub fingerprint: String,
    pub port: u16,
    pub protocol: String,
    pub download: bool,
    pub announce: bool,
}
```

**Step 3: Implement announcer**

Public API:

```rust
pub struct DiscoveryAnnouncer;

impl DiscoveryAnnouncer {
    pub async fn announce_once(info: &DeviceInfo, announce: bool) -> Result<()>;
    pub async fn run(info: DeviceInfo, shutdown: impl Future<Output = ()>) -> Result<()>;
}
```

Send JSON UDP packets to `224.0.0.167:53317`.

**Step 4: Implement browser**

Public API:

```rust
pub struct LanPeer {
    pub info: DeviceInfo,
    pub address: SocketAddr,
}

pub struct DiscoveryBrowser;

impl DiscoveryBrowser {
    pub async fn listen_once(timeout: Duration, own_fingerprint: &str) -> Result<Vec<LanPeer>>;
}
```

**Step 5: Run tests**

Run: `cargo test -p lsi-protocol-localsend-v2 discovery`

Expected: serialization tests pass. Network tests should bind loopback/multicast only if reliable on macOS/Linux CI; otherwise mark them ignored and document manual command.

**Step 6: Commit**

```bash
git add crates/protocol-localsend-v2/src/discovery.rs crates/protocol-localsend-v2/src/lib.rs
git commit -s -m "feat(localsend-v2): add multicast discovery"
```

---

## Task 1.8: Daemon Wiring for Receive

**Files:**
- Modify: `crates/daemon/src/main.rs`
- Modify: `crates/daemon/Cargo.toml`
- Modify: `crates/core/src/paths.rs` only if an explicit inbox path helper is missing

**Step 1: Add daemon CLI flags**

Add:

- `--localsend-port <u16>` default `53317`
- `--alias <String>` default host name fallback to `localsend-improved`
- `--inbox <PathBuf>` default `lsi_core::paths::default_inbox()`
- `--disable-localsend-v2` for debugging

**Step 2: Start server and discovery**

In daemon startup:

1. Load core identity as today.
2. Load LocalSend TLS identity.
3. Start HTTPS server on the configured port.
4. Start UDP announcer with `protocol: "https"`, `download: true`, `deviceType: "server"`.
5. Wait for shutdown and stop both tasks.

**Step 3: Add daemon smoke test**

Either add an integration test under `crates/daemon/tests/localsend_v2_start.rs` or a CLI test that launches daemon with temporary paths and checks:

- `GET /api/localsend/v2/info` responds.
- JSON contains `alias`, `version`, `fingerprint`, `download`.

**Step 4: Run verification**

Run:

```bash
cargo build -p lsi-daemon
cargo test -p lsi-daemon
```

Expected: daemon builds and smoke test passes.

**Step 5: Commit**

```bash
git add crates/daemon crates/core/src/paths.rs
git commit -s -m "feat(daemon): start LocalSend v2 receiver"
```

---

## Task 1.9: CLI LAN Peer Listing

**Files:**
- Modify: `crates/cli/src/cmd/peers.rs`
- Modify: `crates/cli/src/main.rs` only if command nesting changes
- Modify: `crates/cli/Cargo.toml`
- Add/modify: `crates/cli/tests/cli_smoke.rs`

**Step 1: Add command**

Add:

```bash
localsend-improved peers list-lan --timeout-ms 1500
```

**Step 2: Implement output**

For each discovered peer:

```text
ALIAS                 ADDRESS              PROTOCOL  FINGERPRINT
Phone                 192.168.1.20:53317   https     abc...
```

On empty discovery:

```text
no LocalSend peers found
```

**Step 3: Add test for empty result**

Use a tiny timeout and avoid depending on network presence.

Run: `cargo test -p lsi-cli peers_list_lan`

Expected: empty-result test passes.

**Step 4: Commit**

```bash
git add crates/cli
git commit -s -m "feat(cli): list LocalSend LAN peers"
```

---

## Task 1.10: LocalSend v2 Client Upload

**Files:**
- Create: `crates/protocol-localsend-v2/src/client.rs`
- Modify: `crates/protocol-localsend-v2/src/lib.rs`
- Modify: `crates/cli/src/main.rs`
- Create: `crates/cli/src/cmd/send.rs`
- Modify: `crates/cli/src/cmd/mod.rs`

**Step 1: Add client tests**

Use an in-process test server.

Tests:

- `client_posts_prepare_upload`
- `client_uploads_file_with_returned_token`
- `client_rejects_missing_file`
- `client_reports_server_rejection`

**Step 2: Implement client API**

```rust
pub struct LocalSendClient;

impl LocalSendClient {
    pub async fn send_files(peer: LanPeer, paths: Vec<PathBuf>, sender: DeviceInfo) -> Result<()>;
}
```

Use LocalSend Upload API:

1. POST `/api/localsend/v2/prepare-upload`.
2. For every accepted file token, POST binary body to `/api/localsend/v2/upload`.

For HTTPS self-signed peers, use reqwest rustls with certificate validation disabled only for Sprint 1 compat mode, and document that this follows LocalSend's self-signed trust model rather than persistent identity trust.

**Step 3: Add CLI command**

```bash
localsend-improved send --to <fingerprint-or-alias> <path>...
```

For Sprint 1, this may require a peer discovered in the same invocation via `list-lan` cache or direct `--url https://host:53317`.

Prefer adding:

```bash
localsend-improved send --url https://192.168.1.20:53317 ./file.txt
```

then alias/fingerprint resolution can follow later.

**Step 4: Run tests**

Run:

```bash
cargo test -p lsi-protocol-localsend-v2 client
cargo test -p lsi-cli send
```

Expected: client and CLI send tests pass.

**Step 5: Commit**

```bash
git add crates/protocol-localsend-v2/src/client.rs crates/protocol-localsend-v2/src/lib.rs crates/cli
git commit -s -m "feat(localsend-v2): add outbound upload client"
```

---

## Task 1.11: Interop Test Harness

**Files:**
- Create: `tests/interop/localsend_v2_receive.rs` or `crates/protocol-localsend-v2/tests/interop_receive.rs`
- Modify: `.github/workflows/ci.yml`
- Create: `docs/demos/sprint-1-pi.md`

**Step 1: Add local interop test against our own HTTP client**

Before downloading official LocalSend, add a deterministic interop-style test:

1. Start our daemon on a random local port.
2. Use `LocalSendClient` to upload a temp file.
3. Assert final file exists in inbox and incoming file is not published on failure.

Run: `cargo test -p lsi-protocol-localsend-v2 --test interop_receive`

Expected: pass.

**Step 2: Add CI placeholder for official LocalSend**

Add a disabled-by-default or scheduled/manual job:

```yaml
interop-localsend:
  if: github.event_name == 'workflow_dispatch'
```

The job should document the intended steps to download the official Linux artifact. Do not block normal CI until the official artifact can run headless reliably.

**Step 3: Write Pi demo instructions**

Create `docs/demos/sprint-1-pi.md` with:

- firewall ports: TCP/UDP `53317`
- build command
- daemon command
- Android LocalSend steps
- expected inbox path
- troubleshooting for AP isolation and multicast

**Step 4: Commit**

```bash
git add tests crates/protocol-localsend-v2/tests .github/workflows/ci.yml docs/demos/sprint-1-pi.md
git commit -s -m "test(localsend-v2): add interop harness and demo notes"
```

---

## Task 1.12: Sprint 1 Final Verification

**Files:**
- Create: `docs/superpowers/retros/sprint-1.md`

**Step 1: Run full verification**

Run:

```bash
cargo build --workspace --release
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
```

Expected: all commands pass. `cargo fmt` may still print stable-channel warnings until `rustfmt.toml` is cleaned up.

**Step 2: Run local demo**

Run daemon:

```bash
TEST_HOME=$(mktemp -d)
export XDG_CONFIG_HOME=$TEST_HOME/config
export XDG_DATA_HOME=$TEST_HOME/data
export HOME=$TEST_HOME
export RUSTUP_HOME=/Users/chrnx/.rustup
export CARGO_HOME=/Users/chrnx/.cargo
cargo run --release -p lsi-daemon -- --alias "LSI Test" --localsend-port 53317
```

From another terminal or phone:

- Open official LocalSend Android.
- Confirm `LSI Test` appears.
- Send small file.
- Confirm it lands in `~/Downloads/localsend-improved` or the configured `--inbox`.
- Send a large file if available.
- Kill daemon mid-upload and confirm incomplete file stays under `incoming`.

**Step 3: Fill retrospective**

Create `docs/superpowers/retros/sprint-1.md`:

```markdown
# Sprint 1 Retrospective

**Goal achieved?** _yes/no_

**Manual interop evidence:**
- Device:
- LocalSend version:
- OS/network:
- Small file:
- Large file:
- Cancel/kill behavior:

**What went well:**
- ...

**What was harder than expected:**
- ...

**Adjustments for Sprint 2:**
- ...

**Open questions for the spec or plan:**
- ...
```

**Step 4: Commit**

```bash
git add docs/superpowers/retros/sprint-1.md
git commit -s -m "docs: add sprint 1 retrospective"
```

---

## Known Risks Before Implementation

- **MSRV drift:** New network crates may require Rust newer than 1.78. Pin versions before writing protocol code.
- **TLS friction:** LocalSend uses self-signed HTTPS; Rustls/Reqwest validation behavior must be handled deliberately.
- **mDNS ambiguity:** The local v1 design says `_localsend._tcp`, while the official protocol currently documents UDP multicast announcements. Implement UDP multicast first because it is the official Protocol v2.1 path, then add DNS-SD only if official app behavior requires it.
- **Cross-package E2E:** Tests that spawn binaries from other workspace packages must build or locate those binaries robustly.
- **Official LocalSend CI:** Do not make normal CI depend on a GUI/headless artifact until the artifact is proven stable.

