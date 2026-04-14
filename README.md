## Reviewer docs

- [Stack demo](docs/STACK_DEMO.md)
- [Benchmark status](docs/BENCHMARKS.md)
- [NLnet grant materials](grants/README.md)

# LiminalDB

A biologically-inspired, self-adaptive reactive database system written in Rust.

## What is LiminalDB?

LiminalDB models data operations as a living ecosystem instead of traditional CRUD:

- **Cells** (NodeCell) â€” autonomous reactive agents with energy, metabolism, and pattern affinity
- **Impulses** â€” signals flowing through the system (Query / Write / Affect)
- **TRS** â€” PID control loop that automatically adapts system behaviour to changing load
- **Reflexes** â€” feedback rules that respond to stress signals
- **Seed Garden** â€” goal-oriented task lifecycle (plant â†’ grow â†’ harvest)
- **Mirror Timeline** â€” append-only epoch log for replay and audit

## Architecture

```
â”Œâ”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”
â”‚           liminal-core (~9 K LOC)        â”‚
â”‚                                          â”‚
â”‚  â”œâ”€ Cell Management   (lifecycle)        â”‚
â”‚  â”œâ”€ Pattern Routing   (affinity index)   â”‚
â”‚  â”œâ”€ Adaptive Control  (TRS / PID)        â”‚
â”‚  â”œâ”€ Reflex System     (feedback loops)   â”‚
â”‚  â”œâ”€ Seed Garden       (task lifecycle)   â”‚
â”‚  â”œâ”€ Mirror Timeline   (event sourcing)   â”‚
â”‚  â””â”€ Storage Layer     (journal + WAL)    â”‚
â””â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”˜
         â–²              â–²              â–²
   â”Œâ”€â”€â”€â”€â”€â”´â”€â”€â”€â”€â”  â”Œâ”€â”€â”€â”€â”€â”€â”´â”€â”€â”€â”€â”€â”  â”Œâ”€â”€â”€â”´â”€â”€â”€â”€â”€â”€â”
   â”‚   CLI    â”‚  â”‚  WebSocket  â”‚  â”‚ WASM/ABI â”‚
   â””â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”˜  â””â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”˜  â””â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”˜
```

**Design principles:** Hexagonal (Ports & Adapters), Event Sourcing, DDD bounded contexts.  
Core has zero I/O dependencies â€” adapters can be swapped or tested in isolation.

## Quick Start

```bash
git clone https://github.com/safal207/LiminalBD.git
cd LiminalBD

cargo build --release -p liminal-cli
./target/release/liminal-cli --store ./data --ws-port 8787
```

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
> [docs/protocol.md](docs/protocol.md).
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

## Project Status

**Version:** 0.5.x (pre-1.0, active development)

| Area | Status |
|------|--------|
| Core biological engine (cells, impulses, metabolism) | âœ… Done |
| Event sourcing (journal, snapshots, WAL) | âœ… Done |
| PID adaptive control (TRS) | âœ… Done |
| Reflex & variant decision systems | âœ… Done |
| CLI, WebSocket, WASM, FFI bridges | âœ… Done |
| Rust SDK + TypeScript SDK | âœ… Done |
| Protocol specification | âœ… Done |
| Distributed cluster / Raft consensus | ðŸš§ In progress |
| OpenTelemetry tracing | ðŸš§ In progress |
| Performance benchmarks (real hardware) | ðŸš§ In progress |
| Security audit | ðŸš§ In progress |

## Performance Targets

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

## Use Cases

- **Adaptive monitoring** â€” alert thresholds that self-tune to workload
- **IoT data ingestion** â€” burst-tolerant pipeline without manual provisioning
- **Real-time recommendations** â€” pattern matching that specialises over time
- **Autonomous control** â€” PID-regulated decision loops with full audit trail

See [docs/USE_CASE_IOT_MONITORING.md](docs/USE_CASE_IOT_MONITORING.md) for a
detailed IoT scenario with modelled comparisons vs Redis Streams and Kafka.

## Documentation

| Document | Content |
|----------|---------|
| [docs/ARCHITECTURE_ANALYSIS.md](docs/ARCHITECTURE_ANALYSIS.md) | Design decisions, module breakdown |
| [docs/protocol.md](docs/protocol.md) | Client-server wire format |
| [docs/ROADMAP.md](docs/ROADMAP.md) | Development roadmap |
| [docs/PHILOSOPHY.md](docs/PHILOSOPHY.md) | Intellectual and philosophical roots |
| [docs/adr/](docs/adr/) | Architecture Decision Records |
| [CONTRIBUTING.md](CONTRIBUTING.md) | How to contribute |

## Contributing

We welcome contributions in any area â€” tests, docs, benchmarks, SDKs, or core
features. See [CONTRIBUTING.md](CONTRIBUTING.md) for setup instructions and
good first issues.

## License

[Apache-2.0](LICENSE) â€” free to use, modify, and distribute.

## Contact

safal0645@gmail.com

