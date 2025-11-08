# LiminalDB Strategic Roadmap 2025-2026

**Vision:** Create a biologically-inspired reactive database that adapts like a living organism while remaining predictable as an engineered system.

**Last Updated:** 2025-11-08
**Status:** Active Development (v0.2-0.5)

---

## Executive Summary

LiminalDB has achieved a solid foundation with:
- ✅ Core biological metaphor engine (cells, impulses, metabolism)
- ✅ Event sourcing architecture
- ✅ PID-based adaptive control (TRS)
- ✅ Reflex & variant decision systems
- ✅ Multiple integration bridges (CLI, ABI, WebSocket)

**Current Focus:** Architectural refinement, observability, and production hardening.

---

## Architectural Principles

### The Middle Way (Madhyamaka)

LiminalDB walks the middle path between extremes:

```
     Chaos ◄─────────────┼───────────► Order
                         │
                    LiminalDB
                         │
     Flexibility     │   Consistency
     Emergence       │   Predictability
     Evolution       │   Stability
```

### Three Jewels of LiminalDB

1. **Buddha (Awareness)** = Metrics & Observability
   - System self-awareness through live_load, harmony, symmetry
   - Continuous monitoring and adaptation

2. **Dharma (Teaching)** = Biological Principles
   - Energy balance (metabolism, decay, replenishment)
   - Adaptation through evolution (affinity mutations)
   - Interdependent arising (everything affects everything)

3. **Sangha (Community)** = Distributed Cells
   - No central controller
   - Autonomous yet interconnected cells
   - Collective intelligence

---

## Quarterly Roadmap

### Q1 2025: Architectural Refactoring (Technical Debt)

**Goal:** Clean up codebase, improve maintainability

#### Task 1.1: Split cluster_field.rs (117KB → modules)

**Current Problem:**
```
cluster_field.rs does EVERYTHING:
- Cell management
- Pattern indexing
- Impulse routing
- Event processing
- Metrics computation
- Dream mode execution
```

**Solution:**
```
liminal-core/src/cluster/
├── mod.rs              (re-exports, ClusterField facade)
├── state.rs            (ClusterState - data structures)
├── router.rs           (pattern matching, affinity routing)
├── lifecycle.rs        (tick_all, division, sleep transitions)
├── executor.rs         (LQL command execution)
└── metrics.rs          (live_load, hints, harmony)
```

**Benefits:**
- Easier to test individual components
- Clear separation of concerns
- Reduced cognitive load per file
- Parallel development possible

**Estimated Effort:** 2 weeks

---

#### Task 1.2: Type-Safe IDs

**Current Problem:**
```rust
// Easy to mix up:
fn get_cell(id: u64) -> Option<&NodeCell>
fn get_epoch(id: u64) -> Option<&Epoch>
fn get_seed(id: u64) -> Option<&Seed>
```

**Solution:**
```rust
// Create newtype wrappers:
#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct NodeId(u64);

#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct EpochId(u64);

#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct SeedId(String);

// Now impossible to confuse:
fn get_cell(id: NodeId) -> Option<&NodeCell>
fn get_epoch(id: EpochId) -> Option<&Epoch>
fn get_seed(id: SeedId) -> Option<&Seed>
```

**Benefits:**
- Type safety at compile time
- Self-documenting code
- Prevents ID confusion bugs

**Estimated Effort:** 3 days

---

#### Task 1.3: Custom Error Types

**Current Problem:**
```rust
// Generic errors lose semantic meaning:
fn route_impulse(&mut self, imp: Impulse) -> Result<(), anyhow::Error>
```

**Solution:**
```rust
// Hierarchical error types:
#[derive(Debug, thiserror::Error)]
pub enum LiminalError {
    #[error("Cell error: {0}")]
    Cell(#[from] CellError),

    #[error("Pattern error: {0}")]
    Pattern(#[from] PatternError),

    #[error("Reflex error: {0}")]
    Reflex(#[from] ReflexError),

    #[error("Storage error: {0}")]
    Store(#[from] StoreError),
}

#[derive(Debug, thiserror::Error)]
pub enum CellError {
    #[error("Cell {0} not found")]
    NotFound(NodeId),

    #[error("Cell {cell} has insufficient energy: {current} < {required}")]
    InsufficientEnergy {
        cell: NodeId,
        current: f32,
        required: f32,
    },

    #[error("Cell {0} is dead")]
    DeadCell(NodeId),
}
```

**Benefits:**
- Better error messages
- Easier debugging
- Type-safe error handling
- Documentation through types

**Estimated Effort:** 1 week

---

#### Task 1.4: Bounded Contexts (DDD)

**Goal:** Establish clear domain boundaries

