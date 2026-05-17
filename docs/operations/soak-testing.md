# Native Soak Testing

Native soak tests exercise repeated interruption and resume behavior over a
persistent manifest. The current harness simulates reconnects inside the native
transfer crate; it is intended to catch resume math, hashing, and manifest
regressions before longer daemon-level runs.

## Quick Local Run

Run a small deterministic soak:

```bash
NBRG_SOAK_BYTES=1048576 NBRG_SOAK_RECONNECTS=2 bash scripts/native-soak.sh
```

The default script run uses:

- `NBRG_SOAK_BYTES=134217728`
- `NBRG_SOAK_RECONNECTS=10`
- `NBRG_SOAK_SEED=0`
- `NBRG_SOAK_LOG=target/soak/native-soak.log`

## 7-Day Run

Use a bounded shell loop and preserve logs outside git:

```bash
mkdir -p target/soak/evidence
deadline=$((SECONDS + 7 * 24 * 60 * 60))
run=0
while (( SECONDS < deadline )); do
  run=$((run + 1))
  NBRG_SOAK_BYTES=1073741824 \
  NBRG_SOAK_RECONNECTS=50 \
  NBRG_SOAK_SEED="$run" \
  NBRG_SOAK_LOG="target/soak/evidence/native-soak-$run.log" \
    bash scripts/native-soak.sh
done
```

On systems with GNU `timeout`, an equivalent guard is:

```bash
timeout 7d bash -c 'while true; do bash scripts/native-soak.sh; done'
```

## Memory Profiling

Optional heap tracking:

```bash
heaptrack bash scripts/native-soak.sh
```

Optional Valgrind Massif run:

```bash
valgrind --tool=massif --massif-out-file=target/soak/massif.out \
  bash scripts/native-soak.sh
```

## Evidence

Preserve these paths outside git for release review:

- `target/soak/native-soak.log`
- `target/soak/evidence/native-soak-*.log`
- `target/soak/massif.out`
- `heaptrack.*.gz`
- command transcript with OS, CPU, Rust version, commit SHA, and start/end time

## Pass/Fail Criteria

Pass:

- every soak iteration exits 0
- final received file hash matches the source hash
- no manifest corruption or missing-range panic appears in logs
- memory profile does not show unbounded growth across repeated runs

Fail:

- any test exits non-zero
- file hashes diverge
- resume plan reports impossible ranges
- log contains panic, data corruption, or persistent I/O errors
