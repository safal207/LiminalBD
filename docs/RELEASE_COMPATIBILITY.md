# Release and Compatibility Notes

This document defines a lightweight release discipline for protocol and SDK
changes in LiminalDB.

## Release cadence (lightweight)

- Use tagged releases for externally visible changes.
- Keep release notes short and structured:
  - changed
  - compatibility impact
  - upgrade notes
  - benchmark/reliability impact (if applicable)

## Compatibility levels

For each protocol/SDK change, classify impact explicitly:

- `Compatible`: existing clients continue to work unchanged.
- `Additive`: new fields/events/commands added; old behavior remains.
- `Breaking`: old client behavior may fail or change semantics.

## Required notes for protocol/SDK PRs

Every PR touching protocol or SDK surfaces should include:

1. Compatibility level (`Compatible` / `Additive` / `Breaking`).
2. Affected files and client surfaces.
3. Migration note (what users must do, if anything).
4. Test/validation evidence (manual or CI smoke).

## Changelog snippet template

```markdown
### Protocol/SDK
- Change: <what changed>
- Compatibility: <Compatible|Additive|Breaking>
- Migration: <none|steps>
- Validation: <command(s) or test(s)>
```

## Breaking change policy

- Breaking changes require:
  - explicit release-note callout,
  - migration steps,
  - at least one example update in docs.

## Where to link this policy

- `README.md` (review links section)
- `liminal-db/docs/PROTOCOL.md` (top-level reference)
- PR descriptions affecting protocol/SDK code
