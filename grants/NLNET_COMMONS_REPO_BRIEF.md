# LiminalBD - Commons Fund Repository Brief

## Repository

- Project: `LiminalDB` (repository: `LiminalBD`)
- URL: https://github.com/safal207/LiminalBD
- Role in stack: storage and evidence layer
- License: `Apache-2.0`

## Positioning

LiminalBD is the memory and audit layer of the Liminal Stack. It frames data
handling around adaptive behaviour, replayable history, and explicit protocol
surfaces rather than opaque storage internals.

For NLnet, the strongest story is that LiminalBD is a reusable digital commons
component for transparent state handling. It can support downstream systems
that need auditability, reproducibility, and open protocol access without
vendor lock-in.

## What reviewers should notice

- append-only mirror timeline for replayable decision history
- adaptive control model (`TRS`) for self-tuning behaviour
- CLI, protocol, Rust SDK, TypeScript SDK, and bridge surfaces
- documentation depth across architecture, ADRs, and protocol design
- Apache-2.0 licensing aligned with open infrastructure funding

## Proposed grant-facing scope

Within the shared stack application, LiminalBD is the work package for:

- distributed mode and replication/consensus work
- benchmark suite for larger cell counts and replay workloads
- audit replay hardening and traceability improvements
- shared telemetry integration across stack components

## Submission notes

- Use `LiminalDB` as the project name and `LiminalBD` as the repository slug.
- Published live baseline numbers live in `docs/BENCHMARKS.md`; keep README
  targets clearly labelled as design targets where they are not yet validated
  at scale.
- Use this brief together with `NLNET_COMMONS_APPLICATION.md` for submission.
- For a paste-ready stack-wide narrative, see `STACK_EVIDENCE_SNAPSHOT_EN.md`
  and `STACK_EVIDENCE_SNAPSHOT_RU.md` in this directory.
