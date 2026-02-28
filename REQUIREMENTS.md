# PaciNet Requirements

## 1. Node Management

### 1.1 Registration
- Agents register with the controller on startup
- Registration includes: hostname, agent address, labels, PacGate version
- Controller assigns a unique node ID (UUID)

### 1.2 Heartbeat
- Agents send periodic heartbeats (configurable interval, default 30s)
- Heartbeat includes: node state, CPU usage, uptime
- Controller tracks last heartbeat timestamp and uptime
- Heartbeat retries with exponential backoff (500ms, 1s, 2s) on failure
- Agent reuses gRPC connection, reconnects only on failure

### 1.3 Node Lifecycle
- States: Registered → Online → Deploying → Active/Error → Offline
- Valid state transitions enforced by the storage layer
- Invalid transitions return `InvalidStateTransition` error
- Stale node detection: background reaper marks nodes Offline after configurable missed heartbeats

### 1.4 Labels
- Nodes support key=value labels for grouping/filtering
- Labels set at agent startup via CLI flags
- Labels used for batch deploy targeting and fleet status filtering

## 2. Policy Deployment

### 2.1 Single-Node Deployment
- CLI sends YAML rules to controller for a specific node via `--node` flag
- Controller forwards to agent via gRPC with configurable timeout (default 30s)
- Agent writes YAML to temp file and invokes `pacgate compile`
- Compile options: --counters, --rate-limit, --conntrack
- Concurrent deploy protection: only one deploy at a time per node

### 2.2 Batch Deployment
- CLI sends YAML rules to controller with `--label` filter for multi-node deploy
- Controller fans out deploys concurrently via JoinSet
- Per-node timeout and deploy guard
- Returns per-node results (success/failure) and summary

### 2.3 Policy Tracking
- Controller stores active policy per node
- Policy includes: YAML content, hash, deployment timestamp, compile options
- Policy versioning: each deploy creates a PolicyVersion record with version number
- Policy history queryable via gRPC and CLI
- Policy queryable via CLI

### 2.4 Deployment Audit
- Every deployment attempt creates a DeploymentRecord
- Records include: result (Success/AgentFailure/AgentUnreachable/Timeout), message, policy version, hash
- Deployment history queryable via gRPC and CLI

### 2.5 Policy Rollback
- Rollback to any previous policy version by version number
- Rollback to previous version when no version specified
- Re-deploys the historical YAML through the normal deploy flow

## 3. FSM Engine (Deployment Orchestration)

### 3.1 FSM Definitions
- YAML-defined finite state machines for deployment strategies (canary, staged rollout, etc.)
- Parsed via serde_yaml with FsmDefinition, StateDefinition, TransitionDefinition types
- Validation: initial state exists, all transition targets exist, terminal states have no transitions, durations parse, counter conditions validated
- Kinds: Deployment (deploy orchestration), AdaptivePolicy (counter-rate-driven automation)

### 3.2 FSM States and Transitions
- States have optional actions (deploy, rollback, alert) and transitions
- Terminal states have no transitions and mark instance as Completed
- Transition conditions: Simple (all_succeeded, any_failed, manual), Counter (rate_above/rate_below/total_above with for_duration), Compound (and/or/not)
- Timer transitions: `after: 5m` / `30s` / `1h` duration syntax

### 3.3 FSM Actions
- Deploy: select nodes by label filter, optional limit, optional batch_percent, compile options
- Rollback: target previous policy version, redeploys via shared deploy module
- Alert: log message + optional webhook delivery (HTTP POST with JSON payload, bearer/basic auth, retry with backoff)

### 3.4 FSM Engine
- Background evaluation loop running every 5 seconds
- Evaluates all Running instances, checks transition conditions
- Executes target state actions on transition
- Graceful shutdown via watch channel

### 3.5 FSM Instance Lifecycle
- Created from a definition with rules_yaml and compile options
- Adaptive policy instances created with target node label filter (no rules_yaml required)
- Status: Running → Completed / Failed / Cancelled
- Context tracks: target nodes, deployed nodes, failed nodes, batch cursor, last action result, counter_condition_first_true timestamps
- History records all transitions with timestamp, trigger type, and message

