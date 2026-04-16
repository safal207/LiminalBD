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

## Measured baseline (first published sample)

Date: `2026-04-16`  
Commit: `e71cbac`

Environment:

- OS: Windows 11 (`10.0.26200`)
- CPU: AMD Ryzen 7 5700U (8 cores / 16 logical)
- RAM: 16 GB
- Toolchain used for benchmark binaries: `stable-x86_64-pc-windows-msvc`

Commands:

```bash
# Server
target/x86_64-pc-windows-msvc/release/liminal-cli.exe --store ./benchmark-data --ws-port 8787

# Profile A (default)
target/x86_64-pc-windows-msvc/release/examples/live-benchmark.exe

# Profile B (tuned)
target/x86_64-pc-windows-msvc/release/examples/live-benchmark.exe --warmup 100 --query-rounds 100 --batch-rounds 10 --batch-size 1000 --timeline-top 50
```

Results:

### Profile A (default)

- LQL round-trip latency: p50 `0.74 ms`, p95 `0.86 ms`, p99 `0.91 ms`, avg `0.75 ms`
- Ingest batch + LQL probe: p50 `28.96 ms`, p95 `31.57 ms`, p99 `31.57 ms`, avg `30.23 ms`
- Estimated ingest throughput: `~16.5K events/sec`

### Profile B (tuned)

- LQL round-trip latency: p50 `1.00 ms`, p95 `1.31 ms`, p99 `1.44 ms`, avg `1.10 ms`
- Ingest batch + LQL probe: p50 `58.58 ms`, p95 `61.02 ms`, p99 `61.02 ms`, avg `59.00 ms`
- Estimated ingest throughput: `~16.9K events/sec`
- Observed process memory during run (`liminal-cli.exe`): `~22 MB RSS` (Windows task manager sample)

Method notes:

- Runner measures live WebSocket `lql` round-trip latency.
- Batch phase measures impulse ingest followed by a live `lql` probe.
- This is a developer baseline on one machine, not a long soak or multi-node benchmark.

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

- a published benchmark report with hardware and OS metadata
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
