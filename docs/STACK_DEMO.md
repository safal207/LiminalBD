# Liminal Stack Demo

This document describes a minimal end-to-end demo for the current Liminal
Stack:

```text
DAO_lim -> GardenLiminal -> LiminalDB
```

The goal is to give reviewers one short, reproducible scenario showing how the
three repositories fit together today.

## Canonical entrypoint (Windows)

From `LiminalBD` root:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\demo-stack.ps1
```

This script prints the exact server and benchmark commands used by the current
baseline and expected output markers.

## Repositories

- DAO_lim: https://github.com/safal207/DAO_lim
- LiminalBD: https://github.com/safal207/LiminalBD
- GardenLiminal: https://github.com/safal207/GardenLiminal

## 1. Start LiminalDB

From this repository:

```bash
cargo build --release -p liminal-cli
./target/release/liminal-cli --store ./data --ws-port 8787
```

Useful reviewer commands:

```text
:status
:mirror top 10
```

Expected outcome:

- WebSocket endpoint available on `ws://127.0.0.1:8787`
- live state visible in the CLI
- recent events visible through Mirror Timeline

Troubleshooting:

- If the process exits immediately on Windows, run with stdin keepalive:
  `cmd /c "ping -t 127.0.0.1 | .\target\x86_64-pc-windows-msvc\release\liminal-cli.exe --store .\benchmark-data --ws-port 8787"`.
- If benchmark cannot connect (`os error 10061`), verify `ws_server.listening`
  appears in server logs before starting the client.

## 2. Run GardenLiminal against the LiminalDB endpoint

From the `GardenLiminal` repository:

```bash
cargo build --release
LIMINAL_URL=ws://127.0.0.1:8787 \
  sudo -E ./target/release/gl run -f examples/seed-busybox.yaml --store liminal
```

This should emit runtime lifecycle events into the running LiminalDB instance.

Optional helper:

```bash
./examples/demo-liminaldb.sh
```

## 3. Inspect the resulting audit trail

Back in the `liminal-cli` session:

```text
:mirror top 20
```

Expected outcome:

- recent GardenLiminal lifecycle events are visible
- the event trail is replayable inside LiminalDB

If `websocat` is available:

```bash
echo '{"cmd":"mirror.timeline","top":20}' | websocat -n1 ws://127.0.0.1:8787
```

## 4. Start DAO_lim

From the `DAO_lim` repository:

```bash
cargo build --release
./target/release/dao --config configs/dao.toml
```

This exposes:

- admin API on `127.0.0.1:9103`
- Prometheus metrics on `0.0.0.0:9102`

## 5. Inspect DAO routing decisions

From another terminal in `DAO_lim`:

```bash
./target/release/daoctl health
./target/release/daoctl upstreams
./target/release/daoctl explain \
  --host api.example.com \
  --path /v1/chat \
  --intent realtime
```

Expected outcome:

- DAO health is visible through the admin surface
- upstream state is inspectable
- a routing decision can be explained explicitly

## What this demo proves

This demo is intended as evidence of stack composition, not as a claim that the
entire stack is already production-integrated end-to-end.

What it demonstrates today:

- LiminalDB provides a replayable evidence layer
- GardenLiminal can emit runtime events into that layer
- DAO_lim exposes an inspectable control and routing layer

## Recommended next improvement

The next stronger demo would route a live request through DAO_lim into a
GardenLiminal-managed service and store the resulting lifecycle trail in
LiminalDB as one continuous scenario.
