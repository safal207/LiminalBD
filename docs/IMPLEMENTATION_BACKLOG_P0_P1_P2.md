# LiminalDB Implementation Backlog (P0/P1/P2)

Goal: move from documentation-only progress to measurable product proof.

## P0 — Must-do next (execution-critical)

### 1) Define one North Star use-case
**Priority:** P0  
**Effort:** 0.5 day  
**Deliverable:** `docs/USE_CASE_NORTH_STAR.md`

**Done when:**
- one use-case is selected,
- target user + input signal type are explicit,
- success metric is numeric (e.g., latency, false-positive rate, adaptation convergence).

---

### 2) Quickstart success criteria block
**Priority:** P0  
**Effort:** 0.5 day  
**Deliverable:** update `docs/QUICKSTART_5_MIN.md`

**Done when:**
- quickstart includes expected output patterns,
- includes “if this does not happen” troubleshooting,
- provides one pass/fail checkpoint.

---

### 3) `cluster_field` phase-1 split (router/lifecycle)
**Priority:** P0  
**Effort:** 3–4 days  
**Deliverable:** new modules under `liminal-core/src/cluster/`

**Done when:**
- routing logic moved to `router.rs`,
- lifecycle/tick transitions moved to `lifecycle.rs`,
- no functional regressions in existing tests,
- file size and responsibility of old monolith reduced.

---

### 4) Integration scenario test for North Star flow
**Priority:** P0  
**Effort:** 1–2 days  
**Deliverable:** integration test under `conformance/` or crate tests

**Done when:**
- test runs end-to-end: ingest → adapt → metrics/events,
- includes assertion on at least one measurable adaptation signal.

## P1 — Should-do next (product clarity)

### 5) Explainability event contract
**Priority:** P1  
**Effort:** 1 day  
**Deliverable:** protocol doc update + stable event payload keys

**Done when:**
- one canonical event schema is documented,
- fields include `why`/`driver` context where applicable,
- sample payload is included in protocol docs.

---

### 6) Demo script for reproducible run
**Priority:** P1  
**Effort:** 1 day  
**Deliverable:** script + README snippet

**Done when:**
- single command reproduces demo sequence,
- output includes checkpoint lines for quick validation.

---

### 7) Metrics glossary
**Priority:** P1  
**Effort:** 0.5 day  
**Deliverable:** `docs/METRICS_GLOSSARY.md`

**Done when:**
- each key metric has definition, range, and interpretation,
- linked from README and quickstart.

## P2 — Nice-to-have (scale and polish)

### 8) Type-safe IDs rollout audit
**Priority:** P2  
**Effort:** 1 day  
**Deliverable:** checklist of remaining raw-ID surfaces

**Done when:**
- all cross-module ID boundaries are audited,
- migration TODO list is explicit.

---

### 9) Error taxonomy baseline
**Priority:** P2  
**Effort:** 1 day  
**Deliverable:** initial domain error enums + mapping doc

**Done when:**
- core domains have explicit error families,
- top-level errors are not opaque for key flows.

---

### 10) “Why LiminalDB” comparison page
**Priority:** P2  
**Effort:** 0.5 day  
**Deliverable:** pragmatic comparison matrix doc

**Done when:**
- clearly states when to use / not use LiminalDB,
- avoids over-claiming against relational/kv/event stores.

## Suggested order (2-week execution)

1. P0-1 → P0-2 → P0-3 → P0-4
2. P1-5 → P1-6 → P1-7
3. P2-8 → P2-9 → P2-10

## Tracking template

Use this per task:

- **Owner:**
- **Status:** todo / in-progress / blocked / done
- **Metric impact:**
- **Risk:**
- **PR link:**