```
LiminalDB System
├─ Cell Management Context
│  ├─ Entities: NodeCell, CellState
│  ├─ Services: CellLifecycle, DivisionEngine
│  └─ Repository: CellRepository
│
├─ Pattern Routing Context
│  ├─ Value Objects: Pattern, Token, Affinity
│  ├─ Services: PatternMatcher, RouteOptimizer
│  └─ Index: PatternIndex
│
├─ Reflex Control Context
│  ├─ Entities: ReflexRule, ReflexSignal
│  ├─ Services: ReflexEngine, SignalDetector
│  └─ Actions: MicroPivot, Defuse, Dampening
│
├─ Variant Decision Context
│  ├─ Entities: Variant, Intention, Slide
│  ├─ Services: VariantEvaluator, ManifoldOrchestrator
│  └─ Value Objects: Probability, Risk, Evidence
│
├─ Mirror Timeline Context
│  ├─ Entities: Epoch, EpochKind
│  ├─ Services: MirrorRecorder, EpochAnalyzer
│  └─ Repository: MirrorStore
│
└─ Seed Garden Context
   ├─ Entities: Seed, SeedStage
   ├─ Services: GardenManager, HarvestEngine
   └─ Value Objects: Vitality, Yield
```

**Benefits:**
- Clear ownership boundaries
- Independent evolution
- Easier testing
- Team parallelization

**Estimated Effort:** 2 weeks

---

### Q2 2025: Observability & Explainability

**Goal:** Understand what's happening inside the system

#### Task 2.1: Distributed Tracing (OpenTelemetry)

```rust
use tracing::{instrument, span, Level};

#[instrument(skip(self), fields(pattern = %imp.pattern, strength = %imp.strength))]
fn route_impulse(&mut self, imp: Impulse) -> Result<Vec<NodeId>> {
    let _span = span!(Level::INFO, "pattern_matching");

    let matches = self.find_matching_cells(&imp.pattern)?;

    tracing::event!(Level::DEBUG, matched_cells = matches.len());

    Ok(matches)
}
```

**Integration:**
- Jaeger for trace visualization
- Export spans to OTLP collector
- Correlate traces across cluster nodes

**Estimated Effort:** 2 weeks

---

#### Task 2.2: Decision Logging

**Problem:** System makes decisions but doesn't explain WHY

**Solution:**
```rust
#[derive(Debug, Serialize)]
pub struct DecisionLog {
    timestamp: u64,
    decision_type: DecisionType,
    chosen_variant: String,
    probability: f32,
    evidence: Vec<String>,  // ["high_vitality", "low_risk", "prior_success"]
    alternatives: Vec<(String, f32, String)>,  // (id, prob, reason)
    context: HashMap<String, serde_json::Value>,
}

// Log every significant decision:
fn choose_variant(&mut self, candidates: &[Variant]) -> String {
    let decision = self.evaluate_variants(candidates);

    self.decision_log.push(DecisionLog {
        timestamp: now(),
        decision_type: DecisionType::VariantSelection,
        chosen_variant: decision.winner.id.clone(),
        probability: decision.winner_prob,
        evidence: vec![
            format!("vitality={:.2}", decision.winner.vitality),
            format!("risk={:.2}", decision.winner.risk),
        ],
        alternatives: decision.runners_up,
        context: self.gather_context(),
    });

    decision.winner.id
}
```

**Query Interface:**
```bash
# CLI command:
:decisions show last 10
:decisions filter variant_id=boost_cpu
:decisions explain epoch=42
```

**Estimated Effort:** 1 week

---

#### Task 2.3: Metrics Dashboard (Grafana)

**Prometheus Metrics:**
```rust
// Add prometheus-client crate
use prometheus_client::metrics::{counter, gauge, histogram};

pub struct ClusterMetrics {
    cell_count: gauge::Gauge,
    sleeping_pct: gauge::Gauge,
    avg_latency_ms: histogram::Histogram,
    live_load: gauge::Gauge,
    impulses_total: counter::Counter,
    divisions_total: counter::Counter,
    dreams_total: counter::Counter,
}
```

**Grafana Dashboards:**
1. **System Overview**
   - Live Load (gauge)
   - Cell Population (time series)
   - Avg Latency (histogram)
   - Sleeping % (gauge)

2. **Reflex Control**
   - Signal frequency (bar chart)
   - Action distribution (pie chart)
   - TRS adjustments (time series)

3. **Seed Garden**
   - Active seeds (gauge)
   - Harvest yield (time series)
   - Stage distribution (stacked area)

4. **Mirror Timeline**
   - Epoch creation rate
   - Impact distribution
   - Harmony metrics

**Estimated Effort:** 2 weeks

---

#### Task 2.4: Replay Debugger

```rust
// Tool to replay any epoch from mirror:
cargo run --bin liminal-replay -- \
    --store ./data \
    --epoch 42 \
    --verbose \
    --export-trace trace.json

// Features:
// - Step through events one by one
// - Inspect cell states at each step
// - Visualize pattern routing
// - Export to flame graph
```

