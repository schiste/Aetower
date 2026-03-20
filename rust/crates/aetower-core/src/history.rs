use std::collections::{BTreeMap, VecDeque};

use aetower_model::{EntitySnapshot, HostSnapshot, HostTrend, MetricTrend, TimelineEvent, TimelineSeverity};

pub struct History {
    previous_scores: BTreeMap<String, f32>,
    metric_history: BTreeMap<String, MetricTrendState>,
    host_history: HostTrendState,
    timeline: VecDeque<TimelineEvent>,
}

struct MetricTrendState {
    friction: VecDeque<f32>,
    cpu_percent: VecDeque<f32>,
    memory_resident_bytes: VecDeque<u64>,
    disk_activity_bps: VecDeque<u64>,
}

struct HostTrendState {
    machine_friction: VecDeque<f32>,
    cpu_percent: VecDeque<f32>,
    memory_used_bytes: VecDeque<u64>,
    disk_activity_bps: VecDeque<u64>,
}

const MAX_TREND_POINTS: usize = 30;

impl History {
    pub fn new() -> Self {
        Self {
            previous_scores: BTreeMap::new(),
            metric_history: BTreeMap::new(),
            host_history: HostTrendState::default(),
            timeline: VecDeque::with_capacity(256),
        }
    }

    pub fn update(
        &mut self,
        captured_at_millis: u64,
        host: &HostSnapshot,
        entities: &mut [EntitySnapshot],
    ) -> (Vec<TimelineEvent>, HostTrend) {
        let active_entity_ids: Vec<String> = entities.iter().map(|entity| entity.entity_id.clone()).collect();
        self.host_history.push(host);

        for entity in entities.iter_mut() {
            let trend_state = self
                .metric_history
                .entry(entity.entity_id.clone())
                .or_insert_with(MetricTrendState::default);
            trend_state.push(entity);
            entity.trend = trend_state.snapshot();
        }

        self.metric_history
            .retain(|entity_id, _| active_entity_ids.iter().any(|active_id| active_id == entity_id));
        self.previous_scores
            .retain(|entity_id, _| active_entity_ids.iter().any(|active_id| active_id == entity_id));

        for entity in entities.iter().take(5) {
            let previous = self.previous_scores.get(&entity.entity_id).copied().unwrap_or_default();
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

impl Default for MetricTrendState {
    fn default() -> Self {
        Self {
            friction: VecDeque::with_capacity(MAX_TREND_POINTS),
            cpu_percent: VecDeque::with_capacity(MAX_TREND_POINTS),
            memory_resident_bytes: VecDeque::with_capacity(MAX_TREND_POINTS),
            disk_activity_bps: VecDeque::with_capacity(MAX_TREND_POINTS),
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
            entity.metrics.disk_read_bps.saturating_add(entity.metrics.disk_write_bps),
        );
    }

    fn snapshot(&self) -> MetricTrend {
        MetricTrend {
            friction: self.friction.iter().copied().collect(),
            cpu_percent: self.cpu_percent.iter().copied().collect(),
            memory_resident_bytes: self.memory_resident_bytes.iter().copied().collect(),
            disk_activity_bps: self.disk_activity_bps.iter().copied().collect(),
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
    }

    fn snapshot(&self) -> HostTrend {
        HostTrend {
            machine_friction: self.machine_friction.iter().copied().collect(),
            cpu_percent: self.cpu_percent.iter().copied().collect(),
            memory_used_bytes: self.memory_used_bytes.iter().copied().collect(),
            disk_activity_bps: self.disk_activity_bps.iter().copied().collect(),
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
    (cpu_score + memory_score + swap_score).min(100.0)
}