### 3.6 FSM gRPC RPCs
- CreateFsmDefinition, GetFsmDefinition, ListFsmDefinitions, DeleteFsmDefinition
- StartFsm, GetFsmInstance, ListFsmInstances, AdvanceFsm, CancelFsm

### 3.7 FSM Storage
- Stored as JSON blobs in both MemoryStorage and SqliteStorage
- Tables: fsm_definitions (name PK), fsm_instances (instance_id PK, indexed by status and definition_name)

### 3.8 Counter Rate Tracking
- In-memory ring buffer (`CounterSnapshotCache`) stores timestamped counter snapshots per node
- Snapshots recorded on each `ReportCounters` RPC (not persisted to SQLite — intentionally separate)
- Configurable retention period (default 1 hour) and max snapshots per node (default 120)
- Rate calculation from consecutive snapshot pairs: `(newer - older) / time_delta`
- Counter reset handling: if newer < older, rate = 0 (conservative)
- Expired snapshots evicted by the stale node reaper loop

### 3.9 Counter Conditions
- `rate_above` / `rate_below`: threshold on matches_per_second (or bytes_per_second via `field: bytes`)
- `total_above`: threshold on absolute counter value
- `for_duration`: sustained threshold tracking — condition must remain true for specified duration before firing
- `aggregate`: multi-node aggregation mode — `any` (default, fire if any node exceeds), `all`, `sum`
- Counter condition first-true timestamps stored in `FsmContext::counter_condition_first_true` HashMap
- Timestamps cleared on state transitions to reset sustained tracking

### 3.10 Webhook Delivery
- Alert actions can include webhook configuration in the YAML FSM definition
- HTTP POST with JSON payload containing: event type, instance ID, definition name, current state, message, timestamp, deployed nodes
- Authentication: bearer token or basic auth (username/password)
- Custom headers support
- Configurable timeout (default 10 seconds)
- Retry with exponential backoff (default max 2 retries, 1s/2s/4s delays)
- Fire-and-forget via `tokio::spawn` — does not block FSM evaluation
- Metrics recorded for webhook delivery success/failure

## 4. Server-Side Streaming

### 4.1 EventBus
- Wraps three `tokio::sync::broadcast` channels: FSM events, counter events, node events
- Created once in main.rs, cloned into services via `with_event_bus()` builder
- `Option<EventBus>` pattern for backward compatibility (existing tests without event bus still work)
- Buffer size: 256 per channel (hardcoded)
- Events persisted to storage via background subscriber (optional counter events via `--persist-counter-events`)

### 4.2 WatchFsmEvents (stream)
- Streams FSM transitions, deploy progress, and instance completions
- Optional `instance_id` filter — when set, only events for that instance are streamed
- Event types: Transition (from→to state, trigger, message), DeployProgress (deployed/failed/target counts), InstanceCompleted (final status)
- Emitted from FsmEngine on state transitions, deploy actions, cancellations

### 4.3 WatchCounters (stream)
- Streams counter updates with calculated rates (matches/s, bytes/s) per rule
- Optional `node_id` filter — when set, only events for that node are streamed
- Emitted from ControllerService on each `ReportCounters` RPC, with rates calculated from latest snapshot pair

### 4.4 WatchNodeEvents (stream)
- Streams node lifecycle events: Registered, StateChanged, HeartbeatStale, Removed
- Optional `label_filter` — when set, only events for nodes matching all labels are streamed
- Emitted from ControllerService (register, heartbeat state changes, remove) and stale node reaper (heartbeat stale)

### 4.5 Event Emission Points
- `register_node()` → NodeEvent::Registered
- `heartbeat()` → NodeEvent::StateChanged (when state changes)
- `report_counters()` → CounterEvent (with calculated rates)
- `remove_node()` → NodeEvent::Removed
- `fire_transition()` → FsmEvent::Transition, FsmEvent::InstanceCompleted (terminal)
- `execute_deploy()` → FsmEvent::DeployProgress
- `cancel_instance()` → FsmEvent::InstanceCompleted (cancelled)
- Stale reaper → NodeEvent::HeartbeatStale

## 5. Counter Collection

### 5.1 Node Counters
- Agents report rule match counters to controller
- Counters include: rule name, match count, byte count