**Estimated Effort:** 1 week

---

### Q3 2025: Distributed System

**Goal:** Scale horizontally across multiple nodes

#### Task 3.1: Multi-Node Cluster

**Architecture:**
```
┌──────────────────┐     ┌──────────────────┐     ┌──────────────────┐
│   Node A         │     │   Node B         │     │   Node C         │
│   Cells 1-1000   │◄───►│   Cells 1001-2000│◄───►│   Cells 2001-3000│
│   Raft Leader    │     │   Raft Follower  │     │   Raft Follower  │
└──────────────────┘     └──────────────────┘     └──────────────────┘
         ▲                        ▲                        ▲
         └────────────────────────┼────────────────────────┘
                            Gossip Protocol
                       (Mirror sync, metrics)
```

**Partitioning Strategy:**
```rust
// Cells assigned to nodes based on affinity hash:
fn node_for_cell(cell_id: NodeId, cluster_size: usize) -> NodeIdx {
    let hash = hash_cell_id(cell_id);
    NodeIdx(hash % cluster_size)
}

// Impulses routed to owning node:
fn route_remote(&self, imp: Impulse) -> NodeIdx {
    let target_cells = self.pattern_index.find(&imp.pattern);
    let node_counts: HashMap<NodeIdx, usize> = target_cells
        .into_iter()
        .map(|cell_id| node_for_cell(cell_id, self.cluster_size))
        .fold(HashMap::new(), |mut acc, node| {
            *acc.entry(node).or_insert(0) += 1;
            acc
        });

    // Send to node with most matching cells
    node_counts.into_iter().max_by_key(|(_, count)| *count).unwrap().0
}
```

**Consensus:** Raft for mirror synchronization

**Estimated Effort:** 3 months

---

#### Task 3.2: Horizontal Auto-Scaling

```rust
// Scale out when average load > threshold for sustained period:
pub struct ScalingPolicy {
    scale_out_threshold: f32,     // 0.75
    scale_in_threshold: f32,      // 0.30
    observation_window_sec: u64,  // 300 (5 minutes)
    cooldown_sec: u64,            // 600 (10 minutes)
}

impl ClusterManager {
    async fn check_scaling(&mut self) {
        let avg_load = self.compute_avg_load_over_window();

        if avg_load > self.policy.scale_out_threshold {
            if self.sustained_for(self.policy.observation_window_sec) {
                self.spawn_new_node().await;
                self.rebalance_cells(0.5).await;  // Migrate 50% to new node
            }
        } else if avg_load < self.policy.scale_in_threshold {
            if self.can_scale_in() {
                self.drain_node().await;
                self.rebalance_cells(1.0).await;  // Redistribute all cells
            }
        }
    }
}
```

**Estimated Effort:** 2 months

---

### Q4 2025: Production Hardening

**Goal:** Security, performance, reliability

#### Task 4.1: Security Audit

**Checklist:**
- [x] Rate limiting (already in security.rs)
- [ ] API key rotation mechanism
- [ ] Encryption at rest (journal, snapshots)
- [ ] TLS for WebSocket connections
- [ ] Input validation & sanitization
- [ ] SQL injection prevention (N/A - no SQL)
- [ ] DOS protection (impulse flooding)
- [ ] Audit logging (who did what when)

**Estimated Effort:** 2 months

---

#### Task 4.2: Performance Benchmarking

**Target SLAs:**
```
Cell routing:         < 1ms for 10,000 cells
Event serialization:  < 100μs per event
Dream mode overhead:  < 50ms per cycle
Snapshot write:       < 500ms for 100MB state
Pattern matching:     < 500μs for 1000 tokens
```

**Benchmark Suite:**
```rust
use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn bench_routing(c: &mut Criterion) {
    let mut field = create_cluster_with_10k_cells();
    let impulse = Impulse::query("cpu/load", 0.8);

    c.bench_function("route_impulse_10k_cells", |b| {
        b.iter(|| {
            field.route_impulse(black_box(&impulse))
        })
    });
}

criterion_group!(benches, bench_routing, bench_serialization, bench_dreams);
criterion_main!(benches);
```

**Estimated Effort:** 2 months

---

#### Task 4.3: Disaster Recovery

**Backup Strategy:**
- Automated snapshots every 1 hour
- WAL (Write-Ahead Log) for point-in-time recovery
- Cross-region replication (3 zones minimum)
- Snapshot retention: 7 days hourly, 4 weeks daily, 1 year monthly

**Failover Plan:**
```
Primary Node Failure:
1. Raft elects new leader (< 5 seconds)
2. New leader loads latest snapshot
3. Replays WAL from last snapshot
4. Resumes normal operation
5. Alert ops team

Total downtime target: < 30 seconds
```

