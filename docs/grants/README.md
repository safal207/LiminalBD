# Grant Overlays

This directory contains program-specific landing pages layered on top of the main project landing.

Current files:

- `nlnet-commons.html` — overlay for `NGI Zero Commons Fund`
- `_template.html` — starting point for future grant overlays

Naming rule:

- use short, lowercase, hyphenated filenames
- examples:
  - `prototype-fund.html`
  - `security-call.html`
  - `research-fellowship.html`

Pages URL pattern:

- main project landing:
  - `https://safal207.github.io/LiminalBD/`
- grant overlay:
  - `https://safal207.github.io/LiminalBD/grants/<filename>`

Authoring rule:

- keep the main project narrative in `docs/index.html`
- put fund-specific framing, scope, and reviewer emphasis in `docs/grants/*.html`
