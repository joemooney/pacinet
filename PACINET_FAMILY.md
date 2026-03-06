# PaciNet Family Reference

## Purpose
This document is the canonical map of the Paci* product family:

- `/home/joe/ai/pacgate`
- `/home/joe/ai/pacilab`
- `/home/joe/ai/pacilearn`
- `/home/joe/ai/pacinet`
- `/home/joe/ai/paciview`
- `/home/joe/ai/pacmate`

It explains what each product does, why it exists, who uses it, when to use it, and how the products work together as one coherent platform.

## One-Line Positioning
The Paci* family is a full-stack network policy platform that spans:

1. Policy authoring and compilation (`pacgate`)
2. Fleet control and rollout (`pacinet`)
3. Test/simulation and validation (`pacilab`, `pacilearn`)
4. Security operations and response (`pacmate`)
5. Observability and operations dashboards (`paciview`)

## Product Map

| Product | Core Role | Primary Users | Typical Stage |
|---|---|---|---|
| `pacgate` | Packet-filter compiler/runtime from YAML to hardware/software outputs | FPGA engineers, network platform engineers | Build/design-time + runtime compile checks |
| `pacinet` | SDN-style control plane for managing fleets of PacGate-enabled nodes | Network operators, SRE/platform teams | Day-2 operations, rollout, fleet control |
| `pacilab` | Scenario validation/regression/topology testing (implemented as pacgate subcommands) | Validation engineers, CI owners | Pre-deploy testing, CI gates |
| `pacilearn` | Beginner-first workshop/lab environment with simulation-first workflows | New users, solution engineers, field teams | Onboarding, enablement, demos |
| `pacmate` | Security operations plane (detect/deceive/respond/forensics/intel) with FSM playbooks | SecOps, incident response, blue team | Threat operations and response automation |
| `paciview` | Prometheus/Grafana dashboards and alerting for PaciNet + PacMate | NOC/SOC, SRE, management dashboards | Monitoring, reporting, operational visibility |

## Why The Family Exists
Most teams today stitch together separate tools for policy authoring, testing, rollout, incident response, and observability. This creates drift and brittle handoffs.

The Paci* family is designed to remove that fragmentation:

- One policy model (`YAML`) flowing through compile, test, deploy, and operate
- Shared automation concepts (FSM workflows, audit/history, dry-run/rollback)
- Consistent operator surfaces (CLI + web + metrics)
- Simulation-first learning and validation before touching hardware

## System Narrative (End-to-End)
A typical lifecycle looks like this:

1. Author policy in YAML (`pacgate`).
2. Validate and stress test using scenarios/topologies (`pacilab` via pacgate commands).
3. Learn or demo workflows without hardware in local labs (`pacilearn`).
4. Deploy and operate policy across real node fleets (`pacinet`).
5. Handle security events and orchestrated response (`pacmate`).
6. Monitor everything in a unified observability plane (`paciview`).

## Integration Model

### Data/Control Interfaces
- **Policy contract:** YAML rule specs are the primary contract between products.
- **Compile/execute:** `pacinet-agent` and `pacmate-agent` invoke `pacgate` for policy/rule actions.
- **Control APIs:** gRPC + REST in `pacinet`; gRPC in `pacmate`.
- **Telemetry:** Prometheus metrics, event streams, audit logs.
- **Visualization:** Grafana dashboards in `paciview`; rich web UI in `pacinet-web`.

### High-Level Flow

```text
Policy YAML
  -> pacgate (compile/validate/simulate)
  -> pacilab scenarios/regress/topology checks
  -> pacinet deploy/rollback/FSM rollouts to node fleet
  -> pacmate security actions (containment, honeypots, captures, intel-driven response)
  -> paciview dashboards/alerts for ongoing operations
```

## Who Uses What

### FPGA / Platform Engineer
- Uses `pacgate` for rule-to-implementation generation and verification.
- Uses `pacilab` to prove behavior under known scenarios.

