# LiminalDB

LiminalDB is a next-generation database: **reactive, adaptive, and explainable**.

LiminalDB — база данных нового поколения: **реактивная, адаптивная и объяснимая**.

## Why LiminalDB

Most databases focus on storing and retrieving state. LiminalDB keeps this foundation and adds runtime adaptation:

- **Storage**: persist impulses, events, and state snapshots.
- **Adaptation**: tune behavior under changing signal/load patterns.
- **Explainability**: expose decisions through metrics, events, and replay.

Большинство БД фокусируются на хранении и чтении состояния. LiminalDB сохраняет эту основу и добавляет адаптацию во время выполнения:

- **Хранение**: фиксация импульсов, событий и снимков состояния.
- **Адаптация**: подстройка поведения под изменяющиеся сигналы и нагрузку.
- **Объяснимость**: прозрачность решений через метрики, события и replay.

## 5-minute quickstart

```bash
cd liminal-db
cargo run -p liminal-cli
```

Then send a few impulses in CLI:

```text
a cpu/load 0.9
q color/red 0.7
w memory/seed 0.5
```

You should see periodic metrics and hints as the field reacts.

## Repository map

- `liminal-db/` — core crates, CLI, bridges, storage, docs.
- `protocol/` — protocol schemas and codegen.
- `examples/` — integration examples.

## Next reading

- Positioning v1: [`docs/POSITIONING_V1.md`](./docs/POSITIONING_V1.md)
- 5-minute walkthrough: [`docs/QUICKSTART_5_MIN.md`](./docs/QUICKSTART_5_MIN.md)
- North Star use-case: [`docs/USE_CASE_NORTH_STAR.md`](./docs/USE_CASE_NORTH_STAR.md)
- Execution backlog (P0/P1/P2): [`docs/IMPLEMENTATION_BACKLOG_P0_P1_P2.md`](./docs/IMPLEMENTATION_BACKLOG_P0_P1_P2.md)
- Core usage and build commands: [`liminal-db/README.md`](./liminal-db/README.md)
- Protocol reference: [`docs/PROTOCOL.md`](./docs/PROTOCOL.md)
- Explainability event contract: [`docs/EXPLAINABILITY_EVENT_CONTRACT.md`](./docs/EXPLAINABILITY_EVENT_CONTRACT.md)
- Metrics glossary: [`docs/METRICS_GLOSSARY.md`](./docs/METRICS_GLOSSARY.md)
- Architecture and roadmap: [`docs/ARCHITECTURE_ANALYSIS.md`](./docs/ARCHITECTURE_ANALYSIS.md), [`docs/STRATEGIC_ROADMAP_2025.md`](./docs/STRATEGIC_ROADMAP_2025.md)
