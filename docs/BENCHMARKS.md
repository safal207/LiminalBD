# LiminalDB Benchmark Status

This document separates measured data from modelled data and lists what is
still pending for reviewer-grade benchmarking.

## Current state

LiminalDB now publishes a first live benchmark baseline for the single-node
WebSocket runtime path.

What the repository currently contains:

- explicit performance targets in `README.md`
- a synthetic scenario harness in `sdk/rust/examples/iot-benchmark.rs`
- a live benchmark runner in `sdk/rust/examples/live-benchmark.rs`
- a first measured local baseline (published below)
- a modelled use-case writeup in `docs/USE_CASE_IOT_MONITORING.md`
- protocol validation through the `conformance` crate

## Evidence categories

| Evidence type | Status | Where |
|---|---|---|
| Design targets | Available | `README.md` |
| Synthetic scenario harness | Available | `sdk/rust/examples/iot-benchmark.rs` |
| Live benchmark runner | Available | `sdk/rust/examples/live-benchmark.rs` |
| First live benchmark baseline | Available | this document (`Measured baseline`) |
| Modelled comparative writeup | Available | `docs/USE_CASE_IOT_MONITORING.md` |
| Protocol conformance | Available | `conformance/` |
| Continuous performance regression checks | Not yet published | pending |

## Measured baseline (latest verified sample)

Date: `2026-04-17`  
Commit: `ad7dd5cfe91b389f9900e454972631621fc1a7be`

Environment:

- OS: Windows 10 Home (`2009`)
- CPU: AMD Ryzen 7 5700U with Radeon Graphics
- RAM: 16 GB
- Rust toolchain: `rustc 1.93.0 (254b59607 2026-01-19)`
- Toolchain used for benchmark binaries: `stable-x86_64-pc-windows-msvc`

Commands:

```bash
# Server
target/release/liminal-cli.exe --store .\benchmark-data --ws-port 8787

# Verified profile
cargo run --release -p liminaldb-client --example live-benchmark -- --url ws://127.0.0.1:8787 --warmup 50 --query-rounds 25 --batch-rounds 5 --batch-size 500 --timeline-top 20
```

## Reproduce in 3 commands

```bash
# 1) Build benchmark binaries
cargo +stable-x86_64-pc-windows-msvc build --release -p liminal-cli -p liminaldb-client --example live-benchmark --target x86_64-pc-windows-msvc

# 2) Start server (keep this terminal open)
target/x86_64-pc-windows-msvc/release/liminal-cli.exe --store .\benchmark-data --ws-port 8787

# 3) Run measured profile
target/x86_64-pc-windows-msvc/release/examples/live-benchmark.exe --url ws://127.0.0.1:8787 --warmup 50 --query-rounds 25 --batch-rounds 5 --batch-size 500 --timeline-top 20
```

Expected output markers from step 3:

- `Phase 1: live LQL round-trip`
- `Phase 2: ingest batch + LQL probe`
- `est ingest ... events/sec`

Results:

### Verified profile

- LQL round-trip latency: p50 `0.87 ms`, p95 `1.00 ms`, p99 `1.60 ms`, avg `0.95 ms`
- Ingest batch + LQL probe: p50 `30.88 ms`, p95 `32.68 ms`, p99 `32.68 ms`, avg `32.59 ms`
- Estimated ingest throughput: `~15.3K events/sec`

Method notes:

- Runner measures live WebSocket `lql` round-trip latency.
- Batch phase measures impulse ingest followed by a live `lql` probe.
- This is a developer baseline on one machine, not a long soak or multi-node benchmark.
- The current live runner was verified against the server's actual `cmd`/`ev` WebSocket protocol path.

## Public validation refresh (2026-04-17)

Profile rerun:

- `--warmup 50 --query-rounds 25 --batch-rounds 5 --batch-size 500 --timeline-top 20`

Delta vs previous published sample in this repo:

