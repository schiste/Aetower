//! Proactive "what changed since" regression detection over persisted history.
//!
//! Pure analysis: given a window of `SystemSnapshot`s (downsampled, e.g. one
//! per hour over several days), group by `bundle_id` and flag slow regressions
//! the live 60-second rings can't see — sustained memory creep, and idle CPU
//! that rose after an app update. No I/O; the engine supplies the snapshots and
//! turns findings into timeline events.

use std::collections::BTreeMap;

use aetower_model::{SystemSnapshot, TimelineSeverity};

const DAY_MILLIS: u64 = 86_400_000;
/// Minimum distinct day-buckets before a creep verdict is trustworthy.
const MIN_CREEP_DAYS: usize = 3;
/// Memory must grow by at least this ratio to count as creep.
const CREEP_RATIO: f64 = 1.5;
/// …and by at least this absolute amount (ignore noise on tiny processes).
const CREEP_FLOOR_BYTES: f64 = 64.0 * 1024.0 * 1024.0;
/// Idle CPU must at least double after an update to count as a regression.
const IDLE_CPU_RATIO: f64 = 2.0;
/// …and reach this floor (ignore jitter near zero).
const IDLE_CPU_FLOOR_PCT: f64 = 2.0;
/// Minimum samples on each side of an update boundary.
const MIN_SIDE_SAMPLES: usize = 3;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegressionKind {
    /// Resident memory has trended up over multiple days.
    Memory7dCreep,
    /// Idle (background) CPU roughly doubled after a version change.
    IdleCpuSinceUpdate,
}

#[derive(Debug, Clone)]
pub struct RegressionFinding {
    pub bundle_id: String,
    pub display_name: String,
    pub kind: RegressionKind,
    pub summary: String,
    pub severity: TimelineSeverity,
}

#[derive(Default)]
struct BundleSeries {
    display_name: String,
    points: Vec<Point>,
}

struct Point {
    millis: u64,
    cpu_percent: f64,
    is_foreground: bool,
    memory_bytes: u64,
    version: Option<String>,
}

/// Detect regressions across a window of snapshots. `now_millis` is reserved
/// for future age-weighting; detection is currently window-relative.
pub fn detect_regressions(
    snapshots: &[SystemSnapshot],
    _now_millis: u64,
) -> Vec<RegressionFinding> {
    let mut by_bundle: BTreeMap<String, BundleSeries> = BTreeMap::new();
    for snapshot in snapshots {
        for entity in &snapshot.entities {
            let Some(bundle_id) = entity.bundle_id.clone() else {
                continue;
            };
            let series = by_bundle.entry(bundle_id).or_default();
            series.display_name = entity.display_name.clone();
            series.points.push(Point {
                millis: snapshot.captured_at_millis,
                cpu_percent: entity.metrics.cpu_percent as f64,
                is_foreground: entity.metrics.is_foreground,
                memory_bytes: entity.metrics.memory_resident_bytes,
                version: entity.app_version.clone(),
            });
        }
    }

    let mut findings = Vec::new();
    for (bundle_id, mut series) in by_bundle {
        series.points.sort_by_key(|point| point.millis);
        if let Some(finding) = detect_memory_creep(&bundle_id, &series) {
            findings.push(finding);
        }
        if let Some(finding) = detect_idle_cpu_since_update(&bundle_id, &series) {
            findings.push(finding);
        }
    }
    findings
}

/// Median of resident memory per day-bucket; creep if the newest day is
/// materially higher than the oldest across enough days.
fn detect_memory_creep(bundle_id: &str, series: &BundleSeries) -> Option<RegressionFinding> {
    let mut buckets: BTreeMap<u64, Vec<f64>> = BTreeMap::new();
    for point in &series.points {
        buckets
            .entry(point.millis / DAY_MILLIS)
            .or_default()
            .push(point.memory_bytes as f64);
    }
    if buckets.len() < MIN_CREEP_DAYS {
        return None;
    }
    let medians: Vec<f64> = buckets.values().map(|values| median(values)).collect();
    let first = *medians.first()?;
    let last = *medians.last()?;
    if first <= 0.0 || last < first * CREEP_RATIO || last - first < CREEP_FLOOR_BYTES {
        return None;
    }
    Some(RegressionFinding {
        bundle_id: bundle_id.to_owned(),
        display_name: series.display_name.clone(),
        kind: RegressionKind::Memory7dCreep,
        summary: format!(
            "Memory crept from ~{:.0} MB to ~{:.0} MB over {} days — possible leak.",
            first / 1_048_576.0,
            last / 1_048_576.0,
            buckets.len()
        ),
        severity: TimelineSeverity::Warning,
    })
}

