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

1. **pacinet-server** — The central controller. Receives agent registrations, stores node state (in-memory or SQLite), handles policy deployment, batch deploy, fleet status, policy history, rollback, and forwards deploy requests to agents. Includes stale node reaper, FSM evaluation engine (background loop for YAML-defined deployment and adaptive policy state machines), counter snapshot cache (in-memory ring buffer for rate tracking), webhook delivery for alert actions, EventBus with broadcast channels for real-time streaming (FSM transitions, counter updates, node lifecycle events), persistent event log (events stored in storage for historical query), leader election for multi-controller HA (lease-based via SQLite), API key authentication on REST endpoints, Prometheus metrics endpoint, gRPC health service, and axum REST API with SSE endpoints for the web dashboard. Supports mTLS on gRPC channels.

6. **pacinet-web** — React SPA dashboard (not a Rust crate). Provides browser-based fleet visibility: dashboard with live metrics (recharts PieChart/LineChart), node management (table/grid views, sortable columns), policy deployment, counter monitoring with rate charts, FSM management, real-time event streaming via SSE, event history browser, and API key authentication prompt. Built with React 19, TypeScript, Vite 6, Tailwind CSS 4, TanStack React Query, React Router DOM 7, and recharts 2. Styled identically to the aida-web-react project (dark/light theme with localStorage persistence, Inter + JetBrains Mono fonts).

2. **pacinet-agent** — Runs on each PacGate node. Registers with the controller on startup, sends periodic heartbeats with retry/backoff, handles rule deployment by invoking the `pacgate` CLI, auto-detects PacGate version, reports counters and CPU usage. Supports graceful shutdown and mTLS.

3. **pacinet-cli** (`pacinet`) — Operator command-line tool. Connects to the controller to list nodes, deploy policies (single or batch), query counters, view fleet status, diff policies, view policy/deployment history, rollback policies, manage FSM definitions and instances (create, start, advance, cancel), and watch live events (FSM transitions, counter updates, node changes). Supports mTLS.

4. **pacinet-core** — Shared domain model (Node, Policy, PolicyVersion, DeploymentRecord, RuleCounter, CounterSnapshot), error definitions, Storage trait for backend abstraction, TLS configuration helpers, unified policy hash function, and YAML-defined FSM types (FsmDefinition, FsmInstance, FsmContext, conditions including counter rate conditions, actions including webhook config).

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

- **Prometheus metrics**: controller exposes `/metrics` on `--metrics-port` (default 9090) with node gauges, deploy counters/histograms, heartbeat counters, uptime, FSM transitions, counter snapshots, webhook deliveries
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
- **reqwest** (rustls-tls) for webhook HTTP delivery
- **async-stream** for server-side streaming RPC and SSE implementation
- **tokio-stream** for stream consumption in CLI
- **axum 0.8** for REST API (web dashboard backend)
- **tower-http 0.6** for CORS and static file serving
- **React 19** + **TypeScript** + **Vite 6** + **Tailwind CSS 4** for web dashboard
- **TanStack React Query 5** for data fetching and caching
- **React Router DOM 7** for SPA routing
- **recharts 2** for interactive charts (PieChart, LineChart)
- **lucide-react** for icons

## Current Status

**Phase 8 complete** — REST tests, auth, event log, HA, dashboard enhancements:
- **REST integration tests**: 17 tests covering all REST endpoints, auth, SSE, event history
- **API key authentication**: optional Bearer token / ?token= query param, React auth prompt on 401
- **Persistent event log**: events stored in SQLite/memory, queryable via /api/events/history with filters
- **Multi-controller HA**: lease-based leader election via SQLite, leader guards on write operations
- **Dashboard enhancements**: recharts PieChart/LineChart, sortable tables, node grid view, event history tab, dark mode persistence
- **mDNS placeholder**: --mdns-discover flag with runtime message (full mdns-sd integration deferred)
- **Health endpoint**: /api/health (no auth required) with status, auth_required, role
- 110 tests (32 core, 30 server unit, 10 agent, 21 integration, 17 REST integration) all passing, clippy clean

Previous phases:
- Phase 7: Web dashboard with REST API, SSE streaming, React SPA
- Phase 6: gRPC server-side streaming, EventBus, CLI watch commands
- Phase 5b: Counter rate tracking & adaptive policy FSMs, webhook delivery
- Phase 5: YAML-defined FSM engine for deployment orchestration
- Phase 4: mTLS security, Prometheus metrics, policy rollback, CI pipeline
- Phase 3: Production resilience, persistence, fleet management, observability
- Phase 2: End-to-end deployment flow and integration tests
- Phase 1: Initial scaffold with 5 workspace crates