### 5.2 Aggregate Counters
- CLI can query counters for individual nodes or aggregate across nodes
- Aggregation supports label-based filtering

## 6. CLI Interface

### 6.1 Node Commands
- `pacinet node list [--label key=val]` — shows policy hash and heartbeat age columns
- `pacinet node show <node-id>` — shows enriched node details
- `pacinet node remove <node-id>`

### 6.2 Deployment Commands
- `pacinet deploy <rules.yaml> --node <node-id> [--counters] [--rate-limit] [--conntrack]` — single-node deploy
- `pacinet deploy <rules.yaml> --label key=val [--counters]` — batch deploy with per-node result table and summary
- `pacinet deploy history <node-id> [--limit N]` — deployment audit trail

### 6.3 Policy Commands
- `pacinet policy show <node-id>`
- `pacinet policy diff <node-a> <node-b>` — unified diff between two node policies
- `pacinet policy history <node-id> [--limit N]` — policy version history
- `pacinet policy rollback <node-id> [--version N]` — rollback to previous or specific version

### 6.4 Counter Commands
- `pacinet counters <node-id> [--json]`
- `pacinet counters --aggregate [--label key=val]`

### 6.5 Status Commands
- `pacinet status [--label key=val]` — fleet status with node counts by state and enriched node table

### 6.6 FSM Commands
- `pacinet fsm create <file.yaml>` — create FSM definition from YAML file
- `pacinet fsm list [--kind deployment]` — list definitions
- `pacinet fsm show <name>` — show definition YAML
- `pacinet fsm delete <name>` — delete definition
- `pacinet fsm start <name> [--rules <file>] [--counters] [--label key=val]` — start FSM instance (--rules for deployment FSMs, --label for adaptive policy FSMs)
- `pacinet fsm status <instance-id>` — show instance status with transition history
- `pacinet fsm instances [--definition X] [--status running]` — list instances
- `pacinet fsm advance <instance-id> [--state X]` — manually advance instance
- `pacinet fsm cancel <instance-id>` — cancel running instance

### 6.7 Watch Commands
- `pacinet watch fsm [--instance <id>]` — stream FSM transitions, deploy progress, completions
- `pacinet watch counters [--node <id>]` — stream counter updates with rates
- `pacinet watch nodes [--label key=val]` — stream node lifecycle events
- Human-readable output with timestamps (default)
- JSON output via `--json` flag
- Ctrl+C terminates the stream

### 6.8 Output Formats
- Human-readable table output (default)
- JSON output via `--json` flag

## 7. Communication

### 7.1 gRPC Services
- PaciNetController (agent → controller): RegisterNode, Heartbeat, ReportCounters
- PaciNetAgent (controller → agent): DeployRules, GetCounters, GetStatus
- PaciNetManagement (CLI → controller): ListNodes, GetNode, RemoveNode, DeployPolicy, GetPolicy, GetNodeCounters, GetAggregateCounters, BatchDeployPolicy, GetFleetStatus, GetPolicyHistory, GetDeploymentHistory, RollbackPolicy, CreateFsmDefinition, GetFsmDefinition, ListFsmDefinitions, DeleteFsmDefinition, StartFsm, GetFsmInstance, ListFsmInstances, AdvanceFsm, CancelFsm, WatchFsmEvents (stream), WatchCounters (stream), WatchNodeEvents (stream)

### 7.2 Port Assignments
- Controller gRPC: 50054 (configurable)
- Web dashboard REST + static: 8081 (configurable, 0 to disable)
- Vite dev server: 5174 (dev only, proxies /api → 8081)
- Agent: 50055 (configurable per node)
- Prometheus metrics: 9090 (configurable, 0 to disable)

### 7.3 Health Checks
- gRPC health service via tonic-health

### 7.4 gRPC-Web
- HTTP/1 support via tonic-web for browser-based clients

