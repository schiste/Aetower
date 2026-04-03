use std::collections::{BTreeMap, VecDeque};

use aetower_model::{
    EntitySnapshot, HostSnapshot, HostTrend, MetricTrend, TimelineEvent, TimelineSeverity,
};

pub struct History {
    previous_scores: BTreeMap<String, f32>,
    previous_anomaly: BTreeMap<String, bool>,
    previous_thermal_state: String,
    thermal_contributors: Vec<String>,
    cooccurrence: BTreeMap<(String, String), u32>,
    cooccurrence_tick: u32,
    metric_history: BTreeMap<String, MetricTrendState>,
    host_history: HostTrendState,
    timeline: VecDeque<TimelineEvent>,
}

struct MetricTrendState {
    friction: VecDeque<f32>,
    cpu_percent: VecDeque<f32>,
    memory_resident_bytes: VecDeque<u64>,
    disk_activity_bps: VecDeque<u64>,
    network_activity_bps: VecDeque<u64>,
    wakeups_per_second: VecDeque<f32>,
}

struct HostTrendState {
    machine_friction: VecDeque<f32>,
    cpu_percent: VecDeque<f32>,
    memory_used_bytes: VecDeque<u64>,
    disk_activity_bps: VecDeque<u64>,
    network_activity_bps: VecDeque<u64>,
    wakeups_per_second: VecDeque<f32>,
    compressed_memory_bytes: VecDeque<u64>,
    ai_agent_friction: VecDeque<f32>,
}

const MAX_TREND_POINTS: usize = 30;

impl History {
    pub fn new() -> Self {
        Self {
            previous_scores: BTreeMap::new(),
            previous_anomaly: BTreeMap::new(),
            previous_thermal_state: "nominal".to_owned(),
            thermal_contributors: Vec::new(),
            cooccurrence: BTreeMap::new(),
            cooccurrence_tick: 0,
            metric_history: BTreeMap::new(),
            host_history: HostTrendState::default(),
            timeline: VecDeque::with_capacity(256),
        }
    }

    /// Entity IDs currently blamed for thermal throttling.
    pub fn thermal_contributors(&self) -> &[String] {
        &self.thermal_contributors
    }

