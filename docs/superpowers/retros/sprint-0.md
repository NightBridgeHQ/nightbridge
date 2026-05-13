# Sprint 0 Retrospective

**Goal achieved?** yes.

**What went well:**
- The workspace skeleton, core identity primitives, trust store, CLI, daemon, and E2E identity check are in place.
- The daemon and CLI now share the same per-OS identity and trust-store locations.
- The final local demo verified stable identity reads, identity rotation, empty peer listing, and daemon startup against the rotated fingerprint.

**What was harder than expected:**
- The original task order made `cargo check -p lsi-core` fail until all workspace member manifests existed.
- Several semver ranges selected newer transitive versions that no longer support the pinned Rust 1.78 toolchain.
- `rustfmt.toml` contains stable-rust warnings for `imports_granularity` and `group_imports`; formatting still exits successfully.

**Adjustments for Sprint 1:**
- Keep dependency versions pinned tightly when the project claims an MSRV.
- Create compileable workspace member stubs before adding targeted crate checks.
- Prefer sequential Cargo test/build commands when refreshing dependencies to avoid package-cache lock noise.

**Open questions for the spec or plan:**
- Decide whether to keep Rust 1.78 as MSRV or raise it before protocol-heavy Sprint 1 dependencies are added.
- Decide whether to remove unstable rustfmt options or accept the stable-channel warnings.
- Replace the placeholder GitHub repository URL once the real remote is chosen.
