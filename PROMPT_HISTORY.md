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
