# PaciNet — SDN Controller for PacGate

## Feature Summary
- **SDN controller** managing multiple PacGate FPGA packet filter nodes
- **gRPC-based** architecture: controller (southbound + northbound), agent, CLI
- **Web dashboard**: React SPA with REST API, SSE real-time streaming, fleet visualization, recharts, auth
- **API authentication**: optional API key auth (Bearer header + ?token= query param for SSE)
- **Persistent event log**: events stored in SQLite/memory, queryable via REST with type/source/time filters
- **Multi-controller HA**: lease-based leader election via SQLite, leader guards on write operations
- **Node lifecycle**: registration, heartbeat, policy deployment, counter collection
- **End-to-end deployment**: CLI → Controller → Agent → PacGate
- **YAML-defined FSM engine**: operator-defined deployment state machines (canary, staged, rollback)
- **Adaptive policy FSMs**: counter-rate-driven state machines for automated escalation/de-escalation (e.g., DDoS mitigation)
- **Counter rate tracking**: in-memory ring buffer of counter snapshots, rate calculation, multi-node aggregation (any/all/sum)
- **Webhook alerts**: FSM alert actions deliver JSON payloads via HTTP webhooks with bearer/basic auth and retry, delivery history tracked
- **Node annotations**: key-value metadata on nodes for operator notes (tickets, env, maintenance tags)
- **Audit logging**: all write operations tracked with actor, action, resource details; queryable
- **Policy templates**: named reusable YAML templates with tags, CRUD and deploy-from-template
- **Dry-run deploy**: validate and preview without executing — hash diff, per-node change detection
- **Server-side streaming**: WatchFsmEvents, WatchCounters, WatchNodeEvents for real-time event observation
- **SSE streaming**: REST API endpoints for browser-based real-time event observation
- **EventBus**: broadcast channels (FSM, counter, node) for decoupled event emission and streaming delivery
- **FSM orchestration**: background evaluation loop, condition-driven transitions, timer transitions, manual advance
- **PacGate integration**: agent invokes `pacgate` CLI as subprocess (YAML interface)
- **PacGateBackend abstraction**: Real (subprocess) or Mock (for testing)
- **Storage abstraction**: In-memory (default) or SQLite (persistent via `--db`)
- **Fleet management**: batch deploy by label, fleet status with enriched node data
- **Production resilience**: configurable timeouts, retry with backoff, stale node detection
- **Policy versioning**: full version history, deployment audit trail, rollback
- **State machine validation**: enforced valid transitions, concurrent deploy protection
- **mTLS security**: optional mutual TLS on all gRPC channels (server, agent, CLI)
- **Prometheus metrics**: operational metrics on configurable HTTP endpoint
- **Graceful shutdown**: signal handling, connection draining, heartbeat loop cancellation
- **Health checks**: gRPC health service via tonic-health, REST /api/health endpoint
- **CI pipeline**: GitHub Actions (check, clippy, test, fmt); no external `protoc` dependency

## Architecture
```
┌──────────┐    northbound    ┌────────────────┐    southbound    ┌──────────────┐
│ CLI      │───────gRPC──────▶│ Controller     │◀───────gRPC──────│ Agent        │
│ (pacinet)│   (mTLS opt.)    │ (pacinet-server)│───────gRPC──────▶│ (per node)   │
└──────────┘                  └───────┬────────┘   (mTLS opt.)    └──────┬───────┘
                                      │                                  │
┌──────────┐     REST/SSE     ┌───────▼────────┐                  ┌─────▼───────┐
│ Browser  │────HTTP :8081───▶│ axum REST API  │                  │ PacGate CLI │
│ (React)  │                  │ + static files │                  │ (subprocess)│
└──────────┘                  └───────┬────────┘                  └─────────────┘
                                      │
                              ┌───────▼────────┐
                              │ Prometheus     │
                              │ :9090/metrics  │
                              └────────────────┘
```

