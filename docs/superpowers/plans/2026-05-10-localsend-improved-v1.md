# LocalSend Improved v1 — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (recommended) or `superpowers:executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the v1 of a Rust headless daemon for LAN/WAN file transfer targeting homelab/server users, bidirectionally compatible with LocalSend v2 and with a native QUIC+TLS1.3+Ed25519 protocol, dual-licensed AGPL-3.0+DCO open base with BSL VIP crates in a separate repo.

**Architecture:** Cargo workspace with an I/O-free `core` crate exposed via traits, a `daemon` binary that wires up listeners (LocalSend v2 over HTTP, native over QUIC) and a local gRPC+HTTP API, and thin surfaces (CLI, TUI, WebUI, Tauri GUI) that consume the API rather than re-implementing protocol logic. The Tauri GUI can optionally embed `libcore` in-process for a standalone "all-in-one" experience on desktop OSes.

**Tech Stack:**
- Language: Rust (stable, edition 2021, MSRV pinned in `rust-toolchain.toml`)
- Async runtime: `tokio` 1.x
- CLI: `clap` v4 (derive)
- Errors: `thiserror` (libs), `anyhow` (binaries)
- Crypto: `ed25519-dalek`, `sha2`, `hkdf`, `rustls` (later), `blake3`
- Network: `quinn` (QUIC, later), `mdns-sd` or `if-watch` + custom (mDNS, later)
- Storage: `rusqlite` (bundled), `serde` + `serde_json`/`toml`
- Tests: built-in `cargo test`, `proptest`, `tempfile`, `assert_cmd`
- Build/CI: GitHub Actions, `cross`/`zigbuild` for cross-compilation
- License compliance: DCO (Probot DCO or `dco-check` GitHub Action) — **no CLA**

**Spec reference:** `docs/superpowers/specs/2026-05-10-localsend-improved-design.md` (commits `f5ba0f9` and `d6cc6e6` on `main`).

---

## Sprint Overview

| Sprint | Spec Milestone | Weeks | Demo Target |
|---|---|---|---|
| **Sprint 0** | M0 — Skeleton | 1-2 | CLI shows/rotates identity; trust store CRUD works; CI green across 4 platforms |
| **Sprint 1** | M1 — LocalSend v2 compat | 3-5 | Receive a file from the official LocalSend Android app onto a Raspberry Pi running our daemon, with no extra config |
| **Sprint 2** | M2 — Native protocol LAN | 6-9 | Two daemons pair via SAS+QR, exchange a large file with resume after one is killed mid-transfer |
| **Sprint 3** | M3 — Local API + TUI + WebUI | 10-12 | TUI and WebUI both drive a transfer end-to-end via the gRPC API; Python SDK demo in 10 lines |
| **Sprint 4** | M4 — Hooks + observability + packaging | 13-14 | A webhook fires on `transfer.completed`; Prometheus shows in-flight bytes/s; `.deb`/`.rpm`/Docker image built in CI |
| **Sprint 5** | M5 — WAN | 15-17 | Two daemons behind separate NATs pair via a self-hosted rendezvous and transfer directly (no relay) |
| **Sprint 6** | M6 — GUI Tauri | 18-20 | Tauri GUI works standalone on macOS *and* connects remotely to a daemon on a NAS, both modes verified |
| **Sprint 7** | M7 — Hardening + 1.0 | 21-22 | 7-day soak test passes clean; interop test matrix green vs latest LocalSend; docs published; audit invite sent |

**Detail level:**
- **Sprint 0**: fully bite-sized below, ready to execute.
- **Sprints 1-7**: outline only (objectives, task list at task granularity, demo criteria). Bite-sized expansion happens at the start of each sprint, informed by what we learned in the previous one. The "Per-Sprint Iteration Protocol" at the bottom defines how.

---

## File Structure (Sprint 0)

```
localsend-improvements/
├── Cargo.toml                          # workspace root
├── rust-toolchain.toml                 # pin stable channel
├── rustfmt.toml                        # formatting config
├── clippy.toml                         # lint config
├── LICENSE                             # AGPL-3.0
├── README.md                           # project intro, status badges
├── CONTRIBUTING.md                     # DCO requirement, dev setup
├── ARCHITECTURE.md                     # pointer to spec, high-level overview
├── .gitignore
├── .editorconfig
├── .github/
│   └── workflows/
│       ├── ci.yml                      # fmt + clippy + test + build matrix
│       └── dco.yml                     # DCO sign-off check on PRs
├── crates/
│   ├── core/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── error.rs                # `CoreError` enum, `Result` alias
│   │       ├── identity/
│   │       │   ├── mod.rs
│   │       │   ├── keypair.rs          # Ed25519 keypair gen/serde
│   │       │   ├── fingerprint.rs      # SHA-256(pubkey) -> "a1b2-c3d4-..."
│   │       │   ├── vault.rs            # `IdentityVault` trait + `FsVault` impl
│   │       │   └── sas.rs              # Short Authentication String (6 digits)
│   │       └── trust/
│   │           ├── mod.rs
│   │           ├── store.rs            # `TrustStore` over SQLite
│   │           └── schema.sql          # initial schema
│   ├── protocol-localsend-v2/          # empty stub crate (M1)
│   ├── protocol-native-v1/             # empty stub crate (M2)
│   ├── daemon/
│   │   ├── Cargo.toml
│   │   └── src/main.rs                 # skeleton daemon: init identity + trust, signal handling, idle loop
│   ├── cli/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── main.rs
│   │       ├── cmd/
│   │       │   ├── mod.rs
│   │       │   ├── identity.rs         # `identity show`, `identity rotate`
│   │       │   └── peers.rs            # `peers list`, `peers trust`, `peers untrust`
│   │       └── config.rs               # locate config dir + identity file per OS
│   ├── tui/                            # empty stub crate (M3)
│   ├── webui/                          # empty stub crate (M3)
│   ├── gui/                            # empty stub crate (M6)
│   ├── rendezvous/                     # empty stub crate (M5)
│   └── proto/                          # empty stub crate (M3)
└── tests/
    └── e2e_identity.rs                 # CLI + daemon both initialize same identity correctly
```

**Responsibility of each Sprint-0 file:**