### Network Operator / SRE
- Uses `pacinet` for fleet lifecycle: node state, deploy, dry-run, rollback, history, counters, FSM rollouts.
- Uses `paciview` for SLO/SLA and operations visibility.

### Security Operations Team
- Uses `pacmate` for detection/deception/response/forensics/intelligence orchestration.
- Uses `paciview` for incident trend visibility and alerting.

### New User / Solution Architect / Field Engineer
- Uses `pacilearn` to understand the full stack through simulation-first workshops.

## When To Use Which Product

### Use `pacgate` when
- You need to compile and verify policy logic itself.
- You are changing low-level packet rule behavior.

### Use `pacilab` when
- You need reproducible scenario regression in CI.
- You need traffic/topology behavior checks before rollout.

### Use `pacinet` when
- You need centralized, multi-node policy operations.
- You need deployment governance (history, rollback, FSM orchestration).

### Use `pacmate` when
- You need active defense and incident-response workflows.
- You need to correlate security signals and automate containment.

### Use `paciview` when
- You need operational dashboards and alerting across the platform.

### Use `pacilearn` when
- You need onboarding/training without hardware dependencies.

## Cohesion Principles Across The Family

1. **YAML-first policy lifecycle**
   - The same policy artifact should flow from authoring to deployment and validation.

2. **Simulation-first before hardware-first**
   - Validate behavior locally before risking production or hardware cycles.

3. **No hidden state transitions**
   - Use explicit history/audit trails and visible rollout mechanisms.

4. **Composable automation**
   - Reusable templates, FSMs, and scenario definitions are first-class.

5. **Operational symmetry**
   - CLI and web workflows should remain feature-aligned where practical.

## Marketplace Fit

### Problem Space
The family addresses teams building and operating programmable network controls where they need both:

- Deterministic policy behavior (data-plane confidence)
- Scalable operational control (control-plane confidence)

### Competitive Angle
The key differentiator is full-lifecycle continuity:

- One policy definition language flowing through compile, test, deploy, observe, and secure.
- Combined hardware-awareness and software-native operations.
- Built-in simulation and workshop path to reduce adoption friction.

### Likely Buyer/User Environments
- Network/security appliance teams
- Private infrastructure operators (edge, industrial, campus, lab-heavy environments)
- Engineering-led organizations that need verifiable policy pipelines

## Practical Adoption Path

1. **Start with `pacilearn`** for onboarding and shared mental model.
2. **Adopt `pacgate` + `pacilab`** in engineering/CI to harden rule correctness.
3. **Introduce `pacinet`** for centralized multi-node rollout and governance.
4. **Add `paciview`** for operational visibility and alerting.
5. **Layer in `pacmate`** where security automation and incident response are required.

## For AI Agents (Conversation Context)
When assisting users across repos, apply these assumptions:

- `pacgate` is the policy compiler/engine, not the fleet controller.
- `pacinet` is the fleet control plane and operational UI/CLI center.
- `pacilab` capabilities currently run through `pacgate` subcommands.
- `pacilearn` is the onboarding/tutorial environment and should stay beginner-friendly.
- `pacmate` is the security ops plane with detection/response workflows.
- `paciview` is observability packaging (Prometheus/Grafana dashboards and alerts).

If a feature request crosses repos, preserve this division of responsibilities unless explicitly re-architecting.

## Current Family Direction (2026)
- Strengthen template-driven composition and rollout ergonomics in `pacinet`.
- Keep workshop-driven adoption in `pacilearn` up to date with real UI/CLI parity.
- Expand policy/test artifact reuse across `pacgate`, `pacilab`, and `pacinet`.
- Maintain operational observability parity across `pacinet` and `pacmate` via `paciview`.

## Quick Repo Links
- `pacgate`: `/home/joe/ai/pacgate`
- `pacilab`: `/home/joe/ai/pacilab`
- `pacilearn`: `/home/joe/ai/pacilearn`
- `pacinet`: `/home/joe/ai/pacinet`
- `paciview`: `/home/joe/ai/paciview`
- `pacmate`: `/home/joe/ai/pacmate`