### Workspace Crates
| Crate | Type | Purpose |
|-------|------|---------|
| `pacinet-proto` | lib | Generated gRPC/protobuf types |
| `pacinet-core` | lib | Domain model, error types, Storage trait, TLS helpers, hash util, FSM types |
| `pacinet-server` | lib+bin | Controller (gRPC :50054, REST :8081) |
| `pacinet-agent` | lib+bin | Node agent (port 50055) |
| `pacinet-cli` | bin | Operator CLI (`pacinet`) |
| `pacinet-web` | npm | React SPA dashboard (Vite dev :5174) |

### gRPC Services
- **PaciNetController** (agent → controller): RegisterNode, Heartbeat, ReportCounters
- **PaciNetAgent** (controller → agent): DeployRules, GetCounters, GetStatus
- **PaciNetManagement** (CLI → controller): ListNodes, GetNode, RemoveNode, DeployPolicy, GetPolicy, GetNodeCounters, GetAggregateCounters, BatchDeployPolicy, GetFleetStatus, GetPolicyHistory, GetDeploymentHistory, RollbackPolicy, CreateFsmDefinition, GetFsmDefinition, ListFsmDefinitions, DeleteFsmDefinition, StartFsm, GetFsmInstance, ListFsmInstances, AdvanceFsm, CancelFsm, WatchFsmEvents (stream), WatchCounters (stream), WatchNodeEvents (stream), SetNodeAnnotations, QueryAuditLog, CreatePolicyTemplate, GetPolicyTemplate, ListPolicyTemplates, DeletePolicyTemplate, QueryWebhookDeliveries

## Common Commands
```bash
cargo build                    # Build all crates
cargo test                     # Run all unit + integration tests
make run-server                # Start controller on :50054 (in-memory)
make run-server-sqlite         # Start controller on :50054 (SQLite)
make run-server-tls            # Start with mTLS + SQLite + metrics
make run-server-web            # Start with web dashboard on :8081
make run-server-auth           # Start with web dashboard + API key auth
make run-server-ha             # Start with HA leader election (SQLite required)
make run-agent                 # Start agent, connect to controller (plain)
make run-agent-tls             # Start agent with mTLS
make gen-certs                 # Generate dev TLS certificates
make node-list                 # List nodes via CLI
make integration-test          # Run integration tests only
make rest-test                 # Run REST integration tests only
make test-all                  # Run tests + clippy
make web-install               # Install React app dependencies
make web-dev                   # Start Vite dev server on :5174
make web-build                 # Build React app to pacinet-web/dist/
```

