# PaciNet — SDN Controller for PacGate

## Feature Summary
- **SDN controller** managing multiple PacGate FPGA packet filter nodes
- **gRPC-based** architecture: controller (southbound + northbound), agent, CLI
- **Node lifecycle**: registration, heartbeat, policy deployment, counter collection
- **End-to-end deployment**: CLI → Controller → Agent → PacGate (Phase 2)
- **PacGate integration**: agent invokes `pacgate` CLI as subprocess (YAML interface)
- **PacGateBackend abstraction**: Real (subprocess) or Mock (for testing)

## Architecture
```
┌──────────┐    northbound    ┌────────────────┐    southbound    ┌──────────────┐
│ CLI      │───────gRPC──────▶│ Controller     │◀───────gRPC──────│ Agent        │
│ (pacinet)│                  │ (pacinet-server)│───────gRPC──────▶│ (per node)   │
└──────────┘                  └────────────────┘                  └──────┬───────┘
                                                                        │
                                                                  ┌─────▼───────┐
                                                                  │ PacGate CLI │
                                                                  │ (subprocess)│
                                                                  └─────────────┘
```

### Workspace Crates
| Crate | Type | Purpose |
|-------|------|---------|
| `pacinet-proto` | lib | Generated gRPC/protobuf types |
| `pacinet-core` | lib | Domain model, error types |
| `pacinet-server` | lib+bin | Controller (port 50054) |
| `pacinet-agent` | lib+bin | Node agent (port 50055) |
| `pacinet-cli` | bin | Operator CLI (`pacinet`) |

### gRPC Services
- **PaciNetController** (agent → controller): RegisterNode, Heartbeat, ReportCounters
- **PaciNetAgent** (controller → agent): DeployRules, GetCounters, GetStatus
- **PaciNetManagement** (CLI → controller): ListNodes, GetNode, RemoveNode, DeployPolicy, GetPolicy, GetNodeCounters, GetAggregateCounters

## Common Commands
```bash
cargo build                    # Build all crates
cargo test                     # Run all unit + integration tests
make run-server                # Start controller on :50054
make run-agent                 # Start agent, connect to controller
make node-list                 # List nodes via CLI
make integration-test          # Run integration tests only
make test-all                  # Run tests + clippy
```

## Key Design Decisions
- **tonic 0.12 + prost 0.13** for gRPC (matching aida/dsl4test)
- **In-memory registry** (Arc<RwLock<HashMap>>) — no database for MVP
- **PacGate subprocess** via tokio::process::Command — YAML is the interface contract
- **PacGateBackend enum** (Real|Mock) for testability without PacGate binary
- **Deploy forwarding**: controller connects to agent gRPC, 30s timeout, graceful failure
- **Node state transitions**: Deploying → Active (success) or Error (failure)
- Proto types do NOT have serde derives (prost_types::Timestamp incompatibility)
- Domain types in pacinet-core have serde derives for JSON serialization
- Both server and agent expose lib targets for integration testing

## Port Assignments
- Controller: 50054
- Agent: 50055 (configurable per node)
