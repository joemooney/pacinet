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

## 4. Counter Collection

### 4.1 Node Counters
- Agents report rule match counters to controller
- Counters include: rule name, match count, byte count

### 4.2 Aggregate Counters
- CLI can query counters for individual nodes or aggregate across nodes
- Aggregation supports label-based filtering

## 5. CLI Interface

### 5.1 Node Commands
- `pacinet node list [--label key=val]` — shows policy hash and heartbeat age columns
- `pacinet node show <node-id>` — shows enriched node details
- `pacinet node remove <node-id>`

### 5.2 Deployment Commands
- `pacinet deploy <rules.yaml> --node <node-id> [--counters] [--rate-limit] [--conntrack]` — single-node deploy
- `pacinet deploy <rules.yaml> --label key=val [--counters]` — batch deploy with per-node result table and summary
- `pacinet deploy history <node-id> [--limit N]` — deployment audit trail

### 5.3 Policy Commands
- `pacinet policy show <node-id>`
- `pacinet policy diff <node-a> <node-b>` — unified diff between two node policies
- `pacinet policy history <node-id> [--limit N]` — policy version history
- `pacinet policy rollback <node-id> [--version N]` — rollback to previous or specific version

### 5.4 Counter Commands
- `pacinet counters <node-id> [--json]`
- `pacinet counters --aggregate [--label key=val]`

### 5.5 Status Commands
- `pacinet status [--label key=val]` — fleet status with node counts by state and enriched node table

### 5.6 FSM Commands
- `pacinet fsm create <file.yaml>` — create FSM definition from YAML file
- `pacinet fsm list [--kind deployment]` — list definitions
- `pacinet fsm show <name>` — show definition YAML
- `pacinet fsm delete <name>` — delete definition
- `pacinet fsm start <name> [--rules <file>] [--counters] [--label key=val]` — start FSM instance (--rules for deployment FSMs, --label for adaptive policy FSMs)
- `pacinet fsm status <instance-id>` — show instance status with transition history
- `pacinet fsm instances [--definition X] [--status running]` — list instances
- `pacinet fsm advance <instance-id> [--state X]` — manually advance instance
- `pacinet fsm cancel <instance-id>` — cancel running instance

### 5.7 Output Formats
- Human-readable table output (default)
- JSON output via `--json` flag

## 6. Communication

### 6.1 gRPC Services
- PaciNetController (agent → controller): RegisterNode, Heartbeat, ReportCounters
- PaciNetAgent (controller → agent): DeployRules, GetCounters, GetStatus
- PaciNetManagement (CLI → controller): ListNodes, GetNode, RemoveNode, DeployPolicy, GetPolicy, GetNodeCounters, GetAggregateCounters, BatchDeployPolicy, GetFleetStatus, GetPolicyHistory, GetDeploymentHistory, RollbackPolicy, CreateFsmDefinition, GetFsmDefinition, ListFsmDefinitions, DeleteFsmDefinition, StartFsm, GetFsmInstance, ListFsmInstances, AdvanceFsm, CancelFsm

### 6.2 Port Assignments
- Controller: 50054 (configurable)
- Agent: 50055 (configurable per node)
- Prometheus metrics: 9090 (configurable, 0 to disable)

### 6.3 Health Checks
- gRPC health service via tonic-health

### 6.4 gRPC-Web
- HTTP/1 support via tonic-web for browser-based clients

## 7. Security

### 7.1 Mutual TLS (mTLS)
- Optional mTLS on all gRPC channels
- Three flags: `--ca-cert`, `--tls-cert`, `--tls-key` (all required together, or all omitted)
- Channels secured: agent→controller, controller→agent, CLI→controller
- When TLS absent: plain HTTP for development convenience
- Certificate generation script for development (`scripts/gen-certs.sh`)

### 7.2 Certificate Management
- CA certificate, server cert, agent cert, client cert — all signed by the same CA
- Development script uses openssl for self-signed certificates
- Production: external CA/PKI expected

## 8. PacGate Integration

### 8.1 Subprocess Invocation
- Agent invokes `pacgate` binary as subprocess
- YAML rules written to temp file, cleaned up after compilation
- JSON output parsed for success/warnings/errors

### 8.2 Version Detection
- Agent auto-detects PacGate version at startup via `pacgate --version`
- Override via `--pacgate-version` CLI flag
- Version reported to controller during registration

### 8.3 Decoupling
- PaciNet has no compile-time dependency on PacGate
- YAML is the sole interface contract

## 9. Observability

### 9.1 Prometheus Metrics
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

### 9.2 Structured Logging
- via tracing with EnvFilter
- `#[tracing::instrument]` on gRPC handlers
- Debug level for heartbeat to reduce noise
- `RUST_LOG` environment variable for filtering

## 10. Non-Functional Requirements

### 10.1 Storage
- Storage trait (`Arc<dyn Storage>`) for backend abstraction
- MemoryStorage: in-memory with RwLock (default for dev/test)
- SqliteStorage: rusqlite with WAL mode, foreign keys, schema migrations (for production)
- Controller selects backend via `--db <path>` flag (omit for in-memory)

### 10.2 Configuration
- `--deploy-timeout` (default 30s)
- `--heartbeat-expect-interval` (default 30s)
- `--heartbeat-miss-threshold` (default 3)
- `--heartbeat-interval` on agent (default 30s)
- `--metrics-port` (default 9090, 0 to disable)
- `--counter-retention-secs` (default 3600) — counter snapshot cache retention
- `--counter-max-per-node` (default 120) — max cached snapshots per node
- `RUST_LOG` environment variable for log filtering

### 10.3 Error Handling
- Domain errors (PaciNetError) map to gRPC Status codes
- InvalidStateTransition → failed_precondition
- ConcurrentDeploy → aborted
- Graceful handling of agent disconnections

### 10.4 Graceful Shutdown
- Server: SIGINT/SIGTERM → drain in-flight RPCs via serve_with_shutdown
- Agent: SIGINT → stop heartbeat loop via watch channel, drain gRPC server
- Clean log messages on shutdown

### 10.5 Testing
- Unit tests for model types (state transitions, FromStr roundtrips)
- Unit tests for MemoryStorage (9 tests: register, remove, filter, state transitions, invalid transition, concurrent deploy, policy versioning, deployment audit, stale detection)
- Unit tests for SqliteStorage (9 tests mirroring MemoryStorage, using in-memory SQLite)
- Unit tests for PacGate JSON parsing and mock backend
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
- PacGateBackend enum (Real | Mock) for test isolation

### 10.6 CI/CD
- GitHub Actions pipeline on push and pull_request
- Steps: cargo check, clippy (warnings as errors), test, fmt check
- Rust stable toolchain with caching