## Key Design Decisions
- **tonic 0.12 + prost 0.13** for gRPC; **protox 0.7** (pure-Rust) for `.proto` parsing — no external `protoc` binary needed
- **Storage trait** (`Arc<dyn Storage>`) for backend abstraction
- **MemoryStorage** — in-memory with RwLock (default, for dev/test)
- **SqliteStorage** — rusqlite with WAL mode (for production persistence)
- **PacGate subprocess** via tokio::process::Command — YAML is the interface contract
- **PacGateBackend enum** (Real|Mock) for testability without PacGate binary
- **Deploy forwarding**: controller connects to agent gRPC, configurable timeout, graceful failure
- **Node state transitions**: validated (Registered→Online→Deploying→Active/Error)
- **Concurrent deploy protection**: begin_deploy/end_deploy guard per node
- **Stale node reaper**: background task marks nodes Offline after missed heartbeats
- **Policy versioning**: every deploy creates a PolicyVersion record
- **Deployment audit trail**: DeploymentRecord with result enum
- **mTLS**: optional on all channels via --ca-cert/--tls-cert/--tls-key flags; backward compatible (plain HTTP when absent)
- **Prometheus metrics**: `metrics` + `metrics-exporter-prometheus` crates; separate HTTP endpoint on --metrics-port
- **Unified hash**: `pacinet_core::policy_hash()` (SipHash) shared across server and agent
- **Graceful shutdown**: tokio::signal::ctrl_c() + watch channel for heartbeat loop + serve_with_shutdown
- **FSM definitions**: YAML-parsed via serde_yaml, validated for consistency (initial state exists, transition targets valid, terminal states have no transitions)
- **FSM engine**: background eval loop (5s interval), condition evaluation (Simple/Counter/Compound), timer transitions, deploy action execution via shared deploy module
- **Counter condition evaluation**: rate from snapshot pairs, aggregate modes (any/all/sum), `for_duration` sustained threshold tracking via `counter_condition_first_true` HashMap
- **Counter snapshot cache**: in-memory ring buffer (`CounterSnapshotCache`) per node, configurable retention (default 1h) and max snapshots (default 120), evicted by reaper
- **Webhook delivery**: `reqwest` with rustls-tls, bearer/basic auth, custom headers, exponential backoff retry (max 2), fire-and-forget via `tokio::spawn`
- **ConditionDefinition enum ordering**: Counter, Simple, Compound — critical for `serde(untagged)` deserialization (Counter has required `counter` field, Simple before Compound to prevent all-optional Compound matching first)
- **FSM storage**: JSON blob storage in both MemoryStorage and SqliteStorage
- **ActionDefinition as struct**: uses optional fields (deploy/rollback/alert) rather than enum due to serde_yaml 0.9 tag requirements
- **EventBus**: wraps three `tokio::sync::broadcast` channels (FSM, counter, node); created once in main.rs, cloned into services; `Option<EventBus>` for backward compatibility
- **Server-side streaming**: `async_stream::try_stream!` macro with `Pin<Box<dyn Stream>>` return type; `RecvError::Lagged` warns and continues, `RecvError::Closed` breaks
- **Domain→proto event conversion**: separate helper functions per event type to keep streaming RPCs clean
- Proto types do NOT have serde derives (prost_types::Timestamp incompatibility)
- Domain types in pacinet-core have serde derives for JSON serialization
- Both server and agent expose lib targets for integration testing
- **REST API** (`rest.rs`): axum 0.8 router sharing same `AppState` (storage, config, counter_cache, fsm_engine, event_bus, tls_config) as gRPC services; no gRPC self-calls
- **SSE endpoints**: `async_stream::stream!` macro with broadcast channel subscriptions; `RecvError::Lagged` logs and continues
- **Static file serving**: `tower_http::services::ServeDir` with `ServeFile` fallback for SPA routing
- **Dual server**: gRPC (tonic) on :50054 and REST (axum) on :8081, sharing state, coordinated shutdown via `broadcast::<()>`
- **React SPA**: React 19 + TypeScript + Vite 6 + Tailwind CSS 4 + TanStack React Query + React Router DOM 7 + recharts 2
- **Web dev workflow**: Vite dev server on :5174 proxies `/api` to :8081; production serves built SPA from `pacinet-web/dist/`
- **API key auth**: optional `--api-key` / `PACINET_API_KEY` env var; axum middleware checks `Authorization: Bearer` header or `?token=` query param; `/api/health` exempt from auth; React stores key in localStorage, prompts on 401
- **Persistent event log**: `PersistentEvent` model with event_type, source, payload, timestamp; Storage trait with store/query/prune/count methods; subscriber converts EventBus events to persistent records; configurable `--event-max-age-days` pruning
- **Leader election** (`leader.rs`): lease-based via SQLite `leader_lease` table; `BEGIN IMMEDIATE` transactions for atomic acquisition; renewal at lease_duration/2; `Arc<AtomicBool>` is_leader flag shared with config
- **Leader guards**: REST write endpoints return 503 when standby; gRPC write operations blocked; FSM engine skips evaluation; reaper skips on standby
- **Dashboard enhancements**: recharts PieChart (StatusChart), LineChart (CounterRateChart), sortable Table, NodeGrid card view, event history tab on WatchPage, dark mode persistence in localStorage
- **Audit page**: filterable table with action/resource_type dropdowns
- **Templates page**: CRUD form + list with tag filtering
- **Dry-run preview**: DryRunPreview component on Deploy page showing validation and hash diff
- **Node annotations editor**: inline add/remove on NodeDetail panel
- **Webhook history**: delivery table on InstanceDetail panel
- **Storage trait default methods**: new Phase 9 trait methods have `Ok(...)` defaults for backward compatibility

## Port Assignments
- Controller gRPC: 50054
- Web dashboard REST + static: 8081 (configurable, 0 to disable)
- Vite dev server: 5174 (dev only, proxies /api → 8081)
- Agent gRPC: 50055 (configurable per node)
- Prometheus metrics: 9090 (configurable, 0 to disable)