- `Cargo.toml` (root): workspace member list + shared `[workspace.dependencies]` for version pinning across all crates.
- `crates/core/src/identity/keypair.rs`: generate Ed25519 keypair, serialize/deserialize to/from raw bytes. No filesystem.
- `crates/core/src/identity/fingerprint.rs`: derive human-readable fingerprint from a pubkey. Pure function.
- `crates/core/src/identity/vault.rs`: `IdentityVault` trait (load/save) + `FsVault` filesystem implementation with `0600` perms. The trait lets tests inject in-memory vaults.
- `crates/core/src/identity/sas.rs`: 6-digit Short Authentication String derived from two pubkeys + a nonce. Pure function. Used in Sprint 2's pairing flow.
- `crates/core/src/trust/store.rs`: SQLite-backed trust store. CRUD over `peers` table. Migrations via embedded `.sql`.
- `crates/core/src/error.rs`: single `CoreError` enum with `thiserror`. Variants added as features arrive.
- `crates/daemon/src/main.rs`: minimal daemon — load identity, open trust store, install signal handlers, idle. Listeners come in later sprints.
- `crates/cli/src/main.rs` + `cmd/*`: clap-based CLI. In Sprint 0, CLI reads the same files as the daemon (no IPC yet — that's Sprint 3).

---

## Sprint 0 — Skeleton (Bite-Sized)

**Goal:** Establish the workspace, CI, and the identity + trust-store foundation. Both `daemon` and `cli` binaries initialize and use the same identity and trust store files. Nothing transfers files yet.

**Demo at end of Sprint 0:**
1. `cargo build --workspace --release` succeeds on Linux, macOS, Windows.
2. `<app> identity show` prints a stable fingerprint.
3. `<app> identity rotate` regenerates and prints the new fingerprint.
4. `<app> peers list` returns an empty list on a fresh install.
5. CI is green for fmt + clippy + tests on the build matrix.

> **Naming note:** the binary names are `localsend-improved-daemon` and `localsend-improved` (the CLI) for Sprint 0 — placeholders until the user picks a final name (per spec §13). Each task uses these names verbatim; rename happens in a single follow-up sprint task once the name is decided.

---

### Task 0.1: Workspace bootstrap + license + DCO

**Files:**
- Create: `Cargo.toml`
- Create: `rust-toolchain.toml`
- Create: `rustfmt.toml`
- Create: `clippy.toml`
- Create: `LICENSE`
- Create: `.gitignore`
- Create: `.editorconfig`

- [ ] **Step 1: Write `Cargo.toml` workspace root**

Create `Cargo.toml`:

```toml
[workspace]
resolver = "2"
members = [
    "crates/core",
    "crates/protocol-localsend-v2",
    "crates/protocol-native-v1",
    "crates/daemon",
    "crates/cli",
    "crates/tui",
    "crates/webui",
    "crates/gui",
    "crates/rendezvous",
    "crates/proto",
]

[workspace.package]
version = "0.1.0"
edition = "2021"
license = "AGPL-3.0-only"
repository = "https://github.com/<owner>/localsend-improvements"
rust-version = "1.78"

[workspace.dependencies]
# Async
tokio = { version = "1.38", features = ["full"] }
# CLI
clap = { version = "4.5", features = ["derive"] }
# Errors
thiserror = "1.0"
anyhow = "1.0"
# Crypto
ed25519-dalek = { version = "2.1", features = ["rand_core", "serde"] }
sha2 = "0.10"
hkdf = "0.12"
rand = "0.8"
# Storage
rusqlite = { version = "0.31", features = ["bundled"] }
# Serde
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
toml = "0.8"
# Logging
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "json"] }
# Filesystem helpers
directories = "5.0"
# Tests
tempfile = "3.10"
proptest = "1.4"
assert_cmd = "2.0"
predicates = "3.1"

[profile.release]
lto = "thin"
codegen-units = 1
strip = "symbols"
```

- [ ] **Step 2: Write `rust-toolchain.toml`**

Create `rust-toolchain.toml`:

```toml
[toolchain]
channel = "1.78.0"
components = ["rustfmt", "clippy"]
profile = "minimal"
```

- [ ] **Step 3: Write `rustfmt.toml`**

Create `rustfmt.toml`:

```toml
edition = "2021"
max_width = 100
use_small_heuristics = "Max"
imports_granularity = "Crate"
group_imports = "StdExternalCrate"
reorder_imports = true
```

- [ ] **Step 4: Write `clippy.toml`**

Create `clippy.toml`:

```toml
msrv = "1.78.0"
avoid-breaking-exported-api = false
```

- [ ] **Step 5: Write `LICENSE` (AGPL-3.0)**

Download the verbatim AGPL-3.0-only text and save to `LICENSE`. Source: `https://www.gnu.org/licenses/agpl-3.0.txt`.

Run:

```bash
curl -fsSL https://www.gnu.org/licenses/agpl-3.0.txt -o LICENSE
```

Expected: a `LICENSE` file ~34KB starting with `GNU AFFERO GENERAL PUBLIC LICENSE`.

- [ ] **Step 6: Write `.gitignore`**

Create `.gitignore`:

```
/target
**/*.rs.bk
Cargo.lock.bak
.DS_Store
*.swp
*.swo
*.iml
.idea/
.vscode/
*.profraw
*.profdata
*.gcov
*.gcno
*.gcda
*.log
identity.key
api.token
trust.db
trust.db-journal
trust.db-wal
trust.db-shm
manifests.db*
```

> `Cargo.lock` is **committed** for binaries — see `Cargo.toml` policy below. The `.bak` ignore covers tooling artifacts.

- [ ] **Step 7: Write `.editorconfig`**

Create `.editorconfig`:

```ini
root = true

[*]
charset = utf-8
end_of_line = lf
indent_style = space
indent_size = 4
insert_final_newline = true
trim_trailing_whitespace = true

[*.{toml,yml,yaml,md}]
indent_size = 2

[Makefile]
indent_style = tab
```

- [ ] **Step 8: Verify workspace compiles (will fail; members don't exist yet)**

Run: `cargo check --workspace`
Expected: FAIL with `error: failed to read ... crates/core/Cargo.toml`. This proves the workspace declaration is being read correctly.

- [ ] **Step 9: Commit**

```bash
git add Cargo.toml rust-toolchain.toml rustfmt.toml clippy.toml LICENSE .gitignore .editorconfig
git commit -s -m "build: bootstrap Cargo workspace, license, and toolchain pinning

- Cargo workspace with 10 member slots and centralized version pinning
- Rust 1.78 stable toolchain pin
- AGPL-3.0-only license
- rustfmt and clippy config matching spec conventions

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

> The `-s` flag adds `Signed-off-by:` (DCO). Every commit on this project must be signed off.

---

### Task 0.2: GitHub Actions CI (fmt + clippy + tests + DCO)

**Files:**
- Create: `.github/workflows/ci.yml`
- Create: `.github/workflows/dco.yml`

- [ ] **Step 1: Create CI workflow**

Create `.github/workflows/ci.yml`:

```yaml
name: CI

on:
  push:
    branches: [main]
  pull_request:

env:
  CARGO_TERM_COLOR: always
  RUSTFLAGS: "-D warnings"

jobs:
  fmt:
    name: Format
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt
      - run: cargo fmt --all --check

  clippy:
    name: Clippy
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: clippy
      - uses: Swatinem/rust-cache@v2
      - run: cargo clippy --workspace --all-targets -- -D warnings

  test:
    name: Test (${{ matrix.os }})
    runs-on: ${{ matrix.os }}
    strategy:
      fail-fast: false
      matrix:
        os: [ubuntu-latest, macos-latest, windows-latest]
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - run: cargo test --workspace --all-features

  build:
    name: Build (${{ matrix.target }})
    runs-on: ${{ matrix.runner }}
    strategy:
      fail-fast: false
      matrix:
        include:
          - target: x86_64-unknown-linux-gnu
            runner: ubuntu-latest
          - target: aarch64-unknown-linux-gnu
            runner: ubuntu-latest
            cross: true
          - target: x86_64-apple-darwin
            runner: macos-latest
          - target: aarch64-apple-darwin
            runner: macos-latest
          - target: x86_64-pc-windows-msvc
            runner: windows-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          targets: ${{ matrix.target }}
      - uses: Swatinem/rust-cache@v2
      - if: matrix.cross
        run: cargo install cross --locked
      - if: matrix.cross
        run: cross build --workspace --release --target ${{ matrix.target }}
      - if: '!matrix.cross'
        run: cargo build --workspace --release --target ${{ matrix.target }}
```

- [ ] **Step 2: Create DCO workflow**

Create `.github/workflows/dco.yml`:

```yaml
name: DCO

on:
  pull_request:
    types: [opened, synchronize, reopened]

jobs:
  dco:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0
      - name: Check DCO sign-off on all PR commits
        run: |
          BASE_SHA="${{ github.event.pull_request.base.sha }}"
          HEAD_SHA="${{ github.event.pull_request.head.sha }}"
          MISSING=$(git log --format='%H %s' "$BASE_SHA..$HEAD_SHA" | while read sha _rest; do
            if ! git log -1 --format=%B "$sha" | grep -qE '^Signed-off-by: .+ <.+@.+>$'; then
              echo "$sha"
            fi
          done)
          if [ -n "$MISSING" ]; then
            echo "::error::Commits missing Signed-off-by:"
            echo "$MISSING"
            exit 1
          fi
```

- [ ] **Step 3: Commit**

```bash
git add .github/workflows/ci.yml .github/workflows/dco.yml
git commit -s -m "ci: add fmt, clippy, test, build matrix and DCO checks

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Task 0.3: `core` crate skeleton + error type

**Files:**
- Create: `crates/core/Cargo.toml`
- Create: `crates/core/src/lib.rs`
- Create: `crates/core/src/error.rs`

- [ ] **Step 1: Write `crates/core/Cargo.toml`**

```toml
[package]
name = "lsi-core"
version.workspace = true
edition.workspace = true
license.workspace = true
repository.workspace = true
rust-version.workspace = true
description = "Protocol-agnostic core for LocalSend Improved"

[dependencies]
thiserror.workspace = true
serde.workspace = true
tracing.workspace = true
ed25519-dalek.workspace = true
sha2.workspace = true
hkdf.workspace = true
rand.workspace = true
rusqlite.workspace = true

[dev-dependencies]
tempfile.workspace = true
proptest.workspace = true
```

- [ ] **Step 2: Write `crates/core/src/lib.rs`**

```rust
//! Protocol-agnostic core for LocalSend Improved.
//!
//! Exposes identity, trust, and (later) protocol primitives via traits so the
//! daemon and other surfaces can inject I/O implementations.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod error;
pub mod identity;
pub mod trust;

pub use error::{CoreError, Result};
```

- [ ] **Step 3: Write `crates/core/src/error.rs`**

```rust
//! Core error type.

use thiserror::Error;

/// Errors produced anywhere in `lsi-core`.
#[derive(Debug, Error)]
pub enum CoreError {
    /// I/O failure (filesystem, etc.).
    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    /// SQLite failure.
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),

    /// Identity vault corruption or wrong format.
    #[error("identity vault: {0}")]
    IdentityVault(String),

    /// Trust store error.
    #[error("trust store: {0}")]
    TrustStore(String),

    /// Cryptographic failure (signature, key parsing, etc.).
    #[error("crypto: {0}")]
    Crypto(String),
}

/// Convenience `Result` alias for `lsi-core`.
pub type Result<T> = std::result::Result<T, CoreError>;
```

- [ ] **Step 4: Create empty module stubs**

Create `crates/core/src/identity/mod.rs`:

```rust
//! Cryptographic identity (Ed25519 keypair, fingerprint, persistent vault, SAS).

pub mod fingerprint;
pub mod keypair;
pub mod sas;
pub mod vault;

pub use fingerprint::Fingerprint;
pub use keypair::Keypair;
pub use sas::compute_sas;
pub use vault::{FsVault, IdentityVault};
```

Create `crates/core/src/trust/mod.rs`:

```rust
//! Persistent trust store (SQLite).

pub mod store;

pub use store::{Peer, PeerPolicy, TrustStore};
```

Each referenced submodule file gets created in later tasks. For now, create empty placeholder files to make the module tree compile:

```bash
touch crates/core/src/identity/keypair.rs
touch crates/core/src/identity/fingerprint.rs
touch crates/core/src/identity/vault.rs
touch crates/core/src/identity/sas.rs
touch crates/core/src/trust/store.rs
```

> The `pub use` statements in `mod.rs` will fail to compile until the items exist. To avoid red CI between tasks, temporarily comment out the `pub use` lines and uncomment them as each task lands. Each subsequent task explicitly re-enables its export.

In `crates/core/src/identity/mod.rs`, comment all `pub use` lines for now:

```rust
//! Cryptographic identity (Ed25519 keypair, fingerprint, persistent vault, SAS).

pub mod fingerprint;
pub mod keypair;
pub mod sas;
pub mod vault;

// pub use fingerprint::Fingerprint;
// pub use keypair::Keypair;
// pub use sas::compute_sas;
// pub use vault::{FsVault, IdentityVault};
```

Same for `crates/core/src/trust/mod.rs`:

```rust
//! Persistent trust store (SQLite).

pub mod store;

// pub use store::{Peer, PeerPolicy, TrustStore};
```

- [ ] **Step 5: Verify core crate compiles**

Run: `cargo check -p lsi-core`
Expected: success with warnings about empty files. Warnings are fine.

- [ ] **Step 6: Commit**

```bash
git add crates/core
git commit -s -m "feat(core): add lsi-core crate scaffold with error type

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Task 0.4: Ed25519 keypair (TDD)

**Files:**
- Modify: `crates/core/src/identity/keypair.rs`
- Modify: `crates/core/src/identity/mod.rs` (uncomment `pub use keypair::Keypair;`)

- [ ] **Step 1: Write the failing test inside `keypair.rs`**

Replace contents of `crates/core/src/identity/keypair.rs`:

```rust
//! Ed25519 keypair: generation, serialization, signing.

use ed25519_dalek::{Signature, SigningKey, VerifyingKey, Signer, Verifier};
use rand::rngs::OsRng;

use crate::error::{CoreError, Result};

/// An Ed25519 identity keypair owned by this node.
#[derive(Clone)]
pub struct Keypair {
    signing: SigningKey,
}

impl Keypair {
    /// Generate a fresh keypair from OS randomness.
    pub fn generate() -> Self {
        let signing = SigningKey::generate(&mut OsRng);
        Self { signing }
    }

    /// Reconstruct a keypair from its 32-byte secret key.
    pub fn from_secret_bytes(bytes: &[u8; 32]) -> Self {
        Self { signing: SigningKey::from_bytes(bytes) }
    }

    /// Export the 32-byte secret key. Treat as sensitive.
    pub fn secret_bytes(&self) -> [u8; 32] {
        self.signing.to_bytes()
    }

    /// Public verifying key.
    pub fn public(&self) -> VerifyingKey {
        self.signing.verifying_key()
    }

    /// Raw 32-byte public key bytes.
    pub fn public_bytes(&self) -> [u8; 32] {
        self.signing.verifying_key().to_bytes()
    }

    /// Sign a message.
    pub fn sign(&self, msg: &[u8]) -> Signature {
        self.signing.sign(msg)
    }

    /// Verify a signature against this keypair's public key.
    pub fn verify(&self, msg: &[u8], sig: &Signature) -> Result<()> {
        self.signing
            .verifying_key()
            .verify(msg, sig)
            .map_err(|e| CoreError::Crypto(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_secret_bytes() {
        let kp = Keypair::generate();
        let bytes = kp.secret_bytes();
        let reconstructed = Keypair::from_secret_bytes(&bytes);
        assert_eq!(kp.public_bytes(), reconstructed.public_bytes());
    }

    #[test]
    fn sign_and_verify_roundtrip() {
        let kp = Keypair::generate();
        let msg = b"hello, lan";
        let sig = kp.sign(msg);
        assert!(kp.verify(msg, &sig).is_ok());
    }

    #[test]
    fn verify_rejects_tampered_message() {
        let kp = Keypair::generate();
        let sig = kp.sign(b"original");
        assert!(kp.verify(b"tampered", &sig).is_err());
    }

    #[test]
    fn distinct_generations_produce_distinct_keys() {
        let a = Keypair::generate();
        let b = Keypair::generate();
        assert_ne!(a.public_bytes(), b.public_bytes());
    }
}
```

- [ ] **Step 2: Re-export `Keypair` from `identity/mod.rs`**

Edit `crates/core/src/identity/mod.rs`. Uncomment the `Keypair` line so the file reads:

```rust
//! Cryptographic identity (Ed25519 keypair, fingerprint, persistent vault, SAS).

pub mod fingerprint;
pub mod keypair;
pub mod sas;
pub mod vault;

// pub use fingerprint::Fingerprint;
pub use keypair::Keypair;
// pub use sas::compute_sas;
// pub use vault::{FsVault, IdentityVault};
```

- [ ] **Step 3: Run tests to verify they pass**

Run: `cargo test -p lsi-core identity::keypair`
Expected: 4 tests pass.

- [ ] **Step 4: Commit**

```bash
git add crates/core/src/identity/keypair.rs crates/core/src/identity/mod.rs
git commit -s -m "feat(core): add Ed25519 keypair with sign/verify

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Task 0.5: Fingerprint format (TDD)

**Files:**
- Modify: `crates/core/src/identity/fingerprint.rs`
- Modify: `crates/core/src/identity/mod.rs` (uncomment `pub use fingerprint::Fingerprint;`)

- [ ] **Step 1: Write the implementation with tests**

Replace contents of `crates/core/src/identity/fingerprint.rs`:

```rust
//! Human-readable fingerprint derived from a public key.
//!
//! Format: SHA-256(pubkey) truncated to 64 bits, rendered as
//! four groups of four lowercase hex characters separated by `-`,
//! e.g. `a1b2-c3d4-e5f6-7890`.

use std::fmt;
use std::str::FromStr;

use sha2::{Digest, Sha256};

use crate::error::{CoreError, Result};

/// Stable 64-bit fingerprint of a public key.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Fingerprint([u8; 8]);

impl Fingerprint {
    /// Compute the fingerprint of a 32-byte Ed25519 public key.
    pub fn from_pubkey(pubkey: &[u8; 32]) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(pubkey);
        let digest = hasher.finalize();
        let mut bytes = [0u8; 8];
        bytes.copy_from_slice(&digest[..8]);
        Self(bytes)
    }

    /// Raw 8 bytes.
    pub fn as_bytes(&self) -> &[u8; 8] {
        &self.0
    }
}

impl fmt::Display for Fingerprint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let hex: String = self.0.iter().map(|b| format!("{b:02x}")).collect();
        // hex is 16 chars; split into 4 groups of 4
        write!(f, "{}-{}-{}-{}", &hex[0..4], &hex[4..8], &hex[8..12], &hex[12..16])
    }
}

impl fmt::Debug for Fingerprint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Fingerprint({self})")
    }
}

impl FromStr for Fingerprint {
    type Err = CoreError;

    fn from_str(s: &str) -> Result<Self> {
        let cleaned: String = s.chars().filter(|c| *c != '-').collect();
        if cleaned.len() != 16 {
            return Err(CoreError::Crypto(format!(
                "fingerprint must be 16 hex chars (4 groups of 4), got {}",
                cleaned.len()
            )));
        }
        let mut bytes = [0u8; 8];
        for (i, byte) in bytes.iter_mut().enumerate() {
            let hi = u8::from_str_radix(&cleaned[i * 2..i * 2 + 1], 16)
                .map_err(|e| CoreError::Crypto(e.to_string()))?;
            let lo = u8::from_str_radix(&cleaned[i * 2 + 1..i * 2 + 2], 16)
                .map_err(|e| CoreError::Crypto(e.to_string()))?;
            *byte = (hi << 4) | lo;
        }
        Ok(Self(bytes))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_has_expected_shape() {
        let pubkey = [0u8; 32];
        let fp = Fingerprint::from_pubkey(&pubkey);
        let s = fp.to_string();
        assert_eq!(s.len(), 19);
        assert_eq!(s.chars().filter(|c| *c == '-').count(), 3);
    }

    #[test]
    fn deterministic_from_same_pubkey() {
        let pubkey = [42u8; 32];
        assert_eq!(Fingerprint::from_pubkey(&pubkey), Fingerprint::from_pubkey(&pubkey));
    }

    #[test]
    fn distinct_pubkeys_distinct_fingerprints() {
        let a = Fingerprint::from_pubkey(&[1u8; 32]);
        let b = Fingerprint::from_pubkey(&[2u8; 32]);
        assert_ne!(a, b);
    }

    #[test]
    fn display_parse_roundtrip() {
        let original = Fingerprint::from_pubkey(&[99u8; 32]);
        let parsed: Fingerprint = original.to_string().parse().unwrap();
        assert_eq!(original, parsed);
    }

    #[test]
    fn parse_rejects_wrong_length() {
        let result: Result<Fingerprint> = "abcd-1234".parse();
        assert!(result.is_err());
    }

    #[test]
    fn parse_rejects_non_hex() {
        let result: Result<Fingerprint> = "zzzz-1234-5678-9abc".parse();
        assert!(result.is_err());
    }
}
```

- [ ] **Step 2: Re-export `Fingerprint`**

Edit `crates/core/src/identity/mod.rs`, uncomment the fingerprint line:

```rust
pub use fingerprint::Fingerprint;
pub use keypair::Keypair;
// pub use sas::compute_sas;
// pub use vault::{FsVault, IdentityVault};
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p lsi-core identity::fingerprint`
Expected: 6 tests pass.

- [ ] **Step 4: Commit**

```bash
git add crates/core/src/identity/fingerprint.rs crates/core/src/identity/mod.rs
git commit -s -m "feat(core): add human-readable fingerprint format

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Task 0.6: `IdentityVault` trait + filesystem implementation (TDD)

**Files:**
- Modify: `crates/core/src/identity/vault.rs`
- Modify: `crates/core/src/identity/mod.rs` (uncomment vault re-exports)

- [ ] **Step 1: Write the implementation with tests**

Replace contents of `crates/core/src/identity/vault.rs`:

```rust
//! Persistent storage of the node's keypair.

use std::path::{Path, PathBuf};

use crate::error::{CoreError, Result};
use crate::identity::Keypair;

/// Persists and loads the node's [`Keypair`].
pub trait IdentityVault {
    /// Load the keypair from storage; returns `None` if not present.
    fn load(&self) -> Result<Option<Keypair>>;

    /// Save the keypair, overwriting any prior value.
    fn save(&self, kp: &Keypair) -> Result<()>;

    /// Delete the stored keypair, if any.
    fn delete(&self) -> Result<()>;
}

/// Filesystem-backed [`IdentityVault`].
///
/// Stores the 32-byte secret key at the configured path with `0600` perms on
/// Unix. On Windows, the file inherits the user's ACL.
pub struct FsVault {
    path: PathBuf,
}

impl FsVault {
    /// Create a vault pointed at `path`. The path's parent directory is
    /// created on first save.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// The path this vault reads/writes.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl IdentityVault for FsVault {
    fn load(&self) -> Result<Option<Keypair>> {
        if !self.path.exists() {
            return Ok(None);
        }
        let bytes = std::fs::read(&self.path)?;
        if bytes.len() != 32 {
            return Err(CoreError::IdentityVault(format!(
                "expected 32 bytes, found {}",
                bytes.len()
            )));
        }
        let mut secret = [0u8; 32];
        secret.copy_from_slice(&bytes);
        Ok(Some(Keypair::from_secret_bytes(&secret)))
    }

    fn save(&self, kp: &Keypair) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let tmp = self.path.with_extension("key.tmp");
        std::fs::write(&tmp, kp.secret_bytes())?;
        set_owner_only_perms(&tmp)?;
        std::fs::rename(&tmp, &self.path)?;
        Ok(())
    }

    fn delete(&self) -> Result<()> {
        if self.path.exists() {
            std::fs::remove_file(&self.path)?;
        }
        Ok(())
    }
}

#[cfg(unix)]
fn set_owner_only_perms(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = std::fs::metadata(path)?.permissions();
    perms.set_mode(0o600);
    std::fs::set_permissions(path, perms)?;
    Ok(())
}

#[cfg(not(unix))]
fn set_owner_only_perms(_path: &Path) -> Result<()> {
    // On Windows, file inherits the user's ACL; no explicit chmod equivalent
    // needed for owner-only access in a single-user homedir. ACL hardening
    // can be revisited if a real-world threat emerges.
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn save_then_load_roundtrip() {
        let dir = TempDir::new().unwrap();
        let vault = FsVault::new(dir.path().join("identity.key"));
        let kp = Keypair::generate();
        vault.save(&kp).unwrap();
        let loaded = vault.load().unwrap().unwrap();
        assert_eq!(kp.public_bytes(), loaded.public_bytes());
    }

    #[test]
    fn load_returns_none_when_missing() {
        let dir = TempDir::new().unwrap();
        let vault = FsVault::new(dir.path().join("identity.key"));
        assert!(vault.load().unwrap().is_none());
    }

    #[test]
    fn save_creates_parent_directory() {
        let dir = TempDir::new().unwrap();
        let nested = dir.path().join("a").join("b").join("identity.key");
        let vault = FsVault::new(&nested);
        let kp = Keypair::generate();
        vault.save(&kp).unwrap();
        assert!(nested.exists());
    }

    #[test]
    fn delete_removes_file() {
        let dir = TempDir::new().unwrap();
        let vault = FsVault::new(dir.path().join("identity.key"));
        let kp = Keypair::generate();
        vault.save(&kp).unwrap();
        vault.delete().unwrap();
        assert!(vault.load().unwrap().is_none());
    }

    #[test]
    fn load_rejects_wrong_length_file() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("identity.key");
        std::fs::write(&path, b"too short").unwrap();
        let vault = FsVault::new(&path);
        assert!(vault.load().is_err());
    }

    #[cfg(unix)]
    #[test]
    fn save_sets_mode_0600() {
        use std::os::unix::fs::PermissionsExt;
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("identity.key");
        let vault = FsVault::new(&path);
        vault.save(&Keypair::generate()).unwrap();
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }
}
```

- [ ] **Step 2: Re-export vault types**

Edit `crates/core/src/identity/mod.rs`:

```rust
pub use fingerprint::Fingerprint;
pub use keypair::Keypair;
// pub use sas::compute_sas;
pub use vault::{FsVault, IdentityVault};
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p lsi-core identity::vault`
Expected: 6 tests pass (5 on Windows since the perms test is `#[cfg(unix)]`).

- [ ] **Step 4: Commit**

```bash
git add crates/core/src/identity/vault.rs crates/core/src/identity/mod.rs
git commit -s -m "feat(core): add IdentityVault trait and FsVault implementation

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Task 0.7: Short Authentication String (TDD)

**Files:**
- Modify: `crates/core/src/identity/sas.rs`
- Modify: `crates/core/src/identity/mod.rs`

- [ ] **Step 1: Write the implementation with tests**

Replace contents of `crates/core/src/identity/sas.rs`:

```rust
//! Short Authentication String (SAS) derivation.
//!
//! Two peers each compute SAS from `(pubkey_a, pubkey_b, nonce)` after a
//! canonical ordering of the pubkeys. If both peers display the same 6 digits,
//! they have authenticated each other against MITM during first pairing.

use hkdf::Hkdf;
use sha2::Sha256;

/// Length of the SAS in decimal digits.
pub const SAS_DIGITS: usize = 6;

/// Compute the SAS for a pairing session.
///
/// The pubkeys are sorted lexicographically before hashing so both peers
/// derive the same value regardless of which one initiates.
pub fn compute_sas(pubkey_a: &[u8; 32], pubkey_b: &[u8; 32], nonce: &[u8]) -> String {
    let (lo, hi) = if pubkey_a <= pubkey_b {
        (pubkey_a, pubkey_b)
    } else {
        (pubkey_b, pubkey_a)
    };

    let mut ikm = Vec::with_capacity(64 + nonce.len());
    ikm.extend_from_slice(lo);
    ikm.extend_from_slice(hi);
    ikm.extend_from_slice(nonce);

    let hk = Hkdf::<Sha256>::new(None, &ikm);
    let mut okm = [0u8; 4];
    hk.expand(b"lsi-sas-v1", &mut okm).expect("32 bytes -> 4 bytes is valid");

    let n = u32::from_be_bytes(okm);
    let modulus = 10u32.pow(SAS_DIGITS as u32);
    format!("{:0width$}", n % modulus, width = SAS_DIGITS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_inputs_produce_same_sas() {
        let a = [1u8; 32];
        let b = [2u8; 32];
        let nonce = b"session-1";
        assert_eq!(compute_sas(&a, &b, nonce), compute_sas(&a, &b, nonce));
    }

    #[test]
    fn order_of_pubkeys_does_not_matter() {
        let a = [1u8; 32];
        let b = [2u8; 32];
        let nonce = b"session-1";
        assert_eq!(compute_sas(&a, &b, nonce), compute_sas(&b, &a, nonce));
    }

    #[test]
    fn different_nonces_produce_different_sas() {
        let a = [1u8; 32];
        let b = [2u8; 32];
        // We can't guarantee they differ for any pair (birthday-style collisions
        // exist), but for these two arbitrary nonces with this hash it's safe.
        assert_ne!(compute_sas(&a, &b, b"nonce-1"), compute_sas(&a, &b, b"nonce-2"));
    }

    #[test]
    fn sas_has_expected_length() {
        let sas = compute_sas(&[0u8; 32], &[1u8; 32], b"x");
        assert_eq!(sas.len(), SAS_DIGITS);
        assert!(sas.chars().all(|c| c.is_ascii_digit()));
    }

    #[test]
    fn sas_zero_pads() {
        // We don't control what the HKDF gives us, but if the modulus rolls to
        // a small number it MUST be zero-padded to 6 chars. Search for a nonce
        // that produces a value < 10 to exercise the padding.
        let a = [0u8; 32];
        let b = [0u8; 32];
        for i in 0u32..100_000 {
            let sas = compute_sas(&a, &b, &i.to_be_bytes());
            assert_eq!(sas.len(), SAS_DIGITS, "padding broken for nonce {i}");
        }
    }
}
```

- [ ] **Step 2: Re-export `compute_sas`**

Edit `crates/core/src/identity/mod.rs`:

```rust
pub use fingerprint::Fingerprint;
pub use keypair::Keypair;
pub use sas::compute_sas;
pub use vault::{FsVault, IdentityVault};
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p lsi-core identity::sas`
Expected: 5 tests pass.

- [ ] **Step 4: Commit**

```bash
git add crates/core/src/identity/sas.rs crates/core/src/identity/mod.rs
git commit -s -m "feat(core): add SAS derivation for peer pairing

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Task 0.8: Trust store schema + types (TDD)

**Files:**
- Create: `crates/core/src/trust/schema.sql`
- Modify: `crates/core/src/trust/store.rs`
- Modify: `crates/core/src/trust/mod.rs`

- [ ] **Step 1: Write the SQL schema**

Create `crates/core/src/trust/schema.sql`:

```sql
CREATE TABLE IF NOT EXISTS peers (
    fingerprint  TEXT PRIMARY KEY NOT NULL,
    pubkey       BLOB NOT NULL,
    label        TEXT NOT NULL DEFAULT '',
    trusted_at   INTEGER NOT NULL,
    last_seen    INTEGER,
    policy       TEXT NOT NULL CHECK (policy IN ('auto_accept', 'prompt', 'block'))
                              DEFAULT 'prompt'
);

CREATE INDEX IF NOT EXISTS idx_peers_last_seen ON peers(last_seen);
```

- [ ] **Step 2: Write the store implementation with tests**

Replace contents of `crates/core/src/trust/store.rs`:

```rust
//! SQLite-backed trust store.

use std::path::Path;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection};

use crate::error::{CoreError, Result};
use crate::identity::Fingerprint;

const SCHEMA: &str = include_str!("schema.sql");

/// Policy applied when this peer initiates a transfer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PeerPolicy {
    /// Accept incoming transfers without prompt.
    AutoAccept,
    /// Prompt the user/admin per transfer.
    Prompt,
    /// Refuse all transfers from this peer.
    Block,
}

impl PeerPolicy {
    fn as_str(self) -> &'static str {
        match self {
            Self::AutoAccept => "auto_accept",
            Self::Prompt => "prompt",
            Self::Block => "block",
        }
    }

    fn from_str(s: &str) -> Result<Self> {
        match s {
            "auto_accept" => Ok(Self::AutoAccept),
            "prompt" => Ok(Self::Prompt),
            "block" => Ok(Self::Block),
            other => Err(CoreError::TrustStore(format!("unknown policy {other:?}"))),
        }
    }
}

/// A persisted peer record.
#[derive(Debug, Clone)]
pub struct Peer {
    /// Human-readable fingerprint.
    pub fingerprint: Fingerprint,
    /// Raw 32-byte Ed25519 public key.
    pub pubkey: [u8; 32],
    /// User-assigned label (may be empty).
    pub label: String,
    /// Unix timestamp when first trusted.
    pub trusted_at: i64,
    /// Unix timestamp of most recent sighting; `None` until seen.
    pub last_seen: Option<i64>,
    /// Auto-accept / prompt / block.
    pub policy: PeerPolicy,
}

/// Thread-safe handle to the on-disk trust store.
pub struct TrustStore {
    conn: Mutex<Connection>,
}

impl TrustStore {
    /// Open (or create) the store at `path`.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let conn = Connection::open(path.as_ref())?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.execute_batch(SCHEMA)?;
        Ok(Self { conn: Mutex::new(conn) })
    }

    /// Open an in-memory store (for tests).
    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(SCHEMA)?;
        Ok(Self { conn: Mutex::new(conn) })
    }

    /// Insert or update a peer with the given pubkey and label, setting policy.
    pub fn trust(&self, pubkey: [u8; 32], label: &str, policy: PeerPolicy) -> Result<Peer> {
        let fp = Fingerprint::from_pubkey(&pubkey);
        let now = now_unix();
        let conn = self.conn.lock().expect("trust store mutex poisoned");
        conn.execute(
            "INSERT INTO peers (fingerprint, pubkey, label, trusted_at, policy)
             VALUES (?, ?, ?, ?, ?)
             ON CONFLICT(fingerprint) DO UPDATE SET
               label = excluded.label,
               policy = excluded.policy",
            params![fp.to_string(), pubkey.as_slice(), label, now, policy.as_str()],
        )?;
        drop(conn);
        self.get(&fp)?.ok_or_else(|| {
            CoreError::TrustStore("peer disappeared after insert".into())
        })
    }

    /// Remove a peer by fingerprint. Returns whether anything was removed.
    pub fn untrust(&self, fp: &Fingerprint) -> Result<bool> {
        let conn = self.conn.lock().expect("trust store mutex poisoned");
        let n = conn.execute(
            "DELETE FROM peers WHERE fingerprint = ?",
            params![fp.to_string()],
        )?;
        Ok(n > 0)
    }

    /// Fetch a peer by fingerprint.
    pub fn get(&self, fp: &Fingerprint) -> Result<Option<Peer>> {
        let conn = self.conn.lock().expect("trust store mutex poisoned");
        let mut stmt = conn.prepare(
            "SELECT fingerprint, pubkey, label, trusted_at, last_seen, policy
             FROM peers WHERE fingerprint = ?",
        )?;
        let mut rows = stmt.query(params![fp.to_string()])?;
        match rows.next()? {
            Some(row) => Ok(Some(row_to_peer(row)?)),
            None => Ok(None),
        }
    }

    /// List all peers in insertion order.
    pub fn list(&self) -> Result<Vec<Peer>> {
        let conn = self.conn.lock().expect("trust store mutex poisoned");
        let mut stmt = conn.prepare(
            "SELECT fingerprint, pubkey, label, trusted_at, last_seen, policy
             FROM peers ORDER BY trusted_at ASC",
        )?;
        let rows = stmt.query_map([], |row| Ok(row_to_peer(row)))?;
        rows.into_iter()
            .map(|r| r.map_err(CoreError::from).and_then(|inner| inner))
            .collect()
    }

    /// Update `last_seen` for a peer to now.
    pub fn touch(&self, fp: &Fingerprint) -> Result<()> {
        let conn = self.conn.lock().expect("trust store mutex poisoned");
        conn.execute(
            "UPDATE peers SET last_seen = ? WHERE fingerprint = ?",
            params![now_unix(), fp.to_string()],
        )?;
        Ok(())
    }
}

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn row_to_peer(row: &rusqlite::Row<'_>) -> Result<Peer> {
    let fp_str: String = row.get("fingerprint")?;
    let fp: Fingerprint = fp_str.parse()?;
    let pubkey_vec: Vec<u8> = row.get("pubkey")?;
    if pubkey_vec.len() != 32 {
        return Err(CoreError::TrustStore(format!(
            "pubkey wrong length: {}",
            pubkey_vec.len()
        )));
    }
    let mut pubkey = [0u8; 32];
    pubkey.copy_from_slice(&pubkey_vec);
    let policy_str: String = row.get("policy")?;
    Ok(Peer {
        fingerprint: fp,
        pubkey,
        label: row.get("label")?,
        trusted_at: row.get("trusted_at")?,
        last_seen: row.get("last_seen")?,
        policy: PeerPolicy::from_str(&policy_str)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_pubkey(seed: u8) -> [u8; 32] {
        [seed; 32]
    }

    #[test]
    fn trust_and_get_roundtrip() {
        let store = TrustStore::open_in_memory().unwrap();
        let pk = fixture_pubkey(7);
        let peer = store.trust(pk, "NAS de casa", PeerPolicy::AutoAccept).unwrap();
        assert_eq!(peer.label, "NAS de casa");
        assert_eq!(peer.policy, PeerPolicy::AutoAccept);
        let fetched = store.get(&peer.fingerprint).unwrap().unwrap();
        assert_eq!(fetched.pubkey, pk);
    }

    #[test]
    fn trust_is_idempotent_and_updates_label() {
        let store = TrustStore::open_in_memory().unwrap();
        let pk = fixture_pubkey(8);
        store.trust(pk, "before", PeerPolicy::Prompt).unwrap();
        let updated = store.trust(pk, "after", PeerPolicy::AutoAccept).unwrap();
        assert_eq!(updated.label, "after");
        assert_eq!(updated.policy, PeerPolicy::AutoAccept);
    }

    #[test]
    fn untrust_removes_peer() {
        let store = TrustStore::open_in_memory().unwrap();
        let pk = fixture_pubkey(9);
        let peer = store.trust(pk, "tmp", PeerPolicy::Prompt).unwrap();
        assert!(store.untrust(&peer.fingerprint).unwrap());
        assert!(store.get(&peer.fingerprint).unwrap().is_none());
        assert!(!store.untrust(&peer.fingerprint).unwrap());
    }

    #[test]
    fn list_returns_peers_in_insertion_order() {
        let store = TrustStore::open_in_memory().unwrap();
        let _ = store.trust(fixture_pubkey(1), "a", PeerPolicy::Prompt).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(1100));
        let _ = store.trust(fixture_pubkey(2), "b", PeerPolicy::Prompt).unwrap();
        let list = store.list().unwrap();
        assert_eq!(list.len(), 2);
        assert!(list[0].trusted_at <= list[1].trusted_at);
    }

    #[test]
    fn touch_updates_last_seen() {
        let store = TrustStore::open_in_memory().unwrap();
        let peer = store.trust(fixture_pubkey(3), "x", PeerPolicy::Prompt).unwrap();
        assert!(peer.last_seen.is_none());
        store.touch(&peer.fingerprint).unwrap();
        let updated = store.get(&peer.fingerprint).unwrap().unwrap();
        assert!(updated.last_seen.is_some());
    }

    #[test]
    fn persists_to_disk_across_open() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("trust.db");
        let pk = fixture_pubkey(4);
        {
            let store = TrustStore::open(&path).unwrap();
            store.trust(pk, "kept", PeerPolicy::AutoAccept).unwrap();
        }
        let store2 = TrustStore::open(&path).unwrap();
        assert_eq!(store2.list().unwrap().len(), 1);
    }
}
```

- [ ] **Step 3: Re-export trust types**

Edit `crates/core/src/trust/mod.rs`:

```rust
//! Persistent trust store (SQLite).

pub mod store;

pub use store::{Peer, PeerPolicy, TrustStore};
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p lsi-core trust`
Expected: 6 tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/core/src/trust crates/core/src/trust/mod.rs
git commit -s -m "feat(core): add SQLite trust store with CRUD and touch

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Task 0.9: Per-OS config directory resolver

**Files:**
- Create: `crates/core/src/paths.rs`
- Modify: `crates/core/src/lib.rs` (add `pub mod paths;`)

- [ ] **Step 1: Write the implementation with tests**

Create `crates/core/src/paths.rs`:

```rust
//! Resolve per-OS config / state / inbox directories.
//!
//! Follows XDG on Linux, native dirs on macOS/Windows, and falls back to
//! `/var/lib/<app>` etc. when `$HOME` is unavailable (headless servers).

use std::path::PathBuf;

const APP_DIR_NAME: &str = "localsend-improved";

/// Directory holding `identity.key`, `api.token`, `config.toml`.
pub fn config_dir() -> PathBuf {
    if let Some(d) = directories::ProjectDirs::from("dev", "lsi", APP_DIR_NAME) {
        return d.config_dir().to_path_buf();
    }
    PathBuf::from("/etc").join(APP_DIR_NAME)
}

/// Directory holding `trust.db`, `manifests.db`, runtime state.
pub fn state_dir() -> PathBuf {
    if let Some(d) = directories::ProjectDirs::from("dev", "lsi", APP_DIR_NAME) {
        // ProjectDirs has `data_dir` (Application Support on macOS, AppData on Win,
        // ~/.local/share on Linux). That's the right place for state files.
        return d.data_dir().to_path_buf();
    }
    PathBuf::from("/var/lib").join(APP_DIR_NAME)
}

/// Directory where received files land by default.
pub fn default_inbox() -> PathBuf {
    if let Some(d) = directories::UserDirs::new() {
        if let Some(downloads) = d.download_dir() {
            return downloads.join(APP_DIR_NAME);
        }
    }
    if let Some(p) = directories::ProjectDirs::from("dev", "lsi", APP_DIR_NAME) {
        return p.data_dir().join("inbox");
    }
    PathBuf::from("/var/lib").join(APP_DIR_NAME).join("inbox")
}

/// Path to the identity keypair file.
pub fn identity_file() -> PathBuf {
    config_dir().join("identity.key")
}

/// Path to the trust store database.
pub fn trust_db_file() -> PathBuf {
    state_dir().join("trust.db")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_are_absolute() {
        assert!(config_dir().is_absolute());
        assert!(state_dir().is_absolute());
        assert!(default_inbox().is_absolute());
        assert!(identity_file().is_absolute());
        assert!(trust_db_file().is_absolute());
    }

    #[test]
    fn config_and_state_under_app_namespace() {
        let cfg = config_dir().to_string_lossy().to_lowercase();
        let st = state_dir().to_string_lossy().to_lowercase();
        assert!(cfg.contains("localsend-improved"));
        assert!(st.contains("localsend-improved"));
    }
}
```

- [ ] **Step 2: Add `paths` module to `lib.rs`**

Edit `crates/core/src/lib.rs`:

```rust
//! Protocol-agnostic core for LocalSend Improved.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod error;
pub mod identity;
pub mod paths;
pub mod trust;

pub use error::{CoreError, Result};
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p lsi-core paths`
Expected: 2 tests pass on all platforms.

- [ ] **Step 4: Commit**

```bash
git add crates/core/src/paths.rs crates/core/src/lib.rs
git commit -s -m "feat(core): resolve config/state/inbox paths per OS

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Task 0.10: Stub crates compile

**Files:**
- Create: `crates/protocol-localsend-v2/Cargo.toml` and `src/lib.rs`
- Create: `crates/protocol-native-v1/Cargo.toml` and `src/lib.rs`
- Create: `crates/tui/Cargo.toml` and `src/lib.rs`
- Create: `crates/webui/Cargo.toml` and `src/lib.rs`
- Create: `crates/gui/Cargo.toml` and `src/lib.rs`
- Create: `crates/rendezvous/Cargo.toml` and `src/lib.rs`
- Create: `crates/proto/Cargo.toml` and `src/lib.rs`

- [ ] **Step 1: Create each stub crate**

For each crate listed in `Files`, create a `Cargo.toml` like this (substituting the crate name):

```toml
[package]
name = "lsi-protocol-localsend-v2"
version.workspace = true
edition.workspace = true
license.workspace = true
repository.workspace = true
rust-version.workspace = true
description = "LocalSend v2 protocol listener (stub; implemented in Sprint 1)"

[dependencies]
```

And a `src/lib.rs`:

```rust
//! Stub for `lsi-protocol-localsend-v2`. Implemented in Sprint 1.

#![allow(dead_code)]
```

Names to use:
- `crates/protocol-localsend-v2` → `lsi-protocol-localsend-v2`
- `crates/protocol-native-v1` → `lsi-protocol-native-v1`
- `crates/tui` → `lsi-tui`
- `crates/webui` → `lsi-webui`
- `crates/gui` → `lsi-gui`
- `crates/rendezvous` → `lsi-rendezvous`
- `crates/proto` → `lsi-proto`

- [ ] **Step 2: Verify workspace builds**

Run: `cargo build --workspace`
Expected: all 10 crates compile with warnings only (stubs use `#![allow(dead_code)]`).

- [ ] **Step 3: Commit**

```bash
git add crates/protocol-localsend-v2 crates/protocol-native-v1 crates/tui crates/webui crates/gui crates/rendezvous crates/proto
git commit -s -m "build: add empty crate stubs for later sprints

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Task 0.11: CLI binary — `identity show`, `identity rotate`, `peers list`

**Files:**
- Create: `crates/cli/Cargo.toml`
- Create: `crates/cli/src/main.rs`
- Create: `crates/cli/src/cmd/mod.rs`
- Create: `crates/cli/src/cmd/identity.rs`
- Create: `crates/cli/src/cmd/peers.rs`

- [ ] **Step 1: Write `crates/cli/Cargo.toml`**

```toml
[package]
name = "lsi-cli"
version.workspace = true
edition.workspace = true
license.workspace = true
repository.workspace = true
rust-version.workspace = true
description = "CLI for LocalSend Improved"

[[bin]]
name = "localsend-improved"
path = "src/main.rs"

[dependencies]
lsi-core = { path = "../core" }
clap.workspace = true
anyhow.workspace = true
tracing.workspace = true
tracing-subscriber.workspace = true

[dev-dependencies]
assert_cmd.workspace = true
predicates.workspace = true
tempfile.workspace = true
```

- [ ] **Step 2: Write `crates/cli/src/main.rs`**

```rust
//! CLI entry point.

use anyhow::Result;
use clap::{Parser, Subcommand};

mod cmd;

#[derive(Parser)]
#[command(
    name = "localsend-improved",
    version,
    about = "CLI for LocalSend Improved",
    long_about = None
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Manage this node's identity (Ed25519 keypair + fingerprint).
    #[command(subcommand)]
    Identity(cmd::identity::Cmd),
    /// Manage trusted peers.
    #[command(subcommand)]
    Peers(cmd::peers::Cmd),
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_target(false)
        .init();

    let cli = Cli::parse();
    match cli.command {
        Command::Identity(c) => cmd::identity::run(c),
        Command::Peers(c) => cmd::peers::run(c),
    }
}
```

- [ ] **Step 3: Write `crates/cli/src/cmd/mod.rs`**

```rust
pub mod identity;
pub mod peers;
```

- [ ] **Step 4: Write `crates/cli/src/cmd/identity.rs`**

```rust
//! `localsend-improved identity ...` subcommands.

use anyhow::{Context, Result};
use clap::Subcommand;
use lsi_core::identity::{Fingerprint, FsVault, IdentityVault, Keypair};
use lsi_core::paths;

#[derive(Subcommand)]
pub enum Cmd {
    /// Print this node's fingerprint and public key.
    Show,
    /// Generate a new keypair, replacing the existing one. **Invalidates trust.**
    Rotate {
        /// Skip the confirmation prompt.
        #[arg(long)]
        yes: bool,
    },
}

pub fn run(cmd: Cmd) -> Result<()> {
    let vault = FsVault::new(paths::identity_file());
    match cmd {
        Cmd::Show => show(&vault),
        Cmd::Rotate { yes } => rotate(&vault, yes),
    }
}

fn show(vault: &FsVault) -> Result<()> {
    let kp = vault.load()?.unwrap_or_else(|| {
        let fresh = Keypair::generate();
        // Best-effort save on first run.
        let _ = vault.save(&fresh);
        fresh
    });
    let fp = Fingerprint::from_pubkey(&kp.public_bytes());
    println!("fingerprint: {fp}");
    println!("pubkey-hex:  {}", hex_encode(&kp.public_bytes()));
    println!("path:        {}", vault.path().display());
    Ok(())
}

fn rotate(vault: &FsVault, yes: bool) -> Result<()> {
    if !yes {
        eprintln!(
            "Rotating identity will INVALIDATE all existing trust relationships.\n\
             Re-run with --yes to confirm."
        );
        return Ok(());
    }
    let kp = Keypair::generate();
    vault.save(&kp).context("saving new identity")?;
    let fp = Fingerprint::from_pubkey(&kp.public_bytes());
    println!("new fingerprint: {fp}");
    Ok(())
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
```

- [ ] **Step 5: Write `crates/cli/src/cmd/peers.rs`**

```rust
//! `localsend-improved peers ...` subcommands.

use anyhow::{Context, Result};
use clap::Subcommand;
use lsi_core::paths;
use lsi_core::trust::{PeerPolicy, TrustStore};

#[derive(Subcommand)]
pub enum Cmd {
    /// List all trusted peers.
    List,
}

pub fn run(cmd: Cmd) -> Result<()> {
    let path = paths::trust_db_file();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).context("creating state dir")?;
    }
    let store = TrustStore::open(&path).context("opening trust store")?;
    match cmd {
        Cmd::List => list(&store),
    }
}

fn list(store: &TrustStore) -> Result<()> {
    let peers = store.list()?;
    if peers.is_empty() {
        println!("no trusted peers (run `pair` to add some — Sprint 2)");
        return Ok(());
    }
    println!("{:<22} {:<24} {:<11} {:<19}", "FINGERPRINT", "LABEL", "POLICY", "LAST SEEN");
    for p in peers {
        let policy = match p.policy {
            PeerPolicy::AutoAccept => "auto_accept",
            PeerPolicy::Prompt => "prompt",
            PeerPolicy::Block => "block",
        };
        let last = p
            .last_seen
            .map(|t| format!("{t}"))
            .unwrap_or_else(|| "-".into());
        println!("{:<22} {:<24} {:<11} {:<19}", p.fingerprint, p.label, policy, last);
    }
    Ok(())
}
```

- [ ] **Step 6: Write integration test**

Create `crates/cli/tests/cli_smoke.rs`:

```rust
use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

/// Force `directories::ProjectDirs` onto an isolated location per test
/// using env overrides supported by the `directories` crate on each OS.
fn set_isolated_dirs(cmd: &mut Command, dir: &TempDir) {
    let root = dir.path().to_path_buf();
    cmd.env("XDG_CONFIG_HOME", root.join("config"));
    cmd.env("XDG_DATA_HOME", root.join("data"));
    // macOS / Windows fall back to the user's home; redirect that too.
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
    cmd.args(["identity", "show"])
        .assert()
        .success()
        .stdout(predicate::str::is_match(r"fingerprint: [0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}").unwrap());
}

#[test]
fn identity_show_twice_is_stable() {
    let dir = TempDir::new().unwrap();
    let mut a = Command::cargo_bin("localsend-improved").unwrap();
    set_isolated_dirs(&mut a, &dir);
    let out_a = a.args(["identity", "show"]).output().unwrap();
    let mut b = Command::cargo_bin("localsend-improved").unwrap();
    set_isolated_dirs(&mut b, &dir);
    let out_b = b.args(["identity", "show"]).output().unwrap();
    assert_eq!(out_a.stdout, out_b.stdout);
}

#[test]
fn identity_rotate_without_yes_is_a_noop() {
    let dir = TempDir::new().unwrap();
    let mut show1 = Command::cargo_bin("localsend-improved").unwrap();
    set_isolated_dirs(&mut show1, &dir);
    let before = show1.args(["identity", "show"]).output().unwrap().stdout;

    let mut rotate = Command::cargo_bin("localsend-improved").unwrap();
    set_isolated_dirs(&mut rotate, &dir);
    rotate.args(["identity", "rotate"]).assert().success();

    let mut show2 = Command::cargo_bin("localsend-improved").unwrap();
    set_isolated_dirs(&mut show2, &dir);
    let after = show2.args(["identity", "show"]).output().unwrap().stdout;
    assert_eq!(before, after);
}

#[test]
fn identity_rotate_with_yes_changes_fingerprint() {
    let dir = TempDir::new().unwrap();
    let mut show1 = Command::cargo_bin("localsend-improved").unwrap();
    set_isolated_dirs(&mut show1, &dir);
    let before = show1.args(["identity", "show"]).output().unwrap().stdout;

    let mut rotate = Command::cargo_bin("localsend-improved").unwrap();
    set_isolated_dirs(&mut rotate, &dir);
    rotate.args(["identity", "rotate", "--yes"]).assert().success();

    let mut show2 = Command::cargo_bin("localsend-improved").unwrap();
    set_isolated_dirs(&mut show2, &dir);
    let after = show2.args(["identity", "show"]).output().unwrap().stdout;
    assert_ne!(before, after);
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
```

- [ ] **Step 7: Run CLI tests**

Run: `cargo test -p lsi-cli`
Expected: 5 tests pass.

- [ ] **Step 8: Commit**

```bash
git add crates/cli
git commit -s -m "feat(cli): add identity show/rotate and peers list

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Task 0.12: Daemon binary skeleton

**Files:**
- Create: `crates/daemon/Cargo.toml`
- Create: `crates/daemon/src/main.rs`

- [ ] **Step 1: Write `crates/daemon/Cargo.toml`**

```toml
[package]
name = "lsi-daemon"
version.workspace = true
edition.workspace = true
license.workspace = true
repository.workspace = true
rust-version.workspace = true
description = "Daemon for LocalSend Improved"

[[bin]]
name = "localsend-improved-daemon"
path = "src/main.rs"

[dependencies]
lsi-core = { path = "../core" }
tokio.workspace = true
anyhow.workspace = true
tracing.workspace = true
tracing-subscriber.workspace = true
clap.workspace = true
```

- [ ] **Step 2: Write `crates/daemon/src/main.rs`**

```rust
//! Daemon entry point.
//!
//! In Sprint 0 the daemon only:
//!   1. Loads (or generates) the identity at the per-OS config path.
//!   2. Opens the trust store at the per-OS state path.
//!   3. Installs signal handlers and waits.
//!
//! Listeners (LocalSend v2, native QUIC, mDNS, gRPC) arrive in later sprints.

use anyhow::{Context, Result};
use clap::Parser;
use lsi_core::identity::{Fingerprint, FsVault, IdentityVault, Keypair};
use lsi_core::paths;
use lsi_core::trust::TrustStore;
use tokio::signal::unix::{signal, SignalKind};
use tracing::info;

#[derive(Parser)]
#[command(name = "localsend-improved-daemon", version)]
struct Args {
    /// Override the identity file path.
    #[arg(long)]
    identity: Option<std::path::PathBuf>,
    /// Override the trust DB path.
    #[arg(long)]
    trust_db: Option<std::path::PathBuf>,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .json()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let args = Args::parse();
    let identity_path = args.identity.unwrap_or_else(paths::identity_file);
    let trust_path = args.trust_db.unwrap_or_else(paths::trust_db_file);

    if let Some(parent) = identity_path.parent() {
        std::fs::create_dir_all(parent).context("creating config dir")?;
    }
    if let Some(parent) = trust_path.parent() {
        std::fs::create_dir_all(parent).context("creating state dir")?;
    }

    let vault = FsVault::new(&identity_path);
    let kp = match vault.load()? {
        Some(kp) => kp,
        None => {
            let fresh = Keypair::generate();
            vault.save(&fresh).context("saving fresh identity")?;
            info!("generated new identity at {}", identity_path.display());
            fresh
        }
    };
    let fp = Fingerprint::from_pubkey(&kp.public_bytes());
    info!(%fp, "identity loaded");

    let _store = TrustStore::open(&trust_path).context("opening trust store")?;
    info!(path = %trust_path.display(), "trust store opened");

    info!("daemon idle (Sprint 0 skeleton; listeners arrive in M1)");
    wait_for_shutdown().await?;
    info!("shutdown");
    Ok(())
}

#[cfg(unix)]
async fn wait_for_shutdown() -> Result<()> {
    let mut term = signal(SignalKind::terminate())?;
    let mut intr = signal(SignalKind::interrupt())?;
    tokio::select! {
        _ = term.recv() => info!("SIGTERM"),
        _ = intr.recv() => info!("SIGINT"),
    }
    Ok(())
}

#[cfg(not(unix))]
async fn wait_for_shutdown() -> Result<()> {
    tokio::signal::ctrl_c().await?;
    info!("Ctrl-C");
    Ok(())
}
```

- [ ] **Step 3: Verify daemon builds and starts**

Run: `cargo build -p lsi-daemon`
Expected: success.

Run (interactive smoke test):

```bash
cargo run -p lsi-daemon -- --identity /tmp/lsi-test-id.key --trust-db /tmp/lsi-test-trust.db &
DAEMON_PID=$!
sleep 1
kill -TERM $DAEMON_PID
wait $DAEMON_PID 2>/dev/null || true
```

Expected: JSON log lines on stderr containing `"identity loaded"`, `"trust store opened"`, `"daemon idle"`, `"SIGTERM"`, `"shutdown"`. Cleanup: `rm -f /tmp/lsi-test-id.key /tmp/lsi-test-trust.db`.

- [ ] **Step 4: Commit**

```bash
git add crates/daemon
git commit -s -m "feat(daemon): skeleton with identity bootstrap and signal handling

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Task 0.13: E2E test — CLI and daemon agree on identity

**Files:**
- Create: `crates/cli/tests/cli_daemon_share_identity.rs`

> The E2E lives under `crates/cli/tests/` (and not a workspace-level `tests/`) because Cargo only picks up integration tests inside member crates' `tests/` directories.

- [ ] **Step 1: Write the E2E test**

Create `crates/cli/tests/cli_daemon_share_identity.rs`:

```rust
//! E2E: CLI and daemon, run against the same isolated config/state, see the
//! same identity fingerprint.

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
    fn env(&mut self, key: &str, val: impl AsRef<std::ffi::OsStr>) -> &mut Self;
}
impl EnvLike for assert_cmd::Command {
    fn env(&mut self, key: &str, val: impl AsRef<std::ffi::OsStr>) -> &mut Self {
        assert_cmd::Command::env(self, key, val)
    }
}
impl EnvLike for std::process::Command {
    fn env(&mut self, key: &str, val: impl AsRef<std::ffi::OsStr>) -> &mut Self {
        std::process::Command::env(self, key, val)
    }
}

#[test]
fn cli_and_daemon_see_same_fingerprint() {
    let dir = TempDir::new().unwrap();

    // Step 1: CLI generates the identity.
    let mut cli_show = Command::cargo_bin("localsend-improved").unwrap();
    set_isolated_dirs(&mut cli_show, &dir);
    let cli_output = cli_show.args(["identity", "show"]).output().unwrap();
    assert!(cli_output.status.success());
    let cli_stdout = String::from_utf8(cli_output.stdout).unwrap();
    let cli_fp = parse_fingerprint(&cli_stdout);

    // Step 2: Daemon starts, reads the same identity, logs it.
    let daemon_bin = assert_cmd::cargo::cargo_bin("localsend-improved-daemon");
    let mut child = std::process::Command::new(&daemon_bin);
    set_isolated_dirs(&mut child, &dir);
    let mut child = child
        .env("RUST_LOG", "info")
        .stderr(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("daemon spawn");

    // Read stderr lines (JSON logs go to stderr via tracing fmt::json default).
    use std::io::{BufRead, BufReader};
    let stderr = child.stderr.take().expect("stderr");
    let mut reader = BufReader::new(stderr);
    let mut buf = String::new();
    let mut daemon_fp = None;
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while std::time::Instant::now() < deadline {
        buf.clear();
        if reader.read_line(&mut buf).unwrap_or(0) == 0 {
            std::thread::sleep(Duration::from_millis(50));
            continue;
        }
        if buf.contains("identity loaded") {
            // The log line is JSON; the fp is in a `fp` field.
            if let Some(start) = buf.find("\"fp\":\"") {
                let rest = &buf[start + 6..];
                if let Some(end) = rest.find('"') {
                    daemon_fp = Some(rest[..end].to_string());
                    break;
                }
            }
        }
    }

    let _ = child.kill();
    let _ = child.wait();

    let daemon_fp = daemon_fp.expect("daemon did not log identity within 5s");
    assert_eq!(cli_fp, daemon_fp, "CLI and daemon disagree on fingerprint");
}

fn parse_fingerprint(stdout: &str) -> String {
    for line in stdout.lines() {
        if let Some(rest) = line.strip_prefix("fingerprint: ") {
            return rest.trim().to_string();
        }
    }
    panic!("no fingerprint line in CLI output:\n{stdout}");
}
```

- [ ] **Step 2: Run the E2E**

Run: `cargo test -p lsi-cli --test cli_daemon_share_identity`
Expected: 1 test passes.

- [ ] **Step 3: Commit**

```bash
git add crates/cli/tests/cli_daemon_share_identity.rs
git commit -s -m "test(e2e): verify CLI and daemon share the same identity

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Task 0.14: README, CONTRIBUTING, ARCHITECTURE docs

**Files:**
- Create: `README.md`
- Create: `CONTRIBUTING.md`
- Create: `ARCHITECTURE.md`

- [ ] **Step 1: Write `README.md`**

Create `README.md`:

```markdown
# LocalSend Improved

> Headless-first file transfer for homelab and server use cases.
> Bidirectional compatibility with LocalSend v2 + a native QUIC+TLS1.3+Ed25519 protocol.

**Status:** pre-alpha (Sprint 0 — skeleton). See [the v1 roadmap](docs/superpowers/plans/2026-05-10-localsend-improved-v1.md) for what's coming.

## Why?

LocalSend is great on phones and desktops. It's a poor fit for a NAS, a
Raspberry Pi, or a homelab server — there's no headless mode, no first-class
CLI, no daemon, no API. This project fills that gap while staying compatible
with the LocalSend ecosystem so users can send files between LocalSend on
their phone and this daemon on their server without configuring anything.

## License

- **Base** (this repo): AGPL-3.0-only.
- **VIP crates** (separate repo, when shipped): BSL 1.1.
- All contributions require **DCO sign-off** (`git commit -s`). No CLA is
  required — and by deliberate design, **no CLA exists**, so the base can
  never be relicensed.

See [the design spec](docs/superpowers/specs/2026-05-10-localsend-improved-design.md) for the full architecture.
```

- [ ] **Step 2: Write `CONTRIBUTING.md`**

Create `CONTRIBUTING.md`:

```markdown
# Contributing

Thanks for considering a contribution!

## Sign-Off (DCO)

This project uses the [Developer Certificate of Origin](https://developercertificate.org/).
Every commit must include a `Signed-off-by:` trailer.

The easiest way is to commit with `git commit -s`:

```
git commit -s -m "feat(core): add new thing"
```

This appends `Signed-off-by: Your Name <your.email@example.com>` automatically.

**There is no CLA.** You retain copyright on your contribution. This is
deliberate — without a CLA, the project cannot be relicensed away from
AGPL-3.0 in the future, even by the original maintainers.

## Dev setup

1. Install Rust stable (the version pinned in `rust-toolchain.toml`).
2. `cargo build --workspace`
3. `cargo test --workspace`
4. `cargo clippy --workspace --all-targets -- -D warnings`
5. `cargo fmt --all`

## Commit style

We follow conventional commits (`feat:`, `fix:`, `docs:`, `test:`, `build:`, `ci:`, `refactor:`, `chore:`).

## Where to start

Look at `docs/superpowers/plans/` for the active sprint. Open issues that
match the current sprint are good starter tasks.
```

- [ ] **Step 3: Write `ARCHITECTURE.md`**

Create `ARCHITECTURE.md`:

```markdown
# Architecture

See the full design at [`docs/superpowers/specs/2026-05-10-localsend-improved-design.md`](docs/superpowers/specs/2026-05-10-localsend-improved-design.md).

## TL;DR

- **`crates/core`**: protocol-agnostic library. No direct I/O — everything
  routed through traits so the daemon and tests can inject implementations.
- **`crates/daemon`**: the long-running binary. Wires listeners, storage,
  policy, hooks, and the local API together. Single source of truth at
  runtime.
- **`crates/cli`**, **`crates/tui`**, **`crates/webui`**, **`crates/gui`**:
  thin clients of the daemon's local API. They do not re-implement protocol
  logic.
- **`crates/protocol-localsend-v2`**: bidirectional compatibility with
  LocalSend v2 HTTP protocol. Receives from and sends to vanilla LocalSend.
- **`crates/protocol-native-v1`**: our native QUIC+TLS1.3+Ed25519 protocol
  with persistent identities, resume, and extension negotiation.
- **`crates/rendezvous`**: separate binary; users self-host it for WAN
  discovery between two NATted peers.
```

- [ ] **Step 4: Commit**

```bash
git add README.md CONTRIBUTING.md ARCHITECTURE.md
git commit -s -m "docs: add README, CONTRIBUTING (with DCO), ARCHITECTURE

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

### Task 0.15: Sprint 0 demo verification

- [ ] **Step 1: Verify full workspace builds clean**

Run: `cargo build --workspace --release`
Expected: success, no warnings (we have `-D warnings` in CI).

- [ ] **Step 2: Verify all tests pass**

Run: `cargo test --workspace`
Expected: all tests pass (sums of identity, trust, paths, CLI smoke, E2E).

- [ ] **Step 3: Verify clippy is clean**

Run: `cargo clippy --workspace --all-targets -- -D warnings`
Expected: success.

- [ ] **Step 4: Verify fmt is clean**

Run: `cargo fmt --all --check`
Expected: success.

- [ ] **Step 5: Demo end-to-end (manual)**

```bash
export TEST_HOME=$(mktemp -d)
export XDG_CONFIG_HOME=$TEST_HOME/config
export XDG_DATA_HOME=$TEST_HOME/data
export HOME=$TEST_HOME

cargo run --release -p lsi-cli -- identity show
cargo run --release -p lsi-cli -- identity show   # same fingerprint
cargo run --release -p lsi-cli -- identity rotate --yes
cargo run --release -p lsi-cli -- identity show   # different fingerprint
cargo run --release -p lsi-cli -- peers list      # empty

# In another terminal, run the daemon against the same env vars:
cargo run --release -p lsi-daemon &
sleep 2
kill %1

rm -rf $TEST_HOME
```

Expected: every command succeeds. Daemon JSON logs show the same fingerprint that the CLI just rotated to.

- [ ] **Step 6: Push and verify CI**

```bash
git push origin main
```

Wait for GitHub Actions to complete. Expected: all jobs green (fmt, clippy, test on 3 OSes, build on 5 targets, DCO).

- [ ] **Step 7: Sprint 0 retrospective document**

Create `docs/superpowers/retros/sprint-0.md`:

```markdown
# Sprint 0 Retrospective

**Goal achieved?** _yes/no — fill at end of sprint_

**What went well:**
- ...

**What was harder than expected:**
- ...

**Adjustments for Sprint 1:**
- ...

**Open questions for the spec or plan:**
- ...
```

Fill in honestly during the demo session. This document feeds into the Sprint 1 bite-sized expansion.

- [ ] **Step 8: Commit retrospective placeholder**

```bash
mkdir -p docs/superpowers/retros
# fill in sprint-0.md before committing
git add docs/superpowers/retros/sprint-0.md
git commit -s -m "docs: sprint 0 retrospective

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Sprint 1 — LocalSend v2 Compatibility (Outline)

**Goal:** Become wire-compatible with the official LocalSend v2 protocol so a user with the LocalSend app on their phone can send and receive files to our daemon out of the box.

**Demo at end of Sprint 1:** Install our daemon on a Raspberry Pi (or any Linux host with mDNS). Open the LocalSend Android app on a phone on the same Wi-Fi. The Pi appears in the app's device list. Send a file from the phone to the Pi; it lands in the configured inbox. Send a file from the Pi (via CLI) to the phone; the LocalSend app receives it.

**Task outline** (to be expanded bite-sized at sprint kickoff):

1. **`lsi-protocol-localsend-v2` crate skeleton + types**
   - `DeviceInfo`, `Session`, `FileMeta`, `UploadToken`. Mirror LocalSend's JSON schemas exactly (capture from the LocalSend protocol spec at `https://github.com/localsend/protocol`).
2. **TLS self-signed cert generator**
   - Generate an ECDSA P-256 cert at first start, persist to disk; compute its SHA-256 fingerprint for the mDNS TXT record. Use `rcgen`.
3. **HTTP server with `axum`**
   - Endpoints: `GET /api/localsend/v2/info`, `POST /api/localsend/v2/prepare-upload`, `POST /api/localsend/v2/upload`, `POST /api/localsend/v2/cancel`. TLS via `axum-server` + `rustls`.
4. **Session manager**
   - Track active sessions, file tokens, expected sizes, write progress. Reject duplicate tokens. Expire after configurable timeout.
5. **Inbox writer**
   - Stream uploads to `inbox/incoming/<session>/<file>`, atomic rename on completion. BLAKE3 verify if the client provides a hash.
6. **mDNS announcer**
   - Use `mdns-sd` to advertise `_localsend._tcp` with the exact TXT fields LocalSend expects.
7. **mDNS browser**
   - Discover other LocalSend nodes on the LAN. Surface them through a `Peers::list_lan_candidates` core API.
8. **LocalSend v2 client**
   - Outbound send: pick a peer, POST `prepare-upload`, then stream uploads. Used by CLI `send`.
9. **CLI: `send`, `receive accept|reject`, `peers list-lan`**
   - Wire the new core APIs to the CLI surface.
10. **Daemon wiring**
    - Start listener at boot; integrate with trust store (`auto_accept` for known peers, prompt-via-CLI otherwise).
11. **Interop tests in CI**
    - Download the LocalSend `linux-amd64` AppImage in CI, run it headless against our daemon for a roundtrip. Use `expect`-style scripting if needed, or LocalSend's own CLI mode if available at test time.
12. **Demo script and Raspberry Pi instructions**
    - `docs/demos/sprint-1-pi.md`: step-by-step for the demo.

**Acceptance criteria:**
- Roundtrip with LocalSend Android works manually for small (KB) and large (>1 GB) files.
- Interop CI job green.
- A killed-mid-transfer LocalSend upload is detected and the incomplete file is **not** moved to the inbox (it stays in `inbox/incoming/`).

---

## Sprint 2 — Native Protocol LAN (Outline)

**Goal:** Two of our daemons can pair (SAS+QR), negotiate extensions, and exchange files over our native QUIC protocol with real chunked resume.

**Demo at end of Sprint 2:** Start two daemons on two machines on the same LAN. From CLI on machine A, run `pair`; on machine B, accept and verify SAS. Then `send big.bin` from A to B. Mid-transfer, kill the receiver. Restart it. Run `transfers resume` — the transfer continues from the last verified offset and completes. BLAKE3 of the final file matches the source.

**Task outline:**

1. **`lsi-protocol-native-v1` crate skeleton**
2. **QUIC server with `quinn` + `rustls`** (cert with embedded Ed25519 pubkey extension; validation via trust store)
3. **mDNS advertisement of native service** (`_localsend-improved._udp` with QUIC port + pubkey fingerprint)
4. **Control stream framing** (protobuf-based wire format; define `Hello`, `HelloAck`, `RequestTransfer`, `Accept`, `ChunkAck`, `Cancel`, `Done`)
5. **Pairing flow** (`Hello` for unknown peers triggers SAS computation; CLI displays SAS; user confirms on both sides → trust store updated)
6. **Extension negotiation** (both peers list `extensions: ["resume@1", "blake3@1", "zstd@1"]`; intersection chosen)
7. **Chunked send with BLAKE3** (per-chunk hash; receiver acks chunk offsets; sender drives forward from last ack)
8. **Resume logic** (manifest persisted in `manifests.db`; on resume, sender skips chunks present and verified on receiver)
9. **CLI: `pair`, `send` (native variant), `transfers list-active`, `transfers resume <id>`**
10. **QR code generation/parsing** for pairing (via `qrcodegen` and `image` crates; CLI prints ASCII QR, GUI later renders pixel QR)
11. **Property tests for the chunking + resume math**
12. **Soak test: 1 GB random file with 10 forced reconnects must always finish with matching BLAKE3**

**Acceptance criteria:**
- Pairing succeeds with matching SAS; pairing fails (with clear error) when SAS does not match (test the negative case).
- Resume verified by killing the receiver mid-stream and re-running `transfers resume`.
- Property tests pass with `proptest` configured for at least 256 cases per property.

---

## Sprint 3 — Local API + TUI + WebUI (Outline)

**Goal:** Surface the daemon over gRPC + HTTP+SSE on loopback. Build a TUI (ratatui) and a WebUI (Svelte or Solid, served from the daemon) that drive transfers end-to-end via the API. Autogenerate SDKs for Python, TypeScript, and Go.

**Demo at end of Sprint 3:**
- `lsi-tui` shows live peer list, in-flight transfers, inbox.
- WebUI at `http://localhost:53501` shows the same data and can accept/reject incoming transfers.
- A 10-line Python snippet using the generated SDK lists peers and pushes a file.

**Task outline:**

1. **`lsi-proto` crate** with `.proto` files for `Peers`, `Transfers`, `Inbox`, `Discovery`, `Daemon`, `Events`.
2. **Generate Rust server bindings** with `tonic-build` from the protos.
3. **Implement gRPC services in the daemon** (calling into `core` services).
4. **HTTP+SSE mirror** with `axum`: REST-shaped endpoints, server-sent events for streaming.
5. **Bearer-token auth** on the loopback API (token written to `~/.config/<app>/api.token` with `0600`).
6. **WebUI build pipeline** (Svelte preferred for size; `rust-embed` packs the build output into the daemon binary).
7. **WebUI routes**: dashboard, peers, inbox, settings.
8. **`lsi-tui` crate** with `ratatui` + `crossterm`: live peer list, transfer view (with progress bars), inbox view.
9. **Generated SDKs**: Python (`betterproto`), TypeScript (`ts-proto`), Go (`protoc-gen-go-grpc`). Published as `sdks/{python,typescript,go}` in-tree; release pipeline pushes to PyPI / npm / Go module proxy when tags are cut.
10. **CLI rewrite (subtle)**: CLI now talks to the daemon via gRPC instead of touching files directly. Removes the file-sharing kludge from Sprint 0.
11. **API versioning policy doc** at `docs/api-versioning.md`.
12. **SDK example scripts** in `docs/examples/` for each language.

**Acceptance criteria:**
- The CLI works against a running daemon over loopback gRPC.
- The TUI and WebUI can both accept an incoming LocalSend v2 transfer (from M1).
- A Python script using the generated SDK can list peers and send a file in under 10 lines (verified by an in-tree example).
- Bearer token rejects requests with no/bad token (negative test).

---

## Sprint 4 — Hooks, Observability, Packaging (Outline)

**Goal:** Production-ready operability. Webhooks and exec hooks fire on events. Prometheus metrics expose throughput and error rates. Build pipeline produces `.deb`, `.rpm`, Docker, Homebrew, NixOS module.

**Demo at end of Sprint 4:**
- Set a webhook URL in config; transfer a file; the webhook endpoint receives a signed JSON payload.
- `curl localhost:53502/metrics` shows in-flight bytes, peer count, etc.
- `apt install ./localsend-improved_0.1.0_amd64.deb` works end-to-end on a fresh Ubuntu container.
- `docker run ghcr.io/<owner>/localsend-improved:0.1.0` works.

**Task outline:**

1. **Hook registry in `core`** (trait `HookEmitter`, events typed via `serde`).
2. **Webhook delivery** (HTTP POST with HMAC-SHA256 signature header; retry with jitter for 5xx; abandon after N attempts).
3. **Exec hook** (spawn script with env vars; configurable timeout; on Linux optionally `unshare --net --ipc --pid --mount`).
4. **`config.toml` schema + loader** (validates `[[hooks]]` blocks at startup; clear errors).
5. **Prometheus metrics** via `metrics` + `metrics-exporter-prometheus`. Counters/gauges for transfers, bytes, peers, errors.
6. **Health endpoints** `/healthz`, `/readyz` on the HTTP API mirror.
7. **Structured JSON logs** confirmed to be systemd/Docker friendly (timestamp, level, target, fields).
8. **GitHub release workflow** producing signed builds (cosign for checksums).
9. **`.deb` and `.rpm`** via `cargo-deb` and `cargo-generate-rpm`.
10. **systemd unit** at `packaging/systemd/localsend-improved.service`.
11. **Dockerfile** (multi-stage; `distroless/cc` final stage).
12. **Homebrew tap** repo bootstrap with formula template.
13. **NixOS module** at `packaging/nixos/module.nix`.
14. **SBOM generation** via `cargo-cyclonedx`.

**Acceptance criteria:**
- Webhook delivery verified end-to-end with a tiny test server in CI.
- All packaging artifacts produced by a single `git tag` push.
- Smoke test job in CI installs the `.deb` in an Ubuntu container and runs `<app> identity show`.

---

## Sprint 5 — WAN (Outline)

**Goal:** Two daemons behind separate NATs find each other via a self-hosted rendezvous server and transfer directly via QUIC hole punching. No relay.

**Demo at end of Sprint 5:** Run a rendezvous server on a $5 VPS. Run two daemons on home networks A and B, both configured to use the rendezvous. Pair them via WAN (out-of-band exchange of fingerprints — there's no LAN discovery here). Transfer a file. Confirm via packet capture that the actual bytes flow peer-to-peer (not through the rendezvous).

**Task outline:**

1. **`lsi-rendezvous` binary**: registry mapping `pubkey → endpoint + ICE candidates`. Plain QUIC API, no relay of file bytes.
2. **STUN client** integrated in the daemon to learn its own public endpoint.
3. **ICE-like candidate gathering** (local + STUN-reflexive; no TURN).
4. **Rendezvous protocol** (`Register`, `Lookup`, `Notify` — peer B learns A is asking).
5. **Hole punching driver**: both peers fire QUIC initial packets to each other's candidates simultaneously.
6. **Connectivity fallback policy**: report failure cleanly with a diagnostic ("symmetric NAT detected on side A — relay needed; relay is a VIP feature in this version").
7. **Daemon flag `--rendezvous <url>`** + config-file equivalent.
8. **Rendezvous deployment doc** at `docs/deploy/rendezvous-on-a-vps.md` (systemd unit, firewall rules, expected resource use).
9. **Integration tests** using `netns` (Linux network namespaces) to simulate two NATted peers behind a shared rendezvous.
10. **Operational concern doc**: privacy implications, what the rendezvous can/cannot see.

**Acceptance criteria:**
- Two daemons behind cone NATs (the test setup uses simulated cone NATs) successfully connect.
- Symmetric NAT scenario fails with a clear actionable error.
- Rendezvous resource use under 50 MB RAM with 1000 registered peers (load test in CI).

---

## Sprint 6 — GUI Tauri (Outline)

**Goal:** Ship a desktop GUI that uses the same WebUI frontend (Sprint 3) packaged in Tauri. Two run modes: standalone (`libcore` in-process) and remote (connects to a separate daemon).

**Demo at end of Sprint 6:**
- macOS `.dmg` installs cleanly. Launching the app with no daemon running starts an embedded one; everything works.
- "Connect to remote daemon" mode connects to a daemon on a Raspberry Pi over WAN (or LAN) using the user's pubkey identity.

**Task outline:**

1. **`lsi-gui` crate** with Tauri 2.x.
2. **Tauri command bridge** (`#[tauri::command]` wrappers around `lsi-core` for in-process mode, or around the gRPC client for remote mode).
3. **Mode selector** (config file + first-launch UI).
4. **Bundle the WebUI build** as Tauri's `dist/`.
5. **Code-signing** (macOS notarization; Windows Authenticode).
6. **Auto-update** opt-in (Tauri updater pointed at our GitHub releases).
7. **Mac launchd entry / Windows service** for the standalone embedded daemon (optional, on by default? — decide at sprint kickoff).
8. **Tauri-specific tests** with `webdriver` + `tauri-driver`.

**Acceptance criteria:**
- Standalone mode works on macOS, Windows; Linux ships as `.AppImage`.
- Remote mode connects to a daemon and observes the same state the daemon's WebUI shows.

---

## Sprint 7 — Hardening + 1.0 (Outline)

**Goal:** Stabilize, harden, document, and release 1.0.

**Demo at end of Sprint 7:** Public announcement post, 1.0 release page on GitHub with multi-arch binaries, packaging artifacts, signatures, SBOMs, and a 30-second screencast of the LocalSend ↔ our daemon roundtrip plus a native protocol resume demo.

**Task outline:**

1. **7-day soak test** with two daemons, scripted random transfers, must complete with zero leaks (verified with `heaptrack` or `valgrind massif`).
2. **Interop test matrix**: latest 5 versions of LocalSend (Android + iOS + Desktop) in CI nightly.
3. **Crypto audit invite** sent to a chosen auditor (e.g. Trail of Bits / Cure53 — budget per spec ~$15-25k).
4. **Threat model document** at `docs/security/threat-model.md`.
5. **Operator's guide** at `docs/operators/`.
6. **User docs site** (mdBook or similar, hosted on GitHub Pages).
7. **Migration guide** for LocalSend users wanting to move their workflow.
8. **Release announcement post** drafted.
9. **CHANGELOG.md** for 1.0.
10. **Security policy** (`SECURITY.md`) with disclosure email and PGP key.

**Acceptance criteria:**
- All Sprint 0-6 acceptance criteria still pass on `main`.
- Soak test passes for 7 consecutive days without restarts.
- Interop CI is green against all targeted LocalSend versions.
- Audit kickoff scheduled (audit itself completes post-launch; 1.0 release notes acknowledge "audit in progress").

---

## Per-Sprint Iteration Protocol

For each sprint **after** Sprint 0, follow this procedure:

1. **Read Sprint N retrospective** at `docs/superpowers/retros/sprint-(N-1).md`. Incorporate adjustments.
2. **Expand Sprint N tasks bite-sized**: take the outline above, write a new section in this plan document (or a separate sprint-N plan file at `docs/superpowers/plans/<date>-sprint-N.md`) with the bite-sized step-by-step.
3. **Re-run the spec-document-reviewer subagent** on the expanded sprint plan to catch issues before execution.
4. **Execute** via `superpowers:subagent-driven-development` (recommended) or `superpowers:executing-plans`.
5. **Demo** the sprint's acceptance criteria at the end. Tag the demo commit (`v0.X-sprint-N-demo`).
6. **Write the Sprint N retrospective** at `docs/superpowers/retros/sprint-N.md`. Honest. What surprised us. What adjustments for N+1.
7. **Update the spec** (`docs/superpowers/specs/2026-05-10-localsend-improved-design.md`) if reality diverged. Commit the update with a clear log entry. The spec is a living document; it stays in sync with reality.

The bite-sized expansion is intentionally **late-binding** (only generated at sprint kickoff) so each sprint plan benefits from the learning of the prior one. Writing all 800 steps now would freeze decisions we don't yet know how to make.

---

## Execution Handoff

**Plan complete and saved to** `docs/superpowers/plans/2026-05-10-localsend-improved-v1.md`.

Two execution options:

**1. Subagent-Driven (recommended)** — I dispatch a fresh subagent per task in Sprint 0, review between tasks, fast iteration. After Sprint 0 ships, we expand Sprint 1 bite-sized and repeat.

**2. Inline Execution** — Execute Sprint 0 tasks in this session using `superpowers:executing-plans`, batch execution with checkpoints for review.

**Which approach do you want?**
