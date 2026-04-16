# LiminalDB

LiminalDB is a next-generation database: **reactive, adaptive, and explainable**.

LiminalDB — база данных нового поколения: **реактивная, адаптивная и объяснимая**.

A biologically-inspired, self-adaptive reactive database system written in Rust.

## Why LiminalDB

Most databases focus on storing and retrieving state. LiminalDB keeps this foundation and adds runtime adaptation:

- **Storage**: persist impulses, events, and state snapshots.
- **Adaptation**: tune behavior under changing signal/load patterns.
- **Explainability**: expose decisions through metrics, events, and replay.

Большинство БД фокусируются на хранении и чтении состояния. LiminalDB сохраняет эту основу и добавляет адаптацию во время выполнения:

- **Хранение**: фиксация импульсов, событий и снимков состояния.
- **Адаптация**: подстройка поведения под изменяющиеся сигналы и нагрузку.
- **Объяснимость**: прозрачность решений через метрики, события и replay.

## What is LiminalDB?

LiminalDB models data operations as a living ecosystem instead of traditional CRUD:

- **Cells** (NodeCell) — autonomous reactive agents with energy, metabolism, and pattern affinity
- **Impulses** — signals flowing through the system (Query / Write / Affect)
- **TRS** — PID control loop that automatically adapts system behaviour to changing load
- **Reflexes** — feedback rules that respond to stress signals
- **Seed Garden** — goal-oriented task lifecycle (plant → grow → harvest)
- **Mirror Timeline** — append-only epoch log for replay and audit

## Architecture

```
┌─────────────────────────────────────────┐
│           liminal-core (~9 K LOC)        │
│                                          │
│  ├─ Cell Management   (lifecycle)        │
│  ├─ Pattern Routing   (affinity index)   │
│  ├─ Adaptive Control  (TRS / PID)        │
│  ├─ Reflex System     (feedback loops)   │
│  ├─ Seed Garden       (task lifecycle)   │
│  ├─ Mirror Timeline   (event sourcing)   │
│  └─ Storage Layer     (journal + WAL)    │
└─────────────────────────────────────────┘
         ▲              ▲              ▲
   ┌─────┴────┐  ┌──────┴─────┐  ┌───┴──────┐
   │   CLI    │  │  WebSocket  │  │ WASM/ABI │
   └──────────┘  └─────────────┘  └──────────┘
```

**Design principles:** Hexagonal (Ports & Adapters), Event Sourcing, DDD bounded contexts.  
Core has zero I/O dependencies — adapters can be swapped or tested in isolation.

## Quick start

```bash
git clone https://github.com/safal207/LiminalBD.git
cd LiminalBD

cargo build --release -p liminal-cli
./target/release/liminal-cli --store ./data --ws-port 8787
```

From the repo root you can also run the CLI in debug mode:

```bash
cargo run -p liminal-cli
```

Then try a few impulses:

```text
a cpu/load 0.9
q color/red 0.7
w memory/seed 0.5
```

You should see periodic metrics and hints as the field reacts. A guided walkthrough lives in [`docs/QUICKSTART_5_MIN.md`](docs/QUICKSTART_5_MIN.md).

## Repository map

- `liminal-db/` — core crates, CLI, bridges, storage, docs.
- `protocol/` — protocol schemas and codegen.
- `examples/` — integration examples.

### Interactive session

```bash
:seed plant BoostToken cpu/load {"scale":0.2} 60000   # plant a goal
:seed garden                                            # inspect growth
:status                                                 # live metrics
:mirror top 5                                           # recent decisions
:snapshot                                               # persist state
```

### Rust SDK

```rust
use liminal_core::*;

let mut field = ClusterField::new();
field.spawn_cell_with_pattern("cpu/load");

field.ingest_impulse(Impulse::query("cpu/load", 0.8));
field.tick_all();

for event in field.drain_events() {
    println!("{:?}", event);
}
```

### TypeScript SDK (WebSocket)

> The TypeScript SDK wraps the WebSocket protocol defined in
> [liminal-db/docs/PROTOCOL.md](liminal-db/docs/PROTOCOL.md).
> Source: `sdk/ts/src/client.ts`

