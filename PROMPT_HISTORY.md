# PaciNet Development History

## Session 1 — Initial Scaffold (2026-02-27)

### Prompt
Implement the PaciNet SDN Controller scaffold — a control plane companion to PacGate (FPGA packet filter compiler). Create a Rust workspace with 5 crates (proto, core, server, agent, CLI) using gRPC for communication.

### Actions Taken
1. Created workspace structure with 5 crates:
   - `pacinet-proto` — generated gRPC/protobuf types
   - `pacinet-core` — domain model (Node, Policy, RuleCounter) and error types
   - `pacinet-server` — controller binary (port 50054)
   - `pacinet-agent` — node agent binary (port 50055)
   - `pacinet-cli` — operator CLI (`pacinet`)

2. Wrote `proto/pacinet.proto` with 3 gRPC services:
   - PaciNetController (agent → controller): RegisterNode, Heartbeat, ReportCounters
   - PaciNetAgent (controller → agent): DeployRules, GetCounters, GetStatus
   - PaciNetManagement (CLI → controller): full CRUD for nodes, policies, counters

3. Implemented all gRPC service handlers:
   - Controller: node registration, heartbeat tracking, counter storage
   - Agent: rule deployment via PacGate subprocess, status reporting
   - Management: node listing/show/remove, policy deploy/show, counter queries

4. Implemented CLI with clap subcommands:
   - `node list/show/remove`, `deploy`, `policy show`, `counters`, `status`, `version`
   - Human-readable and JSON output modes

5. Created in-memory node registry (Arc<RwLock<HashMap>>)

6. Created PacGate subprocess runner (tokio::process::Command)

7. Registered ports 50054 and 50055 in ~/.ports

8. Created project documentation: CLAUDE.md, OVERVIEW.md, REQUIREMENTS.md

### Technical Notes
- Used tonic 0.12 + prost 0.13 (matching existing projects)
- Proto types do NOT have serde derives due to prost_types::Timestamp incompatibility
- JSON output in CLI uses manual serde_json::Value construction
- Agent heartbeat runs on 30-second interval via tokio::spawn

### Git Operations
- Initialized git repository
- Initial commit with full scaffold
- Pushed to GitHub

## Session 2 — End-to-End Deployment & Integration Tests (2026-02-27)

### Prompt
Implement Phase 2: close end-to-end gaps so the full flow works (CLI → Controller → Agent → PacGate). Add deploy forwarding, real agent state tracking, PacGate mock backend, integration tests.

### Actions Taken
1. **pacinet-agent/src/pacgate.rs** — Added PacGateOutput/PacGateGenerated structs for JSON parsing, enriched CompileResult with rules_count/output_dir, created PacGateBackend enum (Real|Mock), added 5 unit tests

2. **pacinet-agent/src/service.rs** — Expanded AgentState with active_policy_hash, active_rules_yaml, deployed_at, start_time, counters. Updated deploy_rules to store state on success, get_status to return real uptime/state, get_counters to return stored counters

3. **pacinet-agent/src/main.rs** — Initialized new AgentState fields, passed Arc<RwLock<AgentState>> to heartbeat_loop, heartbeat now sends real uptime and node state (Online vs Active)

4. **pacinet-agent/src/lib.rs** — Created lib target exporting pacgate and service modules

5. **pacinet-server/src/registry.rs** — Added update_node_state() method, added 4 unit tests (register_and_get, remove_cleans_up, label_filtering, update_node_state)

6. **pacinet-server/src/service.rs** — Implemented deploy forwarding: controller creates PaciNetAgentClient, connects to agent, calls deploy_rules with 30s timeout, updates node state (Deploying → Active or Error)

7. **pacinet-server/src/lib.rs** — Created lib target exporting registry and service modules

8. **pacinet-server/tests/integration.rs** — 3 integration tests using ephemeral ports: happy path, unreachable agent, PacGate failure

9. **Cargo.toml updates** — Added [lib] sections to server/agent, tokio-stream workspace dep, dev-dependencies

10. **Clippy fixes** — Added Default impls for PacGateRunner/NodeRegistry, fixed map_or, clone_on_copy, print_literal

