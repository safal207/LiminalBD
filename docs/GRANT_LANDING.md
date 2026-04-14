# LiminalDB Grant Landing

## LiminalDB

### A replayable memory and evidence layer for adaptive systems

LiminalDB is the storage layer of the Liminal Stack.
It is an open-source reactive database designed for systems that need more than
plain state storage: they need replay, auditability, and adaptive behavior.

For grant reviewers, the core proposition is:

- store state with replayable history
- expose open protocol and SDK surfaces
- support transparent adaptive systems without vendor lock-in

## Why this matters

Traditional data infrastructure is good at storing records, but weak at showing
how an adaptive system changed over time.

That becomes a problem for:

- event-heavy control systems
- agentic or autonomous workflows
- audit-sensitive infrastructure
- systems that need reproducibility

Teams often end up rebuilding the same layers around a generic store:

- event sourcing
- replay
- protocol bridges
- evidence reconstruction

LiminalDB brings those concerns closer to the core.

## What LiminalDB contributes

LiminalDB gives the stack a memory and evidence layer built around:

- Cells
- TRS adaptive control
- Mirror Timeline
- CLI, WebSocket, Rust SDK, and TypeScript SDK access paths

The point is not novelty for its own sake.
The point is to lower the cost of building systems that need transparent,
replayable operational history.

## Why it fits a grant

LiminalDB is relevant as a commons-oriented infrastructure component because it
is:

- reusable beyond one application
- protocol-oriented rather than platform-locked
- useful for downstream builders who need auditability
- aligned with self-hosted and interoperable deployment models

It is not just "another database."
It is a reusable evidence and memory primitive for adaptive systems.

## Current reviewer evidence

- [Stack demo](STACK_DEMO.md)
- [Benchmark status](BENCHMARKS.md)
- [Use-case writeup](USE_CASE_IOT_MONITORING.md)
- [NLnet materials](../grants/README.md)

## What reviewers should remember

If a system adapts but cannot explain or replay its change history, trust
degrades fast.

LiminalDB exists to make that history:

- persistent
- inspectable
- replayable
- open to other tools

## Best next milestone

The strongest next milestone is a published live benchmark package for the main
runtime, with real measurements for:

- latency
- throughput
- memory
- snapshot and replay paths
