# LiminalDB v0.2

LiminalDB is a next-generation database: reactive, adaptive, and explainable.

LiminalDB — база данных нового поколения: реактивная, адаптивная и объяснимая.

## Workspace crates

- `liminal-core` — cell evolution engine and life loop.
- `liminal-sensor` — host sensors and impulse ingestion.
- `liminal-cli` — interactive CLI and CBOR pipe mode.
- `liminal-bridge-abi` — universal bridge (C ABI, WASM, CBOR protocol).

## Requirements

1. Install [Rust](https://www.rust-lang.org/tools/install) (Rustup), version `1.79+`.
2. Open this directory:

```bash
cd liminal-db
```

## Build and test

```bash
cargo build --workspace
cargo test -p liminal-bridge-abi
cargo run -p liminal-cli -- --pipe-cbor
```

The CBOR mode reads one hex-encoded packet per line from stdin and prints event/metrics responses as hex to stdout.

## Interactive CLI

```bash
cargo run -p liminal-cli
```

After startup, metrics and hints are printed periodically.

Example impulses:

```text
a cpu/load 0.9
q color/red 0.7
w memory/seed 0.5
```

- `q` — Query
- `w` — Write
- `a` — Affect

Patterns are tokenized; matching cells react, may divide, and may transition to sleep.

Stop with `Ctrl+C`.

[![LiminalDB CI](https://github.com/safal207/LiminalBD/actions/workflows/ci.yml/badge.svg)](https://github.com/safal207/LiminalBD/actions/workflows/ci.yml)

## Learn more

- Repository quickstart: [`../docs/QUICKSTART_5_MIN.md`](../docs/QUICKSTART_5_MIN.md)
- North Star scenario: [`../docs/USE_CASE_NORTH_STAR.md`](../docs/USE_CASE_NORTH_STAR.md)
- Protocol reference: [`../docs/PROTOCOL.md`](../docs/PROTOCOL.md)