### Test Results
- 14 tests total, all passing
- cargo clippy --workspace -- -D warnings: clean

### Git Operations
- Committed Phase 2 changes
- Pushed to GitHub

## Session 3 — Production Resilience, Persistence, Fleet Management & Observability (2026-02-27)

### Prompt
Implement Phase 3: storage abstraction (Memory + SQLite), state machine validation, policy versioning, deployment audit trail, fleet management (batch deploy, fleet status), agent resilience (bind fix, retry, version detection), CLI enhancements (batch deploy, policy diff, enriched node list), stale node reaper, gRPC health checks, configurable timeouts.

### Actions Taken

#### 1. Proto Changes
- Added `BatchDeployPolicy` and `GetFleetStatus` RPCs to PaciNetManagement service
- Added BatchDeployPolicyRequest/Response, NodeDeployResult messages
- Added GetFleetStatusRequest/Response, FleetNodeSummary messages
- Enriched NodeInfo with fields 9-11: policy_hash, uptime_seconds, last_heartbeat_age_seconds

#### 2. Core Model Enhancements (pacinet-core)
- **model.rs**: Added `valid_transitions()` and `can_transition_to()` to NodeState, FromStr impl, PolicyVersion struct, DeploymentResult enum (with Display/FromStr), DeploymentRecord struct, uptime_seconds field on Node, new unit tests (state transitions, FromStr, DeploymentResult roundtrip)
- **error.rs**: Added InvalidStateTransition and ConcurrentDeploy error variants with gRPC status mappings
- **storage.rs** (new): Created Storage trait with full API — node ops, counter ops, policy ops with versioning, deploy audit, fleet ops (begin_deploy, end_deploy, mark_stale_nodes, status_summary), StatusSummary type alias
- **lib.rs**: Exported new types (DeploymentRecord, DeploymentResult, PolicyVersion, StatusSummary, Storage)

#### 3. Storage Backends (pacinet-server/src/storage/)
- **mod.rs** (new): blocking() helper wrapping sync Storage calls in spawn_blocking, re-exports
- **memory.rs** (new): MemoryStorage implementing Storage trait (refactored from NodeRegistry), with state transition validation, concurrent deploy protection via HashSet, policy versioning, deployment audit, stale node detection. 9 unit tests
- **sqlite.rs** (new): SqliteStorage with rusqlite, WAL mode, foreign keys, schema initialization
- **schema.sql** (new): DDL for nodes, policies, policy_versions, counters, deployments tables with indexes and cascading deletes
- **Deleted**: pacinet-server/src/registry.rs (replaced by storage/memory.rs)

#### 4. Server Config
- **config.rs** (new): ControllerConfig struct with deploy_timeout, heartbeat_expect_interval, heartbeat_miss_threshold, start_time, stale_threshold() helper

#### 5. Server Service Updates
- Changed `Arc<NodeRegistry>` → `Arc<dyn Storage>` in both services
- ManagementService now takes ControllerConfig
- All storage calls wrapped with blocking() helper
- deploy_policy: begin_deploy/end_deploy guard, configurable timeout, DeploymentRecord audit
- Implemented batch_deploy_policy: concurrent fan-out via JoinSet, per-node timeout and guard
- Implemented get_fleet_status: node counts by state, enriched node summaries with policy hash, heartbeat age, uptime
- Updated node_to_proto to accept optional Policy for enrichment
- Updated list_nodes to batch-fetch policies
- Added #[tracing::instrument] to all gRPC handlers

#### 6. Server Main Updates
- Added --db flag for SQLite backend selection
- Added --deploy-timeout, --heartbeat-expect-interval, --heartbeat-miss-threshold flags
- Spawned stale node reaper background task
- Added tonic-health gRPC health service
- Upgraded tracing subscriber with EnvFilter
- Updated lib.rs to export storage and config modules

#### 7. Agent Fixes
- Fixed bind address bug (used args.host instead of hardcoded "127.0.0.1")
- Added --heartbeat-interval CLI flag (configurable, default 30s)
- Connection reuse: create PaciNetControllerClient once, reconnect on failure
- Retry with exponential backoff: 3 retries per heartbeat (500ms→1s→2s)
- PacGate version detection: tries `pacgate --version` at startup, --pacgate-version override
- Added pacgate_version field to AgentState, used in get_status response
- Upgraded tracing subscriber with EnvFilter

