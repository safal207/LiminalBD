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

## 4) Pass/fail checkpoint (v1)

Pass if within ~30 seconds you observe both:

1. at least one metrics/hints update after sending impulses,
2. at least one visible change in system state indicators (e.g., activity/load/hints evolution).

If neither appears, treat quickstart as failed and run troubleshooting below.

## 5) Troubleshooting

- Ensure you are inside `liminal-db/` before running CLI.
- Ensure Rust toolchain is installed (`rustc --version`, `cargo --version`).
- If build fails, run:

```bash
cargo build --workspace
```

- If no reactions after inputs, retry with stronger affect impulses:

```text
a service/auth/error 1.0
a service/auth/error 1.0
a service/auth/error 1.0
```

## 6) Optional: CBOR pipe mode

```bash
cd liminal-db
cargo run -p liminal-cli -- --pipe-cbor
```

In this mode, stdin expects one hex-encoded CBOR packet per line and stdout prints hex-encoded responses.

## 7) Next steps

- North Star scenario: `docs/USE_CASE_NORTH_STAR.md`
- Metrics glossary: `docs/METRICS_GLOSSARY.md`
- Protocol details: `docs/PROTOCOL.md`
- Architecture analysis: `docs/ARCHITECTURE_ANALYSIS.md`
- Roadmap: `docs/STRATEGIC_ROADMAP_2025.md`
