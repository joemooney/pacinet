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
- Policy history queryable
- Policy queryable via CLI

### 2.4 Deployment Audit
- Every deployment attempt creates a DeploymentRecord
- Records include: result (Success/AgentFailure/AgentUnreachable/Timeout), message, policy version, hash

## 3. Counter Collection

### 3.1 Node Counters
- Agents report rule match counters to controller
- Counters include: rule name, match count, byte count

### 3.2 Aggregate Counters
- CLI can query counters for individual nodes or aggregate across nodes
- Aggregation supports label-based filtering

## 4. CLI Interface

### 4.1 Node Commands
- `pacinet node list [--label key=val]` — shows policy hash and heartbeat age columns
- `pacinet node show <node-id>` — shows enriched node details
- `pacinet node remove <node-id>`

### 4.2 Deployment Commands
- `pacinet deploy <rules.yaml> --node <node-id> [--counters] [--rate-limit] [--conntrack]` — single-node deploy
- `pacinet deploy <rules.yaml> --label key=val [--counters]` — batch deploy with per-node result table and summary

### 4.3 Policy Commands
- `pacinet policy show <node-id>`
- `pacinet policy diff <node-a> <node-b>` — unified diff between two node policies

### 4.4 Counter Commands
- `pacinet counters <node-id> [--json]`
- `pacinet counters --aggregate [--label key=val]`

### 4.5 Status Commands
- `pacinet status [--label key=val]` — fleet status with node counts by state and enriched node table

### 4.6 Output Formats
- Human-readable table output (default)
- JSON output via `--json` flag

## 5. Communication

### 5.1 gRPC Services
- PaciNetController (agent → controller): RegisterNode, Heartbeat, ReportCounters
- PaciNetAgent (controller → agent): DeployRules, GetCounters, GetStatus
- PaciNetManagement (CLI → controller): ListNodes, GetNode, RemoveNode, DeployPolicy, GetPolicy, GetNodeCounters, GetAggregateCounters, BatchDeployPolicy, GetFleetStatus

### 5.2 Port Assignments
- Controller: 50054 (configurable)
- Agent: 50055 (configurable per node)

### 5.3 Health Checks
- gRPC health service via tonic-health

## 6. PacGate Integration

### 6.1 Subprocess Invocation
- Agent invokes `pacgate` binary as subprocess
- YAML rules written to temp file, cleaned up after compilation
- JSON output parsed for success/warnings/errors

### 6.2 Version Detection
- Agent auto-detects PacGate version at startup via `pacgate --version`
- Override via `--pacgate-version` CLI flag
- Version reported to controller during registration

### 6.3 Decoupling
- PaciNet has no compile-time dependency on PacGate
- YAML is the sole interface contract

## 7. Non-Functional Requirements

### 7.1 Storage
- Storage trait (`Arc<dyn Storage>`) for backend abstraction
- MemoryStorage: in-memory with RwLock (default for dev/test)
- SqliteStorage: rusqlite with WAL mode, foreign keys, schema migrations (for production)
- Controller selects backend via `--db <path>` flag (omit for in-memory)

### 7.2 Configuration
- `--deploy-timeout` (default 30s)
- `--heartbeat-expect-interval` (default 30s)
- `--heartbeat-miss-threshold` (default 3)
- `--heartbeat-interval` on agent (default 30s)
- `RUST_LOG` environment variable for log filtering

### 7.3 Logging
- Structured logging via tracing with EnvFilter
- `#[tracing::instrument]` on all gRPC handlers
- Debug level for heartbeat to reduce noise

### 7.4 Error Handling
- Domain errors (PaciNetError) map to gRPC Status codes
- InvalidStateTransition → failed_precondition
- ConcurrentDeploy → aborted
- Graceful handling of agent disconnections

### 7.5 Testing
- Unit tests for model types (state transitions, FromStr roundtrips)
- Unit tests for MemoryStorage (register, remove, filter, state transitions, concurrent deploy, policy versioning, deployment audit, stale detection)
- Unit tests for PacGate JSON parsing and mock backend
- Integration tests using ephemeral ports:
  - Full happy path: register → deploy → counters → query
  - Deploy to unreachable agent: graceful failure, node state = Error
  - Deploy with PacGate failure: returns failure, node state = Error
  - Batch deploy to multiple nodes: all succeed
  - Batch deploy partial failure: mixed results
  - Fleet status: node counts by state, enriched summaries
  - Stale node detection: node goes offline after missed heartbeats
- PacGateBackend enum (Real | Mock) for test isolation
