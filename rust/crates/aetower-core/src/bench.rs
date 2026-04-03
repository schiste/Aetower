use std::time::Instant;

use serde::Serialize;
use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System};

use crate::{attribution, collector::Collector, friction, identity};

#[derive(Debug, Clone, Copy)]
pub struct BenchmarkConfig {
    pub iterations: usize,
    pub warmup_iterations: usize,
    pub enforce_default_budget: bool,
}

impl Default for BenchmarkConfig {
    fn default() -> Self {
        Self {
            iterations: 30,
            warmup_iterations: 3,
            enforce_default_budget: false,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct BenchmarkReport {
    pub iterations: usize,
    pub warmup_iterations: usize,
    pub process_count_average: f64,
    pub entity_count_average: f64,
    pub collect_millis: BenchmarkStats,
    pub identity_millis: BenchmarkStats,
    pub attribution_millis: BenchmarkStats,
    pub friction_millis: BenchmarkStats,
    pub total_millis: BenchmarkStats,
    pub resident_memory_before_bytes: Option<u64>,
    pub resident_memory_after_bytes: Option<u64>,
    pub default_budget: BenchmarkBudget,
    pub default_budget_result: BenchmarkBudgetResult,
}

#[derive(Debug, Serialize)]
pub struct BenchmarkStats {
    pub average: f64,
    pub p95: f64,
    pub max: f64,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct BenchmarkBudget {
    pub collect_average_millis_max: f64,
    pub identity_average_millis_max: f64,
    pub total_p95_millis_max: f64,
    pub resident_memory_growth_bytes_max: u64,
}

#[derive(Debug, Serialize)]
pub struct BenchmarkBudgetResult {
    pub passed: bool,
    pub violations: Vec<String>,
}

pub fn run_benchmark(config: BenchmarkConfig) -> BenchmarkReport {
    let iterations = config.iterations.max(1);
    let mut collector = Collector::new();
    let mut process_counts = Vec::with_capacity(iterations);
    let mut entity_counts = Vec::with_capacity(iterations);
    let mut collect_timings = Vec::with_capacity(iterations);
    let mut identity_timings = Vec::with_capacity(iterations);
    let mut attribution_timings = Vec::with_capacity(iterations);
    let mut friction_timings = Vec::with_capacity(iterations);
    let mut total_timings = Vec::with_capacity(iterations);
    let resident_memory_before_bytes = current_process_memory_bytes();

    for _ in 0..config.warmup_iterations {
        let raw = collector.collect();
        let host = aetower_model::HostSnapshot {
            cpu_percent: raw.host.cpu_percent,
            memory_used_bytes: raw.host.memory_used_bytes,
            memory_total_bytes: raw.host.memory_total_bytes,
            swap_used_bytes: raw.host.swap_used_bytes,
            compressed_memory_bytes: raw.host.compressed_memory_bytes,
            disk_read_bps: raw.host.disk_read_bps,
            disk_write_bps: raw.host.disk_write_bps,
            network_receive_bps: raw.host.network_receive_bps,
            network_send_bps: raw.host.network_send_bps,
            wakeups_per_second: raw.host.wakeups_per_second,
            thermal_state: raw.host.thermal_state,
            on_battery: raw.host.on_battery,
            battery_charge_percent: raw.host.battery_charge_percent,
            low_power_mode: raw.host.low_power_mode,
            frontmost_app_name: None,
            frontmost_window_title: None,
            ai_agent_friction: 0.0,
            ai_agent_count: 0,
            gpu_percent: 0.0,
            ane_percent: 0.0,
            gpu_memory_bytes: 0,
        };
        let identity = identity::resolve(&raw.processes);
        let mut entities = attribution::build_entities(&raw.processes, &identity, None);
        friction::apply(&host, &mut entities);
    }

    for _ in 0..iterations {
        let total_started_at = Instant::now();

        let collect_started_at = Instant::now();
        let raw = collector.collect();
        collect_timings.push(elapsed_millis(collect_started_at));

        let host = aetower_model::HostSnapshot {
            cpu_percent: raw.host.cpu_percent,
            memory_used_bytes: raw.host.memory_used_bytes,
            memory_total_bytes: raw.host.memory_total_bytes,
            swap_used_bytes: raw.host.swap_used_bytes,
            compressed_memory_bytes: raw.host.compressed_memory_bytes,
            disk_read_bps: raw.host.disk_read_bps,
            disk_write_bps: raw.host.disk_write_bps,
            network_receive_bps: raw.host.network_receive_bps,
            network_send_bps: raw.host.network_send_bps,
            wakeups_per_second: raw.host.wakeups_per_second,
            thermal_state: raw.host.thermal_state,
            on_battery: raw.host.on_battery,
            battery_charge_percent: raw.host.battery_charge_percent,
            low_power_mode: raw.host.low_power_mode,
            frontmost_app_name: None,
            frontmost_window_title: None,
            ai_agent_friction: 0.0,
            ai_agent_count: 0,
            gpu_percent: 0.0,
            ane_percent: 0.0,
            gpu_memory_bytes: 0,
        };
        let identity_started_at = Instant::now();
        let identity = identity::resolve(&raw.processes);
        identity_timings.push(elapsed_millis(identity_started_at));

        let attribution_started_at = Instant::now();
        let mut entities = attribution::build_entities(&raw.processes, &identity, None);
        attribution_timings.push(elapsed_millis(attribution_started_at));

        let friction_started_at = Instant::now();
        friction::apply(&host, &mut entities);
        friction_timings.push(elapsed_millis(friction_started_at));

        process_counts.push(raw.processes.len() as f64);
        entity_counts.push(entities.len() as f64);
        total_timings.push(elapsed_millis(total_started_at));
    }

    let resident_memory_after_bytes = current_process_memory_bytes();

    let default_budget = BenchmarkBudget::default();
    let report = BenchmarkReport {
        iterations,
        warmup_iterations: config.warmup_iterations,
        process_count_average: average(&process_counts),
        entity_count_average: average(&entity_counts),
        collect_millis: BenchmarkStats::from_samples(&collect_timings),
        identity_millis: BenchmarkStats::from_samples(&identity_timings),
        attribution_millis: BenchmarkStats::from_samples(&attribution_timings),
        friction_millis: BenchmarkStats::from_samples(&friction_timings),
        total_millis: BenchmarkStats::from_samples(&total_timings),
        resident_memory_before_bytes,
        resident_memory_after_bytes,
        default_budget,
        default_budget_result: BenchmarkBudgetResult {
            passed: true,
            violations: Vec::new(),
        },
    };

    report.with_budget_result(default_budget)
}

impl BenchmarkStats {
    fn from_samples(samples: &[f64]) -> Self {
        let mut sorted = samples.to_vec();
        sorted.sort_by(f64::total_cmp);

        Self {
            average: average(samples),
            p95: percentile(&sorted, 0.95),
            max: sorted.last().copied().unwrap_or_default(),
        }
    }
}

impl BenchmarkBudget {
    pub fn evaluate(&self, report: &BenchmarkReport) -> BenchmarkBudgetResult {
        let mut violations = Vec::new();

        if report.collect_millis.average > self.collect_average_millis_max {
            violations.push(format!(
                "collect average {:.2} ms exceeds {:.2} ms",
                report.collect_millis.average, self.collect_average_millis_max
            ));
        }
        if report.identity_millis.average > self.identity_average_millis_max {
            violations.push(format!(
                "identity average {:.2} ms exceeds {:.2} ms",
                report.identity_millis.average, self.identity_average_millis_max
            ));
        }
        if report.total_millis.p95 > self.total_p95_millis_max {
            violations.push(format!(
                "total p95 {:.2} ms exceeds {:.2} ms",
                report.total_millis.p95, self.total_p95_millis_max
            ));
        }
        if let Some(growth) = report.resident_memory_growth_bytes()
            && growth > self.resident_memory_growth_bytes_max
        {
            violations.push(format!(
                "resident memory growth {} bytes exceeds {} bytes",
                growth, self.resident_memory_growth_bytes_max
            ));
        }

        BenchmarkBudgetResult {
            passed: violations.is_empty(),
            violations,
        }
    }
}

impl Default for BenchmarkBudget {
    fn default() -> Self {
        Self {
            collect_average_millis_max: 100.0,
            identity_average_millis_max: 15.0,
            total_p95_millis_max: 200.0,
            resident_memory_growth_bytes_max: 24 * 1024 * 1024,
        }
    }
}

impl BenchmarkReport {
    pub fn resident_memory_growth_bytes(&self) -> Option<u64> {
        Some(
            self.resident_memory_after_bytes?
                .saturating_sub(self.resident_memory_before_bytes?),
        )
    }

    pub fn with_budget_result(mut self, budget: BenchmarkBudget) -> Self {
        self.default_budget = budget;
        self.default_budget_result = budget.evaluate(&self);
        self
    }
}

fn average(samples: &[f64]) -> f64 {
    if samples.is_empty() {
        0.0
    } else {
        samples.iter().sum::<f64>() / samples.len() as f64
    }
}

fn percentile(sorted_samples: &[f64], ratio: f64) -> f64 {
    if sorted_samples.is_empty() {
        return 0.0;
    }
    let index = ((sorted_samples.len() - 1) as f64 * ratio).round() as usize;
    sorted_samples[index]
}

fn elapsed_millis(started_at: Instant) -> f64 {
    started_at.elapsed().as_secs_f64() * 1_000.0
}

fn current_process_memory_bytes() -> Option<u64> {
    let mut system = System::new();
    system.refresh_processes_specifics(ProcessesToUpdate::All, true, ProcessRefreshKind::nothing());
    system
        .process(sysinfo::Pid::from_u32(std::process::id()))
        .map(|process| process.memory())
}

#[cfg(test)]
mod tests {
    use super::{BenchmarkBudget, BenchmarkBudgetResult, BenchmarkReport, BenchmarkStats};

    fn report() -> BenchmarkReport {
        BenchmarkReport {
            iterations: 1,
            warmup_iterations: 0,
            process_count_average: 100.0,
            entity_count_average: 50.0,
            collect_millis: BenchmarkStats {
                average: 1.0,
                p95: 1.5,
                max: 2.0,
            },
            identity_millis: BenchmarkStats {
                average: 0.5,
                p95: 0.8,
                max: 1.0,
            },
            attribution_millis: BenchmarkStats {
                average: 0.4,
                p95: 0.7,
                max: 1.0,
            },
            friction_millis: BenchmarkStats {
                average: 0.1,
                p95: 0.2,
                max: 0.3,
            },
            total_millis: BenchmarkStats {
                average: 2.0,
                p95: 3.0,
                max: 4.0,
            },
            resident_memory_before_bytes: Some(10),
            resident_memory_after_bytes: Some(20),
            default_budget: BenchmarkBudget::default(),
            default_budget_result: BenchmarkBudgetResult {
                passed: true,
                violations: Vec::new(),
            },
        }
    }

    #[test]
    fn budget_passes_when_metrics_fit_thresholds() {
        let result = BenchmarkBudget::default().evaluate(&report());
        assert!(result.passed);
        assert!(result.violations.is_empty());
    }

    #[test]
    fn budget_collects_violations_when_thresholds_are_exceeded() {
        let mut report = report();
        report.collect_millis.average = 150.0;
        report.total_millis.p95 = 300.0;
        report.resident_memory_after_bytes = Some(40 * 1024 * 1024);
        let result = BenchmarkBudget::default().evaluate(&report);

        assert!(!result.passed);
        assert_eq!(result.violations.len(), 3);
    }
}
