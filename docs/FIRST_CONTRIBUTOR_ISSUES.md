# First Contributor Issue Set

This page defines five starter issues for first-time contributors. Each issue
has a narrow scope, explicit acceptance criteria, and direct links to context.

## FC-1: README command validation pass

Goal: verify all copy-paste commands in `README.md` and fix any broken command
for Windows + Linux paths where possible.

References:

- `README.md`
- `docs/STACK_DEMO.md`

Acceptance criteria:

- Every command block in `README.md` is executable or marked illustrative.
- Any command requiring platform notes has a short note inline.
- Open a PR with a command-by-command checklist in the description.

## FC-2: BENCHMARKS reproducibility screenshots/log snippets

Goal: improve reviewer confidence by adding short output examples to benchmark
reproduction flow.

References:

- `docs/BENCHMARKS.md`
- `sdk/rust/examples/live-benchmark.rs`

Acceptance criteria:

- Add one compact “sample output” block under the 3-command section.
- Keep measured-vs-pending labels unchanged and explicit.
- Do not introduce claims not backed by measured output.

## FC-3: TypeScript SDK quickstart validation

Goal: validate and document a minimal TS client flow that connects, sends one
command, and reads one event.

References:

- `sdk/ts/src/client.ts`
- `README.md`
- `liminal-db/docs/PROTOCOL.md`

Acceptance criteria:

- Add a short TS quickstart section in docs (or improve existing one) with
  install + run steps.
- Commands are tested at least once against a local `liminal-cli`.
- Mention expected success signal and one common failure mode.

## FC-4: Windows troubleshooting expansion

Goal: reduce first-run failure rate on Windows by documenting 2-3 common
errors and exact fixes.

References:

- `docs/WINDOWS_RUST.md`
- `docs/STACK_DEMO.md`
- `scripts/windows-msvc-override.ps1`

Acceptance criteria:

- Add troubleshooting entries for linker/toolchain mismatch and CLI stdin
  behavior.
- Include copy-paste commands for diagnosis and fix.
- Keep language concise and action-oriented.

## FC-5: CI docs command smoke check

Goal: add a lightweight CI job that validates key documented commands still
parse/run (smoke level, non-benchmark-heavy).

References:

- `.github/workflows/ci.yml`
- `README.md`
- `docs/BENCHMARKS.md`

Acceptance criteria:

- New CI step validates at least build + one docs-referenced command path.
- Job runtime increase stays modest.
- Failing smoke check clearly points to the broken docs path.

## How to use this list

- Open each item as a GitHub issue with `good first issue` label.
- Keep one issue = one PR scope.
- Link the acceptance criteria directly in the issue body.
