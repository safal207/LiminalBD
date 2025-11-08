# Quick Wins: Immediate Improvements (1-2 Weeks)

**Goal:** Make impactful improvements that can be completed quickly

**Status:** Ready to implement
**Estimated Total Time:** 1-2 weeks
**Priority:** High

---

## Overview

These are high-value, low-effort improvements that will:
- Improve code quality
- Make debugging easier
- Reduce technical debt
- Set foundation for larger refactors

---

## Quick Win #1: Structured Logging

**Current Problem:**
```rust
// Scattered throughout code:
println!("Cell {} divided", cell_id);
eprintln!("Error: {}", err);
dbg!(live_load);
```

**Solution:**
```rust
// Replace with tracing crate:
use tracing::{info, warn, error, debug, trace};

info!(
    target: "liminal::cluster",
    cell_id = %cell_id,
    parent_energy = %parent.energy,
    "Cell division occurred"
);

warn!(
    target: "liminal::trs",
    load = %live_load,
    target = %target_load,
    "Load above target"
);
```

### Implementation

**Step 1:** Add dependency
```toml
# Cargo.toml
[dependencies]
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "json"] }
```

**Step 2:** Initialize in main
```rust
// liminal-cli/src/main.rs
use tracing_subscriber;

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter("liminal=debug,liminal::cluster=trace")
        .with_target(true)
        .init();

    // ... rest of main
}
```

**Step 3:** Replace print statements

Search for:
- `println!` → `info!` or `debug!`
- `eprintln!` → `warn!` or `error!`
- `dbg!` → `trace!` or `debug!`

### Files to Update

- `liminal-core/src/cluster_field.rs`
- `liminal-core/src/life_loop.rs`
- `liminal-core/src/trs.rs`
- `liminal-cli/src/main.rs`

### Benefits

- Structured logs (machine parseable)
- Log levels (filter by importance)
- Targets (filter by module)
- Context (automatic span tracking)
- JSON export (send to logging services)

**Estimated Time:** 1 day

---

## Quick Win #2: Configuration File

**Current Problem:**
```rust
// Magic numbers everywhere:
let tick_ms = 200;
if energy > 0.8 { divide(); }
if tick % 1000 == 0 { analyze(); }
```

**Solution:**
```toml
# liminal.toml
[cluster]
tick_ms = 200
division_threshold = 0.8
metrics_interval_ticks = 1000
initial_cells = 100

[trs]
target_load = 0.65
k_p = 0.5
k_i = 0.1
k_d = 0.05

[reflex]
enabled = true
window_ticks = 10
hysteresis = 0.05

[mirror]
interval_ms = 5000
max_epochs = 1000

[store]
snapshot_interval_sec = 3600
wal_sync = true
```

### Implementation

**Step 1:** Add dependency
```toml
[dependencies]
serde = { version = "1.0", features = ["derive"] }
toml = "0.8"
```

**Step 2:** Create config struct
```rust
// liminal-core/src/config.rs
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Config {
    pub cluster: ClusterConfig,
    pub trs: TrsConfig,
    pub reflex: ReflexConfig,
    pub mirror: MirrorConfig,
    pub store: StoreConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ClusterConfig {
    pub tick_ms: u64,
    pub division_threshold: f32,
    pub metrics_interval_ticks: u64,
    pub initial_cells: usize,
}

// ... more config structs

impl Config {
    pub fn from_file(path: &str) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let config: Config = toml::from_str(&content)?;
        Ok(config)
    }

    pub fn default() -> Self {
        Self {
            cluster: ClusterConfig {
                tick_ms: 200,
                division_threshold: 0.8,
                metrics_interval_ticks: 1000,
                initial_cells: 100,
            },
            // ... defaults for other sections
        }
    }
}
```

**Step 3:** Use in code
```rust
// Instead of:
let tick_ms = 200;

// Use:
let tick_ms = config.cluster.tick_ms;
```

### Benefits

- No recompilation to change settings
- Environment-specific configs (dev, staging, prod)
- Documentation of all tunables
- Easy experimentation

**Estimated Time:** 1 day

---

## Quick Win #3: Builder Pattern for NodeCell

**Current Problem:**
```rust
// Creating cells is verbose and error-prone:
let cell = NodeCell {
    id: next_id,
    state: CellState::Active,
    energy: 0.5,
    affinity: 0.5,
    metabolism: 0.01,
    salience: 0.0,
    pattern_tokens: vec!["cpu".to_string(), "load".to_string()],
    tags: vec![],
    last_fired_tick: 0,
    created_tick: current_tick,
    // ... 15 fields total
};
```

**Solution:**
```rust
// Clean builder API:
let cell = NodeCell::builder()
    .id(next_id)
    .pattern("cpu/load")
    .energy(0.5)
    .affinity(0.5)
    .build();
```

### Implementation