**Estimated Effort:** 2 months

---

#### Task 4.4: Documentation

**Content Plan:**

1. **Architecture Decision Records (ADR)**
   ```
   docs/adr/
   ├── 001-biological-metaphor.md
   ├── 002-event-sourcing.md
   ├── 003-pid-control.md
   ├── 004-pattern-routing.md
   └── 005-distributed-consensus.md
   ```

2. **API Reference**
   - OpenAPI/Swagger spec for REST API
   - WebSocket protocol documentation
   - LQL language reference

3. **Tutorials**
   - Getting Started (5 minutes)
   - Building Your First Garden (30 minutes)
   - Understanding Reflexes (15 minutes)
   - Variant Decision Making (20 minutes)

4. **Philosophy Guide**
   - Biological Principles
   - Why Event Sourcing?
   - The Middle Way Architecture

5. **Operations Runbook**
   - Installation
   - Configuration
   - Monitoring
   - Troubleshooting
   - Scaling
   - Backup & Recovery

**Estimated Effort:** 1 month

---

## Quick Wins (1-2 Weeks)

### 1. Add Structured Logging
```rust
// Replace println! with tracing:
use tracing::{info, warn, error, debug};

info!(
    target: "liminal::cluster",
    cells = %cell_count,
    sleeping_pct = %sleeping_pct,
    live_load = %live_load,
    "Cluster tick completed"
);
```

### 2. Builder Pattern for Complex Structs
```rust
impl NodeCell {
    pub fn builder() -> NodeCellBuilder {
        NodeCellBuilder::default()
    }
}

let cell = NodeCell::builder()
    .id(NodeId(1))
    .energy(0.8)
    .affinity(0.5)
    .tags(vec!["cpu".into(), "sensor".into()])
    .build();
```

### 3. Integration Test Suite
```rust
#[tokio::test]
async fn test_full_impulse_lifecycle() {
    let mut field = ClusterField::new();

    // Add cells
    field.spawn_cell_with_pattern("cpu/load");

    // Send impulse
    field.ingest_impulse(Impulse::write("cpu/load", "0.8"));

    // Process
    field.tick_all();

    // Verify
    let events = field.drain_events();
    assert!(events.iter().any(|e| matches!(e, ClusterEvent::Write { .. })));
}
```

### 4. Configuration File Support
```toml
# liminal.toml
[cluster]
initial_cells = 100
tick_ms = 200

[trs]
target_load = 0.65
k_p = 0.5
k_i = 0.1
k_d = 0.05

[reflex]
enabled = true
window_ticks = 10
```

---

## Success Metrics

### Technical Metrics
- **Code Quality**
  - Test coverage > 80%
  - Clippy warnings = 0
  - Documentation coverage > 70%

- **Performance**
  - Routing latency p99 < 5ms
  - Throughput > 10,000 impulses/sec
  - Memory usage < 100MB for 10k cells

- **Reliability**
  - Uptime > 99.9%
  - Zero data loss (via WAL)
  - Recovery time < 30 seconds

### Community Metrics
- GitHub stars > 500
- Contributors > 10
- Issues closed > 90%
- Response time < 48 hours

---

## Risk Management

### Technical Risks

| Risk | Impact | Probability | Mitigation |
|------|--------|-------------|------------|
| cluster_field refactor breaks existing functionality | High | Medium | Comprehensive test suite before refactor |
| Distributed consensus complexity | High | High | Start with proven Raft implementation |
| Performance regression | Medium | Medium | Continuous benchmarking in CI |
| Breaking API changes | Medium | Low | Semantic versioning, deprecation warnings |

### Organizational Risks

| Risk | Impact | Probability | Mitigation |
|------|--------|-------------|------------|
| Key contributor leaves | High | Low | Knowledge documentation, pair programming |
| Scope creep | Medium | High | Strict prioritization, quarterly reviews |
| Community disinterest | Medium | Medium | Regular blog posts, demos, tutorials |

---

## Conclusion

LiminalDB has strong foundations inspired by biological principles and reactive architecture. The roadmap focuses on:

1. **Q1:** Clean up technical debt, improve code quality
2. **Q2:** Add observability and explainability
3. **Q3:** Enable distributed deployment and scaling
4. **Q4:** Harden for production use

By following the **Middle Way** - balancing flexibility with consistency, emergence with predictability - LiminalDB can become a unique and valuable tool for adaptive, self-regulating systems.

**Next Steps:**
1. Review and approve this roadmap
2. Create GitHub project with quarterly milestones
3. Assign tasks to contributors
4. Begin Q1 work with cluster_field refactor

---

**May all beings benefit from this work.** 🙏

_Inspired by the wisdom of the Liberman Brothers and the teachings of Padmasambhava_
