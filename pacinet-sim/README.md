# pacinet-sim

Standalone simulator application for exercising pacinet behavior from a dedicated UI.

## What it does

- Registers synthetic nodes in pacinet via gRPC
- Sends heartbeat events with selectable node states
- Reports synthetic rule counters (incremental deltas)
- Runs built-in scenarios:
  - `basic` (single node lifecycle)
  - `burst` (multi-node high traffic)
  - `flap` (rapid state transitions)
  - `canary-traffic` (canary vs stable traffic profile)
- Pulls pacinet REST snapshot (health, fleet, nodes, event history)
- Proxies pacinet SSE streams (nodes, counters, fsm) into simulator UI
- Runs pacgate native scenario commands from UI:
  - `pacgate regress --scenario ... --count ... --json`
  - `pacgate topology --scenario ... --json`

## Run

```bash
cargo run -p pacinet-sim -- \
  --pacinet-grpc http://127.0.0.1:50054 \
  --pacinet-rest http://127.0.0.1:8081
```

Open `http://127.0.0.1:8090`.

If pacinet REST auth is enabled, set:

```bash
PACINET_API_KEY=your-key cargo run -p pacinet-sim
```
