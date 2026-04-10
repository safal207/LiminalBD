# LiminalDB

A biologically-inspired, self-adaptive reactive database system written in Rust.

## What is LiminalDB?

LiminalDB reimagines databases through the lens of biology. Instead of traditional CRUD operations and static schemas, it models data operations as a living ecosystem:

- **Cells** (NodeCell) are autonomous reactive agents with energy, metabolism, and specialization (affinity)
- **Impulses** are signals flowing through the system (Query/Write/Affect)
- **Metabolism & Energy** manage resource allocation and lifecycle
- **PID Control Loop (TRS)** automatically adapts system behavior to changing load
- **Reflexes** provide adaptive, self-regulating responses to stress
- **Seed Garden** enables goal-oriented task cultivation and harvesting
- **Mirror Timeline** records every decision and state change for replay and analysis

The system embodies a **living, evolving architecture** rather than static storage.

## Key Features

✨ **Self-Adaptive Control**
- Automatic load balancing via PID controller (TRS)
- No manual tuning required
- Targets optimal operating point (55-70% live load)

🧬 **Biological Metaphor**
- Cell lifecycle: Active → Idle → Sleep → Dead
- Population dynamics through division and specialization
- Energy-based resource management
- Genetic drift (affinity mutations)

📊 **Complete Observability**
- Event sourcing for full audit trail
- Live metrics: harmony, symmetry, latency
- Decision logging for explainability
- Mirror epochs for speculative analysis

🌐 **Multiple Integration Points**
- CLI for interactive exploration
- WebSocket for real-time streaming
- Rust SDK for native integration
- TypeScript SDK for browser/Node.js
- WASM bridge for embedded systems
- FFI for C/C++ interoperability

⚡ **High Performance**
- Routing: <1ms for 10K cells
- Throughput: 10K+ impulses/sec
- Memory efficient: <100MB for 10K cells
- Event sourcing with snapshots and WAL

## Quick Start

### Installation

```bash
# Clone the repository
git clone https://github.com/safal207/LiminalBD.git
cd LiminalBD

# Build the CLI
cargo build --release -p liminal-cli

# Run the server
./target/release/liminal-cli --store ./data --ws-port 8787
```

### Interactive Demo

```bash
# Plant seeds (goals)
:seed plant BoostToken cpu/load {"scale":0.2} 60000

# Watch the garden grow
:seed garden

# Check system harmony
:status

# View recorded epochs
:mirror top 5

# Save snapshot
:snapshot
```

### Rust SDK

```rust
use liminal_core::*;

#[tokio::main]
async fn main() {
    let mut field = ClusterField::new();
    
    // Spawn cells for a pattern
    field.spawn_cell_with_pattern("cpu/load");
    
    // Send impulse
    let impulse = Impulse::query("cpu/load", 0.8);
    field.ingest_impulse(impulse);
    
    // Process
    field.tick_all();
    
    // Observe
    let events = field.drain_events();
    for event in events {
        println!("{:?}", event);
    }
}
```

### TypeScript SDK

```typescript
import { LiminalClient } from 'liminaldb-client';

const client = new LiminalClient('ws://localhost:8787');

// Subscribe to harmony metrics
client.subscribe('harmony').on('data', (metrics) => {
  console.log(`Live load: ${metrics.live_load}`);
  console.log(`Avg latency: ${metrics.avg_latency_ms}ms`);
});

// Send impulse
client.impulse({
  kind: 'Query',
  pattern: 'cpu/load',
  strength: 0.8
});
```

## Use Cases

### 1. Adaptive Monitoring Systems
Monitor distributed systems that automatically adjust sensitivity and retention based on load. No manual tuning needed.

### 2. IoT Data Ingestion
Self-scaling IoT data pipeline that adapts to sensor bursts without overprovisioning.

### 3. Content Recommendations
Pattern-matching recommendation engine that evolves specialization based on user feedback and system load.

### 4. Real-time Analytics
Streaming analytics that maintains equilibrium and gracefully degrades under spike loads.

### 5. Autonomous Systems
Control logic for robots/drones that must adapt to changing environments while remaining predictable.

## Architecture

