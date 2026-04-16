# LiminalDB Landing Page Copy

## Hero

### Your data layer should remember why a system changed, not just what it stores

LiminalDB is a reactive, audit-friendly database for adaptive systems.
It combines replayable history, protocol surfaces, and self-tuning behavior so
builders can keep state, evidence, and change history in one open layer.

**Primary promise**

- keep a replayable timeline of state changes
- support adaptive behavior instead of static CRUD-only thinking
- expose an open protocol surface for downstream tooling

**CTA**

- Review the stack demo
- Review benchmark status

## Problem

Traditional infrastructure stores data, but often loses the reasoning trail
behind how a system evolved.

That creates real pain for:

- autonomous systems
- adaptive monitoring
- event-heavy control loops
- systems that need auditability and replay

You can store records in conventional databases, but you still end up building
extra layers for:

- event sourcing
- replay
- observability
- cross-language access

## Agitation

When change history is fragmented, teams pay the price in three places:

- debugging gets slower
- compliance evidence gets weaker
- system behavior becomes harder to reproduce

If an adaptive system cannot show how it reached a decision, it becomes harder
to trust, harder to verify, and harder to evolve.

## Solution

LiminalDB treats data flow more like a living adaptive field than a passive
storage bucket.

It brings together:

- Cells as autonomous reactive units
- TRS as a self-tuning control loop
- Mirror Timeline for replayable history
- protocol and SDK surfaces for interoperability

The result is not "just another database." It is a memory and evidence layer
for systems that change over time and need to explain that change.

## What you get

### 1. Replayable audit trail

Mirror Timeline gives you append-only history that can be inspected and
replayed rather than reconstructed from scattered logs.

### 2. Adaptive behavior

The system is built around feedback, tuning, and reactive state rather than a
strictly passive storage model.

### 3. Open access surfaces

Use CLI, WebSocket, Rust SDK, TypeScript SDK, and bridge layers instead of
locking your workflow inside one language boundary.

### 4. Better fit for autonomous systems

For agentic or adaptive workloads, LiminalDB gives you a stronger story around
memory, traceability, and reproducibility.

## Who this is for

- builders of adaptive or agentic systems
- teams that need replayable operational history
- engineers who want open protocol access instead of opaque managed lock-in
- researchers exploring self-tuning infrastructure

## Proof and credibility

- protocol and conformance surfaces in the repository
- synthetic and modelled benchmark transparency in `docs/BENCHMARKS.md`
- stack walkthrough in `docs/STACK_DEMO.md`
- architecture and ADR material in `docs/`
- NLnet-facing grant materials in `grants/`

## Call to action

Use LiminalDB if your system needs more than storage.

Use it when you need:

- memory
- replay
- auditability
- adaptive behavior
- open interfaces

Start with:

- `docs/STACK_DEMO.md`
- `docs/BENCHMARKS.md`
- `docs/USE_CASE_IOT_MONITORING.md`
- `grants/README.md`

## FAQ

### Is this a replacement for every traditional database?

No. The strongest case is adaptive, event-heavy, or audit-sensitive systems.

### Are the current benchmark numbers live production measurements?

Not yet. The repository is explicit about what is modelled versus what is
already measured.

### Why is this better than bolting event sourcing onto another store?

Because replay, adaptive control, and evidence are part of the design story,
not just an afterthought layered on top.
