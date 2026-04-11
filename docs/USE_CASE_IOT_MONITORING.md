# Use Case: Adaptive IoT Monitoring Pipeline

> **Claim classification used in this document**
>
> | Label | Meaning |
> |-------|---------|
> | ✅ implemented | Feature exists in the codebase today |
> | 🚧 in progress | Under active development |
> | 📋 planned | On roadmap, not yet started |
> | *[modelled]* | Derived from architectural analysis; not measured on live hardware |
> | *[illustrative]* | Order-of-magnitude estimate for planning; your numbers will differ |
>
> Real end-to-end benchmarks against a running LiminalDB instance are **planned
> for Q2 2025**. Contributions welcome — see [CONTRIBUTING.md](../CONTRIBUTING.md).

---

## Problem Statement

Traditional IoT monitoring stacks face four compounding challenges:

**1. Static thresholds**
Alert rules don't adapt to changing workloads. A configuration tuned for daytime
load (100K sensors, 1 reading/min) breaks under night-shift bursts (1M sensors,
irregular spikes). Each shift requires manual re-tuning.

**2. Resource inefficiency**
Systems are provisioned for peak capacity that is rarely needed. Under sudden
bursts, graceful degradation is rarely built in — the system either
over-provisions (expensive) or drops events (SLA breach).

**3. Accidental complexity**
A typical production stack assembles five or more tools:
time-series DB (InfluxDB / Prometheus), message queue (Kafka / RabbitMQ),
aggregation layer (custom code), alerting system (PagerDuty), and ops tooling.
Each adds failure modes and on-call burden.

**4. Observability gap**
When the system drops events or degrades, operators cannot see *why*.
Every component is a black box. Root-cause analysis requires cross-correlating
logs from five separate systems.

---

## LiminalDB Approach

A single self-adaptive layer replaces the multi-tool stack:

```
IoT Devices → WebSocket / CLI → LiminalDB → Alerts / Dashboards
                                     |
                          [✅ implemented]
                          Adaptive tick rate (TRS/PID)
                          Pattern-based cell routing
                          Full decision event log (Mirror)
```

### How sensor data flows through the system ✅

**Step 1 — Sensor reading as Impulse**
```
Impulse {
  kind:     Write
  pattern:  "sensor/temperature/warehouse-3"
  strength: 0.7          // signal importance
  data:     b"24.5"      // raw payload (bytes)
}
```

**Step 2 — Cells specialise by pattern** ✅
```
Cell[1] → "sensor/temperature/*"   affinity=0.85
Cell[2] → "sensor/humidity/*"      affinity=0.72
Cell[3] → "sensor/pressure/*"      affinity=0.61
Cell[N] → "alerts/critical/*"      affinity=0.95
```
Affinity drifts over time toward patterns the cell processes most often
(genetic drift). No manual configuration.

**Step 3 — Adaptive control under load** ✅
- Spike detected → TRS (PID controller) tightens tick rate
- Metabolism scales → cells process faster at higher energy cost
- Sleep thresholds adjust → low-salience cells go dormant, freeing capacity
- Model projects this keeps p99 latency bounded under burst; **validation pending**

**Step 4 — Complete decision traceability** ✅
Every significant adaptation is recorded as an epoch in the Mirror log:

```json
{
  "epoch": 1024,
  "timestamp_ms": 1712134412098,
  "event": "cell_specialization_drift",
  "cell_id": "sensor/temp-1",
  "old_affinity": 0.82,
  "new_affinity": 0.87,
  "reason": "increased_match_frequency"
}
```
*(Schema is real; field values above are illustrative)*

---

## Benchmark Results *[modelled]*

> All LiminalDB figures below are **modelled estimates** based on:
> - Architectural analysis of the routing and cell-management subsystems
> - PID control-theory projections for adaptive load scenarios
>
> Redis Streams and Kafka reference numbers are sourced from their respective
> public benchmarks at similar hardware / throughput levels.
> They are not head-to-head measurements on the same hardware.
>
> Run the synthetic scenario harness locally:
> ```
> cargo run -p liminaldb-client --example iot-benchmark --release
> ```
> Real measurements require a running `liminal-cli` instance — see Getting Started.

### Test configuration

**Reference hardware:** 4-core VM, 8 GB RAM (basis for modelled projections)

| Scenario | Description |
|----------|-------------|
| S1 — Steady | 100 K sensors, 1 reading/min ≈ 1.67 K impulses/sec |
| S2 — Spike | 500 K sensors, sustained ≈ 41 K impulses/sec |
| S3 — Burst | 1 M sensors, irregular peaks up to 50 K impulses/sec |

### Latency p99 *[modelled vs. public reference]*

| Scenario | LiminalDB *[modelled]* | Redis Streams | Kafka |
|----------|------------------------|---------------|-------|
| S1 1.67 K/sec | 2.3 ms | 1.8 ms | 3.2 ms |
| S2 41 K/sec | 8.7 ms | 24 ms | 18 ms |
| S3 50 K/sec burst | 12 ms | ~156 ms† | 45 ms |

†Redis Streams single-thread limit; events queued/dropped above ~41 K/sec
(source: Redis Streams benchmark docs)

**LiminalDB projection rationale:** TRS reduces tick interval under spike load,
keeping routing latency sub-15 ms. This is a design-property claim;
empirical validation is pending.

### Throughput *[modelled]*

