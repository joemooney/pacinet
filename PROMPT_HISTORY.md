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

## Session 5 — YAML-Defined FSM Engine (2026-02-27)

### Prompt
Implement Phase 5: YAML-defined FSM engine for deployment orchestration. Add a generic FSM engine to pacinet-core (reusable by PacGate) and integrate it into PaciNet for deployment state machines — operator-defined rollout strategies (canary, staged, rollback) expressed as YAML FSMs.

### Actions Taken

#### 1. FSM Types in pacinet-core (`pacinet-core/src/fsm/`)
- **mod.rs**: Module root with re-exports and `parse_duration()` utility (parses "5m"/"30s"/"1h"/"2h30m" into std::time::Duration). 6 unit tests.
- **definition.rs**: YAML-parseable FSM definition types — FsmDefinition, StateDefinition, TransitionDefinition, ConditionDefinition (enum: Simple/Counter/Compound), ActionDefinition (struct with optional fields: deploy/rollback/alert), DeployAction, NodeSelector, CompileOptions, RollbackAction, AlertAction. `from_yaml()` and `validate()` methods. 9 unit tests.
- **instance.rs**: Runtime FSM execution types — FsmInstance, FsmContext, ActionResult, NodeActionResult, FsmTransitionRecord, TransitionTrigger, FsmInstanceStatus. `new()`, `transition()`, `is_running()` methods. 3 unit tests.
- **error.rs**: FsmError enum (InvalidDefinition, InstanceNotFound, DefinitionNotFound, InvalidState, NoTransition, AlreadyCompleted, ActionError, YamlParse).
- **lib.rs**: Added `pub mod fsm;` and re-exports for FsmDefinition, FsmError, FsmInstance, FsmInstanceStatus, FsmKind.
- **error.rs**: Added `Fsm(FsmError)` variant to PaciNetError with tonic Status mappings.

#### 2. Storage Trait Extension
- Extended `Storage` trait with 8 FSM methods: store/get/list/delete FSM definitions, store/get/update/list FSM instances.
- **MemoryStorage**: Added `fsm_definitions: RwLock<HashMap>` and `fsm_instances: RwLock<HashMap>`, implemented all 8 methods.
- **SqliteStorage**: Added `fsm_definitions` and `fsm_instances` tables with JSON blob storage, implemented all 8 methods.
- **schema.sql**: Added fsm_definitions table (name PK), fsm_instances table (instance_id PK) with indexes on status and definition_name.

#### 3. Proto + CRUD RPCs
- Added 9 RPCs to PaciNetManagement: CreateFsmDefinition, GetFsmDefinition, ListFsmDefinitions, DeleteFsmDefinition, StartFsm, GetFsmInstance, ListFsmInstances, AdvanceFsm, CancelFsm.
- Added ~15 new message types (request/response pairs + FsmInstanceInfo, FsmTransitionInfo, FsmDefinitionSummary).

#### 4. Shared Deploy Module (`pacinet-server/src/deploy.rs`)
- Extracted deploy logic from ManagementService into reusable functions: `deploy_to_node()`, `deploy_to_nodes()`, `forward_deploy_to_agent()`.
- ManagementService refactored to delegate to shared module.
- FsmEngine uses same shared module for deploy actions.

#### 5. FSM Engine (`pacinet-server/src/fsm_engine.rs`)
- Background evaluation loop (5s interval) with shutdown watch channel.
- `start_instance()`: creates instance, executes initial state action.
- `advance_instance()`: manual advance for `manual: true` conditions.
- `cancel_instance()`: marks instance as Cancelled.
- `evaluate_instance()`: evaluates transitions (condition/timer), fires transitions, executes target state actions.
- `evaluate_condition()`: handles Simple (all_succeeded/any_failed/manual), Counter (deferred), Compound (and/or/not).
- `execute_deploy()`: selects nodes by labels, applies limit/batch_percent, calls shared deploy module.
- `execute_rollback()`: rolls back deployed nodes to previous policy version.
- `execute_alert()`: log-only alert.

#### 6. FSM RPCs in ManagementService
- Added `fsm_engine: Option<Arc<FsmEngine>>` field.
- Implemented all 9 FSM RPCs delegating to engine and storage.
- Added `instance_to_proto()` helper for FsmInstance → FsmInstanceInfo conversion.

#### 7. CLI FSM Subcommands (`pacinet-cli/src/main.rs`)
- Added `FsmCommands` enum with 9 subcommands: Create, List, Show, Delete, Start, InstanceStatus, Instances, Advance, Cancel.
- Full implementation for all subcommands with human-readable and JSON output.

