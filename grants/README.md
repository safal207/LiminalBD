# LiminalBD Grant Readiness

## Positioning

LiminalBD is the storage and evidence layer of the Liminal Stack: a reactive,
audit-friendly database with mirror timeline replay, protocol surfaces, and SDK
access paths.

## Why it fits NGI Zero Commons Fund

- infrastructure component with reuse across downstream applications
- emphasis on auditability and replay rather than closed platform lock-in
- protocol and SDK surfaces already present in the repository
- useful independently or as part of the larger stack

## Grant-facing strengths visible in the repository

- protocol documentation under [docs/protocol.md](../docs/protocol.md)
- SDK structure under [sdk](../sdk)
- bridge and interoperability work across protocol and runtime layers
- existing [GRANT_BRIEF.md](../GRANT_BRIEF.md)

## Licensing status

The repository root license is now aligned with the open-source grant story.

- The root [LICENSE](../LICENSE) is `Apache-2.0`.
- Existing subpackages that declare `Apache-2.0` are now directionally
  consistent with the repository root.

## Recommended next fixes before submission

- add an explicit root-level note in the README that the repository is licensed
  under `Apache-2.0`
- do one pass over all subpackages to confirm no stale proprietary wording
  remains in docs