| Scenario | LiminalDB *[modelled]* | Redis | Kafka | Notes |
|----------|------------------------|-------|-------|-------|
| S1 | 1 700/sec | 1 700/sec | 1 700/sec | All sustain at this level |
| S2 | 41 000/sec | 41 000/sec | 41 000/sec | All sustain |
| S3 | 50 000/sec | ~41 000/sec | 50 000/sec | Redis drops ~8 % at peak |

### Memory *[modelled — ~8 KB/active cell + overhead]*

| System | S1 100 K sensors | S2 500 K sensors | S3 1 M sensors |
|--------|-----------------|-----------------|----------------|
| LiminalDB *[modelled]* | 85 MB | 320 MB | 540 MB |
| Redis (measured) | 120 MB | 450 MB | 850 MB |
| Kafka (measured) | 380 MB | 1.2 GB | 2.1 GB |

**Projection rationale:** Under spike load, LiminalDB's adaptive sleep mechanism
reduces active cell count by an estimated 20–30 %, lowering memory footprint.
Actual RSS depends on cell population at runtime.

### Decision traceability ✅

Unlike Redis Streams or Kafka, every adaptive decision is logged with rationale:

```json
{
  "timestamp_ms": 1712134412100,
  "decision": "sleep_threshold_increase",
  "rationale": ["live_load=0.78 > target=0.70", "cell_count=5240 > limit=5000"],
  "action": "sleep_threshold += 0.15",
  "expected_effect": "active_cells reduced ~20 %"
}
```
*(Illustrative schema — actual field names match `ClusterEvent` enum in `liminal-core`)*

---

## Cost Comparison *[illustrative]*

> These figures are **order-of-magnitude estimates** based on typical cloud
> pricing (2024) and industry benchmarks for operational overhead.
> They are intended to illustrate the *shape* of the cost difference, not
> to serve as a precise quote. Your organisation's numbers will differ.

**Scenario:** 1 year, 24/7, 100 K–1 M sensors

| System | Infra | Dev/Ops labour | Training | Total *[illustrative]* |
|--------|-------|----------------|----------|------------------------|
| Redis + custom pipeline | $48 K | $130 K | $20 K | **~$198 K** |
| Kafka + aggregators | $65 K | $150 K | $30 K | **~$245 K** |
| Prometheus + Alertmanager | $35 K | $70 K | $15 K | **~$120 K** |
| **LiminalDB (MIT, single system)** | **$12 K** | **$20 K** | **$5 K** | **~$37 K** |

Primary saving: one system instead of five reduces on-call surface, debugging
time, and cross-tool coordination overhead.

---

## Getting Started

```bash
# 1. Build
cargo build --release -p liminal-cli

# 2. Start (WebSocket on port 8787, persistent store at ./data)
./target/release/liminal-cli --store ./data --ws-port 8787
```

Send sensor data via the **WebSocket interface** (port 8787) or the **CLI**:

```bash
# Via CLI (interactive)
:impulse write sensor/temperature/device-1 strength=0.8 data=24.5C

# Via CLI — check system state
:status            # live load, harmony, latency
:mirror top 10     # 10 most recent adaptive decisions
:seed garden       # active goal lifecycle
```

WebSocket clients use the binary/JSON protocol defined in
[docs/PROTOCOL.md](PROTOCOL.md). See `sdk/ts/src/client.ts` for a working
TypeScript example.

> **Note:** There is no built-in HTTP REST endpoint at this time.
> All programmatic access goes through WebSocket (port 8787) or the ABI bridge.

---

## Illustrative Deployment Scenario

*The following describes a hypothetical rollout to illustrate how LiminalDB
would be integrated. Numbers are illustrative, not measured.*

**Hypothetical:** A logistics company monitoring 1 M warehouse sensors.

**Current state (before):**
- 3 engineers maintaining 5 separate systems
- 2–3 capacity incidents per month
- p99 latency ~40 ms during peak hours (estimated from typical Kafka setups)
- ~$245 K/year operational cost (illustrative)

**After LiminalDB rollout (projected):**
- 1 engineer in rotation for a single system
- 0 manual capacity interventions (TRS auto-adjusts) *[planned — auto-scaling 📋]*
- p99 latency <15 ms target *[modelled]*
- ~$37 K/year operational cost (illustrative)

The specific savings depend entirely on your existing stack and team structure.

---

## Feature Status Summary

| Capability | Status | Notes |
|-----------|--------|-------|
| Pattern routing + affinity | ✅ implemented | `liminal-core` |
| PID adaptive control (TRS) | ✅ implemented | `trs.rs` |
| Mirror decision log | ✅ implemented | Epoch-based event sourcing |
| WebSocket bridge | ✅ implemented | `liminal-bridge-net` |
| CLI interface | ✅ implemented | `liminal-cli` |
| WASM / FFI bridges | ✅ implemented | `liminal-bridge-abi` |
| Real performance benchmarks | 🚧 in progress | Q2 2025 target |
| Distributed cluster / Raft | 🚧 in progress | Q3 2025 target |
| Horizontal auto-scaling | 📋 planned | Q3 2025 roadmap |
| Security audit | 🚧 in progress | Q4 2025 target |

---

**Questions?** Open an issue or email safal0645@gmail.com

**Want to contribute?** See [CONTRIBUTING.md](../CONTRIBUTING.md)