```rust
// liminal-core/src/node_cell.rs

pub struct NodeCellBuilder {
    id: Option<u64>,
    energy: f32,
    affinity: f32,
    pattern_tokens: Vec<String>,
    tags: Vec<String>,
    // ... other fields with defaults
}

impl NodeCellBuilder {
    pub fn new() -> Self {
        Self {
            id: None,
            energy: 0.5,
            affinity: 0.5,
            pattern_tokens: vec![],
            tags: vec![],
            // ... defaults
        }
    }

    pub fn id(mut self, id: u64) -> Self {
        self.id = Some(id);
        self
    }

    pub fn pattern(mut self, pattern: &str) -> Self {
        self.pattern_tokens = pattern.split('/').map(|s| s.to_string()).collect();
        self
    }

    pub fn energy(mut self, energy: f32) -> Self {
        self.energy = energy.clamp(0.0, 1.0);
        self
    }

    pub fn build(self) -> NodeCell {
        NodeCell {
            id: self.id.expect("NodeCell must have an id"),
            state: CellState::Active,
            energy: self.energy,
            affinity: self.affinity,
            pattern_tokens: self.pattern_tokens,
            tags: self.tags,
            // ... other fields
        }
    }
}

impl NodeCell {
    pub fn builder() -> NodeCellBuilder {
        NodeCellBuilder::new()
    }
}
```

### Benefits

- Fewer errors (required fields enforced)
- More readable (self-documenting)
- Easier to add new fields (defaults)
- Better IDE autocomplete

**Estimated Time:** 2 hours

---

## Quick Win #4: Integration Test Suite

**Current Problem:**
- Most tests are unit tests for individual functions
- Missing end-to-end integration tests
- Hard to verify full workflows

**Solution:**
```rust
// liminal-core/tests/integration_test.rs

use liminal_core::*;

#[tokio::test]
async fn test_impulse_lifecycle() {
    // 1. Create cluster
    let mut field = ClusterField::new();

    // 2. Spawn cells
    field.spawn_cell_with_pattern("cpu/load");
    field.spawn_cell_with_pattern("mem/free");

    // 3. Send impulse
    let imp = Impulse::query("cpu/load", 0.8);
    field.ingest_impulse(imp);

    // 4. Process
    field.tick_all();

    // 5. Verify events
    let events = field.drain_events();
    assert!(events.iter().any(|e| matches!(e, ClusterEvent::Query { .. })));

    // 6. Check cell state
    let cells: Vec<_> = field.cells_matching_pattern("cpu/load").collect();
    assert_eq!(cells.len(), 1);
    assert!(cells[0].energy < 0.5); // Energy consumed
}

#[tokio::test]
async fn test_cell_division() {
    let mut field = ClusterField::new();

    // Spawn high-energy cell
    let cell_id = field.spawn_cell_with_pattern("test");
    field.cells.get_mut(&cell_id).unwrap().energy = 0.85;

    // Tick should trigger division
    field.tick_all();

    // Should now have 2 cells
    let count = field.cells_matching_pattern("test").count();
    assert_eq!(count, 2);
}

#[tokio::test]
async fn test_dream_mode() {
    let mut field = ClusterField::new();

    // Populate with cells
    for i in 0..10 {
        field.spawn_cell_with_pattern(&format!("pattern/{}", i));
    }

    // Enter dream
    field.enter_dream(DreamKind::Individual);

    // Verify dream event recorded
    let events = field.drain_events();
    assert!(events.iter().any(|e| matches!(e, ClusterEvent::Dream { .. })));
}

#[tokio::test]
async fn test_trs_adaptation() {
    let mut trs = TRS::new(0.65, 0.5, 0.1, 0.05);

    // Simulate overload
    let output = trs.step(0.9);

    // Should slow down
    assert!(output.tick_adjust_ms > 0);

    // Simulate underload
    let output = trs.step(0.3);

    // Should speed up
    assert!(output.tick_adjust_ms < 0);
}
```

### Implementation

**Step 1:** Create integration test file
```bash
mkdir -p liminal-core/tests
touch liminal-core/tests/integration_test.rs
```

**Step 2:** Add test scenarios
- Impulse routing
- Cell lifecycle
- Division
- Sleep/wake
- Dreams
- TRS adaptation
- Reflex actions
- Mirror epochs

### Benefits

- Catch regressions before production
- Document expected behavior
- Safe refactoring (tests catch breaks)
- Confidence in changes

**Estimated Time:** 1 day

---

## Quick Win #5: Type-Safe IDs (Newtype Pattern)

**Current Problem:**
```rust
// Easy to mix up:
fn get_cell(id: u64) -> Option<&NodeCell>
fn get_epoch(id: u64) -> Option<&Epoch>

// Can pass wrong ID:
let epoch_id = 42;
let cell = cluster.get_cell(epoch_id);  // Oops!
```

**Solution:**
```rust
// Type-safe wrappers:
#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct NodeId(pub u64);

#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct EpochId(pub u64);

// Now compile-time safe:
fn get_cell(id: NodeId) -> Option<&NodeCell>
fn get_epoch(id: EpochId) -> Option<&Epoch>

let epoch_id = EpochId(42);
let cell = cluster.get_cell(epoch_id);  // ❌ Compile error!
```