#### 8. CLI Enhancements
- Batch deploy: `pacinet deploy rules.yaml --label env=prod [--counters]` with per-node result table and summary line
- Fleet status: `pacinet status [--label env=prod]` with node counts by state and enriched node table
- Enriched node list: shows policy hash and heartbeat age columns
- Policy diff: `pacinet policy diff <node-a> <node-b>` using similar crate for unified diff
- Deploy command restructured: --node flag for single-node, --label for batch

#### 9. Dependency Changes
- Added rusqlite 0.32 (bundled), tonic-health 0.12 to workspace
- Added rusqlite, tonic-health to pacinet-server
- Added similar 2 to pacinet-cli

#### 10. Testing
- 26 tests total, all passing:
  - 5 model/core unit tests (creation, display, fromstr, transitions, deployment result)
  - 5 PacGate unit tests (JSON parsing, mock backend)
  - 9 MemoryStorage unit tests (register, remove, filter, state transitions, invalid transition, concurrent deploy, policy versioning, deployment audit, stale detection)
  - 7 integration tests (happy path, unreachable agent, PacGate failure, batch deploy all succeed, batch deploy partial failure, fleet status, stale node detection)
- cargo clippy --workspace -- -D warnings: clean

### Git Operations
- Committed Phase 3 changes
- Pushed to GitHub

## Session 4 — Security, Metrics, Rollback & CI (2026-02-27)

### Prompt
Implement Phase 4: mTLS security on all gRPC channels, Prometheus metrics, policy history & deployment audit RPCs, policy rollback, graceful shutdown, code quality fixes (unified hash, CPU usage, consistent state naming), SQLite storage tests, GitHub Actions CI, Makefile updates.

### Actions Taken

#### 1. Proto Changes
- Added 3 new RPCs to PaciNetManagement service: GetPolicyHistory, GetDeploymentHistory, RollbackPolicy
- Added 7 new message types: GetPolicyHistoryRequest/Response, PolicyVersionInfo, GetDeploymentHistoryRequest/Response, DeploymentInfo, RollbackPolicyRequest/Response

#### 2. Core Modules
- **hash.rs** (new): Unified `policy_hash()` function using SipHash, replacing duplicated `md5_hash()` and `hash_content()` across server and agent
- **tls.rs** (new): `TlsConfig` struct, `load_server_tls()` and `load_client_tls()` helpers using tonic's built-in TLS (rustls-backed)
- **lib.rs**: Exported `hash` and `tls` modules, added `pub use hash::policy_hash`

#### 3. Code Quality Fixes
- Removed `md5_hash()` from `pacinet-server/src/service.rs`, replaced with `pacinet_core::policy_hash()`
- Removed `hash_content()` from `pacinet-agent/src/service.rs`, replaced with `pacinet_core::policy_hash()`
- Removed `#[allow(dead_code)]` from AgentState — fields now used in shutdown handler
- Fixed CLI `state_name()` to return lowercase strings matching storage conventions

#### 4. Policy History & Rollback RPCs
- Implemented `get_policy_history` in ManagementService — delegates to `storage.get_policy_history()`
- Implemented `get_deployment_history` — delegates to `storage.get_deployments()`
- Implemented `rollback_policy` — fetches target version's YAML, re-deploys through existing `do_deploy()` flow
- CLI: added `policy history <node-id> [--limit N]` command
- CLI: added `policy rollback <node-id> [--version N]` command
- CLI: added `deploy history <node-id> [--limit N]` top-level command

#### 5. SQLite Storage Tests
- Added `#[cfg(test)] mod tests` to `sqlite.rs` with 9 tests mirroring MemoryStorage tests
- Tests use in-memory SQLite (`:memory:`) for speed
- All 18 storage tests pass (9 memory + 9 SQLite)