#### 8. Server Main Wiring (`pacinet-server/src/main.rs`)
- Creates `Arc<FsmEngine>` with storage, config, and TLS config.
- Spawns FSM engine background loop with watch channel shutdown.
- Passes engine to ManagementService via `.with_fsm_engine()`.
- Updated shutdown handler to signal FSM engine.

#### 9. FSM Metrics (`pacinet-server/src/metrics.rs`)
- `record_fsm_transition()`: increments transition counter.
- `record_fsm_instance_status()`: increments instance lifecycle counter.
- `update_fsm_running_gauge()`: updates running instances gauge.

#### 10. Integration Tests
- 6 new FSM integration tests:
  - `test_fsm_definition_crud`: create, get, list, delete definitions via gRPC.
  - `test_fsm_start_and_auto_complete`: deploy succeeds → all_succeeded → complete (terminal).
  - `test_fsm_manual_advance`: deploy then manually advance to terminal state.
  - `test_fsm_cancel_running_instance`: start then cancel before timer fires.
  - `test_fsm_list_instances`: filter by definition name and status.
  - `test_fsm_deploy_failure_triggers_transition`: unreachable agent → any_failed → failed (terminal).

#### 11. Example YAML
- Created `examples/canary-rollout.yaml` with canary → validate → staged → complete/rollback FSM.

#### 12. Documentation
- Updated CLAUDE.md: added FSM features, FSM RPCs, FSM design decisions.
- Updated OVERVIEW.md: added FSM engine to component descriptions, updated status to Phase 5.
- Updated REQUIREMENTS.md: added section 3 (FSM Engine) with 7 subsections, added FSM CLI commands, FSM metrics, FSM integration tests, renumbered subsequent sections.
- Updated PROMPT_HISTORY.md: added Session 5 with full details.

### Errors Encountered & Fixed
- **serde_yaml 0.9 enum serialization**: Changed ActionDefinition from enum to struct with optional fields.
- **Missing imports in fsm_engine.rs**: Added ConditionDefinition, RollbackAction, AlertAction to `pub use` in mod.rs.
- **Deprecated chrono method**: Replaced `chrono::Duration::max_value()` with `chrono::Duration::MAX`.
- **Type inference errors**: Added explicit type annotation `|c: &ConditionDefinition|` for compound condition closures.
- **Unused imports in service.rs**: Removed DeploymentRecord, DeploymentResult, debug after deploy refactoring.

### Test Results
- 68 tests total, all passing:
  - 27 core tests (7 model/hash + 18 FSM definition/instance/parse_duration + 2 hash)
  - 18 server storage tests (9 memory + 9 SQLite)
  - 10 agent tests (5 pacgate × 2)
  - 13 integration tests (7 existing + 6 FSM)
- cargo clippy --workspace -- -D warnings: clean

### Git Operations
- Committed Phase 5 changes
- Pushed to GitHub

## Session 6 — Counter Rate Tracking & Adaptive Policy FSMs (Phase 5b) (2026-02-27)

### Prompt
Implement Phase 5b: Counter rate tracking (timestamped counter snapshots with rate calculation), adaptive policy FSMs (counter conditions that fire when rates exceed thresholds), and webhook delivery (AlertAction sends HTTP webhooks).

### Actions Taken

#### 1. Core Types (pacinet-core)
- **model.rs**: Added `CounterSnapshot` struct (node_id, collected_at, counters Vec)
- **fsm/definition.rs**: Extended `CounterCondition` with `aggregate: Option<String>` and `field: Option<String>`. Added `WebhookConfig` struct (url, method, bearer_token, basic_auth, timeout_seconds, max_retries, headers). Added `BasicAuth` struct. Extended `AlertAction` with `webhook: Option<WebhookConfig>`. Added counter condition validation in `validate()`. Reordered `ConditionDefinition` enum to Counter, Simple, Compound for correct `serde(untagged)` deserialization. 5 new unit tests.
- **fsm/instance.rs**: Extended `FsmContext` with `counter_condition_first_true: HashMap<String, DateTime<Utc>>`. Added `FsmContext::for_adaptive_policy(target_nodes)` constructor.
- **fsm/mod.rs**: Added exports for `BasicAuth`, `CounterCondition`, `WebhookConfig`
- **lib.rs**: Added `CounterSnapshot` to re-exports

#### 2. Counter Cache (pacinet-server/src/counter_cache.rs) — NEW
- In-memory ring buffer `CounterSnapshotCache` using `RwLock<HashMap<String, VecDeque<CounterSnapshot>>>`
- Methods: `record()`, `latest_pair()`, `latest()`, `snapshots_in_window()`, `remove_node()`, `evict_expired()`, `node_ids()`, `total_snapshots()`
- Configurable retention period and max snapshots per node
- 6 unit tests

