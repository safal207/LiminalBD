# NGI Zero Commons Fund - Application
# Liminal Stack: Trustworthy Open Infrastructure for Autonomous AI Systems

## Fund status

`NGI Zero Entrust` is closed. `NGI Zero Commons Fund` is the relevant open
program.

Relevant NLnet dates checked on April 14, 2026:

- The Commons Fund main page showed the twelfth call deadline as
  `April 1, 2026, 12:00 CEST`.
- NLnet news published on `April 9, 2026` states:
  `The next deadline is June 1st 2026.`

Sources:

- https://nlnet.nl/commonsfund/
- https://nlnet.nl/news/2026/20260409-announce-commons-fund.html

## Project name

Liminal Stack: Adaptive Routing, Reactive Storage and Secure Containers
for Trustworthy AI Infrastructure

## Requested amount

EUR 50,000

## Duration

12 months

## Summary

Liminal Stack is a three-component open-source infrastructure stack for
trustworthy, auditable, and privacy-preserving AI systems: DAO_lim
(intent-aware reverse proxy), LiminalDB (bio-inspired adaptive database),
and GardenLiminal (secure container runtime). Together they form a
full-stack building block for deploying autonomous AI agents with
transparency, auditability, and data sovereignty.

## Problem

Modern AI infrastructure lacks trustworthy, open building blocks at every
layer:

- Routing layer: existing proxies such as Nginx and Traefik use static routing
  and are unaware of semantic intent or safety properties of AI backends.
- Storage layer: traditional databases such as Redis or Kafka do not adapt
  autonomously and do not produce a causal audit trail by default.
- Runtime layer: standard container runtimes such as Docker and containerd
  provide limited isolation for untrusted AI workloads and do not offer
  syscall-level filtering with auditable access logs as a first-class feature.

Result: AI systems are often deployed on opaque, non-adaptive, and
non-auditable infrastructure, which undermines trust and data sovereignty.

## Solution: Liminal Stack

Three interoperable open-source components, written in Rust and intended to
ship under permissive libre licenses:

### 1. DAO_lim - Dynamic Awareness Orchestrator

https://github.com/safal207/DAO_lim

Intent-aware reverse proxy for AI backends:

- Routes by p95 latency, error rate, and semantic intent
  (`realtime`, `batch`, `streaming`)
- Resonant load balancing using RPS spikes and backend affinity scoring
- WASM plugin architecture, hot config reload, and Prometheus metrics
- CLI `daoctl`, planned in Rust with `tokio`, `hyper`, and `wasmtime`

### 2. LiminalDB - Bio-Inspired Adaptive Reactive Database

https://github.com/safal207/LiminalBD

Self-tuning reactive database with full audit trail:

- `Cells`: autonomous data units with energy, affinity, and lifecycle states
- `TRS`: PID-style controller that auto-tunes under load without manual
  intervention
- `Mirror Timeline`: append-only event sourcing and replayable audit history
- Rust SDK, TypeScript SDK, and WASM/FFI bridge surfaces

### 3. GardenLiminal - Secure Container Runtime

https://github.com/safal207/GardenLiminal

Lightweight secure runtime for AI workloads:

- `pivot_root` isolation instead of `chroot`
- seccomp BPF syscall filtering with hardened profiles
- capability dropping and `no_new_privs` before seccomp enforcement
- tamper-evident audit trail per container via LiminalDB integration
- authentication layer, DNS isolation, and CNI networking

## Stack architecture

```text
[ Client / AI Agent ]
        |
        v
[ DAO_lim ] - intent-aware routing, WASM plugins
        |
        v
[ GardenLiminal ] - secure isolated container per workload
        |
        v
[ LiminalDB ] - adaptive storage and append-only audit trail
```

## Alignment with NGI Zero Commons Fund

| NGI0 goal | How Liminal Stack addresses it |
|---|---|
| Open source, libre licensed | Permissive licensing across all three repositories |
| Digital commons building blocks | Each component is reusable independently |
| Trustworthy infrastructure | Auditability at routing, execution, and storage layers |
| Data sovereignty | Fully self-hosted deployment path |
| Full-stack approach | Routing, storage, and container runtime in one stack |
| Privacy-preserving | Syscall filtering, capability model, and explicit audit controls |

## Milestones (12 months)

| # | Milestone | Deliverable | Month |
|---|---|---|---|
| 1 | DAO_lim v1.0 stable | gRPC support, circuit breaker, benchmarks vs Traefik | M3 |
| 2 | LiminalDB distributed mode | Replication or consensus milestone, benchmark suite, audit replay hardening | M5 |
| 3 | GardenLiminal security audit | External review, CVE process, hardened profiles v2 | M6 |
| 4 | Stack integration | End-to-end demo: DAO_lim -> GardenLiminal -> LiminalDB | M7 |
| 5 | OpenTelemetry | Unified tracing across all three components | M9 |
| 6 | Docs and community | Docs site, getting started guide, blog posts, conference submission | M12 |

## Budget (EUR 50,000)

| Item | Amount |
|---|---|
| Developer / researcher (12 months, remote) | EUR 36,000 |
| Compute / CI / benchmarks / demo infrastructure | EUR 6,000 |
| External security review | EUR 5,000 |
| Documentation / dissemination / design | EUR 3,000 |

## Notes before submission

- Verify the public repository URLs and stack naming one final time.
- Keep repository-specific license wording exact in each repo brief.
- Replace any aspirational milestone with wording that matches the current
  implementation state if the scope changes before submission.
- Keep the application focused on digital commons, interoperability, and
  trustworthy infrastructure rather than startup positioning.