```
┌─────────────────────────────────────────┐
│         Application Core                 │
│       (liminal-core, ~9K LOC)            │
│                                          │
│  ├─ Cell Management (lifecycle)          │
│  ├─ Pattern Routing (affinity matching)  │
│  ├─ Adaptive Control (TRS/PID)           │
│  ├─ Reflex System (feedback loops)       │
│  ├─ Seed Garden (task management)        │
│  ├─ Mirror Timeline (event sourcing)     │
│  └─ Storage Layer (journal + snapshots)  │
└─────────────────────────────────────────┘
           ▲           ▲           ▲
           │           │           │
    ┌──────┴───┐  ┌───┴────┐  ┌──┴────────┐
    │ CLI      │  │ WebSocket│ │ WASM/ABI  │
    │ Adapter  │  │ Server   │ │ Bridge    │
    └──────────┘  └──────────┘ └───────────┘
```

**Hexagonal Architecture:**
- Core domain logic has zero IO dependencies
- Adapters can be swapped (CLI → Dashboard → API)
- Easy to test without network/filesystem
- Protocol-agnostic

## Philosophy

LiminalDB is inspired by:
- **Buddhist philosophy** (Madhyamaka: middle way between chaos and order)
- **Systems biology** (metabolism, homeostasis, adaptation)
- **Control theory** (PID loops, feedback systems)
- **Domain-driven design** (bounded contexts, ubiquitous language)

The three principles:

1. **Awareness (Buddha)** – System knows itself through live metrics
2. **Teaching (Dharma)** – Biological principles guide behavior
3. **Community (Sangha)** – Distributed cells collaborate without central authority

## Project Status

**Current Version:** 0.2-0.5 (Active Development)

✅ Implemented:
- Core biological metaphor engine
- Event sourcing with snapshots
- PID-based adaptive control
- Reflex & variant decision systems
- Multiple integration bridges
- Protocol specification

🚧 In Progress:
- Distributed cluster support
- OpenTelemetry tracing
- Performance benchmarking
- Security hardening

📋 Planned:
- Horizontal auto-scaling
- Cross-region replication
- Dashboard UI
- Python SDK

See [STRATEGIC_ROADMAP_2025.md](docs/STRATEGIC_ROADMAP_2025.md) for detailed planning.

## Documentation

- **[Architecture Analysis](docs/ARCHITECTURE_ANALYSIS.md)** – Deep dive into design decisions
- **[Protocol Specification](docs/PROTOCOL.md)** – Client-server communication format
- **[Strategic Roadmap](docs/STRATEGIC_ROADMAP_2025.md)** – Q1-Q4 2025 planning
- **[Cognitive Garden Scenario](docs/COGNITIVE_GARDEN_SCENARIO.md)** – Interactive demo walkthrough
- **[Architecture Decision Records](docs/adr/)** – Decision history and rationale

## Contributing

We welcome contributions! Areas of interest:

- **Documentation** – Tutorials, blog posts, API docs
- **Performance** – Benchmarking, optimization, profiling
- **Observability** – Tracing, metrics, dashboards
- **Testing** – Unit tests, integration tests, property-based testing
- **Distributed Systems** – Cluster support, consensus, replication
- **SDKs** – Python, Go, Java bindings

See [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

## Performance Targets

| Metric | Target |
|--------|--------|
| Cell routing latency (p99) | <5ms for 10K cells |
| Throughput | >10K impulses/sec |
| Memory usage | <100MB for 10K cells |
| Recovery time | <30 seconds |
| Snapshot write | <500ms for 100MB state |

## License

MIT License – See [LICENSE](LICENSE) for details.

## Citation

If you use LiminalDB in research, please cite:

```bibtex
@software{liminaldb2025,
  title={LiminalDB: A Biologically-Inspired Adaptive Database System},
  author={Contributors},
  url={https://github.com/safal207/LiminalBD},
  year={2025}
}
```

## Support

- 📖 **Documentation:** See `/docs` folder
- 💬 **Discussions:** GitHub Issues and Discussions
- 📧 **Email:** safal0645@gmail.com

## Acknowledgments

Inspired by:
- The Liberman Brothers (neuroscience)
- Padmasambhava (Buddhist philosophy)
- Control theory pioneers (PID, adaptive systems)
- Open source communities (Rust, async/await)

---

**May all beings benefit from this work.** 🙏