### 7.5 REST API
- Axum 0.8 HTTP server on configurable port (default 8081, 0 to disable)
- Shares same state (storage, config, counter_cache, fsm_engine, event_bus) as gRPC services
- Endpoints:
  - GET/DELETE `/api/nodes`, `/api/nodes/:id` — node CRUD
  - GET `/api/nodes/:id/policy`, `/api/nodes/:id/counters` — node data
  - GET `/api/nodes/:id/policy/history`, `/api/nodes/:id/deploy/history` — audit
  - POST `/api/nodes/:id/policy/rollback` — rollback
  - GET `/api/fleet` — fleet status
  - GET `/api/counters` — aggregate counters
  - POST `/api/deploy`, `/api/deploy/batch` — policy deployment
  - GET/POST/DELETE `/api/fsm/definitions`, `/api/fsm/definitions/:name` — FSM def CRUD
  - GET/POST `/api/fsm/instances`, `/api/fsm/instances/:id` — FSM instance CRUD
  - POST `/api/fsm/instances/:id/advance`, `/api/fsm/instances/:id/cancel` — FSM actions
  - GET `/api/health` — health check (no auth required), returns status, auth_required, role
  - GET `/api/events/history` — persistent event log with ?type=&source=&since=&until=&limit= filters
- Query params: `?label=key%3Dval` for label filters, `?limit=N` for history
- Auth middleware: validates Bearer token or ?token= query param when API key configured
- CORS enabled for browser access
- Leader guards: write endpoints return 503 when controller is standby
- JSON request/response with serde Serialize/Deserialize types

### 7.6 SSE (Server-Sent Events)
- GET `/api/events/nodes` — node lifecycle events (optional `?label=key%3Dval` filter)
- GET `/api/events/counters` — counter updates (optional `?node=<id>` filter)
- GET `/api/events/fsm` — FSM transitions (optional `?instance=<id>` filter)
- Subscribes to EventBus broadcast channels
- KeepAlive enabled for connection persistence
- Lagged events logged and skipped

### 7.7 Static File Serving
- Serves SPA from `--static-dir` (default `pacinet-web/dist/`)
- SPA fallback: unmatched GET routes serve `index.html`
- When static dir not found, REST API still works (dev mode)

## 8. Security

### 8.0 API Key Authentication
- Optional API key for REST API via `--api-key` or `PACINET_API_KEY` env var
- Validated via `Authorization: Bearer <key>` header or `?token=<key>` query param (for SSE)
- `/api/health` exempt from auth (for load balancer probes)
- When no key configured, all requests pass (backward compatible)
- React SPA: stores key in localStorage, prompts on 401 via custom event

### 8.1 Mutual TLS (mTLS)
- Optional mTLS on all gRPC channels
- Three flags: `--ca-cert`, `--tls-cert`, `--tls-key` (all required together, or all omitted)
- Channels secured: agent→controller, controller→agent, CLI→controller
- When TLS absent: plain HTTP for development convenience
- Certificate generation script for development (`scripts/gen-certs.sh`)

### 8.2 Certificate Management
- CA certificate, server cert, agent cert, client cert — all signed by the same CA
- Development script uses openssl for self-signed certificates
- Production: external CA/PKI expected

## 9. PacGate Integration

### 9.1 Subprocess Invocation
- Agent invokes `pacgate` binary as subprocess
- YAML rules written to temp file, cleaned up after compilation
- JSON output parsed for success/warnings/errors

### 9.2 Version Detection
- Agent auto-detects PacGate version at startup via `pacgate --version`
- Override via `--pacgate-version` CLI flag
- Version reported to controller during registration

### 9.3 Decoupling
- PaciNet has no compile-time dependency on PacGate
- YAML is the sole interface contract

## 10. Observability

### 10.1 Prometheus Metrics
- HTTP endpoint on configurable port (default 9090, 0 to disable)
- Metrics exposed:
  - `pacinet_nodes_total` (gauge) — total registered nodes
  - `pacinet_nodes_by_state{state}` (gauge) — node count per state
  - `pacinet_deploys_total{result}` (counter) — deploy attempts by result
  - `pacinet_deploy_duration_seconds` (histogram) — deploy latency
  - `pacinet_heartbeats_total` (counter) — heartbeat RPCs received
  - `pacinet_heartbeats_missed_total` (counter) — stale detections
  - `pacinet_batch_deploys_total` (counter) — batch deploy operations
  - `pacinet_batch_deploy_nodes{result}` (counter) — per-node batch results
  - `pacinet_controller_uptime_seconds` (gauge) — process uptime
  - `pacinet_fsm_transitions_total` (counter) — FSM state transitions
  - `pacinet_fsm_instances{status}` (counter) — FSM instance lifecycle events
  - `pacinet_fsm_running_instances` (gauge) — currently running FSM instances
  - `pacinet_counter_snapshots_total` (counter) — counter snapshots recorded
  - `pacinet_counter_snapshots_cached` (gauge) — snapshots currently in cache
  - `pacinet_webhook_deliveries_total{result}` (counter) — webhook delivery attempts
  - `pacinet_counter_evals_total{result}` (counter) — counter condition evaluations

