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
                    │          │                               │
                    │     ┌────▼────────────────────┐         │
                    │     │   Prometheus Metrics     │         │
                    │     │   (:9090/metrics)        │         │
                    │     └────────────────────────┘          │
                    └─────────────────────────────────────────┘
                              ▲                    │
                    Register/ │                    │ Deploy/
                    Heartbeat │                    │ GetStatus
                    (mTLS)    │                    │ (mTLS)
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

1. **pacinet-server** — The central controller. Receives agent registrations, stores node state (in-memory or SQLite), handles policy deployment, batch deploy, fleet status, policy history, rollback, and forwards deploy requests to agents. Includes stale node reaper, FSM evaluation engine (background loop for YAML-defined deployment state machines), Prometheus metrics endpoint, and gRPC health service. Supports mTLS.

2. **pacinet-agent** — Runs on each PacGate node. Registers with the controller on startup, sends periodic heartbeats with retry/backoff, handles rule deployment by invoking the `pacgate` CLI, auto-detects PacGate version, reports counters and CPU usage. Supports graceful shutdown and mTLS.

3. **pacinet-cli** (`pacinet`) — Operator command-line tool. Connects to the controller to list nodes, deploy policies (single or batch), query counters, view fleet status, diff policies, view policy/deployment history, rollback policies, and manage FSM definitions and instances (create, start, advance, cancel). Supports mTLS.

4. **pacinet-core** — Shared domain model (Node, Policy, PolicyVersion, DeploymentRecord, RuleCounter), error definitions, Storage trait for backend abstraction, TLS configuration helpers, unified policy hash function, and YAML-defined FSM types (FsmDefinition, FsmInstance, FsmContext, conditions, actions).

5. **pacinet-proto** — Generated gRPC/protobuf types from `proto/pacinet.proto`.

## Interface with PacGate

PaciNet is fully decoupled from PacGate internals. The agent invokes `pacgate` as a subprocess:
- Writes received YAML rules to a temp file
- Runs `pacgate compile <path> [--counters] [--json]`
- Parses the result
- Reports success/failure back to the controller

YAML is the interface contract between the two systems.

## Security

PaciNet supports mutual TLS (mTLS) on all gRPC channels:
- **Agent → Controller**: agent authenticates to controller with client cert
- **Controller → Agent**: controller authenticates to agent when pushing deploys
- **CLI → Controller**: CLI authenticates with client cert

TLS is optional — all three flags (`--ca-cert`, `--tls-cert`, `--tls-key`) must be provided to enable mTLS; otherwise connections are plaintext for development convenience.

Development certificates can be generated with `make gen-certs` (requires openssl).

## Observability

- **Prometheus metrics**: controller exposes `/metrics` on `--metrics-port` (default 9090) with node gauges, deploy counters/histograms, heartbeat counters, uptime
- **Structured logging**: via tracing with EnvFilter (`RUST_LOG` environment variable)
- **gRPC health checks**: via tonic-health
- **Policy audit trail**: every deployment recorded with result and version

## Technology Stack

- **Rust** (edition 2021)
- **tonic 0.12** / **prost 0.13** for gRPC (with optional TLS via rustls)
- **tokio** async runtime
- **clap 4** for CLI
- **tracing** for structured logging with EnvFilter
- **rusqlite** (bundled) for persistent storage
- **metrics** + **metrics-exporter-prometheus** for Prometheus metrics
- **tonic-health** for gRPC health checks
- **tonic-web** for gRPC-Web support
- **similar** for policy diff

## Current Status

**Phase 5 complete** — YAML-defined FSM engine for deployment orchestration:
- **FSM definitions**: YAML-parsed deployment state machines (canary, staged, rollback strategies)
- **FSM engine**: background evaluation loop (5s interval) with condition-driven, timer, and manual transitions
- **FSM actions**: deploy (with node selector, batch percent, compile options), rollback, alert (log-only)
- **9 FSM RPCs**: CRUD for definitions + start/get/list/advance/cancel for instances
- **CLI `fsm` subcommand**: create, list, show, delete definitions; start, status, instances, advance, cancel
- **Shared deploy module**: refactored deploy logic reused by both ManagementService and FsmEngine
- **FSM storage**: JSON blob storage in both MemoryStorage and SqliteStorage
- **FSM metrics**: transition counter, instance status counter, running instances gauge
- 68 tests (27 core, 18 server storage, 10 agent, 13 integration) all passing, clippy clean

Previous phases:
- Phase 4: mTLS security, Prometheus metrics, policy rollback, CI pipeline
- Phase 3: Production resilience, persistence, fleet management, observability
- Phase 2: End-to-end deployment flow and integration tests
- Phase 1: Initial scaffold with 5 workspace crates