#### 6. Prometheus Metrics
- **metrics.rs** (new): `install_metrics()` starts PrometheusBuilder HTTP listener on configurable port
- Metric functions: `record_deploy()`, `record_heartbeat()`, `record_heartbeat_missed()`, `record_batch_deploy()`, `update_node_gauges()`, `record_uptime()`
- Instrumented: heartbeat handler, deploy flow (with timing histogram), batch deploy, stale node reaper
- Server main: `--metrics-port` flag (default 9090, 0 to disable)
- Reaper loop: updates uptime gauge and node count gauges on each tick

#### 7. Graceful Shutdown
- **Server**: `serve_with_shutdown` with `tokio::signal::ctrl_c()`, logs shutdown message
- **Agent**: `tokio::sync::watch` channel signals heartbeat loop to stop, `tokio::select!` in heartbeat loop, `serve_with_shutdown` for gRPC server, shutdown handler reads and logs AgentState fields

#### 8. mTLS on All Channels
- **Server**: `--ca-cert`, `--tls-cert`, `--tls-key` flags; `Server::builder().tls_config()` when present; ManagementService carries `tls_config` for controller→agent push connections
- **Agent**: `--ca-cert`, `--tls-cert`, `--tls-key` flags; `connect_to_controller()` helper with TLS support; agent gRPC server uses server TLS; heartbeat loop uses TLS for reconnections
- **CLI**: `--ca-cert`, `--tls-cert`, `--tls-key` global flags; `connect()` function handles TLS channel construction
- Agent address scheme switches http/https based on TLS config presence
- Backward compatible: all TLS flags optional, plain HTTP when absent

#### 9. CPU Usage Collection
- Agent heartbeat reports CPU load via `read_cpu_usage()` reading `/proc/loadavg`
- Returns 1-minute load average as proxy metric, falls back to 0.0

#### 10. Dev Certificate Generation
- **scripts/gen-certs.sh**: generates CA, server, agent, and CLI client certs using openssl
- Outputs to configurable `certs/` directory
- Development/testing only

#### 11. GitHub Actions CI
- **.github/workflows/ci.yml**: runs on push and pull_request
- Steps: checkout, rust-toolchain (stable), rust-cache, cargo check, clippy (-D warnings), test, fmt check

#### 12. Makefile Updates
- Added `gen-certs` target (runs gen-certs.sh)
- Added `run-server-tls` target (server with mTLS + SQLite + metrics)
- Added `run-agent-tls` target (agent with mTLS)

#### 13. Documentation Updates
- Updated CLAUDE.md: added mTLS, metrics, graceful shutdown, CI features; updated architecture diagram; added new commands; updated design decisions
- Updated OVERVIEW.md: added security section, observability section, updated technology stack, updated status to Phase 4
- Updated REQUIREMENTS.md: added security (6), observability (8), graceful shutdown (9.4), CI/CD (9.6), policy rollback (2.5), new CLI commands, new gRPC services, metrics port
- Updated PROMPT_HISTORY.md: added Session 4 with full details
- Updated .gitignore: added certs/, *.db files

#### 14. Dependency Changes
- Workspace Cargo.toml: `tonic` features = ["tls"], `metrics = "0.24"`, `metrics-exporter-prometheus = { version = "0.16", features = ["http-listener"] }`
- pacinet-server: added `metrics`, `metrics-exporter-prometheus`

### Errors Encountered & Fixed
- **tonic TLS types not found**: needed `features = ["tls"]` on tonic in workspace Cargo.toml
- **Box<dyn Error> vs Box<dyn Error + Send + Sync>**: tls.rs returns `Send + Sync`, main.rs callers use `.map_err()` conversion
- **Clippy too_many_arguments**: handle_deploy had 8 args, added `#[allow]` attribute
- **Clippy print_literal**: inlined literal strings into format patterns
- **dead_code on AgentState fields**: resolved by reading fields in shutdown handler

### Test Results
- 35 tests total, all passing:
  - 7 core tests (model + hash)
  - 18 server storage tests (9 memory + 9 SQLite)
  - 10 agent tests (5 pacgate + 5 service)
  - 7 integration tests
- cargo clippy --workspace -- -D warnings: clean

### Git Operations
- Committed Phase 4 changes
- Pushed to GitHub
