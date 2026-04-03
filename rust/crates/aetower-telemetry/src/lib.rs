use std::{
    io::{Read, Write},
    net::TcpStream,
    time::Duration,
};

use aetower_diagnostics::{
    DiagnosticsEvent, DiagnosticsLevel, DiagnosticsStore, DiagnosticsSubsystem,
};
use aetower_model::{EntitySnapshot, RuntimeLagMetrics, SystemSnapshot, ThermalState};
use serde::Serialize;
use url::Url;

/// Configuration for the OpenTelemetry exporter.
#[derive(Debug, Clone)]
pub struct OtlpConfig {
    pub endpoint: String,
    pub export_interval_secs: u32,
    pub enabled: bool,
}

impl Default for OtlpConfig {
    fn default() -> Self {
        Self {
            endpoint: "http://localhost:4318/v1/metrics".to_owned(),
            export_interval_secs: 30,
            enabled: false,
        }
    }
}

/// Metric names exported by Aetower.
pub mod metric_names {
    pub const HOST_CPU: &str = "aetower.host.cpu_percent";
    pub const HOST_MEMORY: &str = "aetower.host.memory_used_bytes";
    pub const HOST_FRICTION: &str = "aetower.host.machine_friction";
    pub const HOST_THERMAL: &str = "aetower.host.thermal_state";
    pub const HOST_GPU: &str = "aetower.host.gpu_percent";
    pub const HOST_ANE: &str = "aetower.host.ane_percent";
    pub const HOST_AI_FRICTION: &str = "aetower.host.ai_agent_friction";
    pub const ENTITY_FRICTION: &str = "aetower.entity.friction";
    pub const ENTITY_CPU: &str = "aetower.entity.cpu_percent";
    pub const ENTITY_MEMORY: &str = "aetower.entity.memory_bytes";
    pub const ENTITY_ENERGY: &str = "aetower.entity.energy_impact";
    pub const ENTITY_NETWORK: &str = "aetower.entity.network_bps";
    pub const ENTITY_WAKEUPS: &str = "aetower.entity.wakeups_per_second";
    pub const HOST_ENGINE_TICK_LATENCY: &str = "aetower.host.engine_tick_millis";
    pub const HOST_COLLECT_LATENCY: &str = "aetower.host.collect_millis";
    pub const HOST_IDENTITY_LATENCY: &str = "aetower.host.identity_millis";
    pub const HOST_ATTRIBUTION_LATENCY: &str = "aetower.host.attribution_millis";
    pub const HOST_FRICTION_LATENCY: &str = "aetower.host.friction_millis";
    pub const HOST_ENRICH_LATENCY: &str = "aetower.host.enrich_millis";
    pub const HOST_HISTORY_LATENCY: &str = "aetower.host.history_millis";
    pub const HOST_PERSIST_LATENCY: &str = "aetower.host.persist_millis";
    pub const HOST_BRIDGE_FETCH_LATENCY: &str = "aetower.host.bridge_fetch_millis";
    pub const HOST_UI_REFRESH_LATENCY: &str = "aetower.host.ui_refresh_millis";
    pub const HOST_SNAPSHOT_TO_UI_LATENCY: &str = "aetower.host.snapshot_to_ui_millis";
    pub const HOST_SNAPSHOT_TO_RENDER_LATENCY: &str = "aetower.host.snapshot_to_render_millis";
    pub const HOST_RENDER_COMMIT_LATENCY: &str = "aetower.host.render_commit_millis";
    pub const HOST_DISPLAY_FRAME_INTERVAL: &str = "aetower.host.display_frame_interval_millis";
    pub const HOST_DISPLAY_REFRESH_HZ: &str = "aetower.host.display_refresh_hz";
    pub const HOST_DISPLAY_DROPPED_FRAMES: &str = "aetower.host.display_dropped_frames";
    pub const HOST_INPUT_AVG_LATENCY: &str = "aetower.host.input_avg_latency_millis";
    pub const HOST_INPUT_MAX_LATENCY: &str = "aetower.host.input_max_latency_millis";
    pub const HOST_INPUT_SAMPLE_COUNT: &str = "aetower.host.input_sample_count";
}

