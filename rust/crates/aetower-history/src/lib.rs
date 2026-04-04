use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};

use aetower_model::{
    EntitySnapshot, HostSnapshot, HostTrend, MetricTrend, Recommendation, ThermalState,
    TimelineCategory, TimelineEvent, TimelineSeverity,
};

pub struct History {
    previous_scores: BTreeMap<String, f32>,
    previous_anomaly: BTreeMap<String, bool>,
    previous_thermal_state: ThermalState,
    thermal_contributors: Vec<String>,
    entity_index: HashMap<String, u16>,
    entity_reverse_index: Vec<String>,
    next_entity_idx: u16,
    cooccurrence: BTreeMap<(u16, u16), u32>,
    cooccurrence_tick: u32,
    metric_history: BTreeMap<String, MetricTrendState>,
    host_history: HostTrendState,
    timeline: VecDeque<TimelineEvent>,
    previous_processes: HashMap<ProcessKey, ProcessObservation>,
    recent_exit_windows: BTreeMap<String, VecDeque<u64>>,
    last_restart_loop_report: BTreeMap<String, u64>,
    previous_pressure_band: PressureBand,
    previous_wakeup_band: WakeupBand,
    previous_entity_behaviors: BTreeMap<String, EntityBehaviorFlags>,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct ProcessKey {
    pid: u32,
    start_time_millis: u64,
}

#[derive(Debug, Clone)]
struct ProcessObservation {
    entity_id: String,
    entity_name: String,
    title: String,
    executable_path: Option<String>,
    launched_by: Option<String>,
    cpu_percent: f32,
    disk_activity_bps: u64,
    network_activity_bps: u64,
    wakeups_per_second: f32,
}

impl ProcessObservation {
    fn impact_score(&self) -> f32 {
        let disk_mib = self.disk_activity_bps as f32 / 1_048_576.0;
        let network_mib = self.network_activity_bps as f32 / 1_048_576.0;
        self.cpu_percent + (disk_mib * 2.5) + (network_mib * 1.5) + (self.wakeups_per_second / 50.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
enum PressureBand {
    #[default]
    Nominal,
    Elevated,
    Severe,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
enum WakeupBand {
    #[default]
    Nominal,
    Elevated,
    Severe,
}

#[derive(Debug, Clone, Copy, Default)]
struct EntityBehaviorFlags {
    disk_churn: bool,
    wakeup_churn: bool,
    background_network: bool,
}

const MAX_TREND_POINTS: usize = 30;
const PROCESS_LAUNCH_BURST_WINDOW_MILLIS: u64 = 15_000;
const SHORT_LIVED_PROCESS_MILLIS: u64 = 30_000;
const RESTART_LOOP_WINDOW_MILLIS: u64 = 60_000;

impl History {
    pub fn new() -> Self {
        Self {
            previous_scores: BTreeMap::new(),
            previous_anomaly: BTreeMap::new(),
            previous_thermal_state: ThermalState::Nominal,
            thermal_contributors: Vec::new(),
            entity_index: HashMap::new(),
            entity_reverse_index: Vec::new(),
            next_entity_idx: 0,
            cooccurrence: BTreeMap::new(),
            cooccurrence_tick: 0,
            metric_history: BTreeMap::new(),
            host_history: HostTrendState::default(),
            timeline: VecDeque::with_capacity(256),
            previous_processes: HashMap::new(),
            recent_exit_windows: BTreeMap::new(),
            last_restart_loop_report: BTreeMap::new(),
            previous_pressure_band: PressureBand::Nominal,
            previous_wakeup_band: WakeupBand::Nominal,
            previous_entity_behaviors: BTreeMap::new(),
        }
    }

    fn intern_entity_id(&mut self, entity_id: &str) -> u16 {
        if let Some(&idx) = self.entity_index.get(entity_id) {
            return idx;
        }
        let idx = self.next_entity_idx;
        self.next_entity_idx = self.next_entity_idx.wrapping_add(1);
        self.entity_index.insert(entity_id.to_owned(), idx);
        if self.entity_reverse_index.len() <= idx as usize {
            self.entity_reverse_index
                .resize(idx as usize + 1, String::new());
        }
        self.entity_reverse_index[idx as usize] = entity_id.to_owned();
        idx
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
        for entity in entities.iter_mut() {
            entity.recent_change_summary = None;
        }
        let active_entity_ids: Vec<String> = entities
            .iter()
            .map(|entity| entity.entity_id.clone())
            .collect();
        self.host_history.push(host);
        self.update_host_observability(captured_at_millis, host, entities);
        self.update_process_observability(captured_at_millis, entities);

        for entity in entities.iter_mut() {
            let trend_state = self
                .metric_history
                .entry(entity.entity_id.clone())
                .or_default();
            trend_state.push(entity);
            entity.trend = trend_state.snapshot();
        }

        self.update_entity_behavior_observability(captured_at_millis, entities);

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
                    TimelineCategory::Anomaly,
                    TimelineSeverity::Warning,
                    Some(entity.entity_id.clone()),
                    format!("{} anomaly detected", entity.display_name),
                    format!(
                        "Friction {:.1} is unusually high for this entity",
                        entity.friction.total_score
                    ),
                );
                set_recent_change_summary(
                    entity,
                    format!(
                        "Friction jumped to {:.1}, which is unusual for this entity.",
                        entity.friction.total_score
                    ),
                );
            }
            self.previous_anomaly
                .insert(entity.entity_id.clone(), is_anomaly);
        }

        // Feature 5: Thermal contribution tracking.
        // When thermal state degrades, snapshot top-3 CPU consumers.
        let thermal_degraded = host.thermal_state != ThermalState::Nominal
            && (self.previous_thermal_state == ThermalState::Nominal
                || thermal_severity(host.thermal_state)
                    > thermal_severity(self.previous_thermal_state));
        if thermal_degraded {
            self.thermal_contributors = entities
                .iter()
                .take(3)
                .map(|e| e.entity_id.clone())
                .collect();
            self.push_event(
                captured_at_millis,
                TimelineCategory::Thermal,
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
        if host.thermal_state == ThermalState::Nominal {
            self.thermal_contributors.clear();
        }
        self.previous_thermal_state = host.thermal_state;

        // Set thermal_contribution on blamed entities.
        for entity in entities.iter_mut() {
            if self.thermal_contributors.contains(&entity.entity_id)
                && host.thermal_state != ThermalState::Nominal
            {
                entity.thermal_contribution = Some(format!(
                    "Likely contributing to {} thermal state",
                    host.thermal_state
                ));
            }
        }

        // Feature 6: Co-occurrence tracking with compact u16 indices.
        self.cooccurrence_tick += 1;
        let active_indices: Vec<u16> = active_entity_ids
            .iter()
            .map(|id| self.intern_entity_id(id))
            .collect();
        for (i, &a) in active_indices.iter().enumerate() {
            for &b in active_indices.iter().skip(i + 1) {
                let key = if a < b { (a, b) } else { (b, a) };
                *self.cooccurrence.entry(key).or_insert(0) += 1;
            }
        }
        if self.cooccurrence_tick >= 15 {
            let threshold = (self.cooccurrence_tick as f32 * 0.8) as u32;
            let names: BTreeMap<String, String> = entities
                .iter()
                .map(|e| (e.entity_id.clone(), e.display_name.clone()))
                .collect();
            let mut suggestions: BTreeMap<String, String> = BTreeMap::new();
            for (&(a_idx, b_idx), count) in &self.cooccurrence {
                if *count < threshold {
                    continue;
                }
                let a = &self.entity_reverse_index[a_idx as usize];
                let b = &self.entity_reverse_index[b_idx as usize];
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
        self.previous_entity_behaviors.retain(|entity_id, _| {
            active_entity_ids
                .iter()
                .any(|active_id| active_id == entity_id)
        });
        let active_idx_set: HashSet<u16> = active_indices.iter().copied().collect();
        self.cooccurrence
            .retain(|(a, b), _| active_idx_set.contains(a) && active_idx_set.contains(b));

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
                    TimelineCategory::Lifecycle,
                    TimelineSeverity::Warning,
                    Some(entity.entity_id.clone()),
                    if matches!(
                        entity.entity_kind,
                        aetower_model::EntityKind::Service | aetower_model::EntityKind::Daemon
                    ) && !entity.metrics.is_foreground
                    {
                        format!("Background agent {} became active", entity.display_name)
                    } else {
                        format!("{} became active", entity.display_name)
                    },
                    entity.friction.reasons.join(", "),
                );
            } else if delta >= 15.0 {
                self.push_event(
                    captured_at_millis,
                    TimelineCategory::Friction,
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
        category: TimelineCategory,
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
            category,
            severity,
            entity_id,
            title,
            detail,
        });
    }

    fn update_host_observability(
        &mut self,
        captured_at_millis: u64,
        host: &HostSnapshot,
        entities: &mut [EntitySnapshot],
    ) {
        let pressure_band = pressure_band(host);
        if pressure_band > self.previous_pressure_band {
            let top_memory = entities
                .iter()
                .take(3)
                .map(|entity| entity.display_name.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            self.push_event(
                captured_at_millis,
                TimelineCategory::Host,
                if pressure_band == PressureBand::Severe {
                    TimelineSeverity::Critical
                } else {
                    TimelineSeverity::Warning
                },
                None,
                match pressure_band {
                    PressureBand::Nominal => "Memory pressure normalized".to_owned(),
                    PressureBand::Elevated => "Memory pressure rising".to_owned(),
                    PressureBand::Severe => "Memory pressure severe".to_owned(),
                },
                format!(
                    "{:.1} GB compressed, {:.1} GB swap in use. Top memory entities: {}",
                    host.compressed_memory_bytes as f32 / 1_073_741_824.0,
                    host.swap_used_bytes as f32 / 1_073_741_824.0,
                    if top_memory.is_empty() {
                        "none".to_owned()
                    } else {
                        top_memory
                    }
                ),
            );
            for entity in entities
                .iter_mut()
                .take(3)
                .filter(|entity| entity.metrics.memory_resident_bytes > 0)
            {
                set_recent_change_summary(
                    entity,
                    "Host memory pressure is rising while this entity remains one of the largest residents."
                        .to_owned(),
                );
            }
        }
        self.previous_pressure_band = pressure_band;

        let wakeup_band = wakeup_band(host.wakeups_per_second);
        if wakeup_band > self.previous_wakeup_band {
            let top_wakeup = entities
                .iter()
                .filter(|entity| entity.metrics.wakeups_per_second > 50.0)
                .take(3)
                .map(|entity| {
                    format!(
                        "{} ({:.0}/s)",
                        entity.display_name, entity.metrics.wakeups_per_second
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");
            self.push_event(
                captured_at_millis,
                TimelineCategory::Host,
                if wakeup_band == WakeupBand::Severe {
                    TimelineSeverity::Critical
                } else {
                    TimelineSeverity::Warning
                },
                None,
                match wakeup_band {
                    WakeupBand::Nominal => "Wakeups normalized".to_owned(),
                    WakeupBand::Elevated => "Wakeup churn rising".to_owned(),
                    WakeupBand::Severe => "Wakeup storm detected".to_owned(),
                },
                format!(
                    "Host wakeups reached {:.0}/s. Top wakeup sources: {}",
                    host.wakeups_per_second,
                    if top_wakeup.is_empty() {
                        "none".to_owned()
                    } else {
                        top_wakeup
                    }
                ),
            );
            for entity in entities
                .iter_mut()
                .filter(|entity| entity.metrics.wakeups_per_second >= 300.0)
                .take(3)
            {
                set_recent_change_summary(
                    entity,
                    "Host wakeups are elevated and this entity is one of the dominant wakeup sources."
                        .to_owned(),
                );
            }
        }
        self.previous_wakeup_band = wakeup_band;
    }

    fn update_process_observability(
        &mut self,
        captured_at_millis: u64,
        entities: &mut [EntitySnapshot],
    ) {
        let current = collect_process_observations(entities);
        let mut new_launches: BTreeMap<String, Vec<ProcessObservation>> = BTreeMap::new();
        for (key, observation) in &current {
            if !self.previous_processes.contains_key(key)
                && captured_at_millis.saturating_sub(key.start_time_millis)
                    <= PROCESS_LAUNCH_BURST_WINDOW_MILLIS
            {
                new_launches
                    .entry(observation.entity_id.clone())
                    .or_default()
                    .push(observation.clone());
            }
        }

        for (entity_id, launches) in new_launches {
            let launch_count = launches.len();
            let hot_launch = launches.iter().any(|value| value.impact_score() >= 35.0);
            let restarting_signature = launches.iter().find_map(|value| {
                let signature = value
                    .executable_path
                    .clone()
                    .unwrap_or_else(|| value.title.clone());
                self.recent_exit_windows
                    .get(&signature)
                    .filter(|window| window.len() >= 2)
                    .map(|_| signature)
            });
            if launch_count < 3 && !hot_launch && restarting_signature.is_none() {
                continue;
            }
            let entity_name = launches
                .first()
                .map(|value| value.entity_name.clone())
                .unwrap_or_else(|| entity_id.clone());
            let launch_examples = launches
                .iter()
                .take(3)
                .map(|value| value.title.clone())
                .collect::<Vec<_>>()
                .join(", ");
            let launch_sources = launches
                .iter()
                .filter_map(|value| value.launched_by.clone())
                .collect::<Vec<_>>();
            let launch_source = launch_sources
                .first()
                .cloned()
                .unwrap_or_else(|| "the current app/process tree".to_owned());
            if let Some(signature) = restarting_signature.clone() {
                self.push_event(
                    captured_at_millis,
                    TimelineCategory::Lifecycle,
                    TimelineSeverity::Critical,
                    Some(entity_id.clone()),
                    format!("{entity_name} may be restart-looping"),
                    format!(
                        "{} relaunched after multiple short-lived exits. Most likely launcher: {}.",
                        signature, launch_source
                    ),
                );
            } else {
                self.push_event(
                    captured_at_millis,
                    TimelineCategory::Lifecycle,
                    if hot_launch {
                        TimelineSeverity::Warning
                    } else {
                        TimelineSeverity::Info
                    },
                    Some(entity_id.clone()),
                    format!("{entity_name} spawned {launch_count} new process(es)"),
                    format!(
                        "Recent launches: {}. Most likely launcher: {}.",
                        launch_examples, launch_source
                    ),
                );
            }
            if let Some(entity) = entities
                .iter_mut()
                .find(|value| value.entity_id == entity_id)
            {
                if let Some(signature) = restarting_signature {
                    push_recommendation(
                        entity,
                        "Investigate restart loop",
                        format!(
                            "Aetower saw {} relaunch after repeated short-lived exits. Check helpers, launch agents, extensions, or child processes that may be crashing and restarting.",
                            signature
                        ),
                    );
                    set_recent_change_summary(
                        entity,
                        format!(
                            "Repeated short-lived relaunches suggest a restart loop for {signature}."
                        ),
                    );
                } else {
                    push_recommendation(
                        entity,
                        "Inspect recent process churn",
                        format!(
                            "Aetower saw {} new process(es) under this entity in the last few seconds. Check helpers, extensions, watchers, or child tasks first.",
                            launch_count
                        ),
                    );
                    set_recent_change_summary(
                        entity,
                        format!(
                            "A burst of {} new processes appeared under this entity.",
                            launch_count
                        ),
                    );
                }
            }
        }

        let previous_keys: Vec<ProcessKey> = self.previous_processes.keys().copied().collect();
        for key in previous_keys {
            let Some(previous) = self.previous_processes.get(&key).cloned() else {
                continue;
            };
            if current.contains_key(&key) {
                continue;
            }
            let runtime_millis = captured_at_millis.saturating_sub(key.start_time_millis);
            let signature = previous
                .executable_path
                .clone()
                .unwrap_or_else(|| previous.title.clone());
            let exit_count = {
                let exit_window = self
                    .recent_exit_windows
                    .entry(signature.clone())
                    .or_default();
                exit_window.push_back(captured_at_millis);
                while let Some(front) = exit_window.front().copied() {
                    if captured_at_millis.saturating_sub(front) > RESTART_LOOP_WINDOW_MILLIS {
                        exit_window.pop_front();
                    } else {
                        break;
                    }
                }
                exit_window.len()
            };

            if runtime_millis <= SHORT_LIVED_PROCESS_MILLIS && exit_count >= 3 {
                let already_reported = self
                    .last_restart_loop_report
                    .get(&signature)
                    .copied()
                    .unwrap_or(0);
                if captured_at_millis.saturating_sub(already_reported) > RESTART_LOOP_WINDOW_MILLIS
                {
                    self.last_restart_loop_report
                        .insert(signature.clone(), captured_at_millis);
                    self.push_event(
                        captured_at_millis,
                        TimelineCategory::Lifecycle,
                        TimelineSeverity::Critical,
                        Some(previous.entity_id.clone()),
                        format!("{} may be in a restart loop", previous.entity_name),
                        format!(
                            "{} exited {} times within about a minute. Last seen helper: {}.",
                            signature, exit_count, previous.title
                        ),
                    );
                    if let Some(entity) = entities
                        .iter_mut()
                        .find(|value| value.entity_id == previous.entity_id)
                    {
                        push_recommendation(
                            entity,
                            "Investigate restart loop",
                            format!(
                                "Aetower saw repeated short-lived exits for {}. Check helpers, launch agents, extensions, or child processes that may be crashing and restarting.",
                                signature
                            ),
                        );
                        set_recent_change_summary(
                            entity,
                            format!(
                                "Aetower observed repeated short-lived exits for {} within about a minute.",
                                signature
                            ),
                        );
                    }
                }
            } else if runtime_millis <= SHORT_LIVED_PROCESS_MILLIS
                && previous.impact_score() >= 45.0
            {
                self.push_event(
                    captured_at_millis,
                    TimelineCategory::Lifecycle,
                    TimelineSeverity::Warning,
                    Some(previous.entity_id.clone()),
                    format!("{} lost a short-lived hot process", previous.entity_name),
                    format!(
                        "{} exited after {:.1}s with {:.1}% CPU / {:.1} MiB/s disk / {:.1} MiB/s network.",
                        previous.title,
                        runtime_millis as f32 / 1000.0,
                        previous.cpu_percent,
                        previous.disk_activity_bps as f32 / 1_048_576.0,
                        previous.network_activity_bps as f32 / 1_048_576.0,
                    ),
                );
            }
        }

        self.previous_processes = current;
    }

    fn update_entity_behavior_observability(
        &mut self,
        captured_at_millis: u64,
        entities: &mut [EntitySnapshot],
    ) {
        for entity in entities.iter_mut() {
            let previous = self
                .previous_entity_behaviors
                .get(&entity.entity_id)
                .copied()
                .unwrap_or_default();
            let disk_activity = entity
                .metrics
                .disk_read_bps
                .saturating_add(entity.metrics.disk_write_bps);
            let network_activity = entity
                .metrics
                .network_receive_bps
                .saturating_add(entity.metrics.network_send_bps);
            let current = EntityBehaviorFlags {
                disk_churn: disk_activity >= 32 * 1_048_576,
                wakeup_churn: entity.metrics.wakeups_per_second >= 1_200.0,
                background_network: !entity.metrics.is_foreground
                    && network_activity >= 16 * 1_048_576,
            };

            if current.disk_churn && !previous.disk_churn {
                self.push_event(
                    captured_at_millis,
                    TimelineCategory::Friction,
                    TimelineSeverity::Warning,
                    Some(entity.entity_id.clone()),
                    format!("{} is causing disk churn", entity.display_name),
                    format!(
                        "This entity is driving about {:.1} MiB/s of read + write throughput across {} process(es).",
                        disk_activity as f32 / 1_048_576.0,
                        entity.metrics.process_count
                    ),
                );
                push_recommendation(
                    entity,
                    "Reduce disk churn",
                    "Aetower detected sustained local disk throughput from this entity. Pause sync, large file operations, indexers, or cache rebuilds first.".to_owned(),
                );
                set_recent_change_summary(
                    entity,
                    format!(
                        "Disk throughput climbed to {:.1} MiB/s across this entity.",
                        disk_activity as f32 / 1_048_576.0
                    ),
                );
            }

            if current.wakeup_churn && !previous.wakeup_churn {
                self.push_event(
                    captured_at_millis,
                    TimelineCategory::Friction,
                    TimelineSeverity::Warning,
                    Some(entity.entity_id.clone()),
                    format!("{} is generating timer churn", entity.display_name),
                    format!(
                        "Wakeups climbed to about {:.0}/s, which is consistent with a noisy watcher, polling loop, extension host, or background refresh cycle.",
                        entity.metrics.wakeups_per_second
                    ),
                );
                set_recent_change_summary(
                    entity,
                    format!(
                        "Wakeups climbed to about {:.0}/s for this entity.",
                        entity.metrics.wakeups_per_second
                    ),
                );
            }

            if current.background_network && !previous.background_network {
                self.push_event(
                    captured_at_millis,
                    TimelineCategory::Friction,
                    TimelineSeverity::Warning,
                    Some(entity.entity_id.clone()),
                    format!("{} is busy in the background", entity.display_name),
                    format!(
                        "Background network activity reached {:.1} MiB/s. Check sync, downloads, uploads, or remote dev sessions tied to this entity.",
                        network_activity as f32 / 1_048_576.0
                    ),
                );
                set_recent_change_summary(
                    entity,
                    format!(
                        "Background network activity reached {:.1} MiB/s for this entity.",
                        network_activity as f32 / 1_048_576.0
                    ),
                );
            }

            self.previous_entity_behaviors
                .insert(entity.entity_id.clone(), current);
        }
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

fn thermal_severity(state: ThermalState) -> u8 {
    match state {
        ThermalState::Critical => 3,
        ThermalState::Serious => 2,
        ThermalState::Fair => 1,
        ThermalState::Nominal => 0,
    }
}

fn collect_process_observations(
    entities: &[EntitySnapshot],
) -> HashMap<ProcessKey, ProcessObservation> {
    let mut observations = HashMap::new();
    for entity in entities {
        for component in &entity.components {
            let Some(pid) = component.process_id else {
                continue;
            };
            if component.start_time_millis == 0 {
                continue;
            }
            let key = ProcessKey {
                pid,
                start_time_millis: component.start_time_millis,
            };
            observations.insert(
                key,
                ProcessObservation {
                    entity_id: entity.entity_id.clone(),
                    entity_name: entity.display_name.clone(),
                    title: component.title.clone(),
                    executable_path: component.executable_path.clone(),
                    launched_by: component.launched_by.clone(),
                    cpu_percent: component.cpu_percent,
                    disk_activity_bps: 0,
                    network_activity_bps: 0,
                    wakeups_per_second: 0.0,
                },
            );
        }
    }

    for entity in entities {
        let process_count = entity
            .components
            .iter()
            .filter(|component| component.process_id.is_some() && component.start_time_millis > 0)
            .count()
            .max(1) as u64;
        let disk_per_process = entity
            .metrics
            .disk_read_bps
            .saturating_add(entity.metrics.disk_write_bps)
            / process_count;
        let network_per_process = entity
            .metrics
            .network_receive_bps
            .saturating_add(entity.metrics.network_send_bps)
            / process_count;
        let wakeups_per_process = entity.metrics.wakeups_per_second / process_count as f32;
        for component in &entity.components {
            let Some(pid) = component.process_id else {
                continue;
            };
            if component.start_time_millis == 0 {
                continue;
            }
            let key = ProcessKey {
                pid,
                start_time_millis: component.start_time_millis,
            };
            if let Some(observation) = observations.get_mut(&key) {
                observation.disk_activity_bps = disk_per_process;
                observation.network_activity_bps = network_per_process;
                observation.wakeups_per_second = wakeups_per_process;
            }
        }
    }

    observations
}

fn push_recommendation(entity: &mut EntitySnapshot, title: &str, detail: String) {
    if entity
        .recommendations
        .iter()
        .any(|existing| existing.title == title)
    {
        return;
    }
    entity.recommendations.push(Recommendation {
        title: title.to_owned(),
        detail,
    });
    if entity.recommendations.len() > 4 {
        entity.recommendations.truncate(4);
    }
}

fn set_recent_change_summary(entity: &mut EntitySnapshot, summary: String) {
    if entity.recent_change_summary.is_none() {
        entity.recent_change_summary = Some(summary);
    }
}

fn pressure_band(host: &HostSnapshot) -> PressureBand {
    let total_memory = host.memory_total_bytes.max(1) as f32;
    let compressed_ratio = host.compressed_memory_bytes as f32 / total_memory;
    let swap_ratio = host.swap_used_bytes as f32 / total_memory;
    if compressed_ratio >= 0.12 || swap_ratio >= 0.08 {
        PressureBand::Severe
    } else if compressed_ratio >= 0.05 || swap_ratio >= 0.02 {
        PressureBand::Elevated
    } else {
        PressureBand::Nominal
    }
}

fn wakeup_band(wakeups_per_second: f32) -> WakeupBand {
    if wakeups_per_second >= 3_000.0 {
        WakeupBand::Severe
    } else if wakeups_per_second >= 1_500.0 {
        WakeupBand::Elevated
    } else {
        WakeupBand::Nominal
    }
}

#[cfg(test)]
mod tests {
    use aetower_model::{
        AggregateMetrics, ComponentKind, ComponentSnapshot, EntityKind, FrictionBreakdown,
        HostSnapshot, MetricTrend, ProvenanceSnapshot,
    };

    use super::History;

    fn entity_with_process(
        id: &str,
        name: &str,
        pid: u32,
        start_time_millis: u64,
    ) -> aetower_model::EntitySnapshot {
        aetower_model::EntitySnapshot {
            entity_id: id.to_owned(),
            display_name: name.to_owned(),
            primary_provenance: Some(ProvenanceSnapshot::default()),
            launcher_summary: Some(name.to_owned()),
            attribution_notes: Vec::new(),
            bundle_id: None,
            executable_path: None,
            oldest_process_start_millis: start_time_millis,
            newest_process_start_millis: start_time_millis,
            entity_kind: EntityKind::App,
            metrics: AggregateMetrics {
                cpu_percent: 45.0,
                memory_resident_bytes: 512 * 1024 * 1024,
                disk_read_bps: 24 * 1024 * 1024,
                disk_write_bps: 8 * 1024 * 1024,
                network_receive_bps: 0,
                network_send_bps: 0,
                wakeups_per_second: 400.0,
                process_count: 1,
                is_foreground: false,
            },
            friction: FrictionBreakdown::default(),
            components: vec![ComponentSnapshot {
                kind: ComponentKind::Process,
                title: format!("{name} Helper"),
                detail: String::new(),
                adapter_context: None,
                provenance: None,
                process_id: Some(pid),
                start_time_millis,
                executable_path: Some(format!("/Applications/{name}.app/Helper")),
                command_line: None,
                parent_summary: None,
                launched_by: Some(name.to_owned()),
                cpu_percent: 45.0,
                memory_bytes: 128 * 1024 * 1024,
                cwd: None,
                user: None,
            }],
            trend: MetricTrend::default(),
            badges: Vec::new(),
            active_window_title: None,
            recent_change_summary: None,
            anomaly_detected: false,
            thermal_contribution: None,
            grouping_suggestion: None,
            agent_cost: None,
            session_markers: Vec::new(),
            recommendations: Vec::new(),
        }
    }

    fn entity_with_processes(
        id: &str,
        name: &str,
        processes: &[(u32, u64)],
    ) -> aetower_model::EntitySnapshot {
        let mut entity = entity_with_process(id, name, processes[0].0, processes[0].1);
        entity.metrics.process_count = processes.len() as u32;
        entity.components = processes
            .iter()
            .map(|(pid, start_time_millis)| ComponentSnapshot {
                kind: ComponentKind::Process,
                title: format!("{name} Helper {pid}"),
                detail: String::new(),
                adapter_context: None,
                provenance: None,
                process_id: Some(*pid),
                start_time_millis: *start_time_millis,
                executable_path: Some(format!("/Applications/{name}.app/Helper")),
                command_line: None,
                parent_summary: None,
                launched_by: Some(name.to_owned()),
                cpu_percent: 45.0,
                memory_bytes: 128 * 1024 * 1024,
                cwd: None,
                user: None,
            })
            .collect();
        entity.oldest_process_start_millis = processes
            .iter()
            .map(|(_, start_time_millis)| *start_time_millis)
            .min()
            .unwrap_or(0);
        entity.newest_process_start_millis = processes
            .iter()
            .map(|(_, start_time_millis)| *start_time_millis)
            .max()
            .unwrap_or(0);
        entity
    }

    #[test]
    fn emits_process_launch_burst_event_for_recent_helpers() {
        let mut history = History::new();
        let host = HostSnapshot::default();
        let mut entities = vec![entity_with_processes(
            "app:test",
            "Test",
            &[(100, 10_000), (101, 10_500), (102, 11_000)],
        )];
        history.update(20_000, &host, &mut entities);

        assert!(
            entities[0]
                .recommendations
                .iter()
                .any(|recommendation| recommendation.title == "Inspect recent process churn")
        );
        assert!(
            entities[0]
                .recent_change_summary
                .as_deref()
                .unwrap_or_default()
                .contains("new processes")
        );
    }

    #[test]
    fn emits_restart_loop_recommendation_for_repeated_short_lived_processes() {
        let mut history = History::new();
        let host = HostSnapshot::default();

        let mut first = vec![entity_with_process("app:test", "Test", 100, 1_000)];
        history.update(5_000, &host, &mut first);
        let mut empty = Vec::new();
        history.update(8_000, &host, &mut empty);

        let mut second = vec![entity_with_process("app:test", "Test", 101, 9_000)];
        history.update(12_000, &host, &mut second);
        history.update(15_000, &host, &mut empty);

        let mut third = vec![entity_with_process("app:test", "Test", 102, 16_000)];
        history.update(19_000, &host, &mut third);
        history.update(22_000, &host, &mut empty);

        let mut relaunched = vec![entity_with_process("app:test", "Test", 103, 23_000)];
        history.update(26_000, &host, &mut relaunched);

        assert!(relaunched.iter().any(|entity| {
            entity
                .recommendations
                .iter()
                .any(|recommendation| recommendation.title == "Investigate restart loop")
        }));
    }

    #[test]
    fn emits_memory_pressure_event_when_host_pressure_rises() {
        let mut history = History::new();
        let mut entities = vec![entity_with_process("app:test", "Test", 100, 1_000)];
        let nominal_host = HostSnapshot {
            memory_total_bytes: 16 * 1024 * 1024 * 1024,
            ..HostSnapshot::default()
        };
        history.update(5_000, &nominal_host, &mut entities);

        let pressured_host = HostSnapshot {
            memory_total_bytes: 16 * 1024 * 1024 * 1024,
            compressed_memory_bytes: 3 * 1024 * 1024 * 1024,
            swap_used_bytes: 2 * 1024 * 1024 * 1024,
            ..HostSnapshot::default()
        };
        history.update(10_000, &pressured_host, &mut entities);

        assert!(
            history
                .timeline
                .iter()
                .any(|event| event.title.contains("Memory pressure"))
        );
        assert!(
            entities[0]
                .recent_change_summary
                .as_deref()
                .unwrap_or_default()
                .contains("memory pressure")
        );
    }

    #[test]
    fn emits_disk_churn_recommendation_for_heavy_background_disk_usage() {
        let mut history = History::new();
        let host = HostSnapshot::default();
        let mut entity = entity_with_process("app:test", "Test", 100, 1_000);
        entity.metrics.disk_read_bps = 28 * 1024 * 1024;
        entity.metrics.disk_write_bps = 12 * 1024 * 1024;
        let mut entities = vec![entity];

        history.update(5_000, &host, &mut entities);

        assert!(
            entities[0]
                .recommendations
                .iter()
                .any(|recommendation| recommendation.title == "Reduce disk churn")
        );
    }
}
