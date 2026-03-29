# LiminalDB Metrics Glossary (v1)

This glossary defines runtime metrics that appear in CLI output, events, and diagnostics.

## Core metrics (`Metrics`)

### `cells`
- **Type:** `usize`
- **Range:** `>= 0`
- **Meaning:** Number of currently alive cells participating in the field.
- **Interpretation:**
  - rising fast may indicate aggressive division,
  - flat or falling values may indicate stabilization or decay.

### `sleeping_pct`
- **Type:** `f32`
- **Range:** `0.0 ..= 1.0`
- **Meaning:** Fraction of cells in sleeping state.
- **Interpretation:**
  - high values imply low active responsiveness,
  - low values imply high active participation.

### `avg_metabolism`
- **Type:** `f32`
- **Range:** project-dependent positive float (typically low single digits)
- **Meaning:** Average metabolic activity across cells.
- **Interpretation:**
  - rising values imply higher resource/energy turnover,
  - very low values can indicate under-stimulation.

### `avg_latency_ms`
- **Type:** `f32`
- **Range:** `>= 0`
- **Meaning:** Average response latency in milliseconds.
- **Interpretation:**
  - lower is usually better for responsiveness,
  - sustained spikes indicate overload or routing pressure.

## Harmony loop metrics (`harmony` event)

### `strength`
- **Type:** `f32`
- **Range:** normalized (implementation-specific, commonly near `0..1`)
- **Meaning:** Aggregate signal strength indicator.

### `latency`
- **Type:** `f32`
- **Range:** milliseconds, `>= 0`
- **Meaning:** Latency component used in harmony status estimation.

### `entropy`
- **Type:** `f32`
- **Range:** normalized, `0..1` in current examples
- **Meaning:** Diversity/disorder indicator of impulse dynamics.

### `status`
- **Type:** enum-like string (`ok`, `drift`, `overload`)
- **Meaning:** Coarse health state derived by symmetry/harmony loop.
- **Interpretation:**
  - `ok`: stable balance,
  - `drift`: deviation emerging,
  - `overload`: corrective action likely required.

## Practical thresholds (starting heuristics)

These are default heuristics for v1 demos, not strict SLA:

- `sleeping_pct > 0.70` + low incoming activity → likely over-sleeping.
- `avg_latency_ms` sustained above baseline x2 → investigate route pressure.
- frequent `status = overload` windows → validate impulse mix and adaptation tuning.

## Notes

- Exact metric dynamics depend on workload profile and tuning.
- Treat this glossary as operational guidance, not fixed theoretical limits.
