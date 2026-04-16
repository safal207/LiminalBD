# LiminalDB Benchmark Status

This document separates what is currently measured, what is modelled, and what
still needs to be published for reviewer-grade performance evidence.

## Current state

LiminalDB now has a first live-hardware baseline against `liminal-cli` over
WebSocket, captured as three back-to-back runs on the same host. It is a
developer-machine baseline, not a reviewer-grade benchmark report, and it is
documented as such below.

What the repository contains today:

- explicit performance targets in `README.md`
- a synthetic scenario harness in `sdk/rust/examples/iot-benchmark.rs`
- a live benchmark runner in `sdk/rust/examples/live-benchmark.rs`
- a first live baseline (3 runs + RSS footprint) against `liminal-cli` (see below)
- a modelled use-case writeup in `docs/USE_CASE_IOT_MONITORING.md`
- protocol validation through the `conformance` crate

### First live baseline (3 runs)

Three back-to-back runs of `live-benchmark` against a single `liminal-cli`
instance on the same host. Workload per run: 100 warmup impulses, 100 LQL
round-trips, then 10 batches of 1000 impulses each followed by an LQL drain.

| Metric | Run 1 | Run 2 | Run 3 |
|---|---:|---:|---:|
| LQL round-trip p50 (ms) | 0.32 | 0.40 | 0.42 |
| LQL round-trip p95 (ms) | 0.56 | 0.50 | 0.53 |
| LQL round-trip p99 (ms) | 0.68 | 0.55 | 0.59 |
| LQL round-trip avg (ms) | 0.35 | 0.41 | 0.46 |
| Ingest batch p50 (ms)   | 66.97 | 54.03 | 32.22 |
| Ingest batch p95 (ms)   | 76.46 | 59.60 | 61.91 |
| Ingest batch p99 (ms)   | 76.46 | 59.60 | 61.91 |
| Ingest batch avg (ms)   | 67.82 | 55.49 | 44.30 |
| Est. ingest throughput (events/sec) | 14,746 | 18,022 | 22,573 |

Server RSS footprint, measured from `ps -o rss` of the `liminal-cli` PID:

| Sample point | RSS |
|---|---:|
| Idle baseline (a few seconds after start, before benchmark) | ~1.86 GB |
| After run 1 (10k extra impulses) | ~1.95 GB |
| After run 2 (20k extra impulses)  | ~2.03 GB |
| After run 3 (30k extra impulses)  | ~2.09 GB |

Notes on these numbers:

- Idle RSS is inflated because `liminal-cli` starts host sensors that
  continuously fire synthetic impulses even without benchmark load. This is
  reproducible and should be treated as a property of the default CLI shape,
  not a benchmark artefact.
- Per-run RSS delta for 10,000 additional benchmark impulses is roughly
  80-90 MB in this build. That is a single-host observation, not a soak test.
- Latency improves across runs (batch p50 drops from 66.97 ms to 32.22 ms) and
  throughput rises from ~14.7K to ~22.6K events/sec, which is consistent with
  typical cache warm-up on the same process.

Environment:

- commit: `claude/setup-liminalbd-testing-N2chY` branch
- rustc: `1.94.1`
- OS: `Linux x86_64` (container, 16 cores reported)
- server: `liminal-cli --store /tmp/liminal-bench-data --ws-port 8787`
- client: `live-benchmark --warmup 100 --query-rounds 100 --batch-rounds 10 --batch-size 1000`

This baseline should be treated as a reproducible developer sanity floor, not
a published reviewer-grade benchmark. It still does not cover multi-host,
sustained soak, snapshot/replay timing, or side-by-side comparison.

## Evidence categories

| Evidence type | Status | Where |
|---|---|---|
| Design targets | Available | `README.md` |
| Synthetic scenario harness | Available | `sdk/rust/examples/iot-benchmark.rs` |
| Live benchmark runner | Available | `sdk/rust/examples/live-benchmark.rs` |
| Modelled comparative writeup | Available | `docs/USE_CASE_IOT_MONITORING.md` |
| Protocol conformance | Available | `conformance/` |
| Live benchmark report | First baseline captured | 3-run developer baseline above; reviewer-grade report still pending |
| Server memory footprint under load | First data point captured | RSS growth per run documented above; soak test still pending |
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
  --batch-size 500
```

What it measures today:

- live LQL round-trip latency over WebSocket
- batch ingest followed by an LQL drain probe (`SELECT bench/live WINDOW 1000`)
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
