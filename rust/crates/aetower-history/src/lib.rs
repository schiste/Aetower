use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};

use aetower_model::{
    AlertLevel, AlertThreshold, EntitySnapshot, HostSnapshot, HostTrend, MetricTrend,
    Recommendation, ThermalState, ThresholdDirection, TimelineCategory, TimelineEvent,
    TimelineSeverity,
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
    sensor_alert_thresholds: Vec<AlertThreshold>,
    previous_sensor_alert_levels: BTreeMap<String, AlertLevel>,
    /// Millisecond timestamp of the most recent tick on which we saw a
    /// reading for a given sensor key. Used by `prune_stale_sensor_alerts`
    /// to force-clear alert state when readings disappear — without this
    /// a sensor that was `Critical` before an SMC outage would appear
    /// "still Critical" when readings resumed, and the transition detector
    /// would skip re-firing because `current == previous`.
    last_sensor_reading_millis: BTreeMap<String, u64>,
    /// Cumulative energy per AI agent entity, in nanojoules. Accumulated
    /// each tick from `entity.metrics.energy_nj_per_s * TICK_SECONDS`.
    /// When an AI agent entity disappears (process exited), the
    /// accumulated value is used to emit a "session ended — X Wh" timeline
    /// event, then the entry is removed.
    ai_session_energy_nj: BTreeMap<String, u64>,
    /// Debounce counter for AI session end detection. When an AI agent
    /// entity disappears, we wait this many consecutive absent ticks
    /// before emitting the session-end event. This prevents a single-
    /// tick collector hiccup (process briefly invisible) from flushing
    /// accumulated energy and emitting a spurious "session ended" event.
    ai_session_absent_ticks: BTreeMap<String, u8>,
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

/// Stable identifier for the CPU package temperature alert.
///
/// The history tracker uses this as both the alert key and the lookup key
/// for the "previous level" map, so any string here has to stay consistent
/// across ticks. Concrete per-sensor keys (`cpu_temperature`, `fan_0_rpm`)
/// are matched against the sensor reading label at evaluation time.
const SENSOR_KEY_CPU_TEMPERATURE: &str = "cpu_temperature";
const SENSOR_KEY_BATTERY_HEALTH: &str = "battery_health";
const SENSOR_KEY_FAN_STUCK: &str = "fan_stuck_under_load";
const SENSOR_KEY_GPU_MEMORY: &str = "gpu_memory_ratio";

/// How long a sensor key can go without a fresh reading before the alert
/// tracker treats it as "unavailable" and force-clears the stored alert
/// level.
///
/// Chosen to be a multiple of the collector's 2s tick so that two or three
/// missed ticks (a brief hiccup) do not clear state, but a sustained
/// outage of ~15 s does. The value is long enough to avoid clearing
/// whenever the collector is briefly starved on a busy SoC and short
/// enough that when readings return after an SMC outage, the user sees a
/// fresh alert rather than continuing silence.
const SENSOR_GAP_MILLIS: u64 = 15_000;

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
            sensor_alert_thresholds: default_sensor_alert_thresholds(),
            previous_sensor_alert_levels: BTreeMap::new(),
            last_sensor_reading_millis: BTreeMap::new(),
            ai_session_energy_nj: BTreeMap::new(),
            ai_session_absent_ticks: BTreeMap::new(),
        }
    }

    /// Replace the active alert thresholds.
    ///
    /// Kept public so a future settings UI can wire user-customised
    /// thresholds through the engine without having to rebuild the
    /// history state. Calling this resets the "previous level" map so
    /// the next tick re-emits any currently-active alert under the new
    /// thresholds — this is the conservative choice because a user who
    /// just tightened a threshold probably wants to see the alert fire
    /// immediately rather than waiting for another transition.
    pub fn set_sensor_alert_thresholds(&mut self, thresholds: Vec<AlertThreshold>) {
        self.sensor_alert_thresholds = thresholds;
        self.previous_sensor_alert_levels.clear();
        self.last_sensor_reading_millis.clear();
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
        self.update_sensor_alerts(captured_at_millis, host);
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

        // Session energy journaling: accumulate per-tick energy for each
        // AI agent entity, and emit a summary event when the entity
        // disappears (session ended).
        let tick_seconds = 2.0f64; // matches collector TICK_SECONDS
        let active_ai_ids: HashSet<String> = entities
            .iter()
            .filter(|e| matches!(e.entity_kind, aetower_model::EntityKind::AiAgent))
            .map(|e| e.entity_id.clone())
            .collect();

        // Accumulate energy for active AI agents and write back to
        // entity.agent_cost.session_energy_nj so the UI can display it.
        for entity in entities
            .iter_mut()
            .filter(|e| matches!(e.entity_kind, aetower_model::EntityKind::AiAgent))
        {
            let delta_nj = (entity.metrics.energy_nj_per_s * tick_seconds) as u64;
            let cumulative = self
                .ai_session_energy_nj
                .entry(entity.entity_id.clone())
                .or_insert(0);
            *cumulative = cumulative.saturating_add(delta_nj);
            // Ensure the entity has an AgentCostSummary to write to.
            let cost = entity.agent_cost.get_or_insert_with(Default::default);
            cost.session_energy_nj = *cumulative;
        }

        // Detect AI agents that were present recently but have been absent
        // for several consecutive ticks (session truly ended, not a brief
        // collector hiccup). Without debounce, a single-tick gap would
        // emit a spurious "session ended" event and lose the accumulated
        // energy.
        const SESSION_END_DEBOUNCE_TICKS: u8 = 3;
        // Increment absent-tick counters for tracked sessions whose entity
        // is not in this tick's active set.
        let tracked_ids: Vec<String> = self.ai_session_energy_nj.keys().cloned().collect();
        for id in &tracked_ids {
            if active_ai_ids.contains(id) {
                // Agent is active — reset debounce counter.
                self.ai_session_absent_ticks.remove(id);
            } else {
                let counter = self.ai_session_absent_ticks.entry(id.clone()).or_insert(0);
                *counter = counter.saturating_add(1);
            }
        }
        // Emit session-end events only for agents absent for the full
        // debounce window.
        let ended_sessions: Vec<(String, u64)> = self
            .ai_session_absent_ticks
            .iter()
            .filter(|(_, ticks)| **ticks >= SESSION_END_DEBOUNCE_TICKS)
            .filter_map(|(entity_id, _)| {
                self.ai_session_energy_nj
                    .get(entity_id)
                    .map(|energy| (entity_id.clone(), *energy))
            })
            .collect();
        for (entity_id, energy_nj) in ended_sessions {
            self.ai_session_energy_nj.remove(&entity_id);
            self.ai_session_absent_ticks.remove(&entity_id);
            if energy_nj == 0 {
                continue;
            }
            let wh = energy_nj as f64 / 3.6e12;
            let mah = wh / 3.7 * 1000.0;
            let detail = if wh >= 1.0 {
                format!("Session consumed {wh:.2} Wh ({mah:.0} mAh at 3.7V nominal)")
            } else {
                let mwh = wh * 1000.0;
                format!("Session consumed {mwh:.1} mWh ({mah:.1} mAh at 3.7V nominal)")
            };
            // Extract a human-readable name from the entity_id.
            let display = entity_id
                .strip_prefix("ai-agent:")
                .or_else(|| entity_id.strip_prefix("ai-agent-daemon:"))
                .unwrap_or(&entity_id)
                .to_owned();
            self.push_event(
                captured_at_millis,
                TimelineCategory::Lifecycle,
                TimelineSeverity::Info,
                Some(entity_id),
                format!("{display} session ended"),
                detail,
            );
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

    /// Evaluate the configured alert thresholds against the current sensor
    /// snapshot and emit timeline events on state transitions.
    ///
    /// We only emit on transitions between `AlertLevel` states, not on
    /// every tick that a reading stays above a threshold. Three kinds of
    /// events are produced:
    ///
    /// - **Escalation** (nominal → warning, nominal → critical, warning →
    ///   critical): a new `Warning` or `Critical` timeline entry with the
    ///   current value baked into the detail text.
    /// - **De-escalation** (critical → warning): a downgrade event at
    ///   `Warning` severity so the user knows things are improving.
    /// - **Recovery** (any non-nominal → nominal): an `Info` event so the
    ///   timeline has a clean "closed" marker for the earlier alert.
    ///
    /// Before evaluating any sensor, a gap-detection pass force-clears
    /// any stored alert levels whose underlying sensor reading has not
    /// been observed for `SENSOR_GAP_MILLIS`. Without this, a `Critical`
    /// CPU temperature recorded before an SMC outage would appear "still
    /// Critical" when readings resumed (`current == previous` short-
    /// circuits the transition detector), and the user would never see a
    /// fresh alert re-fire despite the machine continuing to run hot.
    ///
    /// Evaluation iterates the per-sensor reading lists in the host
    /// snapshot so fan IDs and temperature labels participate in their own
    /// tracking — there is no global "CPU temperature" reading that works
    /// across SoCs, only labelled per-core readings.
    fn update_sensor_alerts(&mut self, captured_at_millis: u64, host: &HostSnapshot) {
        self.prune_stale_sensor_alerts(captured_at_millis);

        // CPU temperature: evaluate against the hottest core/package we see
        // this tick. Per-core thresholds would be noisier without adding
        // signal — any one core above 100°C is an emergency for the whole
        // SoC because the thermal envelope is shared.
        if let Some(hottest) = host
            .cpu_temperatures
            .iter()
            .map(|reading| reading.celsius)
            .fold(None, |max: Option<f32>, celsius| {
                Some(max.map_or(celsius, |current| current.max(celsius)))
            })
        {
            self.record_sensor_reading(SENSOR_KEY_CPU_TEMPERATURE, captured_at_millis);
            self.evaluate_alert_transition(
                captured_at_millis,
                SENSOR_KEY_CPU_TEMPERATURE,
                hottest,
                "CPU temperature",
                |value, level| {
                    format!(
                        "CPU temperature {value:.0}°C ({level})",
                        level = level_label(level)
                    )
                },
            );
        }

        // Battery health: only meaningful when the machine actually has a
        // battery AND the IOPS layer reported both the design capacity
        // and current capacity so we can compute a ratio. `None` for
        // `health_percent` is the "missing keys" signal and we must not
        // trip an alert on it — before the Option conversion, the zero
        // default here silently bypassed desktops by the `> 0.0` guard,
        // but it would have classified a genuinely-zero reading the
        // same way. Now the distinction is explicit in the type.
        if let Some(battery) = host.battery_health.as_ref()
            && let Some(health_percent) = battery.health_percent
        {
            self.record_sensor_reading(SENSOR_KEY_BATTERY_HEALTH, captured_at_millis);
            self.evaluate_alert_transition(
                captured_at_millis,
                SENSOR_KEY_BATTERY_HEALTH,
                health_percent,
                "Battery health",
                |value, level| {
                    format!(
                        "Battery health {value:.0}% of design capacity ({level})",
                        level = level_label(level)
                    )
                },
            );
        }

        // GPU (Metal heap) memory as a percentage of unified memory.
        // On Apple Silicon, when this ratio climbs past 75% the system
        // is approaching the point where Metal page evictions will tank
        // inference throughput. This is the most actionable signal for
        // a local LLM user deciding "should I try the 13B model or
        // stick with the 7B?".
        if host.gpu_memory_bytes > 0 && host.memory_total_bytes > 0 {
            let gpu_memory_percent =
                (host.gpu_memory_bytes as f32 / host.memory_total_bytes as f32) * 100.0;
            self.record_sensor_reading(SENSOR_KEY_GPU_MEMORY, captured_at_millis);
            self.evaluate_alert_transition(
                captured_at_millis,
                SENSOR_KEY_GPU_MEMORY,
                gpu_memory_percent,
                "GPU memory",
                |value, level| {
                    format!(
                        "GPU memory at {value:.0}% of unified memory ({level})",
                        level = level_label(level)
                    )
                },
            );
        }

        // Fan stuck: tracked as a separate key per fan ID because a
        // single-fan machine and a dual-fan Pro are very different cases.
        //
        // The fan-stuck condition is a compound boolean: `fan.current_rpm
        // <= 0.0 && thermal_state >= Fair`. Both halves matter — a fan
        // idling at 0 RPM on a cool SoC is normal, and a spinning fan
        // under thermal stress is also normal. Only the intersection
        // ("the SoC is hot and yet nothing is cooling it") is actionable.
        //
        // We evaluate every fan on every tick, even when thermal is
        // nominal, so that `previous_sensor_alert_levels` can drop back to
        // `Nominal` and emit a recovery event the moment the compound
        // condition clears. The previous implementation gated the entire
        // loop on `thermal_is_elevated`, which left stale `Warning` entries
        // in the map when the thermal state fell back to nominal and never
        // emitted a recovery event.
        //
        // We intentionally bypass the numeric threshold machinery here: the
        // compound boolean doesn't map cleanly to an `AlertThreshold { warning,
        // critical, direction }` record, and the previous synthetic-
        // threshold shim (warning=1.0, critical=1.0) could only ever fire
        // `Critical` because `classify_alert(1.0, above)` takes the
        // `>= critical` branch first. A dedicated boolean code path here
        // produces Warning severity (the UI can still escalate to Critical
        // after N consecutive stuck ticks in the future if we want).
        let thermal_is_elevated =
            thermal_severity(host.thermal_state) >= thermal_severity(ThermalState::Fair);
        for fan in &host.fans {
            let key = format!("{SENSOR_KEY_FAN_STUCK}_{id}", id = fan.id);
            // The fan is being observed this tick even if it's spinning
            // happily — recording the timestamp prevents the gap sweep
            // from clearing a legitimate in-progress alert just because
            // the current tick happens to be Nominal.
            self.record_sensor_reading(&key, captured_at_millis);
            let is_stuck_under_load = fan.current_rpm <= 0.0 && thermal_is_elevated;
            let current = if is_stuck_under_load {
                AlertLevel::Warning
            } else {
                AlertLevel::Nominal
            };
            let previous = self
                .previous_sensor_alert_levels
                .get(&key)
                .copied()
                .unwrap_or_default();
            if current == previous {
                continue;
            }
            self.previous_sensor_alert_levels.insert(key, current);
            if current == AlertLevel::Nominal {
                self.push_event(
                    captured_at_millis,
                    TimelineCategory::Host,
                    TimelineSeverity::Info,
                    None,
                    format!("{name} fan stall cleared", name = fan.name),
                    format!(
                        "{name} is now spinning or the SoC is no longer under thermal stress",
                        name = fan.name
                    ),
                );
                continue;
            }
            self.push_event(
                captured_at_millis,
                TimelineCategory::Host,
                alert_level_to_severity(current),
                None,
                format!("{name} stalled", name = fan.name),
                format!(
                    "{name} is at 0 RPM while the SoC is thermally stressed (warning)",
                    name = fan.name
                ),
            );
        }
    }

    /// Mark a sensor key as observed on the current tick.
    ///
    /// The gap-detection sweep compares each tracked key's last-seen
    /// timestamp against `captured_at_millis`, so every sensor code path
    /// that *did* evaluate a reading must call this before returning.
    /// Forgetting to call it would be a silent bug — the key would look
    /// stale on the next tick and its alert state would be cleared under
    /// the user's feet.
    fn record_sensor_reading(&mut self, sensor_key: &str, captured_at_millis: u64) {
        self.last_sensor_reading_millis
            .insert(sensor_key.to_owned(), captured_at_millis);
    }

    /// Sweep the alert tracker for sensors that have not produced a
    /// reading for `SENSOR_GAP_MILLIS`.
    ///
    /// For each stale key:
    /// 1. If the last known alert level was non-nominal, emit an Info
    ///    timeline event noting the gap so the user has a visible marker
    ///    for "the previous alert was closed not because the condition
    ///    improved, but because the sensor stopped reporting".
    /// 2. Drop the entry from both `previous_sensor_alert_levels` and
    ///    `last_sensor_reading_millis`. The next tick that produces a
    ///    real reading will re-classify from `Nominal` → whatever the
    ///    current value dictates, which re-fires a fresh escalation
    ///    event if the condition has persisted through the outage.
    fn prune_stale_sensor_alerts(&mut self, captured_at_millis: u64) {
        let stale_keys: Vec<String> = self
            .last_sensor_reading_millis
            .iter()
            .filter(|(_, last_seen)| {
                captured_at_millis.saturating_sub(**last_seen) >= SENSOR_GAP_MILLIS
            })
            .map(|(key, _)| key.clone())
            .collect();
        for key in stale_keys {
            let previous_level = self
                .previous_sensor_alert_levels
                .get(&key)
                .copied()
                .unwrap_or_default();
            if previous_level != AlertLevel::Nominal {
                self.push_event(
                    captured_at_millis,
                    TimelineCategory::Host,
                    TimelineSeverity::Info,
                    None,
                    format!("{key} readings unavailable"),
                    format!(
                        "The previous {level} alert was cleared because the sensor stopped \
                         reporting. A fresh alert will fire if the condition persists when \
                         readings resume.",
                        level = level_label(previous_level)
                    ),
                );
            }
            self.previous_sensor_alert_levels.remove(&key);
            self.last_sensor_reading_millis.remove(&key);
        }
    }

    /// Look up the configured threshold for a sensor key and run the
    /// transition machinery. Keys with no configured threshold are
    /// silently skipped — this is how a user would "disable" an alert
    /// without having to remove the sensor entirely.
    fn evaluate_alert_transition(
        &mut self,
        captured_at_millis: u64,
        sensor_key: &str,
        value: f32,
        title_label: &str,
        detail_builder: impl Fn(f32, AlertLevel) -> String,
    ) {
        let threshold = self
            .sensor_alert_thresholds
            .iter()
            .find(|threshold| threshold.sensor_key == sensor_key)
            .cloned();
        let Some(threshold) = threshold else {
            return;
        };
        self.evaluate_alert_with_threshold(
            captured_at_millis,
            &threshold,
            value,
            title_label,
            detail_builder,
        );
    }

    /// Shared transition machinery used by both configured and synthetic
    /// thresholds. Split out so the fan-stuck code path (which builds its
    /// own per-fan synthetic threshold) can reuse the same emission rules
    /// as the lookup-based path.
    fn evaluate_alert_with_threshold(
        &mut self,
        captured_at_millis: u64,
        threshold: &AlertThreshold,
        value: f32,
        title_label: &str,
        detail_builder: impl Fn(f32, AlertLevel) -> String,
    ) {
        let key = threshold.sensor_key.clone();
        let previous = self
            .previous_sensor_alert_levels
            .get(&key)
            .copied()
            .unwrap_or_default();
        let current = classify_alert(value, threshold);
        if current == previous {
            return;
        }
        self.previous_sensor_alert_levels.insert(key, current);

        // Recovery: any non-nominal level dropping to Nominal produces
        // an `Info` marker so the user sees the alert close cleanly.
        if current == AlertLevel::Nominal {
            self.push_event(
                captured_at_millis,
                TimelineCategory::Host,
                TimelineSeverity::Info,
                None,
                format!("{title_label} normalized"),
                detail_builder(value, current),
            );
            return;
        }

        // Escalation / de-escalation: emit at the new level's severity.
        self.push_event(
            captured_at_millis,
            TimelineCategory::Host,
            alert_level_to_severity(current),
            None,
            format!(
                "{title_label} {transition}",
                transition = transition_verb(previous, current)
            ),
            detail_builder(value, current),
        );
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

/// Default thresholds used when the history tracker is first constructed.
///
/// These are the values Aetower ships with; a future settings UI can
/// replace them via `History::set_sensor_alert_thresholds`. The numbers
/// were chosen to match the conventional bands used by iStat Menus,
/// Sensei, and Apple's own System Information:
///
/// - **CPU temperature**: 90°C is where M-series SoCs start throttling
///   aggressively, and 100°C is the hard thermal cut-off. Anything
///   sustained above 90°C is actionable; above 100°C is "the machine is
///   about to shut down".
/// - **Battery health**: 80% is Apple's own threshold for recommending
///   service (a ~1000-cycle M-series battery); 50% is the point at which
///   the machine essentially cannot run on battery for anything useful.
/// - **Fan stuck under load**: separate code path (tracked by key, not
///   a numeric threshold) — fires when a fan reports 0 RPM while the
///   thermal state is already above `Fair`.
fn default_sensor_alert_thresholds() -> Vec<AlertThreshold> {
    vec![
        AlertThreshold {
            sensor_key: SENSOR_KEY_CPU_TEMPERATURE.to_owned(),
            warning_value: 90.0,
            critical_value: 100.0,
            direction: ThresholdDirection::Above,
        },
        AlertThreshold {
            sensor_key: SENSOR_KEY_BATTERY_HEALTH.to_owned(),
            warning_value: 80.0,
            critical_value: 50.0,
            direction: ThresholdDirection::Below,
        },
        // GPU (Metal heap) memory as a percentage of total unified memory.
        // On Apple Silicon, GPU and CPU share the same physical RAM pool.
        // When the Metal heap exceeds ~75% of total memory, the system is
        // under pressure and a large model load or additional allocation
        // risks pushing into swap. At 90%, inference throughput typically
        // falls off a cliff as Metal pages get evicted to the compressor.
        AlertThreshold {
            sensor_key: SENSOR_KEY_GPU_MEMORY.to_owned(),
            warning_value: 75.0,
            critical_value: 90.0,
            direction: ThresholdDirection::Above,
        },
    ]
}

/// Classify a single reading against a threshold.
///
/// For `Above` thresholds: anything at or above `critical` is `Critical`,
/// at or above `warning` is `Warning`, everything else is `Nominal`.
/// `Below` mirrors this — values at or below `critical` are `Critical`,
/// at or below `warning` are `Warning`.
///
/// The comparison is deliberately inclusive (`>=` / `<=`) so that a value
/// pinned exactly to the threshold always trips the alarm. A user who
/// sets "warn at 90°C" and reads "90°C" expects the alert to fire.
fn classify_alert(value: f32, threshold: &AlertThreshold) -> AlertLevel {
    match threshold.direction {
        ThresholdDirection::Above => {
            if value >= threshold.critical_value {
                AlertLevel::Critical
            } else if value >= threshold.warning_value {
                AlertLevel::Warning
            } else {
                AlertLevel::Nominal
            }
        }
        ThresholdDirection::Below => {
            if value <= threshold.critical_value {
                AlertLevel::Critical
            } else if value <= threshold.warning_value {
                AlertLevel::Warning
            } else {
                AlertLevel::Nominal
            }
        }
    }
}

/// Map an alert level onto the timeline severity used by the UI.
fn alert_level_to_severity(level: AlertLevel) -> TimelineSeverity {
    match level {
        AlertLevel::Nominal => TimelineSeverity::Info,
        AlertLevel::Warning => TimelineSeverity::Warning,
        AlertLevel::Critical => TimelineSeverity::Critical,
    }
}

/// Human-readable label for an alert level used inside timeline detail text.
fn level_label(level: AlertLevel) -> &'static str {
    match level {
        AlertLevel::Nominal => "nominal",
        AlertLevel::Warning => "warning",
        AlertLevel::Critical => "critical",
    }
}

/// Verb describing a transition between two alert levels for the event title.
///
/// We distinguish escalation ("elevated"), de-escalation ("improving"), and
/// recovery ("normalized") so the user can read the timeline top-to-bottom
/// and understand the shape of the incident without cross-referencing
/// timestamps.
fn transition_verb(previous: AlertLevel, current: AlertLevel) -> &'static str {
    match (previous, current) {
        (_, AlertLevel::Critical) => "critical",
        (AlertLevel::Critical, AlertLevel::Warning) => "improving",
        (_, AlertLevel::Warning) => "elevated",
        (_, AlertLevel::Nominal) => "normalized",
    }
}

#[cfg(test)]
mod tests {
    use aetower_model::{
        AggregateMetrics, ComponentKind, ComponentSnapshot, EntityKind, FrictionBreakdown,
        HostSnapshot, MetricTrend, ProvenanceSnapshot, TimelineSeverity,
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
                memory_physical_footprint_bytes: 0,
                disk_read_bps: 24 * 1024 * 1024,
                disk_write_bps: 8 * 1024 * 1024,
                network_receive_bps: 0,
                network_send_bps: 0,
                wakeups_per_second: 400.0,
                energy_nj_per_s: 0.0,
                estimated_gpu_percent: 0.0,
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
                memory_physical_footprint_bytes: 0,
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
                memory_physical_footprint_bytes: 0,
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

    fn host_with_cpu_temperature(celsius: f32) -> HostSnapshot {
        HostSnapshot {
            cpu_temperatures: vec![aetower_model::TemperatureReading {
                label: "p-cluster".to_owned(),
                celsius,
            }],
            ..Default::default()
        }
    }

    fn host_with_battery_health(health_percent: f32) -> HostSnapshot {
        HostSnapshot {
            battery_health: Some(aetower_model::BatteryHealthSnapshot {
                cycle_count: Some(500),
                design_capacity_mah: Some(5000),
                max_capacity_mah: Some(((health_percent / 100.0) * 5000.0) as u32),
                health_percent: Some(health_percent),
                condition: aetower_model::BatteryCondition::Good,
                temperature_celsius: Some(32.0),
            }),
            ..Default::default()
        }
    }

    #[test]
    fn cpu_temperature_transitions_emit_one_event_per_state_change() {
        let mut history = History::new();
        let mut entities: Vec<aetower_model::EntitySnapshot> = Vec::new();

        // Cool tick — no alert event.
        let (events, _) = history.update(1_000, &host_with_cpu_temperature(60.0), &mut entities);
        assert!(
            events.is_empty(),
            "no transition from nominal to nominal should emit an event"
        );

        // Warming into the warning band.
        let (events, _) = history.update(2_000, &host_with_cpu_temperature(92.0), &mut entities);
        let warning_events: Vec<_> = events
            .iter()
            .filter(|event| event.title.starts_with("CPU temperature"))
            .collect();
        assert_eq!(warning_events.len(), 1);
        assert_eq!(warning_events[0].severity, TimelineSeverity::Warning);

        // Staying in the warning band — no duplicate event.
        let (events, _) = history.update(3_000, &host_with_cpu_temperature(95.0), &mut entities);
        assert!(
            events
                .iter()
                .filter(|event| event.title.starts_with("CPU temperature"))
                .count()
                <= 1,
            "sustained warning must not spam the timeline"
        );

        // Critical escalation.
        let (events, _) = history.update(4_000, &host_with_cpu_temperature(102.0), &mut entities);
        let critical_events: Vec<_> = events
            .iter()
            .filter(|event| {
                event.title.starts_with("CPU temperature") && event.timestamp_millis == 4_000
            })
            .collect();
        assert_eq!(critical_events.len(), 1);
        assert_eq!(critical_events[0].severity, TimelineSeverity::Critical);

        // Recovery.
        let (events, _) = history.update(5_000, &host_with_cpu_temperature(55.0), &mut entities);
        let recovery_events: Vec<_> = events
            .iter()
            .filter(|event| {
                event.title.contains("CPU temperature") && event.timestamp_millis == 5_000
            })
            .collect();
        assert_eq!(recovery_events.len(), 1);
        assert_eq!(recovery_events[0].severity, TimelineSeverity::Info);
        assert!(recovery_events[0].title.contains("normalized"));
    }

    #[test]
    fn battery_health_below_threshold_fires_warning() {
        let mut history = History::new();
        let mut entities: Vec<aetower_model::EntitySnapshot> = Vec::new();

        let (events, _) = history.update(1_000, &host_with_battery_health(95.0), &mut entities);
        assert!(
            !events
                .iter()
                .any(|event| event.title.starts_with("Battery health"))
        );

        let (events, _) = history.update(2_000, &host_with_battery_health(75.0), &mut entities);
        let alerts: Vec<_> = events
            .iter()
            .filter(|event| event.title.starts_with("Battery health"))
            .collect();
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].severity, TimelineSeverity::Warning);

        let (events, _) = history.update(3_000, &host_with_battery_health(45.0), &mut entities);
        let critical: Vec<_> = events
            .iter()
            .filter(|event| {
                event.title.starts_with("Battery health") && event.timestamp_millis == 3_000
            })
            .collect();
        assert_eq!(critical.len(), 1);
        assert_eq!(critical[0].severity, TimelineSeverity::Critical);
    }

    #[test]
    fn battery_health_absent_skips_alert() {
        let mut history = History::new();
        let mut entities: Vec<aetower_model::EntitySnapshot> = Vec::new();

        let (events, _) = history.update(1_000, &HostSnapshot::default(), &mut entities);
        assert!(
            !events
                .iter()
                .any(|event| event.title.starts_with("Battery health"))
        );
    }

    #[test]
    fn fan_stuck_fires_only_under_thermal_stress() {
        use aetower_model::{FanReading, ThermalState};
        let mut history = History::new();
        let mut entities: Vec<aetower_model::EntitySnapshot> = Vec::new();

        // Fan at 0 RPM but thermal state nominal — no alert. This is the
        // case where a fan is legitimately idle and the SoC is cool.
        let nominal = HostSnapshot {
            thermal_state: ThermalState::Nominal,
            fans: vec![FanReading {
                id: 0,
                name: "Fan 0".to_owned(),
                current_rpm: 0.0,
                min_rpm: 1000.0,
                max_rpm: 6000.0,
            }],
            ..Default::default()
        };
        let (events, _) = history.update(1_000, &nominal, &mut entities);
        assert!(!events.iter().any(|event| event.title.contains("Fan")));

        // Same fan, but the SoC is now thermally stressed — this is the
        // actionable case.
        let stressed = HostSnapshot {
            thermal_state: ThermalState::Serious,
            fans: nominal.fans.clone(),
            ..Default::default()
        };
        let (events, _) = history.update(2_000, &stressed, &mut entities);
        let fan_events: Vec<_> = events
            .iter()
            .filter(|event| event.title.contains("stalled"))
            .collect();
        assert_eq!(fan_events.len(), 1);
        // Regression: the old synthetic-threshold shim could only ever
        // fire `Critical` because warning_value == critical_value == 1.0
        // and `classify_alert(1.0, above)` takes the `>= critical` branch
        // first. The fan-stuck path now has its own boolean classifier
        // and must produce exactly Warning.
        assert_eq!(fan_events[0].severity, TimelineSeverity::Warning);
    }

    /// Regression: if CPU temperature readings disappear for more than
    /// SENSOR_GAP_MILLIS, the alert tracker must force-clear the
    /// previous level and emit an "unavailable" info event. Otherwise a
    /// critical SoC temperature recorded before an SMC outage would be
    /// treated as "still critical" when readings resumed, and the
    /// transition detector would short-circuit on `current == previous`
    /// — the user would never see a fresh alert re-fire despite the
    /// machine continuing to run hot.
    #[test]
    fn cpu_temperature_gap_emits_unavailable_event_and_reclassifies() {
        let mut history = History::new();
        let mut entities: Vec<aetower_model::EntitySnapshot> = Vec::new();

        // Step 1: CPU hits critical → event fires.
        let (events, _) = history.update(1_000, &host_with_cpu_temperature(102.0), &mut entities);
        assert_eq!(
            events
                .iter()
                .filter(|event| event.title.starts_with("CPU temperature"))
                .count(),
            1
        );

        // Step 2: readings disappear for several ticks. The gap
        // threshold is 15s, so we skip forward past it.
        let empty_host = HostSnapshot::default();
        let (events, _) = history.update(5_000, &empty_host, &mut entities);
        assert!(
            !events
                .iter()
                .any(|event| event.title.contains("readings unavailable")),
            "gap event must not fire before the threshold"
        );
        let (events, _) = history.update(20_000, &empty_host, &mut entities);
        let gap_events: Vec<_> = events
            .iter()
            .filter(|event| {
                event.title.contains("readings unavailable") && event.timestamp_millis == 20_000
            })
            .collect();
        assert_eq!(
            gap_events.len(),
            1,
            "gap event must fire exactly once when the threshold is crossed"
        );

        // Step 3: readings resume, still hot. The re-classification must
        // start from Nominal and re-emit a fresh Critical event, because
        // the user otherwise would have no indication that the machine
        // is still in trouble.
        let (events, _) = history.update(22_000, &host_with_cpu_temperature(103.0), &mut entities);
        let fresh_events: Vec<_> = events
            .iter()
            .filter(|event| {
                event.title.starts_with("CPU temperature") && event.timestamp_millis == 22_000
            })
            .collect();
        assert_eq!(fresh_events.len(), 1, "fresh alert must re-fire after gap");
        assert_eq!(fresh_events[0].severity, TimelineSeverity::Critical);
    }

    /// A brief two-tick hiccup under the gap threshold must NOT clear
    /// alert state. The collector sometimes drops one sample when the
    /// SoC is busy, and we do not want the user to see phantom
    /// "readings unavailable" events for those cases.
    #[test]
    fn cpu_temperature_brief_hiccup_does_not_clear_alert() {
        let mut history = History::new();
        let mut entities: Vec<aetower_model::EntitySnapshot> = Vec::new();

        let (_, _) = history.update(1_000, &host_with_cpu_temperature(95.0), &mut entities);
        // Gap below threshold (15s).
        let (events, _) = history.update(3_000, &HostSnapshot::default(), &mut entities);
        assert!(
            !events
                .iter()
                .any(|event| event.title.contains("readings unavailable")),
            "brief hiccup must not emit gap event"
        );
        // Readings resume at same level — no event because previous state
        // is still Warning.
        let (events, _) = history.update(5_000, &host_with_cpu_temperature(95.0), &mut entities);
        assert!(
            !events
                .iter()
                .any(|event| event.title.contains("CPU temperature")
                    && event.timestamp_millis == 5_000),
            "no fresh alert: the state machine remembered the previous warning"
        );
    }

    /// Regression: when the SoC cools back to nominal while the fan is
    /// still at 0 RPM, the alert should clear and emit a recovery event.
    /// The previous implementation gated the whole fan-evaluation loop on
    /// `thermal_is_elevated`, so the `previous_sensor_alert_levels` entry
    /// stayed at `Warning` forever and no recovery event was ever emitted.
    #[test]
    fn fan_stuck_clears_when_thermal_drops_back_to_nominal() {
        use aetower_model::{FanReading, ThermalState};
        let mut history = History::new();
        let mut entities: Vec<aetower_model::EntitySnapshot> = Vec::new();

        // Step 1: Fan stuck while thermal is serious → Warning emitted.
        let stressed = HostSnapshot {
            thermal_state: ThermalState::Serious,
            fans: vec![FanReading {
                id: 0,
                name: "Fan 0".to_owned(),
                current_rpm: 0.0,
                min_rpm: 1000.0,
                max_rpm: 6000.0,
            }],
            ..Default::default()
        };
        let (events, _) = history.update(1_000, &stressed, &mut entities);
        assert_eq!(
            events
                .iter()
                .filter(|event| event.title.contains("stalled"))
                .count(),
            1
        );

        // Step 2: Thermal drops to nominal, fan still at 0 RPM. The
        // compound condition (stuck AND stressed) is no longer true, so
        // the alert must clear with a recovery event, not stay silent.
        let cooled = HostSnapshot {
            thermal_state: ThermalState::Nominal,
            fans: stressed.fans.clone(),
            ..Default::default()
        };
        let (events, _) = history.update(2_000, &cooled, &mut entities);
        let recovery: Vec<_> = events
            .iter()
            .filter(|event| event.title.contains("Fan") && event.timestamp_millis == 2_000)
            .collect();
        assert_eq!(
            recovery.len(),
            1,
            "recovery event should fire when thermal drops even if fan RPM is unchanged"
        );
        assert_eq!(recovery[0].severity, TimelineSeverity::Info);
    }

    #[test]
    fn classify_alert_handles_both_directions() {
        use super::{AlertLevel, AlertThreshold, ThresholdDirection, classify_alert};

        let above = AlertThreshold {
            sensor_key: "t".to_owned(),
            warning_value: 90.0,
            critical_value: 100.0,
            direction: ThresholdDirection::Above,
        };
        assert_eq!(classify_alert(60.0, &above), AlertLevel::Nominal);
        // Inclusive lower bound — exactly 90 must fire warning.
        assert_eq!(classify_alert(90.0, &above), AlertLevel::Warning);
        assert_eq!(classify_alert(99.9, &above), AlertLevel::Warning);
        assert_eq!(classify_alert(100.0, &above), AlertLevel::Critical);

        let below = AlertThreshold {
            sensor_key: "h".to_owned(),
            warning_value: 80.0,
            critical_value: 50.0,
            direction: ThresholdDirection::Below,
        };
        assert_eq!(classify_alert(95.0, &below), AlertLevel::Nominal);
        assert_eq!(classify_alert(80.0, &below), AlertLevel::Warning);
        assert_eq!(classify_alert(50.0, &below), AlertLevel::Critical);
        assert_eq!(classify_alert(10.0, &below), AlertLevel::Critical);
    }

    fn ai_agent_entity(
        id: &str,
        name: &str,
        energy_nj_per_s: f64,
    ) -> aetower_model::EntitySnapshot {
        let mut entity = entity_with_process(id, name, 999, 1_000);
        entity.entity_kind = aetower_model::EntityKind::AiAgent;
        entity.metrics.energy_nj_per_s = energy_nj_per_s;
        entity
    }

    #[test]
    fn ai_session_accumulates_energy_across_ticks() {
        let mut history = History::new();
        let host = HostSnapshot::default();

        // 1 W = 1e9 nJ/s. Over 2s tick → 2e9 nJ per tick.
        let mut entities = vec![ai_agent_entity(
            "ai-agent:1",
            "Claude Code",
            1_000_000_000.0,
        )];
        let _ = history.update(1_000, &host, &mut entities);
        assert_eq!(
            entities[0].agent_cost.as_ref().map(|c| c.session_energy_nj),
            Some(2_000_000_000)
        );

        // Second tick → cumulative 4e9 nJ.
        let mut entities = vec![ai_agent_entity(
            "ai-agent:1",
            "Claude Code",
            1_000_000_000.0,
        )];
        let _ = history.update(3_000, &host, &mut entities);
        assert_eq!(
            entities[0].agent_cost.as_ref().map(|c| c.session_energy_nj),
            Some(4_000_000_000)
        );
    }

    #[test]
    fn ai_session_end_emits_energy_summary() {
        let mut history = History::new();
        let host = HostSnapshot::default();

        // Two ticks of 1 W → 4e9 nJ cumulative.
        let mut entities = vec![ai_agent_entity(
            "ai-agent:1",
            "Claude Code",
            1_000_000_000.0,
        )];
        let _ = history.update(1_000, &host, &mut entities);
        let mut entities = vec![ai_agent_entity(
            "ai-agent:1",
            "Claude Code",
            1_000_000_000.0,
        )];
        let _ = history.update(3_000, &host, &mut entities);

        // Agent disappears — must be absent for 3 consecutive ticks
        // (the debounce window) before the session-end event fires.
        let mut entities: Vec<aetower_model::EntitySnapshot> = Vec::new();
        let (events, _) = history.update(5_000, &host, &mut entities);
        assert!(
            !events
                .iter()
                .any(|event| event.title.contains("session ended")),
            "session end must NOT fire after only 1 absent tick (debounce)"
        );
        let mut entities: Vec<aetower_model::EntitySnapshot> = Vec::new();
        let _ = history.update(7_000, &host, &mut entities);
        let mut entities: Vec<aetower_model::EntitySnapshot> = Vec::new();
        let (events, _) = history.update(9_000, &host, &mut entities);
        let session_end: Vec<_> = events
            .iter()
            .filter(|event| event.title.contains("session ended"))
            .collect();
        assert_eq!(
            session_end.len(),
            1,
            "session end event must fire after 3 absent ticks: {events:?}"
        );
        assert_eq!(session_end[0].severity, TimelineSeverity::Info);
        assert!(session_end[0].detail.contains("Wh") || session_end[0].detail.contains("mWh"));
    }

    /// Regression: a single-tick entity disappearance (collector hiccup)
    /// must NOT flush accumulated energy or emit a spurious event.
    #[test]
    fn ai_session_survives_single_tick_gap() {
        let mut history = History::new();
        let host = HostSnapshot::default();

        // Two ticks of active energy.
        let mut entities = vec![ai_agent_entity(
            "ai-agent:1",
            "Claude Code",
            1_000_000_000.0,
        )];
        let _ = history.update(1_000, &host, &mut entities);
        let mut entities = vec![ai_agent_entity(
            "ai-agent:1",
            "Claude Code",
            1_000_000_000.0,
        )];
        let _ = history.update(3_000, &host, &mut entities);

        // Entity disappears for one tick.
        let mut entities: Vec<aetower_model::EntitySnapshot> = Vec::new();
        let (events, _) = history.update(5_000, &host, &mut entities);
        assert!(
            !events
                .iter()
                .any(|event| event.title.contains("session ended")),
            "single-tick gap must not flush the session"
        );

        // Entity returns — cumulative energy must continue from where
        // it left off, NOT restart from zero.
        let mut entities = vec![ai_agent_entity(
            "ai-agent:1",
            "Claude Code",
            1_000_000_000.0,
        )];
        let _ = history.update(7_000, &host, &mut entities);
        assert_eq!(
            entities[0].agent_cost.as_ref().map(|c| c.session_energy_nj),
            Some(6_000_000_000), // 3 ticks × 2e9 nJ/tick
            "cumulative energy must not reset after a brief gap"
        );
    }

    #[test]
    fn gpu_memory_warning_fires_at_75_percent() {
        let mut history = History::new();
        let mut entities: Vec<aetower_model::EntitySnapshot> = Vec::new();

        // 12 GB of 16 GB = 75% → exactly at warning threshold.
        let host = HostSnapshot {
            memory_total_bytes: 16 * 1024 * 1024 * 1024,
            gpu_memory_bytes: 12 * 1024 * 1024 * 1024,
            ..Default::default()
        };
        let (events, _) = history.update(1_000, &host, &mut entities);
        let gpu_events: Vec<_> = events
            .iter()
            .filter(|event| event.title.starts_with("GPU memory"))
            .collect();
        assert_eq!(gpu_events.len(), 1);
        assert_eq!(gpu_events[0].severity, TimelineSeverity::Warning);
    }

    #[test]
    fn gpu_memory_critical_fires_at_90_percent() {
        let mut history = History::new();
        let mut entities: Vec<aetower_model::EntitySnapshot> = Vec::new();

        // Warm up with a warning first to test the transition.
        let warning_host = HostSnapshot {
            memory_total_bytes: 16 * 1024 * 1024 * 1024,
            gpu_memory_bytes: 12 * 1024 * 1024 * 1024,
            ..Default::default()
        };
        let _ = history.update(1_000, &warning_host, &mut entities);

        // 14.5 GB of 16 GB = ~90.6% → critical.
        let critical_host = HostSnapshot {
            memory_total_bytes: 16 * 1024 * 1024 * 1024,
            gpu_memory_bytes: (14.5 * 1024.0 * 1024.0 * 1024.0) as u64,
            ..Default::default()
        };
        let (events, _) = history.update(2_000, &critical_host, &mut entities);
        let critical: Vec<_> = events
            .iter()
            .filter(|event| {
                event.title.starts_with("GPU memory") && event.timestamp_millis == 2_000
            })
            .collect();
        assert_eq!(critical.len(), 1);
        assert_eq!(critical[0].severity, TimelineSeverity::Critical);
    }

    #[test]
    fn gpu_memory_recovers_when_freed() {
        let mut history = History::new();
        let mut entities: Vec<aetower_model::EntitySnapshot> = Vec::new();

        // Critical state.
        let critical = HostSnapshot {
            memory_total_bytes: 16 * 1024 * 1024 * 1024,
            gpu_memory_bytes: 15 * 1024 * 1024 * 1024,
            ..Default::default()
        };
        let _ = history.update(1_000, &critical, &mut entities);

        // Drop below 75% → recovery.
        let recovered = HostSnapshot {
            memory_total_bytes: 16 * 1024 * 1024 * 1024,
            gpu_memory_bytes: 8 * 1024 * 1024 * 1024,
            ..Default::default()
        };
        let (events, _) = history.update(2_000, &recovered, &mut entities);
        let recovery: Vec<_> = events
            .iter()
            .filter(|event| event.title.contains("GPU memory") && event.timestamp_millis == 2_000)
            .collect();
        assert_eq!(recovery.len(), 1);
        assert_eq!(recovery[0].severity, TimelineSeverity::Info);
        assert!(recovery[0].title.contains("normalized"));
    }
}