pub struct TelemetryExporter {
    config: OtlpConfig,
    diagnostics: Option<DiagnosticsStore>,
}

impl TelemetryExporter {
    pub fn new(config: OtlpConfig) -> Self {
        Self {
            config,
            diagnostics: None,
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    pub fn config(&self) -> &OtlpConfig {
        &self.config
    }

    pub fn update_config(&mut self, config: OtlpConfig) {
        self.config = config;
    }

    pub fn set_diagnostics(&mut self, diagnostics: DiagnosticsStore) {
        self.diagnostics = Some(diagnostics);
    }

    pub fn export(
        &self,
        snapshot: &SystemSnapshot,
        lag_metrics: &RuntimeLagMetrics,
    ) -> Result<(), String> {
        if !self.config.enabled {
            return Ok(());
        }
        let started_at = std::time::Instant::now();
        if let Some(diagnostics) = self.diagnostics.as_ref() {
            diagnostics.emit(
                DiagnosticsEvent::builder(
                    DiagnosticsLevel::Debug,
                    DiagnosticsSubsystem::Telemetry,
                    "telemetry-export-started",
                    "Starting OTLP telemetry export.",
                )
                .sequence(snapshot.sequence)
                .field("endpoint", &self.config.endpoint)
                .field("entity_count", snapshot.entities.len())
                .build(),
            );
        }

        let endpoint = Url::parse(&self.config.endpoint)
            .map_err(|error| format!("invalid telemetry endpoint: {error}"))?;
        if endpoint.scheme() != "http" {
            return Err("only http OTLP endpoints are supported".to_owned());
        }

        let payload = build_resource_metrics(snapshot, lag_metrics);
        let body =
            serde_json::to_vec(&payload).map_err(|error| format!("telemetry encode: {error}"))?;
        let result = post_json(&endpoint, &body);
        if let Some(diagnostics) = self.diagnostics.as_ref() {
            let latency_millis = started_at.elapsed().as_millis();
            match &result {
                Ok(()) => diagnostics.emit(
                    DiagnosticsEvent::builder(
                        DiagnosticsLevel::Info,
                        DiagnosticsSubsystem::Telemetry,
                        "telemetry-export-succeeded",
                        "OTLP telemetry export succeeded.",
                    )
                    .sequence(snapshot.sequence)
                    .field("endpoint", &self.config.endpoint)
                    .field("payload_bytes", body.len())
                    .field("metric_count", count_metrics(snapshot))
                    .field("latency_millis", latency_millis)
                    .build(),
                ),
                Err(error) => diagnostics.emit(
                    DiagnosticsEvent::builder(
                        DiagnosticsLevel::Error,
                        DiagnosticsSubsystem::Telemetry,
                        "telemetry-export-failed",
                        "OTLP telemetry export failed.",
                    )
                    .sequence(snapshot.sequence)
                    .field("endpoint", &self.config.endpoint)
                    .field("payload_bytes", body.len())
                    .field("metric_count", count_metrics(snapshot))
                    .field("latency_millis", latency_millis)
                    .field("error", error)
                    .build(),
                ),
            }
        }
        result
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ResourceMetricsEnvelope {
    resource_metrics: Vec<ResourceMetrics>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ResourceMetrics {
    resource: Resource,
    scope_metrics: Vec<ScopeMetrics>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Resource {
    attributes: Vec<KeyValue>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ScopeMetrics {
    scope: InstrumentationScope,
    metrics: Vec<Metric>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct InstrumentationScope {
    name: String,
    version: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Metric {
    name: String,
    unit: String,
    gauge: Gauge,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Gauge {
    data_points: Vec<NumberDataPoint>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct NumberDataPoint {
    attributes: Vec<KeyValue>,
    time_unix_nano: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    as_double: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    as_int: Option<i64>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct KeyValue {
    key: String,
    value: AnyValue,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct AnyValue {
    #[serde(skip_serializing_if = "Option::is_none")]
    string_value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    int_value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    double_value: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    bool_value: Option<bool>,
}

fn build_resource_metrics(
    snapshot: &SystemSnapshot,
    lag_metrics: &RuntimeLagMetrics,
) -> ResourceMetricsEnvelope {
    let timestamp = snapshot_time_nanos(snapshot.captured_at_millis);
    let mut metrics = vec![
        host_double_metric(
            metric_names::HOST_CPU,
            "percent",
            snapshot.host.cpu_percent as f64,
            timestamp.clone(),
            &[],
        ),
        host_int_metric(
            metric_names::HOST_MEMORY,
            "By",
            snapshot.host.memory_used_bytes as i64,
            timestamp.clone(),
            &[],
        ),
        host_double_metric(
            metric_names::HOST_FRICTION,
            "score",
            machine_friction_score(snapshot),
            timestamp.clone(),
            &[],
        ),
        host_int_metric(
            metric_names::HOST_THERMAL,
            "state",
            thermal_state_value(snapshot.host.thermal_state) as i64,
            timestamp.clone(),
            &[kv_string("state", snapshot.host.thermal_state.to_string())],
        ),
        host_double_metric(
            metric_names::HOST_GPU,
            "percent",
            snapshot.host.gpu_percent as f64,
            timestamp.clone(),
            &[],
        ),
        host_double_metric(
            metric_names::HOST_ANE,
            "percent",
            snapshot.host.ane_percent as f64,
            timestamp.clone(),
            &[],
        ),
        host_double_metric(
            metric_names::HOST_AI_FRICTION,
            "score",
            snapshot.host.ai_agent_friction as f64,
            timestamp.clone(),
            &[kv_int("count", snapshot.host.ai_agent_count as i64)],
        ),
        host_double_metric(
            metric_names::HOST_ENGINE_TICK_LATENCY,
            "ms",
            lag_metrics.engine_tick_millis as f64,
            timestamp.clone(),
            &[],
        ),
        host_double_metric(
            metric_names::HOST_COLLECT_LATENCY,
            "ms",
            lag_metrics.collect_millis as f64,
            timestamp.clone(),
            &[],
        ),
        host_double_metric(
            metric_names::HOST_IDENTITY_LATENCY,
            "ms",
            lag_metrics.identity_millis as f64,
            timestamp.clone(),
            &[],
        ),
        host_double_metric(
            metric_names::HOST_ATTRIBUTION_LATENCY,
            "ms",
            lag_metrics.attribution_millis as f64,
            timestamp.clone(),
            &[],
        ),
        host_double_metric(
            metric_names::HOST_FRICTION_LATENCY,
            "ms",
            lag_metrics.friction_millis as f64,
            timestamp.clone(),
            &[],
        ),
        host_double_metric(
            metric_names::HOST_ENRICH_LATENCY,
            "ms",
            lag_metrics.enrich_millis as f64,
            timestamp.clone(),
            &[],
        ),
        host_double_metric(
            metric_names::HOST_HISTORY_LATENCY,
            "ms",
            lag_metrics.history_millis as f64,
            timestamp.clone(),
            &[],
        ),
        host_double_metric(
            metric_names::HOST_PERSIST_LATENCY,
            "ms",
            lag_metrics.persist_millis as f64,
            timestamp.clone(),
            &[],
        ),
        host_double_metric(
            metric_names::HOST_BRIDGE_FETCH_LATENCY,
            "ms",
            lag_metrics.bridge_fetch_millis as f64,
            timestamp.clone(),
            &[],
        ),
        host_double_metric(
            metric_names::HOST_UI_REFRESH_LATENCY,
            "ms",
            lag_metrics.ui_refresh_millis as f64,
            timestamp.clone(),
            &[],
        ),
        host_double_metric(
            metric_names::HOST_SNAPSHOT_TO_UI_LATENCY,
            "ms",
            lag_metrics.snapshot_to_ui_millis as f64,
            timestamp.clone(),
            &[],
        ),
        host_double_metric(
            metric_names::HOST_SNAPSHOT_TO_RENDER_LATENCY,
            "ms",
            lag_metrics.snapshot_to_render_millis as f64,
            timestamp.clone(),
            &[],
        ),
        host_double_metric(
            metric_names::HOST_RENDER_COMMIT_LATENCY,
            "ms",
            lag_metrics.render_commit_millis as f64,
            timestamp.clone(),
            &[],
        ),
        host_double_metric(
            metric_names::HOST_DISPLAY_FRAME_INTERVAL,
            "ms",
            lag_metrics.display_frame_interval_millis as f64,
            timestamp.clone(),
            &[],
        ),
        host_double_metric(
            metric_names::HOST_DISPLAY_REFRESH_HZ,
            "Hz",
            lag_metrics.display_refresh_hz as f64,
            timestamp.clone(),
            &[],
        ),
        host_int_metric(
            metric_names::HOST_DISPLAY_DROPPED_FRAMES,
            "1",
            lag_metrics.display_dropped_frames as i64,
            timestamp.clone(),
            &[],
        ),
        host_double_metric(
            metric_names::HOST_INPUT_AVG_LATENCY,
            "ms",
            lag_metrics.input_avg_latency_millis as f64,
            timestamp.clone(),
            &[],
        ),
        host_double_metric(
            metric_names::HOST_INPUT_MAX_LATENCY,
            "ms",
            lag_metrics.input_max_latency_millis as f64,
            timestamp.clone(),
            &[],
        ),
        host_int_metric(
            metric_names::HOST_INPUT_SAMPLE_COUNT,
            "1",
            lag_metrics.input_sample_count as i64,
            timestamp.clone(),
            &[],
        ),
    ];

    for entity in &snapshot.entities {
        metrics.extend(entity_metrics(entity, &timestamp));
    }

    ResourceMetricsEnvelope {
        resource_metrics: vec![ResourceMetrics {
            resource: Resource {
                attributes: vec![
                    kv_string("service.name", "aetower".to_owned()),
                    kv_string("service.version", env!("CARGO_PKG_VERSION").to_owned()),
                ],
            },
            scope_metrics: vec![ScopeMetrics {
                scope: InstrumentationScope {
                    name: "aetower.telemetry".to_owned(),
                    version: env!("CARGO_PKG_VERSION").to_owned(),
                },
                metrics,
            }],
        }],
    }
}

fn entity_metrics(entity: &EntitySnapshot, timestamp: &str) -> Vec<Metric> {
    let attributes = vec![
        kv_string("entity.id", entity.entity_id.clone()),
        kv_string("entity.name", entity.display_name.clone()),
        kv_string(
            "entity.kind",
            serde_json::to_string(&entity.entity_kind)
                .unwrap_or_else(|_| "\"unknown\"".to_owned())
                .trim_matches('"')
                .to_owned(),
        ),
    ];
    vec![
        host_double_metric(
            metric_names::ENTITY_FRICTION,
            "score",
            entity.friction.total_score as f64,
            timestamp.to_owned(),
            &attributes,
        ),
        host_double_metric(
            metric_names::ENTITY_CPU,
            "percent",
            entity.metrics.cpu_percent as f64,
            timestamp.to_owned(),
            &attributes,
        ),
        host_int_metric(
            metric_names::ENTITY_MEMORY,
            "By",
            entity.metrics.memory_resident_bytes as i64,
            timestamp.to_owned(),
            &attributes,
        ),
        host_double_metric(
            metric_names::ENTITY_ENERGY,
            "score",
            entity.friction.energy_impact_score as f64,
            timestamp.to_owned(),
            &attributes,
        ),
        host_int_metric(
            metric_names::ENTITY_NETWORK,
            "By/s",
            entity
                .metrics
                .network_receive_bps
                .saturating_add(entity.metrics.network_send_bps) as i64,
            timestamp.to_owned(),
            &attributes,
        ),
        host_double_metric(
            metric_names::ENTITY_WAKEUPS,
            "wakeups/s",
            entity.metrics.wakeups_per_second as f64,
            timestamp.to_owned(),
            &attributes,
        ),
    ]
}

fn count_metrics(snapshot: &SystemSnapshot) -> usize {
    26 + snapshot.entities.len() * 6
}

fn host_double_metric(
    name: &str,
    unit: &str,
    value: f64,
    timestamp: String,
    attributes: &[KeyValue],
) -> Metric {
    Metric {
        name: name.to_owned(),
        unit: unit.to_owned(),
        gauge: Gauge {
            data_points: vec![NumberDataPoint {
                attributes: attributes.to_vec(),
                time_unix_nano: timestamp,
                as_double: Some(value),
                as_int: None,
            }],
        },
    }
}

fn host_int_metric(
    name: &str,
    unit: &str,
    value: i64,
    timestamp: String,
    attributes: &[KeyValue],
) -> Metric {
    Metric {
        name: name.to_owned(),
        unit: unit.to_owned(),
        gauge: Gauge {
            data_points: vec![NumberDataPoint {
                attributes: attributes.to_vec(),
                time_unix_nano: timestamp,
                as_double: None,
                as_int: Some(value),
            }],
        },
    }
}

fn kv_string(key: &str, value: String) -> KeyValue {
    KeyValue {
        key: key.to_owned(),
        value: AnyValue {
            string_value: Some(value),
            int_value: None,
            double_value: None,
            bool_value: None,
        },
    }
}

fn kv_int(key: &str, value: i64) -> KeyValue {
    KeyValue {
        key: key.to_owned(),
        value: AnyValue {
            string_value: None,
            int_value: Some(value.to_string()),
            double_value: None,
            bool_value: None,
        },
    }
}

fn machine_friction_score(snapshot: &SystemSnapshot) -> f64 {
    let host = &snapshot.host;
    let cpu_score = host.cpu_percent.min(100.0) as f64 * 0.5;
    let memory_ratio = if host.memory_total_bytes == 0 {
        0.0
    } else {
        host.memory_used_bytes as f64 / host.memory_total_bytes as f64
    };
    let memory_score = memory_ratio.min(1.0) * 35.0;
    let swap_score = if host.swap_used_bytes == 0 {
        0.0
    } else {
        ((host.swap_used_bytes as f64 / 1_073_741_824.0).min(8.0) / 8.0) * 15.0
    };
    let compressed_score = if host.memory_total_bytes == 0 {
        0.0
    } else {
        (host.compressed_memory_bytes as f64 / host.memory_total_bytes as f64).min(1.0) * 12.0
    };
    let network_score = ((host
        .network_receive_bps
        .saturating_add(host.network_send_bps)) as f64
        / 8_388_608.0)
        .min(1.0)
        * 10.0;
    let wakeups_score = (host.wakeups_per_second as f64 / 500.0).min(1.0) * 8.0;
    (cpu_score + memory_score + swap_score + compressed_score + network_score + wakeups_score)
        .min(100.0)
}

fn thermal_state_value(state: ThermalState) -> u8 {
    match state {
        ThermalState::Nominal => 0,
        ThermalState::Fair => 1,
        ThermalState::Serious => 2,
        ThermalState::Critical => 3,
    }
}

fn snapshot_time_nanos(captured_at_millis: u64) -> String {
    captured_at_millis.saturating_mul(1_000_000).to_string()
}

fn post_json(endpoint: &Url, body: &[u8]) -> Result<(), String> {
    let host = endpoint
        .host_str()
        .ok_or_else(|| "telemetry endpoint host missing".to_owned())?;
    let port = endpoint.port_or_known_default().unwrap_or(4318);
    let mut path = endpoint.path().to_owned();
    if path.is_empty() {
        path = "/v1/metrics".to_owned();
    }
    if let Some(query) = endpoint.query() {
        path.push('?');
        path.push_str(query);
    }

    let mut stream =
        TcpStream::connect((host, port)).map_err(|error| format!("telemetry connect: {error}"))?;
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .map_err(|error| format!("telemetry set_read_timeout: {error}"))?;
    stream
        .set_write_timeout(Some(Duration::from_secs(2)))
        .map_err(|error| format!("telemetry set_write_timeout: {error}"))?;

    let request = format!(
        "POST {path} HTTP/1.1\r\nHost: {host}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream
        .write_all(request.as_bytes())
        .and_then(|_| stream.write_all(body))
        .map_err(|error| format!("telemetry write: {error}"))?;

    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .map_err(|error| format!("telemetry read: {error}"))?;
    let status = response.lines().next().unwrap_or_default();
    if status.contains(" 200 ") || status.contains(" 202 ") || status.contains(" 204 ") {
        return Ok(());
    }

    Err(format!("telemetry endpoint rejected export: {status}"))
}

#[cfg(test)]
mod tests {
    use super::{OtlpConfig, TelemetryExporter, build_resource_metrics, metric_names};
    use aetower_model::{
        EntitySnapshot, HostSnapshot, RuntimeLagMetrics, SystemSnapshot, ThermalState,
    };

    #[test]
    fn default_otlp_endpoint_uses_http_metrics_path() {
        let config = OtlpConfig::default();
        assert_eq!(config.endpoint, "http://localhost:4318/v1/metrics");
    }

    #[test]
    fn export_payload_contains_host_and_entity_metrics() {
        let snapshot = SystemSnapshot {
            captured_at_millis: 1_000,
            host: HostSnapshot {
                cpu_percent: 12.5,
                memory_used_bytes: 1024,
                thermal_state: ThermalState::Fair,
                gpu_percent: 33.0,
                ..HostSnapshot::default()
            },
            entities: vec![EntitySnapshot {
                entity_id: "entity".to_owned(),
                display_name: "Entity".to_owned(),
                friction: aetower_model::FrictionBreakdown {
                    total_score: 44.0,
                    energy_impact_score: 9.0,
                    ..Default::default()
                },
                metrics: aetower_model::AggregateMetrics {
                    cpu_percent: 11.0,
                    memory_resident_bytes: 4096,
                    ..Default::default()
                },
                ..Default::default()
            }],
            ..Default::default()
        };

        let lag_metrics = RuntimeLagMetrics {
            engine_tick_millis: 9.5,
            display_refresh_hz: 120.0,
            input_avg_latency_millis: 2.5,
            ..RuntimeLagMetrics::default()
        };
        let payload =
            serde_json::to_value(build_resource_metrics(&snapshot, &lag_metrics)).unwrap();
        let metrics = payload["resourceMetrics"][0]["scopeMetrics"][0]["metrics"]
            .as_array()
            .unwrap();
        assert!(
            metrics
                .iter()
                .any(|metric| metric["name"] == metric_names::HOST_GPU)
        );
        assert!(
            metrics
                .iter()
                .any(|metric| metric["name"] == metric_names::ENTITY_FRICTION)
        );
        assert!(
            metrics
                .iter()
                .any(|metric| metric["name"] == metric_names::HOST_ENGINE_TICK_LATENCY)
        );
        assert!(
            metrics
                .iter()
                .any(|metric| metric["name"] == metric_names::HOST_INPUT_AVG_LATENCY)
        );
    }

    #[test]
    fn disabled_exporter_is_noop() {
        let exporter = TelemetryExporter::new(OtlpConfig::default());
        assert!(
            exporter
                .export(&SystemSnapshot::default(), &RuntimeLagMetrics::default())
                .is_ok()
        );
    }
}
