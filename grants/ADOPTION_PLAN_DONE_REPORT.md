# Adoption Plan Done Report (30/60/90)

Date: `2026-04-17`  
Repository: `LiminalBD`  
Snapshot commit at report time: `0adf6e5`

## Status

The 30/60/90 adoption implementation plan is complete for all planned tasks:

1. golden path + docs reproducibility
2. benchmark reproducibility + trust messaging
3. contributor onboarding lane
4. lightweight release/compatibility cadence
5. public validation refresh with caveats

## What was shipped

### 1) Golden path and onboarding

- Added Windows single-entrypoint script:
  - `scripts/demo-stack.ps1`
- Aligned onboarding docs:
  - `README.md`
  - `docs/STACK_DEMO.md`
- Added expected output markers and Windows troubleshooting for first-run
  failures.

### 2) Benchmark reproducibility and trust messaging

- Expanded benchmark evidence doc:
  - `docs/BENCHMARKS.md`
- Added:
  - `Reproduce in 3 commands`
  - expected output markers
  - measured-vs-pending matrix
  - public validation refresh section with delta and caveats

### 3) First-contributor lane

- Added structured starter issue set:
  - `docs/FIRST_CONTRIBUTOR_ISSUES.md`
- Linked contributor lane from:
  - `CONTRIBUTING.md`

### 4) Release/compatibility discipline

- Added release and compatibility policy:
  - `docs/RELEASE_COMPATIBILITY.md`
- Linked policy from:
  - `README.md`
  - `liminal-db/docs/PROTOCOL.md`

### 5) Public validation refresh

- Ran the benchmark profile and updated published baseline in:
  - `docs/BENCHMARKS.md`
- Kept limitations explicit:
  - single-machine developer baseline
  - not a soak/multi-node production package

## Grant-facing value

- Faster reviewer onboarding with one canonical path.
- Stronger evidence posture via reproducible measured baseline.
- Clear separation of measured vs pending claims.
- Better external contributor conversion path.
- Lightweight governance for protocol/SDK changes.

## Next recommended step (post-plan)

Create one reviewer-facing summary page that links:

- benchmark baseline (`docs/BENCHMARKS.md`)
- stack demo (`docs/STACK_DEMO.md`)
- first-contributor lane (`docs/FIRST_CONTRIBUTOR_ISSUES.md`)
- release compatibility policy (`docs/RELEASE_COMPATIBILITY.md`)

This can be used as a single entry page in grant and partner reviews.
