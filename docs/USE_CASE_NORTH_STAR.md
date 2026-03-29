# LiminalDB North Star Use-Case (v1)

## Use-case name

**Adaptive Signal Routing for Event-Driven Operations**

## Target user

Platform/SRE team operating event-heavy services where signal volume and quality fluctuate (alerts, health events, telemetry impulses).

## Problem

Traditional pipelines persist events but react statically under changing conditions.
Teams need the runtime to adapt routing intensity and preserve explainability under load spikes.

## Why LiminalDB fits

- Signal-native ingestion model (`Query` / `Write` / `Affect` impulses).
- Runtime adaptation loop for dynamic behavior tuning.
- Explainable output through metrics/events/replay.

## Input profile

- Event rate: bursty (baseline + spikes)
- Impulse mix (initial benchmark):
  - Affect: 50%
  - Query: 35%
  - Write: 15%
- Patterns: hierarchical tokens (e.g. `cpu/load`, `mem/free`, `service/auth/error`).

## Success metrics (numeric)

1. **Adaptation convergence:** after a sustained spike, key stability metric returns to target band within **20 ticks**.
2. **Hint relevance:** at least **70%** of emitted hints during overload windows map to active high-load patterns.
3. **Explainability completeness:** **100%** of sampled adaptation windows include metrics/events sufficient to explain behavior shift.

## Non-goals (for v1)

- Replacing OLTP relational databases.
- Full autonomous policy optimization.
- Multi-cluster distributed consensus benchmarking.

## Demo acceptance (for quick validation)

A demo run is successful when:

- a spike phase is induced by scripted impulses,
- runtime metrics visibly diverge and then recover,
- adaptation-related hints/events are captured and interpretable,
- convergence and hint relevance thresholds are met.