/// Compare median idle (background) CPU before vs after the most recent version
/// change; flag if it roughly doubled.
fn detect_idle_cpu_since_update(
    bundle_id: &str,
    series: &BundleSeries,
) -> Option<RegressionFinding> {
    // Find the most recent version-change boundary.
    let mut boundary: Option<(u64, String, String)> = None;
    let mut current: Option<&str> = None;
    for point in &series.points {
        if let Some(version) = point.version.as_deref() {
            if let Some(previous) = current
                && previous != version
            {
                boundary = Some((point.millis, previous.to_owned(), version.to_owned()));
            }
            current = Some(version);
        }
    }
    let (boundary_millis, old_version, new_version) = boundary?;

    let before: Vec<f64> = series
        .points
        .iter()
        .filter(|point| !point.is_foreground && point.millis < boundary_millis)
        .map(|point| point.cpu_percent)
        .collect();
    let after: Vec<f64> = series
        .points
        .iter()
        .filter(|point| !point.is_foreground && point.millis >= boundary_millis)
        .map(|point| point.cpu_percent)
        .collect();
    if before.len() < MIN_SIDE_SAMPLES || after.len() < MIN_SIDE_SAMPLES {
        return None;
    }
    let before_median = median(&before);
    let after_median = median(&after);
    if before_median <= 0.0
        || after_median < before_median * IDLE_CPU_RATIO
        || after_median < IDLE_CPU_FLOOR_PCT
    {
        return None;
    }
    Some(RegressionFinding {
        bundle_id: bundle_id.to_owned(),
        display_name: series.display_name.clone(),
        kind: RegressionKind::IdleCpuSinceUpdate,
        summary: format!(
            "Idle CPU rose from ~{before_median:.1}% to ~{after_median:.1}% after updating from {old_version} to {new_version}.",
        ),
        severity: TimelineSeverity::Warning,
    })
}

fn median(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(|left, right| left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal));
    let mid = sorted.len() / 2;
    if sorted.len().is_multiple_of(2) {
        (sorted[mid - 1] + sorted[mid]) / 2.0
    } else {
        sorted[mid]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aetower_model::{AggregateMetrics, EntitySnapshot, SystemSnapshot};

    fn snapshot_at(millis: u64, entity: EntitySnapshot) -> SystemSnapshot {
        SystemSnapshot {
            captured_at_millis: millis,
            entities: vec![entity],
            ..Default::default()
        }
    }

    fn app(memory_mb: u64, cpu: f32, foreground: bool, version: &str) -> EntitySnapshot {
        EntitySnapshot {
            entity_id: "e".to_owned(),
            bundle_id: Some("com.acme.app".to_owned()),
            display_name: "Acme".to_owned(),
            app_version: Some(version.to_owned()),
            metrics: AggregateMetrics {
                cpu_percent: cpu,
                memory_resident_bytes: memory_mb * 1_048_576,
                is_foreground: foreground,
                ..Default::default()
            },
            ..Default::default()
        }
    }

    #[test]
    fn detects_memory_creep_over_days() {
        let mut snapshots = Vec::new();
        // 4 days, memory rising 100 → 400 MB.
        for (day, mem) in [100u64, 200, 300, 400].iter().enumerate() {
            snapshots.push(snapshot_at(
                day as u64 * DAY_MILLIS + 1,
                app(*mem, 1.0, false, "1.0"),
            ));
        }
        let findings = detect_regressions(&snapshots, 4 * DAY_MILLIS);
        assert!(
            findings
                .iter()
                .any(|f| f.kind == RegressionKind::Memory7dCreep),
            "expected a memory-creep finding: {findings:?}"
        );
    }

    #[test]
    fn detects_idle_cpu_doubling_after_update() {
        let mut snapshots = Vec::new();
        // v1: idle CPU ~2% for several background samples.
        for i in 0..5 {
            snapshots.push(snapshot_at(i * 3_600_000, app(150, 2.0, false, "1.0")));
        }
        // v2: idle CPU ~6% afterward.
        for i in 5..10 {
            snapshots.push(snapshot_at(i * 3_600_000, app(150, 6.0, false, "2.0")));
        }
        let findings = detect_regressions(&snapshots, 10 * 3_600_000);
        assert!(
            findings
                .iter()
                .any(|f| f.kind == RegressionKind::IdleCpuSinceUpdate),
            "expected an idle-CPU-since-update finding: {findings:?}"
        );
    }

    #[test]
    fn no_findings_on_flat_series() {
        let mut snapshots = Vec::new();
        for (day, _) in [0u64, 1, 2, 3].iter().enumerate() {
            snapshots.push(snapshot_at(
                day as u64 * DAY_MILLIS + 1,
                app(150, 2.0, false, "1.0"),
            ));
        }
        let findings = detect_regressions(&snapshots, 4 * DAY_MILLIS);
        assert!(
            findings.is_empty(),
            "flat series must not regress: {findings:?}"
        );
    }
}