### 10.2 Structured Logging
- via tracing with EnvFilter
- `#[tracing::instrument]` on gRPC handlers
- Debug level for heartbeat to reduce noise
- `RUST_LOG` environment variable for filtering

## 11. Non-Functional Requirements

### 11.0 Persistent Event Log
- `PersistentEvent` model: id, event_type, source, payload (JSON), timestamp
- Storage trait extensions: store_event, query_events (with type/source/since/until/limit filters), prune_events, count_events
- Default implementations for backward compatibility
- SQLite: events table with indexes on timestamp DESC and event_type
- MemoryStorage: Vec with 10,000 event cap
- Background subscriber converts EventBus events to PersistentEvent via `to_persistent()` methods
- Counter events optionally persisted via `--persist-counter-events` flag (default off, high frequency)
- Event pruning in reaper loop via `--event-max-age-days` (default 7)
- REST endpoint: GET /api/events/history with type/source/since/until/limit query params
- React: History tab on WatchPage with `useEventHistory()` hook

### 11.0b Multi-Controller HA
- Lease-based leader election using SQLite `leader_lease` table
- Single-row pattern with CHECK (id = 1) constraint
- Atomic acquisition via `BEGIN IMMEDIATE` transaction
- Lease renewal at lease_duration/2 (default 30s lease)
- `Arc<AtomicBool>` is_leader flag shared between LeaderElection and ControllerConfig
- Leader guards on: REST write endpoints (503 for standby), gRPC write ops, FSM evaluation, event persistence, stale reaper
- MemoryStorage always returns true for leader acquisition (single-node default)
- `/api/health` returns `role: "leader"` or `role: "standby"`
- CLI flags: `--cluster-id` (enables HA), `--lease-duration` (seconds, default 30)
- Requires `--db` when `--cluster-id` is set (validation at startup)

### 11.1 Storage
- Storage trait (`Arc<dyn Storage>`) for backend abstraction
- MemoryStorage: in-memory with RwLock (default for dev/test)
- SqliteStorage: rusqlite with WAL mode, foreign keys, schema migrations (for production)
- Controller selects backend via `--db <path>` flag (omit for in-memory)

### 11.2 Configuration
- `--deploy-timeout` (default 30s)
- `--heartbeat-expect-interval` (default 30s)
- `--heartbeat-miss-threshold` (default 3)
- `--heartbeat-interval` on agent (default 30s)
- `--metrics-port` (default 9090, 0 to disable)
- `--counter-retention-secs` (default 3600) — counter snapshot cache retention
- `--counter-max-per-node` (default 120) — max cached snapshots per node
- `RUST_LOG` environment variable for log filtering

### 11.3 Error Handling
- Domain errors (PaciNetError) map to gRPC Status codes
- InvalidStateTransition → failed_precondition
- ConcurrentDeploy → aborted
- Graceful handling of agent disconnections

### 11.4 Graceful Shutdown
- Server: SIGINT/SIGTERM → drain in-flight RPCs via serve_with_shutdown
- Agent: SIGINT → stop heartbeat loop via watch channel, drain gRPC server
- Clean log messages on shutdown

