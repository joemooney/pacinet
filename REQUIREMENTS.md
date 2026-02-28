# PaciNet Requirements

## 1. Node Management

### 1.1 Registration
- Agents register with the controller on startup
- Registration includes: hostname, agent address, labels, PacGate version
- Controller assigns a unique node ID (UUID)

### 1.2 Heartbeat
- Agents send periodic heartbeats (30s interval)
- Heartbeat includes: node state, CPU usage, uptime
- Controller tracks last heartbeat timestamp

### 1.3 Node Lifecycle
- States: Registered → Online → Deploying → Active → Offline → Error
- Nodes can be removed via CLI

### 1.4 Labels
- Nodes support key=value labels for grouping/filtering
- Labels set at agent startup via CLI flags

## 2. Policy Deployment

### 2.1 Rule Deployment
- CLI sends YAML rules to controller for a specific node
- Controller forwards to agent via gRPC
- Agent writes YAML to temp file and invokes `pacgate compile`
- Compile options: --counters, --rate-limit, --conntrack

### 2.2 Policy Tracking
- Controller stores active policy per node
- Policy includes: YAML content, hash, deployment timestamp, compile options
- Policy queryable via CLI

## 3. Counter Collection

### 3.1 Node Counters
- Agents report rule match counters to controller
- Counters include: rule name, match count, byte count

### 3.2 Aggregate Counters
- CLI can query counters for individual nodes or aggregate across nodes
- Aggregation supports label-based filtering

## 4. CLI Interface

### 4.1 Node Commands
- `pacinet node list [--label key=val]`
- `pacinet node show <node-id>`
- `pacinet node remove <node-id>`

### 4.2 Deployment Commands
- `pacinet deploy <node-id> <rules.yaml> [--counters] [--rate-limit] [--conntrack]`
- `pacinet policy show <node-id>`

### 4.3 Counter Commands
- `pacinet counters <node-id> [--json]`
- `pacinet counters --aggregate [--label key=val]`

### 4.4 Status Commands
- `pacinet status`
- `pacinet version`

### 4.5 Output Formats
- Human-readable table output (default)
- JSON output via `--json` flag

## 5. Communication

### 5.1 gRPC Services
- PaciNetController (agent → controller): RegisterNode, Heartbeat, ReportCounters
- PaciNetAgent (controller → agent): DeployRules, GetCounters, GetStatus
- PaciNetManagement (CLI → controller): ListNodes, GetNode, RemoveNode, DeployPolicy, GetPolicy, GetNodeCounters, GetAggregateCounters

### 5.2 Port Assignments
- Controller: 50054 (configurable)
- Agent: 50055 (configurable per node)

## 6. PacGate Integration

### 6.1 Subprocess Invocation
- Agent invokes `pacgate` binary as subprocess
- YAML rules written to temp file, cleaned up after compilation
- JSON output parsed for success/warnings/errors

### 6.2 Decoupling
- PaciNet has no compile-time dependency on PacGate
- YAML is the sole interface contract

## 7. Non-Functional Requirements

### 7.1 Storage
- In-memory HashMap for MVP (no database)
- Thread-safe via Arc<RwLock<HashMap>>

### 7.2 Logging
- Structured logging via tracing
- Debug mode via --debug flag

### 7.3 Error Handling
- Domain errors (PaciNetError) map to gRPC Status codes
- Graceful handling of agent disconnections

### 7.4 Testing
- Unit tests for PacGate JSON parsing and mock backend
- Unit tests for NodeRegistry (register, remove, filter, state update)
- Integration tests using ephemeral ports (no port conflicts):
  - Full happy path: register → deploy → counters → query
  - Deploy to unreachable agent: graceful failure, node state = Error
  - Deploy with PacGate failure: returns failure, node state = Error
- PacGateBackend enum (Real | Mock) for test isolation

### 7.5 Deploy Forwarding
- Controller forwards DeployPolicy to agent via PaciNetAgent gRPC client
- 30-second timeout for agent communication
- Node state transitions: Deploying → Active (success) or Error (failure)
- Policy stored locally regardless of agent reachability
