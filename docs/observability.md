---
type: reference
status: active
review_cycle: 180
last_reviewed: 2026-05-30
---

# Observability: OTLP metrics, Prometheus, and Grafana

Aetower exports its host and per-process metrics over **OTLP/HTTP (JSON)** to an
OpenTelemetry Collector. The collector then forwards to Prometheus (or any OTLP
sink), and Grafana visualizes them. A ready-made dashboard ships in
[`assets/grafana/aetower-dashboard.json`](../assets/grafana/aetower-dashboard.json).

This counters the typical "single-machine glances" tooling by making Aetower's
signal first-class in an ops stack.

## Enabling export

In **Settings → Integrations → Observability export**, enable export and set the
collector endpoint (default `http://localhost:4318/v1/metrics`). Or override via
environment:

- `AETOWER_OTLP_ENDPOINT` — full OTLP/HTTP metrics URL (default
  `http://localhost:4318/v1/metrics`).
- `AETOWER_OTLP_INTERVAL_SECS` — export cadence in seconds (default `30`,
  minimum `5`).

All metrics are **gauges**. The export carries the resource attribute
`service.name=aetower`. Per-entity metrics add the labels `entity.id`,
`entity.name`, and `entity.kind` — so cardinality is `26 + (entities × 6)` data
points per export. On a busy machine with ~100 attributed entities that's ~626
series; size your collector/Prometheus retention accordingly.

> Metric names are dotted on the OTLP wire (`aetower.host.cpu_percent`). The
> OpenTelemetry Collector's Prometheus exporter rewrites dots to underscores, so
> in Prometheus/Grafana you query `aetower_host_cpu_percent`. The collector may
> also append unit suffixes; if a query returns nothing, check the exact series
> name in Prometheus first.

## Metric reference

### Host metrics (`aetower.host.*`)

| OTLP name | Prometheus name | Unit | Source |
| --- | --- | --- | --- |
| `aetower.host.cpu_percent` | `aetower_host_cpu_percent` | percent | host CPU |
| `aetower.host.memory_used_bytes` | `aetower_host_memory_used_bytes` | bytes | host memory used |
| `aetower.host.machine_friction` | `aetower_host_machine_friction` | score | aggregate machine friction |
| `aetower.host.thermal_state` | `aetower_host_thermal_state` | enum (0–3) | thermal state (label `state`) |
| `aetower.host.gpu_percent` | `aetower_host_gpu_percent` | percent | GPU utilization |
| `aetower.host.ane_percent` | `aetower_host_ane_percent` | percent | Apple Neural Engine utilization |
| `aetower.host.ai_agent_friction` | `aetower_host_ai_agent_friction` | score | AI-agent friction (label `count`) |

### Engine / UI latency (`aetower.host.*_millis`, gauges)

`engine_tick_millis`, `collect_millis`, `identity_millis`, `attribution_millis`,
`friction_millis`, `enrich_millis`, `history_millis`, `persist_millis`,
`gpu_sample_millis`, `target_tick_millis`, `bridge_fetch_millis`,
`ui_refresh_millis`, `snapshot_to_ui_millis`, `snapshot_to_render_millis`,
`render_commit_millis`, `display_frame_interval_millis`,
`input_avg_latency_millis`, `input_max_latency_millis` (all milliseconds), plus
`history_queue_depth`, `diagnostics_queue_depth`, `display_dropped_frames`,
`input_sample_count` (counts) and `display_refresh_hz` (Hz). Each is exported as
`aetower.host.<name>` → `aetower_host_<name>`.

### Per-entity metrics (`aetower.entity.*`, labels `entity.id` / `entity.name` / `entity.kind`)

| OTLP name | Prometheus name | Unit | Source |
| --- | --- | --- | --- |
| `aetower.entity.friction` | `aetower_entity_friction` | score | per-entity friction |
| `aetower.entity.cpu_percent` | `aetower_entity_cpu_percent` | percent | per-entity CPU |
| `aetower.entity.memory_bytes` | `aetower_entity_memory_bytes` | bytes | resident memory |
| `aetower.entity.energy_impact` | `aetower_entity_energy_impact` | score | energy-impact score |
| `aetower.entity.network_bps` | `aetower_entity_network_bps` | bytes/s | rx+tx throughput |
| `aetower.entity.wakeups_per_second` | `aetower_entity_wakeups_per_second` | wakeups/s | wakeup rate |

The metric-name constants are the single source of truth in
`rust/crates/aetower-telemetry/src/lib.rs` (`pub mod metric_names`).

## Collector + Prometheus setup

Minimal `otel-collector-config.yaml` that receives OTLP/HTTP from Aetower and
exposes a Prometheus scrape endpoint:

```yaml
receivers:
  otlp:
    protocols:
      http:
        endpoint: 0.0.0.0:4318

exporters:
  prometheus:
    endpoint: 0.0.0.0:9464

service:
  pipelines:
    metrics:
      receivers: [otlp]
      exporters: [prometheus]
```

Prometheus scrape config:

```yaml
scrape_configs:
  - job_name: aetower
    static_configs:
      - targets: ["localhost:9464"]
```

Run the collector, point Prometheus at `:9464`, and enable Aetower's export to
`http://localhost:4318/v1/metrics`.

## Importing the Grafana dashboard

1. In Grafana, **Dashboards → New → Import**.
2. Upload [`assets/grafana/aetower-dashboard.json`](../assets/grafana/aetower-dashboard.json).
3. Select your Prometheus data source when prompted (the dashboard uses a
   templated `${datasource}` variable).

The dashboard includes host CPU/memory/GPU/thermal, machine + AI-agent friction,
a top-N per-entity friction table, per-entity energy and network, and an
engine-latency row.

## Verifying the exporter

`sh scripts/telemetry-smoke.sh` runs a temporary in-process OTLP/HTTP receiver
and confirms Aetower delivers a well-formed payload to `/v1/metrics`.
