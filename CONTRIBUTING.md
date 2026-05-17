# Contributing

Thanks for considering a contribution.

## Sign-Off (DCO)

This project uses the [Developer Certificate of Origin](https://developercertificate.org/).
Every commit must include a `Signed-off-by:` trailer.

The easiest way is to commit with `git commit -s`:

```bash
git commit -s -m "feat(core): add new thing"
```

This appends `Signed-off-by: Your Name <your.email@example.com>` automatically.

**There is no CLA.** You retain copyright on your contribution. This is
deliberate: without a CLA, the project cannot be relicensed away from
AGPL-3.0 in the future, even by the original maintainers.

## Dev Setup

1. Install Rust stable (the version pinned in `rust-toolchain.toml`).
2. Run `cargo build --workspace`.
3. Run `cargo test --workspace`.
4. Run `cargo clippy --workspace --all-targets -- -D warnings`.
5. Run `cargo fmt --all`.

## Commit Style

We follow conventional commits (`feat:`, `fix:`, `docs:`, `test:`, `build:`,
`ci:`, `refactor:`, `chore:`).

## Where To Start

Look at `specs/superpowers/plans/` for the active sprint. Open issues that
match the current sprint are good starter tasks.
