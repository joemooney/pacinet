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
