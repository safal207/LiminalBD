# Architecture Decision Records (ADR)

This directory contains Architecture Decision Records (ADRs) for LiminalDB.

## What is an ADR?

An Architecture Decision Record (ADR) is a document that captures an important architectural decision made along with its context and consequences.

## Format

We use the format defined in [ADR-000: Template](000-template.md).

## Index

| ADR | Title | Status | Date |
|-----|-------|--------|------|
| [000](000-template.md) | ADR Template | Template | N/A |
| [001](001-biological-metaphor.md) | Use Biological Metaphor for System Architecture | Accepted | 2024-Q3 |
| [002](002-event-sourcing.md) | Event Sourcing Architecture | Proposed | TBD |
| [003](003-pid-control.md) | PID-Based Adaptive Control (TRS) | Proposed | TBD |
| [004](004-pattern-routing.md) | Pattern-Based Routing with Affinity | Proposed | TBD |

## Creating a New ADR

1. Copy `000-template.md` to a new file with the next number
2. Fill in all sections
3. Submit PR for review
4. Update this index when merged

## Lifecycle

- **Proposed:** Decision is being considered
- **Accepted:** Decision is approved and being implemented
- **Deprecated:** Decision is no longer recommended but not yet replaced
- **Superseded:** Decision has been replaced (link to new ADR)

## Principles

1. **Document significant decisions:** If it affects architecture, document it
2. **Capture context:** Future readers need to know WHY we decided
3. **Consider alternatives:** Show we evaluated options
4. **Be honest about trade-offs:** Every decision has costs
5. **Keep it concise:** 2-3 pages maximum
6. **Update when changed:** If decision evolves, create new ADR

## Philosophy

> "The Dharma of decisions is impermanence. What seems right today may not be right tomorrow. Document the path, not just the destination."

ADRs embody the **Middle Way**:
- Not too rigid (detailed spec docs)
- Not too informal (Slack messages)
- Just right (structured but readable)

---

**Questions?** See [000-template.md](000-template.md) for detailed guidance.