    pub fn update(
        &mut self,
        captured_at_millis: u64,
        host: &HostSnapshot,
        entities: &mut [EntitySnapshot],
    ) -> (Vec<TimelineEvent>, HostTrend) {
        let active_entity_ids: Vec<String> = entities
            .iter()
            .map(|entity| entity.entity_id.clone())
            .collect();
        self.host_history.push(host);

        for entity in entities.iter_mut() {
            let trend_state = self
                .metric_history
                .entry(entity.entity_id.clone())
                .or_default();
            trend_state.push(entity);
            entity.trend = trend_state.snapshot();
        }

        // Anomaly detection: flag entities whose current friction exceeds
        // mean + 2*stddev of their recent trend (z-score > 2).
        for entity in entities.iter_mut() {
            let is_anomaly = self
                .metric_history
                .get(&entity.entity_id)
                .map(|trend| detect_anomaly(&trend.friction, entity.friction.total_score))
                .unwrap_or(false);
            entity.anomaly_detected = is_anomaly;

            let was_anomaly = self
                .previous_anomaly
                .get(&entity.entity_id)
                .copied()
                .unwrap_or(false);
            if is_anomaly && !was_anomaly {
                self.push_event(
                    captured_at_millis,
                    TimelineSeverity::Warning,
                    Some(entity.entity_id.clone()),
                    format!("{} anomaly detected", entity.display_name),
                    format!(
                        "Friction {:.1} is unusually high for this entity",
                        entity.friction.total_score
                    ),
                );
            }
            self.previous_anomaly
                .insert(entity.entity_id.clone(), is_anomaly);
        }

        // Feature 5: Thermal contribution tracking.
        // When thermal state degrades, snapshot top-3 CPU consumers.
        let thermal_degraded = host.thermal_state != "nominal"
            && (self.previous_thermal_state == "nominal"
                || thermal_severity(&host.thermal_state)
                    > thermal_severity(&self.previous_thermal_state));
        if thermal_degraded {
            self.thermal_contributors = entities
                .iter()
                .take(3)
                .map(|e| e.entity_id.clone())
                .collect();
            self.push_event(
                captured_at_millis,
                TimelineSeverity::Warning,
                None,
                format!("Thermal state: {}", host.thermal_state),
                format!(
                    "Top contributors: {}",
                    entities
                        .iter()
                        .take(3)
                        .map(|e| e.display_name.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            );
        }
        if host.thermal_state == "nominal" {
            self.thermal_contributors.clear();
        }
        self.previous_thermal_state = host.thermal_state.clone();

        // Set thermal_contribution on blamed entities.
        for entity in entities.iter_mut() {
            if self.thermal_contributors.contains(&entity.entity_id)
                && host.thermal_state != "nominal"
            {
                entity.thermal_contribution = Some(format!(
                    "Likely contributing to {} thermal state",
                    host.thermal_state
                ));
            }
        }

        // Feature 6: Co-occurrence tracking for grouping suggestions.
        self.cooccurrence_tick += 1;
        for (i, a) in active_entity_ids.iter().enumerate() {
            for b in active_entity_ids.iter().skip(i + 1) {
                let key = if a < b {
                    (a.clone(), b.clone())
                } else {
                    (b.clone(), a.clone())
                };
                *self.cooccurrence.entry(key).or_insert(0) += 1;
            }
        }
        // Generate grouping suggestions for pairs co-occurring >80% of ticks.
        if self.cooccurrence_tick >= 15 {
            let threshold = (self.cooccurrence_tick as f32 * 0.8) as u32;
            let names: BTreeMap<String, String> = entities
                .iter()
                .map(|e| (e.entity_id.clone(), e.display_name.clone()))
                .collect();
            let mut suggestions: BTreeMap<String, String> = BTreeMap::new();
            for ((a, b), count) in &self.cooccurrence {
                if *count < threshold {
                    continue;
                }
                if let Some(b_name) = names.get(b) {
                    suggestions.entry(a.clone()).or_insert_with(|| {
                        format!("Often runs alongside {b_name} — consider grouping")
                    });
                }
                if let Some(a_name) = names.get(a) {
                    suggestions.entry(b.clone()).or_insert_with(|| {
                        format!("Often runs alongside {a_name} — consider grouping")
                    });
                }
            }
            for entity in entities.iter_mut() {
                if entity.grouping_suggestion.is_none() {
                    entity.grouping_suggestion = suggestions.remove(&entity.entity_id);
                }
            }
        }

        self.metric_history.retain(|entity_id, _| {
            active_entity_ids
                .iter()
                .any(|active_id| active_id == entity_id)
        });
        self.previous_scores.retain(|entity_id, _| {
            active_entity_ids
                .iter()
                .any(|active_id| active_id == entity_id)
        });
        self.previous_anomaly.retain(|entity_id, _| {
            active_entity_ids
                .iter()
                .any(|active_id| active_id == entity_id)
        });

        for entity in entities.iter().take(5) {
            let previous = self
                .previous_scores
                .get(&entity.entity_id)
                .copied()
                .unwrap_or_default();
            let delta = entity.friction.total_score - previous;
            if previous == 0.0 && entity.friction.total_score >= 20.0 {
                self.push_event(
                    captured_at_millis,
                    TimelineSeverity::Warning,
                    Some(entity.entity_id.clone()),
                    format!("{} became active", entity.display_name),
                    entity.friction.reasons.join(", "),
                );
            } else if delta >= 15.0 {
                self.push_event(
                    captured_at_millis,
                    TimelineSeverity::Critical,
                    Some(entity.entity_id.clone()),
                    format!("{} friction spiked", entity.display_name),
                    entity.friction.reasons.join(", "),
                );
            }
            self.previous_scores
                .insert(entity.entity_id.clone(), entity.friction.total_score);
        }

        while self.timeline.len() > 120 {
            self.timeline.pop_front();
        }

        (
            self.timeline.iter().cloned().collect(),
            self.host_history.snapshot(),
        )
    }

    fn push_event(
        &mut self,
        timestamp_millis: u64,
        severity: TimelineSeverity,
        entity_id: Option<String>,
        title: String,
        detail: String,
    ) {
        self.timeline.push_back(TimelineEvent {
            id: format!(
                "{}:{}",
                entity_id.clone().unwrap_or_else(|| "system".to_owned()),
                timestamp_millis
            ),
            timestamp_millis,
            severity,
            entity_id,
            title,
            detail,
        });
    }
}

impl Default for History {
    fn default() -> Self {
        Self::new()
    }
}

impl Default for MetricTrendState {
    fn default() -> Self {
        Self {
            friction: VecDeque::with_capacity(MAX_TREND_POINTS),
            cpu_percent: VecDeque::with_capacity(MAX_TREND_POINTS),
            memory_resident_bytes: VecDeque::with_capacity(MAX_TREND_POINTS),
            disk_activity_bps: VecDeque::with_capacity(MAX_TREND_POINTS),
            network_activity_bps: VecDeque::with_capacity(MAX_TREND_POINTS),
            wakeups_per_second: VecDeque::with_capacity(MAX_TREND_POINTS),
        }
    }
}

impl MetricTrendState {
    fn push(&mut self, entity: &EntitySnapshot) {
        push_point(&mut self.friction, entity.friction.total_score);
        push_point(&mut self.cpu_percent, entity.metrics.cpu_percent);
        push_point(
            &mut self.memory_resident_bytes,
            entity.metrics.memory_resident_bytes,
        );
        push_point(
            &mut self.disk_activity_bps,
            entity
                .metrics
                .disk_read_bps
                .saturating_add(entity.metrics.disk_write_bps),
        );
        push_point(
            &mut self.network_activity_bps,
            entity
                .metrics
                .network_receive_bps
                .saturating_add(entity.metrics.network_send_bps),
        );
        push_point(
            &mut self.wakeups_per_second,
            entity.metrics.wakeups_per_second,
        );
    }

    fn snapshot(&self) -> MetricTrend {
        MetricTrend {
            friction: self.friction.iter().copied().collect(),
            cpu_percent: self.cpu_percent.iter().copied().collect(),
            memory_resident_bytes: self.memory_resident_bytes.iter().copied().collect(),
            disk_activity_bps: self.disk_activity_bps.iter().copied().collect(),
            network_activity_bps: self.network_activity_bps.iter().copied().collect(),
            wakeups_per_second: self.wakeups_per_second.iter().copied().collect(),
        }
    }
}

impl Default for HostTrendState {
    fn default() -> Self {
        Self {
            machine_friction: VecDeque::with_capacity(MAX_TREND_POINTS),
            cpu_percent: VecDeque::with_capacity(MAX_TREND_POINTS),
            memory_used_bytes: VecDeque::with_capacity(MAX_TREND_POINTS),
            disk_activity_bps: VecDeque::with_capacity(MAX_TREND_POINTS),
            network_activity_bps: VecDeque::with_capacity(MAX_TREND_POINTS),
            wakeups_per_second: VecDeque::with_capacity(MAX_TREND_POINTS),
            compressed_memory_bytes: VecDeque::with_capacity(MAX_TREND_POINTS),
            ai_agent_friction: VecDeque::with_capacity(MAX_TREND_POINTS),
        }
    }
}

impl HostTrendState {
    fn push(&mut self, host: &HostSnapshot) {
        push_point(&mut self.machine_friction, machine_friction_score(host));
        push_point(&mut self.cpu_percent, host.cpu_percent);
        push_point(&mut self.memory_used_bytes, host.memory_used_bytes);
        push_point(
            &mut self.disk_activity_bps,
            host.disk_read_bps.saturating_add(host.disk_write_bps),
        );
        push_point(
            &mut self.network_activity_bps,
            host.network_receive_bps
                .saturating_add(host.network_send_bps),
        );
        push_point(&mut self.wakeups_per_second, host.wakeups_per_second);
        push_point(
            &mut self.compressed_memory_bytes,
            host.compressed_memory_bytes,
        );
        push_point(&mut self.ai_agent_friction, host.ai_agent_friction);
    }

    fn snapshot(&self) -> HostTrend {
        HostTrend {
            machine_friction: self.machine_friction.iter().copied().collect(),
            cpu_percent: self.cpu_percent.iter().copied().collect(),
            memory_used_bytes: self.memory_used_bytes.iter().copied().collect(),
            disk_activity_bps: self.disk_activity_bps.iter().copied().collect(),
            network_activity_bps: self.network_activity_bps.iter().copied().collect(),
            wakeups_per_second: self.wakeups_per_second.iter().copied().collect(),
            compressed_memory_bytes: self.compressed_memory_bytes.iter().copied().collect(),
            ai_agent_friction: self.ai_agent_friction.iter().copied().collect(),
        }
    }
}

fn push_point<T>(series: &mut VecDeque<T>, value: T) {
    if series.len() == MAX_TREND_POINTS {
        series.pop_front();
    }
    series.push_back(value);
}

fn machine_friction_score(host: &HostSnapshot) -> f32 {
    let cpu_score = host.cpu_percent.min(100.0) * 0.5;
    let memory_ratio = if host.memory_total_bytes == 0 {
        0.0
    } else {
        host.memory_used_bytes as f32 / host.memory_total_bytes as f32
    };
    let memory_score = (memory_ratio.min(1.0)) * 35.0;
    let swap_score = if host.swap_used_bytes == 0 {
        0.0
    } else {
        ((host.swap_used_bytes as f32 / 1_073_741_824.0).min(8.0) / 8.0) * 15.0
    };
    let compressed_score = if host.memory_total_bytes == 0 {
        0.0
    } else {
        ((host.compressed_memory_bytes as f32 / host.memory_total_bytes as f32).min(1.0)) * 12.0
    };
    let network_score = ((host
        .network_receive_bps
        .saturating_add(host.network_send_bps)) as f32
        / 8_388_608.0)
        .min(1.0)
        * 10.0;
    let wakeups_score = (host.wakeups_per_second / 500.0).min(1.0) * 8.0;
    (cpu_score + memory_score + swap_score + compressed_score + network_score + wakeups_score)
        .min(100.0)
}

/// Z-score anomaly detection: returns true if `current` is more than 2
/// standard deviations above the mean of `series`.  Requires at least 5
/// samples to avoid false positives during warmup.
fn detect_anomaly(series: &VecDeque<f32>, current: f32) -> bool {
    if series.len() < 5 {
        return false;
    }
    let n = series.len() as f32;
    let mean = series.iter().sum::<f32>() / n;
    let variance = series.iter().map(|v| (v - mean).powi(2)).sum::<f32>() / n;
    let stddev = variance.sqrt();
    if stddev < 1.0 {
        return false; // not enough variance to be meaningful
    }
    (current - mean) / stddev > 2.0
}

fn thermal_severity(state: &str) -> u8 {
    match state {
        "critical" => 3,
        "serious" => 2,
        "fair" => 1,
        _ => 0,
    }
}
