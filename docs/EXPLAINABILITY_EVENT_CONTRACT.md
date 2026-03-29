# LiminalDB Explainability Event Contract (v1)

Goal: define a stable, pragmatic event envelope for adaptation/explainability flows.

## Canonical event envelope

```json
{
  "ev": "harmony",
  "ts_ms": 1712134415123,
  "source": "liminal-core",
  "scope": "cluster/global",
  "trace_id": "2dd84f1f-77f3-4c52-9cd1-21f8b1abf6ce",
  "meta": {
    "status": "drift",
    "strength": 0.51,
    "latency": 110.0,
    "entropy": 0.72
  },
  "decision": {
    "kind": "rebalance",
    "target": "routing",
    "impact": "medium"
  },
  "reason": [
    "latency_above_baseline",
    "entropy_growth"
  ]
}
```

## Required top-level fields

- `ev` (`string`): event name (`harmony`, `trs_trace`, `awaken`, `introspect`, ...).
- `ts_ms` (`u64`): unix timestamp in milliseconds.
- `source` (`string`): subsystem producing event.
- `scope` (`string`): logical scope (`cluster/global`, `node/<id>`, `model/<id>`).
- `meta` (`object`): event-specific measurable payload.

## Optional top-level fields

- `trace_id` (`string`): correlation ID for multi-step flows.
- `decision` (`object`): adaptation action summary (`kind`, `target`, `impact`).
- `reason` (`array[string]`): machine-readable explanation tokens.

## Event naming convention

- lower snake case for event names.
- nouns for state snapshots (`harmony`, `introspect`).
- noun + suffix for trace streams (`trs_trace`).

## Compatibility rules

1. New optional fields may be added without version bump.
2. Required field removal or rename requires protocol version bump.
3. Consumers must ignore unknown fields.

## Minimal validation checklist

- Every explainability event includes: `ev`, `ts_ms`, `source`, `scope`, `meta`.
- If adaptation occurs, include at least one of `decision` or `reason`.
- If event is emitted inside a larger scenario, include `trace_id`.
