use aetower_model::SystemSnapshot;

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
            endpoint: "http://localhost:4317".to_owned(),
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
}

/// OpenTelemetry metrics exporter.
///
/// Architecture (when fully implemented):
/// - Uses `opentelemetry` + `opentelemetry-otlp` crates
/// - Maps SystemSnapshot fields to OTLP gauge metrics
/// - Runs on a background thread with configurable export interval
/// - Supports gRPC (port 4317) and HTTP (port 4318) endpoints
pub struct TelemetryExporter {
    config: OtlpConfig,
}

impl TelemetryExporter {
    pub fn new(config: OtlpConfig) -> Self {
        Self { config }
    }

    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    /// Export the current snapshot as OTLP metrics.
    /// Placeholder — requires opentelemetry + opentelemetry-otlp dependency.
    pub fn export(&self, _snapshot: &SystemSnapshot) {
        if !self.config.enabled {}
        // TODO: map snapshot fields to OTLP gauge metrics and push
    }
}
