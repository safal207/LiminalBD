# LiminalDB Benchmark Status

This document separates what is currently measured, what is modelled, and what
still needs to be published for reviewer-grade performance evidence.

## Current state

LiminalDB does **not** yet publish a full live-hardware benchmark report for
the main database runtime.

What the repository does already contain:

- explicit performance targets in `README.md`
- a synthetic scenario harness in `sdk/rust/examples/iot-benchmark.rs`
- a live benchmark runner in `sdk/rust/examples/live-benchmark.rs`
- a modelled use-case writeup in `docs/USE_CASE_IOT_MONITORING.md`
- protocol validation through the `conformance` crate

## Evidence categories

| Evidence type | Status | Where |
|---|---|---|
| Design targets | Available | `README.md` |
| Synthetic scenario harness | Available | `sdk/rust/examples/iot-benchmark.rs` |
| Live benchmark runner | Available | `sdk/rust/examples/live-benchmark.rs` |
| Modelled comparative writeup | Available | `docs/USE_CASE_IOT_MONITORING.md` |
| Protocol conformance | Available | `conformance/` |
| Live benchmark report | Not yet published | runner exists; published numbers still pending |
| Continuous performance regression checks | Not yet published | pending |

## What reviewers can rely on today

Reviewers can reasonably rely on the following statements:

- the repository has explicit performance goals
- the repository includes a synthetic benchmark harness for scenario modelling
- the repository includes a live benchmark runner for a real WebSocket endpoint
- the repository is transparent that current IoT comparison numbers are modelled
- the protocol surface already has a conformance suite

Reviewers should **not** yet treat the current repository as having published
production-grade benchmark results for live deployments.

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

What it measures today:

- live LQL round-trip latency over WebSocket
- batch ingest followed by a `mirror.timeline` probe
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

Until live numbers are published, the cleanest positioning is:

- "performance targets are documented"
- "synthetic and modelled scenarios are available for transparency"
- "live benchmarks are the next evidence milestone"

That framing is materially stronger than presenting modelled figures as if they
were already measured.