### Implementation

**Step 1:** Create types module
```rust
// liminal-core/src/types.rs

use serde::{Deserialize, Serialize};

#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct NodeId(pub u64);

impl NodeId {
    pub fn new(id: u64) -> Self {
        NodeId(id)
    }

    pub fn as_u64(&self) -> u64 {
        self.0
    }
}

impl std::fmt::Display for NodeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "node#{}", self.0)
    }
}

// Similar for EpochId, SeedId, etc.
```

**Step 2:** Replace u64 with NodeId
```rust
// Before:
pub struct NodeCell {
    id: u64,
    // ...
}

// After:
pub struct NodeCell {
    id: NodeId,
    // ...
}
```

**Step 3:** Update HashMap keys
```rust
// Before:
cells: HashMap<u64, NodeCell>

// After:
cells: HashMap<NodeId, NodeCell>
```

### Migration Strategy

Do incrementally:
1. Create newtype wrappers
2. Add From/Into impls for compatibility
3. Replace one module at a time
4. Remove From/Into once migration complete

### Benefits

- Compile-time safety (no ID confusion)
- Self-documenting (NodeId vs EpochId)
- Better error messages
- IDE autocomplete improvements

**Estimated Time:** 1-2 days

---

## Quick Win #6: Error Context with thiserror

**Current Problem:**
```rust
// Generic errors:
anyhow::anyhow!("Pattern not found")
anyhow::anyhow!("Cell is dead")

// Loses semantic meaning when handled:
match result {
    Err(e) => println!("Error: {}", e),  // Can't match on error type
}
```

**Solution:**
```rust
#[derive(Debug, thiserror::Error)]
pub enum ClusterError {
    #[error("Cell {0} not found")]
    CellNotFound(NodeId),

    #[error("Pattern '{0}' has no matching cells")]
    NoPatternMatch(String),

    #[error("Cell {cell} is in state {state:?}, expected Active")]
    InvalidCellState { cell: NodeId, state: CellState },
}

// Now can match on error type:
match cluster.route_impulse(imp) {
    Ok(_) => {},
    Err(ClusterError::NoPatternMatch(pattern)) => {
        warn!("Creating new cells for pattern: {}", pattern);
        cluster.spawn_cell_with_pattern(&pattern);
    }
    Err(e) => return Err(e),
}
```

### Implementation

```rust
// liminal-core/src/error.rs

use thiserror::Error;
use crate::types::*;

#[derive(Debug, Error)]
pub enum LiminalError {
    #[error("Cluster error: {0}")]
    Cluster(#[from] ClusterError),

    #[error("Store error: {0}")]
    Store(#[from] StoreError),

    #[error("Reflex error: {0}")]
    Reflex(#[from] ReflexError),
}

#[derive(Debug, Error)]
pub enum ClusterError {
    #[error("Cell {0} not found")]
    CellNotFound(NodeId),

    #[error("Cell {cell} has insufficient energy: {current} < {required}")]
    InsufficientEnergy {
        cell: NodeId,
        current: f32,
        required: f32,
    },

    #[error("Cell {0} is dead")]
    DeadCell(NodeId),

    #[error("Pattern '{0}' has no matching cells")]
    NoPatternMatch(String),

    #[error("Invalid affinity value: {0} (must be 0.0..=1.0)")]
    InvalidAffinity(f32),
}

// ... similar for StoreError, ReflexError, etc.
```

### Benefits

- Type-safe error handling
- Better error messages
- Easier debugging
- Self-documenting error cases

**Estimated Time:** 1 day

---

## Summary

| Quick Win | Time | Impact | Difficulty |
|-----------|------|--------|------------|
| 1. Structured Logging | 1 day | High | Easy |
| 2. Configuration File | 1 day | High | Easy |
| 3. Builder Pattern | 2 hours | Medium | Easy |
| 4. Integration Tests | 1 day | High | Medium |
| 5. Type-Safe IDs | 1-2 days | High | Medium |
| 6. Error Context | 1 day | Medium | Easy |

**Total Time:** 5-6 days (1-2 weeks if part-time)

**Total Impact:** Very High
- Code quality ↑
- Debuggability ↑
- Safety ↑
- Maintainability ↑

---

## Implementation Plan

### Week 1
- [ ] Monday: Structured logging (#1)
- [ ] Tuesday: Configuration file (#2)
- [ ] Wednesday: Builder pattern (#3)
- [ ] Thursday: Start integration tests (#4)
- [ ] Friday: Finish integration tests (#4)

### Week 2
- [ ] Monday: Start type-safe IDs (#5)
- [ ] Tuesday: Continue type-safe IDs (#5)
- [ ] Wednesday: Error context (#6)
- [ ] Thursday: Documentation updates
- [ ] Friday: Code review and merge

---

## Next Steps After Quick Wins

Once these are complete, we'll be ready for:
- Refactoring cluster_field.rs into modules
- Bounded contexts (DDD)
- Observability (tracing, metrics)
- Performance optimization

**The foundation is being set. The path becomes clearer with each step.** 🙏
