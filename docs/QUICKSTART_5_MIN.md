# LiminalDB Quickstart (5 minutes)

This guide is intentionally minimal: start CLI, send impulses, observe adaptation signals.

## 1) Run interactive CLI

```bash
cd liminal-db
cargo run -p liminal-cli
```

Expected: periodic metrics and hints appear in stdout.

## 2) Send three impulses

In the running CLI, enter:

```text
a cpu/load 0.9
q color/red 0.7
w memory/seed 0.5
```

Meaning:
- `a` = Affect
- `q` = Query
- `w` = Write

## 3) Observe runtime behavior

Look for:
- changing metrics over time,
- hints printed by the loop,
- evidence that cells react and lifecycle transitions occur.

## 4) Optional: CBOR pipe mode

```bash
cd liminal-db
cargo run -p liminal-cli -- --pipe-cbor
```

In this mode, stdin expects one hex-encoded CBOR packet per line and stdout prints hex-encoded responses.

## 5) Next steps

- Protocol details: `docs/PROTOCOL.md`
- Architecture analysis: `docs/ARCHITECTURE_ANALYSIS.md`
- Roadmap: `docs/STRATEGIC_ROADMAP_2025.md`
