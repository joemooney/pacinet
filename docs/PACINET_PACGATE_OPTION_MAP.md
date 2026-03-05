# PaciNet <-> PacGate Compile Option Map

This guide defines how PaciNet compile options map to `pacgate` CLI flags.

## Option Mapping

| PaciNet field | PacGate flag | Default | Notes |
|---|---|---:|---|
| `counters` | `--counters` | `false` | Enables per-rule counters. |
| `rate_limit` | `--rate-limit` | `false` | Enables rate-limit RTL path. |
| `conntrack` | `--conntrack` | `false` | Enables connection-tracking RTL. |
| `axi` | `--axi` | `false` | Required by several advanced features. |
| `ports` | `--ports <N>` | `1` | Multi-port wrapper. |
| `target` | `--target <name>` | `standalone` | `standalone`, `opennic`, `corundum`. |
| `dynamic` | `--dynamic` | `false` | Runtime-updateable flow table mode. |
| `dynamic_entries` | `--dynamic-entries <N>` | `16` | Used when `dynamic` is enabled. |
| `width` | `--width <bits>` | `8` | Supported by pacgate: `8/64/128/256/512`. |
| `ptp` | `--ptp` | `false` | Includes PTP hardware clock path. |
| `rss` | `--rss` | `false` | Enables RSS multi-queue dispatch. |
| `rss_queues` | `--rss-queues <N>` | `4` | Queue count `1..16`; non-default implies RSS path. |
| `int_enabled` | `--int` | `false` | Enables INT metadata output path. |
| `int_switch_id` | `--int-switch-id <id>` | `0` | Non-zero implies INT path. |

## Capability Keys Used By PaciNet

PaciNet server validates node capabilities before deployment:

- `compile.axi`
- `compile.ports`
- `compile.target`
- `compile.dynamic`
- `compile.width`
- `compile.ptp`
- `compile.rss`
- `compile.rss_queues`
- `compile.int`
- `compile.int_switch_id`

## Normalization Rules In PaciNet Server

Before capability checks and deployment, PaciNet normalizes missing/zero values:

- `ports=0` -> `1`
- `dynamic_entries=0` -> `16`
- `width=0` -> `8`
- `rss_queues=0` -> `4`
- empty `target` -> `standalone`

This keeps behavior deterministic when clients omit newer fields.