```typescript
import { LiminalClient } from './sdk/ts/src/index';

const client = new LiminalClient('ws://localhost:8787');

// Subscribe to harmony events
client.on('harmony', (msg) => {
  console.log('live_load:', msg.meta?.live_load);
});

// Send an impulse via the protocol
client.send(JSON.stringify({
  cmd: 'impulse',
  pattern: 'cpu/load',
  strength: 0.8,
}));
```

## Project status

**Version:** 0.5.x (pre-1.0, active development)

| Area | Status |
|------|--------|
| Core biological engine (cells, impulses, metabolism) | ✅ Done |
| Event sourcing (journal, snapshots, WAL) | ✅ Done |
| PID adaptive control (TRS) | ✅ Done |
| Reflex & variant decision systems | ✅ Done |
| CLI, WebSocket, WASM, FFI bridges | ✅ Done |
| Rust SDK + TypeScript SDK | ✅ Done |
| Protocol specification | ✅ Done |
| Distributed cluster / Raft consensus | 🚧 In progress |
| OpenTelemetry tracing | 🚧 In progress |
| Performance benchmarks (real hardware) | 🚧 In progress |
| Security audit | 🚧 In progress |

## Performance targets

These are **design targets**, not yet validated measurements.
Benchmarking against a live instance is tracked in
[docs/USE_CASE_IOT_MONITORING.md](docs/USE_CASE_IOT_MONITORING.md).

| Metric | Target |
|--------|--------|
| Cell routing latency p99 | < 5 ms for 10 K cells |
| Impulse throughput | > 10 K / sec |
| Memory | < 100 MB for 10 K cells |
| Snapshot write (100 MB state) | < 500 ms |
| Recovery after node failure | < 30 s |

## Use cases

- **Adaptive monitoring** — alert thresholds that self-tune to workload
- **IoT data ingestion** — burst-tolerant pipeline without manual provisioning
- **Real-time recommendations** — pattern matching that specialises over time
- **Autonomous control** — PID-regulated decision loops with full audit trail

See [docs/USE_CASE_IOT_MONITORING.md](docs/USE_CASE_IOT_MONITORING.md) for a
detailed IoT scenario with modelled comparisons vs Redis Streams and Kafka.

## Documentation

| Document | Content |
|----------|---------|
| [docs/POSITIONING_V1.md](docs/POSITIONING_V1.md) | Product positioning |
| [docs/QUICKSTART_5_MIN.md](docs/QUICKSTART_5_MIN.md) | Five-minute walkthrough |
| [docs/USE_CASE_NORTH_STAR.md](docs/USE_CASE_NORTH_STAR.md) | North-star use case |
| [docs/IMPLEMENTATION_BACKLOG_P0_P1_P2.md](docs/IMPLEMENTATION_BACKLOG_P0_P1_P2.md) | P0/P1/P2 backlog |
| [docs/EXPLAINABILITY_EVENT_CONTRACT.md](docs/EXPLAINABILITY_EVENT_CONTRACT.md) | Explainability events |
| [docs/METRICS_GLOSSARY.md](docs/METRICS_GLOSSARY.md) | Metrics glossary |
| [liminal-db/README.md](liminal-db/README.md) | Workspace crates and build |
| [liminal-db/docs/PROTOCOL.md](liminal-db/docs/PROTOCOL.md) | Client-server wire format |
| [docs/ARCHITECTURE_ANALYSIS.md](docs/ARCHITECTURE_ANALYSIS.md) | Design decisions, module breakdown |
| [docs/ROADMAP.md](docs/ROADMAP.md) | Development roadmap |
| [docs/PHILOSOPHY.md](docs/PHILOSOPHY.md) | Intellectual and philosophical roots |
| [docs/adr/](docs/adr/) | Architecture Decision Records |
| [CONTRIBUTING.md](CONTRIBUTING.md) | How to contribute |

## Contributing

We welcome contributions in any area — tests, docs, benchmarks, SDKs, or core
features. See [CONTRIBUTING.md](CONTRIBUTING.md) for setup instructions and
good first issues.

## License

[Apache-2.0](LICENSE) — free to use, modify, and distribute.

## Contact

safal0645@gmail.com