- LQL p50: `18.15 ms` -> `0.87 ms` (improved by ~`95.2%`)
- Batch avg: `97.63 ms` -> `32.59 ms` (improved by ~`66.6%`)
- Estimated ingest throughput: `~5.1K/sec` -> `~15.3K/sec` (~`3.0x`)

Caveats:

- These numbers are from a single local machine and are sensitive to local load.
- Refresh runs are useful for trend checking, not for broad hardware claims.
- Production-grade evidence still requires soak, replay, and multi-node packs.

## What reviewers can rely on now

Reviewers can reasonably rely on the following statements:

- the repository has explicit performance goals
- the repository includes a synthetic benchmark harness for scenario modelling
- the repository includes a live benchmark runner for a real WebSocket endpoint
- the repository now includes a first measured live baseline with reproducible commands
- the repository is transparent that current IoT comparison numbers are modelled
- the protocol surface already has a conformance suite

Reviewers should **not** yet treat this as a full production-grade benchmark
package for live deployments.

## Measured vs pending evidence

| Item | Status | Notes |
|---|---|---|
| Single-node live LQL latency | Measured | Published in this file |
| Single-node ingest + LQL probe throughput | Measured | Published in this file |
| Soak / long-duration stability | Pending | Not yet published |
| Snapshot + replay timing package | Pending | Planned expansion |
| Multi-node/consensus performance | Pending | Planned expansion |
| CI performance regression gate | Pending | Nightly smoke planned |

## Run the live benchmark runner

Start a real LiminalDB instance first:

```bash
cargo build --release -p liminal-cli
./target/release/liminal-cli --store ./data --ws-port 8787
```

Then run the live benchmark:

```bash
cargo run -p liminaldb-client --example live-benchmark --release
```

Optional:

```bash
cargo run -p liminaldb-client --example live-benchmark --release -- \
  --url ws://127.0.0.1:8787 \
  --warmup 50 \
  --query-rounds 25 \
  --batch-rounds 5 \
  --batch-size 500 \
  --timeline-top 20
```

What it measures:

- live LQL round-trip latency over WebSocket
- batch ingest followed by a live `lql` probe
- estimated ingest throughput for that benchmark shape

What it does **not** yet replace:

- a broader benchmark package with soak, replay, and multi-node evidence
- long-duration soak measurements
- multi-node or Raft/distribution measurements

## Run the current synthetic harness

```bash
cargo run -p liminaldb-client --example iot-benchmark --release
```

This is useful for scenario communication, but it is not a substitute for a
live benchmark against a running instance.

## Minimum useful live benchmark package

The next benchmark package should publish, at minimum:

1. hardware and OS details
2. dataset or workload description
3. command lines used to start the server
4. command lines used to drive load
5. p50, p95, and p99 latency
6. throughput over sustained load
7. memory footprint under load
8. replay or recovery timing for Mirror Timeline / snapshot paths

## Suggested first benchmark scenarios

### Scenario 1: Cell routing latency

- workload: `10K` active cells
- measure: p50 / p95 / p99 for query-style impulses
- goal: validate the `<5 ms p99` target or revise it publicly

### Scenario 2: Impulse throughput

- workload: sustained write-heavy stream
- measure: events per second and memory growth
- goal: validate or revise the `>10K/sec` target

### Scenario 3: Snapshot and replay

- workload: write until snapshot threshold, then restart and replay
- measure: snapshot wall time and recovery duration
- goal: produce concrete evidence for auditability claims

### Scenario 4: Protocol surface overhead

- workload: WebSocket client ingest via official SDK
- measure: end-to-end latency including bridge path
- goal: show interoperability costs in real numbers

## Reviewer-facing recommendation

Current safe positioning:

- "performance targets are documented"
- "synthetic and modelled scenarios are available for transparency"
- "a first measured live baseline is published with reproducible commands"
- "reviewer-grade expansion (soak, replay, multi-node, CI perf gates) is pending"
