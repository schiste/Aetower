use std::collections::BTreeMap;

use sysinfo::{
    CpuExt, CpuRefreshKind, NetworkExt, PidExt, ProcessExt, ProcessRefreshKind, RefreshKind,
    System, SystemExt,
};

#[derive(Debug, Clone)]
pub struct RawProcessSample {
    pub pid: u32,
    pub parent_pid: Option<u32>,
    pub name: String,
    pub exe: Option<String>,
    pub cmd: Vec<String>,
    pub cpu_percent: f32,
    pub memory_bytes: u64,
    pub virtual_memory_bytes: u64,
    pub disk_read_bytes: u64,
    pub disk_write_bytes: u64,
}

#[derive(Debug, Clone, Default)]
pub struct RawHostSample {
    pub cpu_percent: f32,
    pub memory_used_bytes: u64,
    pub memory_total_bytes: u64,
    pub swap_used_bytes: u64,
    pub network_receive_bps: u64,
    pub network_send_bps: u64,
}

#[derive(Debug, Clone, Default)]
pub struct RawSnapshot {
    pub host: RawHostSample,
    pub processes: Vec<RawProcessSample>,
}

#[derive(Debug, Clone, Copy, Default)]
struct NetworkTotals {
    received: u64,
    transmitted: u64,
}

pub struct Collector {
    system: System,
    previous_network_totals: NetworkTotals,
}

impl Collector {
    pub fn new() -> Self {
        let refresh = RefreshKind::new()
            .with_cpu(CpuRefreshKind::everything())
            .with_memory()
            .with_processes(ProcessRefreshKind::everything());
        let mut system = System::new_with_specifics(refresh);
        system.refresh_all();
        Self {
            system,
            previous_network_totals: NetworkTotals::default(),
        }
    }

    pub fn collect(&mut self) -> RawSnapshot {
        self.system.refresh_cpu();
        self.system.refresh_memory();
        self.system.refresh_processes();
        self.system.refresh_processes_specifics(ProcessRefreshKind::everything());

        let mut network_totals = NetworkTotals::default();
        for (_name, data) in self.system.networks() {
            network_totals.received = network_totals.received.saturating_add(data.received());
            network_totals.transmitted =
                network_totals.transmitted.saturating_add(data.transmitted());
        }

        let host = RawHostSample {
            cpu_percent: self.system.global_cpu_info().cpu_usage(),
            memory_used_bytes: self.system.used_memory() * 1024,
            memory_total_bytes: self.system.total_memory() * 1024,
            swap_used_bytes: self.system.used_swap() * 1024,
            network_receive_bps: network_totals
                .received
                .saturating_sub(self.previous_network_totals.received),
            network_send_bps: network_totals
                .transmitted
                .saturating_sub(self.previous_network_totals.transmitted),
        };
        self.previous_network_totals = network_totals;

        let processes = self
            .system
            .processes()
            .values()
            .map(|process| RawProcessSample {
                pid: process.pid().as_u32(),
                parent_pid: process.parent().map(|parent| parent.as_u32()),
                name: process.name().to_owned(),
                exe: path_to_string(process.exe()),
                cmd: process
                    .cmd()
                    .iter()
                    .map(|segment| segment.to_string())
                    .collect(),
                cpu_percent: process.cpu_usage(),
                memory_bytes: process.memory() * 1024,
                virtual_memory_bytes: process.virtual_memory() * 1024,
                disk_read_bytes: process.disk_usage().read_bytes,
                disk_write_bytes: process.disk_usage().written_bytes,
            })
            .collect();

        RawSnapshot { host, processes }
    }
}

fn path_to_string(path: &std::path::Path) -> Option<String> {
    if path.as_os_str().is_empty() {
        None
    } else {
        Some(path.to_string_lossy().into_owned())
    }
}

pub fn index_processes(processes: &[RawProcessSample]) -> BTreeMap<u32, &RawProcessSample> {
    processes.iter().map(|process| (process.pid, process)).collect()
}