#### 3. Counter Rate Calculation (pacinet-server/src/counter_rate.rs) — NEW
- `CounterRate` struct, `AggregateMode` enum (Any/All/Sum)
- `calculate_rate()`: rate from two snapshots with counter reset handling (newer < older → rate = 0)
- `get_counter_total()`: absolute value lookup
- `parse_aggregate_mode()`: string parsing with Any default
- 6 unit tests

#### 4. Webhook Delivery (pacinet-server/src/webhook.rs) — NEW
- `WebhookPayload` struct (Serialize) with event, instance_id, definition_name, current_state, message, timestamp, deployed_nodes
- `deliver_webhook()`: async HTTP POST with reqwest, bearer/basic auth, custom headers, exponential backoff retry (max 2)
- Fire-and-forget via `tokio::spawn` in FSM engine

#### 5. Server Infrastructure
- **lib.rs**: Added `pub mod counter_cache;`, `counter_rate;`, `webhook;`
- **config.rs**: Added `counter_snapshot_retention: Duration` (default 1h) and `counter_snapshot_max_per_node: usize` (default 120)
- **metrics.rs**: Added `record_counter_snapshot()`, `update_counter_snapshot_gauge()`, `record_webhook_delivery()`, `record_counter_eval()`
- **Cargo.toml** (workspace + server): Added `reqwest = { version = "0.12", features = ["json", "rustls-tls"] }`

#### 6. ControllerService Updates (service.rs)
- Added `counter_cache: Option<Arc<CounterSnapshotCache>>` field with `with_counter_cache()` builder
- `report_counters()` now records snapshot in cache after `store_counters()`
- `start_fsm()` routes to `start_adaptive_instance()` when `target_label_filter` is non-empty

#### 7. Server Main (main.rs)
- Added CLI args: `--counter-retention-secs` (default 3600), `--counter-max-per-node` (default 120)
- Creates `Arc<CounterSnapshotCache>`, passes to FsmEngine and ControllerService
- Reaper loop calls `evict_expired()` and updates snapshot gauge metric

#### 8. FSM Engine Rewrite (fsm_engine.rs)
- Added `counter_cache: Arc<CounterSnapshotCache>` field
- `evaluate_counter_condition()`: checks rate/total thresholds per node, applies aggregate modes (Any/All/Sum), manages `for_duration` tracking via `counter_condition_first_true` HashMap
- `start_adaptive_instance()`: selects nodes by label, creates `FsmContext::for_adaptive_policy()`
- `execute_alert()`: spawns webhook delivery if webhook config present
- `fire_transition()`: clears `counter_condition_first_true` entries for old state on transitions
- `evaluate_instance()`: persists instance at end for counter_condition_first_true updates

#### 9. Proto + CLI Updates
- **proto/pacinet.proto**: Added `map<string, string> target_label_filter = 4` to `StartFsmRequest`
- **pacinet-cli/src/main.rs**: `fsm start` now has optional `--rules` and `--label key=val` args; builds `target_label_filter` HashMap

#### 10. Integration Tests (pacinet-server/tests/integration.rs)
- Updated `start_controller_with_fsm()` to create and return CounterSnapshotCache
- All existing StartFsmRequest constructors updated with `target_label_filter: HashMap::new()`
- 4 new tests: `test_counter_snapshot_cache_basic`, `test_counter_rate_calculation`, `test_counter_condition_fires_transition`, `test_counter_condition_for_duration`

#### 11. Example YAML
- `examples/ddos-auto-escalate.yaml`: adaptive policy FSM with monitoring → escalating → escalated → de_escalating cycle, webhook alerts

### Errors Encountered & Fixed
- **serde(untagged) enum ordering**: `SimpleCondition` (all optional fields) matched any map before `CounterCondition` could try. Fixed by reordering enum to Counter, Simple, Compound.
- **CompoundCondition matching before SimpleCondition**: When ordered Counter, Compound, Simple — `{all_succeeded: true}` deserialized as empty Compound. Fixed by putting Simple before Compound.
- **Missing `DateTime` import in counter_cache test module**: Added `use chrono::DateTime;` in test mod after removing from main imports.
- **Missing `target_label_filter` field in integration tests**: 6 existing StartFsmRequest constructors needed `target_label_filter: HashMap::new()` after proto change.

### Test Results
- 89 tests total, all passing:
  - 32 core tests (7 model/hash + 23 FSM definition/instance/parse_duration + 2 hash)
  - 30 server unit tests (18 storage + 6 counter_cache + 6 counter_rate)
  - 10 agent tests (5 pacgate + 5 service)
  - 17 integration tests (13 existing + 4 new counter/FSM)
- cargo clippy --workspace -- -D warnings: clean

### Git Operations
- Committed Phase 5b changes
- Pushed to GitHub