### 11.5 Testing
- Unit tests for model types (state transitions, FromStr roundtrips)
- Unit tests for MemoryStorage (9 tests: register, remove, filter, state transitions, invalid transition, concurrent deploy, policy versioning, deployment audit, stale detection)
- Unit tests for SqliteStorage (9 tests mirroring MemoryStorage, using in-memory SQLite)
- Unit tests for PacGate JSON parsing and mock backend
- REST integration tests (17 tests):
  - Node CRUD: list empty, list/get, 404, delete
  - Fleet status with label filter
  - Policy, deploy history (404 cases)
  - FSM definitions CRUD, instance 404
  - Health endpoint (no auth required)
  - Auth: 401 without key, 200 with Bearer, 200 with ?token=, no auth when no key configured
  - SSE node events via EventBus
  - Event history with filters
  - Deploy 404 (node not found)
  - Aggregate counters empty
- Integration tests using ephemeral ports:
  - Full happy path: register → deploy → counters → query
  - Deploy to unreachable agent: graceful failure, node state = Error
  - Deploy with PacGate failure: returns failure, node state = Error
  - Batch deploy to multiple nodes: all succeed
  - Batch deploy partial failure: mixed results
  - Fleet status: node counts by state, enriched summaries
  - Stale node detection: node goes offline after missed heartbeats
  - FSM definition CRUD: create, get, list, delete
  - FSM start and auto-complete: deploy succeeds → all_succeeded → terminal
  - FSM manual advance: deploy then manual gate transition
  - FSM cancel running instance
  - FSM list instances with filters
  - FSM deploy failure triggers transition: unreachable agent → any_failed → terminal
  - Counter snapshot cache: record, query, eviction
  - Counter rate calculation: rate from snapshots, counter reset handling
  - Counter condition fires transition: inject snapshots → rate_above threshold met → state transition
  - Counter condition for_duration: verify sustained threshold tracking
  - Watch node events: subscribe, register node, verify Registered event
  - Watch counters report: subscribe, report counters, verify CounterUpdate with rates
  - Watch counters filter: two nodes, watch one, verify only filtered events
  - Watch FSM transition: start FSM, advance, verify transition + completed events
- PacGateBackend enum (Real | Mock) for test isolation

### 11.6 CI/CD
- GitHub Actions pipeline on push and pull_request
- Steps: cargo check, clippy (warnings as errors), test, fmt check
- Rust stable toolchain with caching

## 12. Web Dashboard

### 12.1 Technology Stack
- React 19 + TypeScript 5.8 + Vite 6
- Tailwind CSS 4 (dark/light theme with localStorage persistence, identical to aida-web-react)
- TanStack React Query 5 for data fetching and caching
- React Router DOM 7 for SPA routing
- recharts 2 for interactive charts (PieChart, LineChart)
- lucide-react for icons
- Inter + JetBrains Mono fonts

### 12.2 Pages
- **Dashboard** (`/`): fleet metrics cards, recharts PieChart (interactive with tooltips), live event feed, FSM summary
- **Nodes** (`/nodes`): filterable table or grid view (toggle), sortable columns, click-to-detail panel with policy, counters, deploy history, remove action
- **Deploy** (`/deploy`): single/batch mode toggle, YAML textarea, compile options, result display
- **Counters** (`/counters`): node selector, counter rate line chart (recharts), counter table with live rates via SSE, aggregate view
- **FSM** (`/fsm`): tabbed view — definitions (CRUD from YAML) and instances (start/advance/cancel, transition timeline)
- **Watch** (`/watch`): Live/History tab toggle. Live: combined SSE feed with type/text filters, auto-scroll. History: persistent event log with type/source/limit filters via REST

### 12.3 Real-Time Updates
- SSE (Server-Sent Events) via EventSource API for live data, with ?token= auth support
- React Query with 5s refetch interval for REST data
- Pause-on-hover for auto-scrolling event feeds

### 12.4 Authentication
- API key stored in localStorage, attached as Authorization: Bearer header on all REST requests
- SSE connections append ?token= query param
- 401 responses dispatch `pacinet:auth-required` custom event
- ApiKeyPrompt modal shown on auth-required event
- Successful auth invalidates all React Query caches for refresh

### 12.5 Build & Dev Workflow
- `make web-install` — install npm dependencies
- `make web-dev` — Vite dev server on :5174 (proxies /api → :8081)
- `make web-build` — build to pacinet-web/dist/
- `make run-server-web` — server with built SPA on :8081
- `make run-server-auth` — server with API key auth enabled
- Production: build React, serve from `--static-dir`
