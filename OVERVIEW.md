# PaciNet — SDN Controller for PacGate FPGA Packet Filters

## Vision

PaciNet is the **control plane** companion to PacGate (the data plane). While PacGate compiles YAML firewall rules into FPGA-synthesizable Verilog for individual nodes, PaciNet manages fleets of PacGate nodes across a network — providing centralized registration, policy deployment, health monitoring, and counter aggregation.

Think of it as: PacGate is to a single firewall what PaciNet is to an SDN controller managing many firewalls.

## Architecture

PaciNet follows a classic SDN controller architecture:

```
                    ┌─────────────────────────────────────────┐
                    │            PaciNet Controller            │
                    │         (pacinet-server :50054)          │
                    │                                          │
 Northbound API ──▶ │  ┌──────────────┐  ┌────────────────┐  │ ◀── Southbound API
 (CLI/Web)          │  │ Management   │  │ Controller     │  │     (Agents)
                    │  │ Service      │  │ Service        │  │
                    │  └──────────────┘  └────────────────┘  │
                    │          │                │              │
                    │     ┌────▼────────────────▼────┐        │
                    │     │   Storage Backend         │        │
                    │     │   (Memory or SQLite)      │        │
                    │     └─────────────────────────┘         │
                    └─────────────────────────────────────────┘
                              ▲                    │
                    Register/ │                    │ Deploy/
                    Heartbeat │                    │ GetStatus
                              │                    ▼
                    ┌─────────────────┐   ┌─────────────────┐
                    │  PaciNet Agent  │   │  PaciNet Agent  │
                    │  (node-1:50055) │   │  (node-2:50055) │
                    │    ┌─────────┐  │   │    ┌─────────┐  │
                    │    │ PacGate │  │   │    │ PacGate │  │
                    │    │  CLI    │  │   │    │  CLI    │  │
                    │    └─────────┘  │   │    └─────────┘  │
                    └─────────────────┘   └─────────────────┘
```

### Key Components

1. **pacinet-server** — The central controller. Receives agent registrations, stores node state (in-memory or SQLite), handles policy deployment, batch deploy, fleet status, and forwards deploy requests to agents. Includes stale node reaper and gRPC health service.

2. **pacinet-agent** — Runs on each PacGate node. Registers with the controller on startup, sends periodic heartbeats with retry/backoff, handles rule deployment by invoking the `pacgate` CLI, auto-detects PacGate version, and reports counters.

3. **pacinet-cli** (`pacinet`) — Operator command-line tool. Connects to the controller to list nodes, deploy policies (single or batch), query counters, view fleet status, and diff policies between nodes.

4. **pacinet-core** — Shared domain model (Node, Policy, PolicyVersion, DeploymentRecord, RuleCounter), error definitions, and the Storage trait for backend abstraction.

5. **pacinet-proto** — Generated gRPC/protobuf types from `proto/pacinet.proto`.

## Interface with PacGate

PaciNet is fully decoupled from PacGate internals. The agent invokes `pacgate` as a subprocess:
- Writes received YAML rules to a temp file
- Runs `pacgate compile <path> [--counters] [--json]`
- Parses the result
- Reports success/failure back to the controller

YAML is the interface contract between the two systems.

## Technology Stack

- **Rust** (edition 2021)
- **tonic 0.12** / **prost 0.13** for gRPC
- **tokio** async runtime
- **clap 4** for CLI
- **tracing** for structured logging with EnvFilter
- **rusqlite** (bundled) for persistent storage
- **tonic-health** for gRPC health checks
- **similar** for policy diff

## Current Status

**Phase 3 complete** — production resilience, persistence, fleet management, and observability:
- **Storage abstraction**: Storage trait with MemoryStorage and SqliteStorage backends
- **State machine validation**: enforced valid transitions, concurrent deploy protection
- **Policy versioning**: version history and deployment audit trail
- **Fleet management**: batch deploy by label, fleet status with enriched node data
- **Agent resilience**: bind address fix, connection reuse, heartbeat retry with exponential backoff, PacGate version detection
- **Stale node reaper**: background task marks nodes Offline after missed heartbeats
- **Configurable**: deploy timeout, heartbeat interval/threshold via CLI flags
- **gRPC health service**: via tonic-health
- **CLI enhancements**: batch deploy output, fleet status, policy diff, enriched node list
- 26 tests (5 model+core, 5 pacgate, 9 storage, 7 integration) all passing, clippy clean
