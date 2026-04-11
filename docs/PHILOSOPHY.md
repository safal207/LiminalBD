# LiminalDB — Philosophy & Inspiration

This document captures the intellectual and philosophical roots of LiminalDB.
It is intentionally separate from the engineering documentation so that each
audience can engage at the depth they prefer.

---

## The Middle Way (Madhyamaka)

Traditional databases sit at one of two extremes:

```
Too Rigid (SQL)          Too Chaotic (some NoSQL)
─────────────────        ──────────────────────────
Fixed schema             No structure
ACID everywhere          Eventual consistency only
Manual tuning            Unpredictable behaviour

                   LiminalDB
                   ──────────
              Flexible patterns
          Adaptive consistency (TRS)
           Self-regulating (PID)
```

The *Middle Way* is not a compromise — it is a third path that transcends both
extremes by building adaptability into the core.

---

## Three Jewels mapped to system design

| Jewel | Engineering equivalent |
|-------|------------------------|
| **Buddha** (Awareness) | Metrics & observability — `live_load`, `harmony`, `symmetry` |
| **Dharma** (Teaching) | Biological principles — energy, metabolism, adaptation |
| **Sangha** (Community) | Distributed cells — autonomous yet interdependent |

---

## Three Marks of Existence (Trilakshana)

### 1. Anicca — Impermanence
Nothing in the system is static:
- Cell states transition: Active → Idle → Sleep → Dead
- Energy rises and falls with each tick
- Affinity drifts through genetic mutation

### 2. Dukkha — Stress
When the system deviates from equilibrium it experiences "stress":
- `live_load > 0.85` triggers reflex signals
- TRS measures the error and applies corrective force
- The goal is not to eliminate change but to respond to it gracefully

### 3. Anatta — Non-Self
No cell has a fixed, inherent identity:
- `affinity` changes with each mutation
- `energy` fluctuates constantly
- `state` is always transitional

---

## Dependent Origination (Pratītyasamutpāda)

Everything arises in dependence on everything else:

```
Impulse → Cell → Energy → State → Division → Population
   ▲                                              │
   └──────────────────────────────────────────────┘

Metrics → TRS → Tick Rate → Load → Metrics
   ▲                                    │
   └────────────────────────────────────┘
```

No component is independent. The system is a web of mutual causation.

---

## Scientific Foundations

The philosophical framing maps directly onto established science:

| Concept | Scientific counterpart |
|---------|----------------------|
| Cell energy & metabolism | Bioenergetics, ATP cycles |
| Affinity & pattern matching | Receptor-ligand binding |
| Cell division | Mitosis triggered by growth signals |
| Sleep/wake cycles | Circadian rhythm, neural inhibition |
| PID control (TRS) | Classical control theory |
| Reflex arcs | Spinal reflex, autonomic nervous system |
| Event sourcing / Mirror | Episodic memory, long-term potentiation |

---

## Acknowledgements

LiminalDB draws inspiration from:

- **The Liberman Brothers** — neuroscience and systems thinking
- **Padmasambhava** — transmission of wisdom across contexts
- **Control theory pioneers** — Ziegler, Nichols, Åström
- **Domain-driven design** — Eric Evans
- **Rust community** — enabling fearless systems programming

---

*"The journey of a thousand miles begins with a single step."* — Lao Tzu

*May all beings benefit from this work.* 🙏
