use std::collections::{BTreeMap, HashMap};

use aetower_model::{
    BatteryHealthSnapshot, BluetoothDeviceBattery, BootSessionSnapshot, DiskHealthSnapshot,
    NetworkInterfaceSnapshot, ThermalState,
};
use serde::{Deserialize, Serialize};
use sysinfo::{Networks, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind, Users};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CollectorConfig {
    pub full_collection: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawProcessSample {
    pub pid: u32,
    pub parent_pid: Option<u32>,
    pub start_time_millis: u64,
    pub name: String,
    pub exe: Option<String>,
    pub cmd: Vec<String>,
    pub cpu_percent: f32,
    pub memory_bytes: u64,
    #[serde(default)]
    pub memory_physical_footprint_bytes: u64,
    pub disk_read_bytes: u64,
    pub disk_write_bytes: u64,
    #[serde(default)]
    pub wakeups_per_second: f32,
    /// Per-second energy draw in nanojoules (= nanowatts). Derived from
    /// the delta of `ri_billed_energy` between ticks. Zero when the
    /// process just spawned, the kernel does not support V4, or the
    /// platform is not macOS.
    #[serde(default)]
    pub energy_nj_per_s: f64,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub user: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RawHostSample {
    pub cpu_percent: f32,
    pub memory_used_bytes: u64,
    pub memory_total_bytes: u64,
    pub swap_used_bytes: u64,
    #[serde(default)]
    pub compressed_memory_bytes: u64,
    pub disk_read_bps: u64,
    pub disk_write_bps: u64,
    pub network_receive_bps: u64,
    pub network_send_bps: u64,
    #[serde(default)]
    pub wakeups_per_second: f32,
    pub thermal_state: ThermalState,
    pub on_battery: bool,
    pub battery_charge_percent: Option<u8>,
    pub low_power_mode: bool,
    #[serde(default)]
    pub battery_health: Option<BatteryHealthSnapshot>,
    #[serde(default)]
    pub boot_session: Option<BootSessionSnapshot>,
    #[serde(default)]
    pub network_interfaces: Vec<NetworkInterfaceSnapshot>,
    #[serde(default)]
    pub disks: Vec<DiskHealthSnapshot>,
    #[serde(default)]
    pub bluetooth_devices: Vec<BluetoothDeviceBattery>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RawSnapshot {
    pub host: RawHostSample,
    pub processes: Vec<RawProcessSample>,
    #[serde(default)]
    pub self_process: Option<RawProcessSample>,
    #[serde(default)]
    pub timings: CollectorTimings,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CollectorTimings {
    #[serde(default)]
    pub host_refresh_millis: f32,
    #[serde(default)]
    pub process_refresh_millis: f32,
    #[serde(default)]
    pub environment_refresh_millis: f32,
    #[serde(default)]
    pub process_sampling_millis: f32,
    #[serde(default)]
    pub post_process_millis: f32,
    #[serde(default)]
    pub storage_refresh_millis: f32,
    #[serde(default)]
    pub discovery_scan: bool,
    #[serde(default)]
    pub sampled_rusage: bool,
}

#[derive(Debug, Clone, Copy, Default)]
struct ProcessCounterSample {
    start_time_millis: u64,
    wakeups: u64,
    energy_nj: u64,
    physical_footprint_bytes: u64,
    disk_read_bytes: u64,
    disk_write_bytes: u64,
    wakeups_per_second: f32,
    energy_nj_per_s: f64,
}

#[derive(Debug, Clone)]
struct ProcessIdentitySample {
    start_time_millis: u64,
    name: String,
    exe: Option<String>,
    cmd: Vec<String>,
    user: Option<String>,
}

#[derive(Debug, Clone, Default)]
struct NetworkInterfaceIdentitySample {
    name: String,
    mac_address: String,
    is_up: bool,
}

const USER_DIRECTORY_INITIAL_REFRESH_TICKS: u8 = 10;
const USER_DIRECTORY_REFRESH_INTERVAL_TICKS: u8 = 120;
const PROCESS_DISCOVERY_INTERVAL_TICKS: u8 = 30;
const PROCESS_MEMORY_REFRESH_INTERVAL_TICKS: u8 = 5;
/// Fixed collector cadence in seconds — the engine drives `Collector::collect()`
/// on this interval, so deltas reported by `sysinfo` can be divided by it to
/// convert bytes-per-tick into bytes-per-second.
const TICK_SECONDS: f32 = 2.0;
/// Disk SMART data barely changes — `PERCENTAGE_USED` typically ticks up once
/// every few days on a normal workload, and `POWER_ON_HOURS` by one every
/// hour. Shelling out to `diskutil` is not free (spawns a subprocess, reads
/// plist, parses it), so refresh every ~2 minutes is plenty fresh for a
/// health dashboard without burning CPU on something the user cannot act on
/// in real time.
const DISK_REFRESH_INTERVAL_TICKS: u8 = 60;
/// Bluetooth battery percent changes on the order of hours under light use.
/// Refresh every ~30 seconds so the user sees "just connected" devices
/// populate quickly without hammering `ioreg`.
const BLUETOOTH_REFRESH_INTERVAL_TICKS: u8 = 15;
/// How often per-process wakeups/energy/physical-footprint are sampled via
/// `proc_pid_rusage`. Sampling every tick issues one syscall per known PID
/// every 2 s, which dominates collector CPU on hosts with many processes.
/// These counters change slowly and are not actionable in real time, so we
/// sample on the same ~10 s cadence as the memory refresh — the two already
/// share the `sample_rusage` branch below, so aligning them means rusage is
/// effectively free on the ticks in between. New PIDs are always sampled
/// immediately regardless of this interval.
const WAKEUPS_SAMPLE_INTERVAL_TICKS: u8 = 5;

pub struct Collector {
    config: CollectorConfig,
    system: System,
    networks: Networks,
    self_pid: u32,
    process_metadata_tick: u8,
    host_environment_refresh_tick: u8,
    cached_host_environment: HostEnvironment,
    users: Users,
    user_directory_refresh_tick: u8,
    previous_process_counters: HashMap<u32, ProcessCounterSample>,
    process_identity_cache: HashMap<u32, ProcessIdentitySample>,
    known_pids: Vec<sysinfo::Pid>,
    cwd_cache: HashMap<u32, String>,
    wakeups_sample_tick: u8,
    cached_network_interfaces: Vec<NetworkInterfaceIdentitySample>,
    cached_disks: Vec<DiskHealthSnapshot>,
    disk_refresh_tick: u8,
    cached_bluetooth_devices: Vec<BluetoothDeviceBattery>,
    bluetooth_refresh_tick: u8,
    /// True until the first `collect()` call completes. `sysinfo`'s
    /// `received()` / `transmitted()` on the first refresh after the
    /// `Collector` is constructed represents the delta between
    /// `Networks::new_with_refreshed_list()` and whatever time has
    /// elapsed before the engine calls `collect()` — which is neither
    /// a full tick nor a predictable interval, so dividing by
    /// `TICK_SECONDS` gives a misleading rate that is typically under-
    /// or over-reported by a factor of two or more. Seeding the per-
    /// interface rates to zero on the first tick ensures the UI never
    /// shows a misleading initial burst and only displays real rates
    /// once the tick cadence has stabilised.
    first_network_tick: bool,
}

impl Collector {
    pub fn new() -> Self {
        let mut system = System::new();
        system.refresh_all();
        let mut networks = Networks::new_with_refreshed_list();
        networks.refresh(true);
        Self {
            config: CollectorConfig::default(),
            system,
            networks,
            self_pid: std::process::id(),
            process_metadata_tick: 0,
            host_environment_refresh_tick: 0,
            cached_host_environment: HostEnvironment::default(),
            users: Users::new(),
            user_directory_refresh_tick: 0,
            previous_process_counters: HashMap::new(),
            process_identity_cache: HashMap::new(),
            known_pids: Vec::new(),
            cwd_cache: HashMap::new(),
            wakeups_sample_tick: 0,
            cached_network_interfaces: Vec::new(),
            cached_disks: Vec::new(),
            disk_refresh_tick: 0,
            cached_bluetooth_devices: Vec::new(),
            bluetooth_refresh_tick: 0,
            first_network_tick: true,
        }
    }

    pub fn configure(&mut self, config: CollectorConfig) {
        self.config = config;
    }

    pub fn collect(&mut self) -> RawSnapshot {
        let host_refresh_started = std::time::Instant::now();
        self.system.refresh_cpu_all();
        self.system.refresh_memory();
        self.networks.refresh(true);
        let host_refresh_millis = host_refresh_started.elapsed().as_secs_f64() * 1000.0;
        // Separate process discovery from memory refresh. Enumerating the full
        // process table is the expensive step, so do it on a much slower
        // cadence while still refreshing memory on known PIDs regularly.
        let discovery_scan = self.config.full_collection
            || self.known_pids.is_empty()
            || self
                .process_metadata_tick
                .is_multiple_of(PROCESS_DISCOVERY_INTERVAL_TICKS);
        let refresh_memory = self.config.full_collection
            || discovery_scan
            || self
                .process_metadata_tick
                .is_multiple_of(PROCESS_MEMORY_REFRESH_INTERVAL_TICKS);
        let process_refresh_started = std::time::Instant::now();
        if discovery_scan {
            self.system.refresh_processes_specifics(
                ProcessesToUpdate::All,
                true,
                process_refresh_kind(true, true),
            );
            self.known_pids = self.system.processes().keys().copied().collect();
        } else {
            self.system.refresh_processes_specifics(
                ProcessesToUpdate::Some(&self.known_pids),
                false,
                process_refresh_kind(false, refresh_memory),
            );
        }
        let process_refresh_millis = process_refresh_started.elapsed().as_secs_f64() * 1000.0;
        self.process_metadata_tick = self.process_metadata_tick.wrapping_add(1);
        self.user_directory_refresh_tick = self.user_directory_refresh_tick.wrapping_add(1);
        let refresh_host_environment = self.host_environment_refresh_tick == 0;
        let environment_refresh_started = std::time::Instant::now();
        if refresh_host_environment {
            self.cached_host_environment = read_environment();
        }
        self.host_environment_refresh_tick = (self.host_environment_refresh_tick + 1) % 5;

        // Per-interface metadata is currently not surfaced in the hot UI path,
        // so keep it on the low-frequency host-environment cadence instead of
        // rebuilding MAC/IP-derived rows every collection tick.
        if self.config.full_collection || refresh_host_environment {
            self.cached_network_interfaces = collect_network_interface_metadata(&self.networks);
        }
        let environment_refresh_millis =
            environment_refresh_started.elapsed().as_secs_f64() * 1000.0;

        // Disk SMART data is sampled on an even slower cadence: shelling out
        // to `diskutil` is the main cost, and the values change over hours
        // not seconds. Tick 0 seeds the cache; subsequent ticks rotate the
        // counter and refresh only when it lands back on 0.
        let refresh_disks = self.cached_disks.is_empty()
            || self.config.full_collection
            || self.disk_refresh_tick == 0;
        if refresh_disks {
            self.cached_disks = platform::sample_disks();
        }
        self.disk_refresh_tick = (self.disk_refresh_tick + 1) % DISK_REFRESH_INTERVAL_TICKS;

        // Bluetooth peripheral battery is on its own cadence — fast enough
        // that "just connected" devices show up within half a minute, slow
        // enough that we're not invoking `ioreg` every tick.
        let storage_refresh_started = std::time::Instant::now();
        let refresh_bluetooth = self.cached_bluetooth_devices.is_empty()
            || self.config.full_collection
            || self.bluetooth_refresh_tick == 0;
        if refresh_bluetooth {
            self.cached_bluetooth_devices = platform::sample_bluetooth_devices();
        }
        self.bluetooth_refresh_tick =
            (self.bluetooth_refresh_tick + 1) % BLUETOOTH_REFRESH_INTERVAL_TICKS;
        let storage_refresh_millis = storage_refresh_started.elapsed().as_secs_f64() * 1000.0;

        let network_interfaces = build_network_interface_snapshots(
            &self.networks,
            &self.cached_network_interfaces,
            self.first_network_tick,
        );
        let network_first_tick = self.first_network_tick;
        self.first_network_tick = false;

        let metadata_refresh =
            self.config.full_collection || self.process_metadata_tick == 1 || discovery_scan;
        let sample_wakeups = self.config.full_collection
            || self
                .wakeups_sample_tick
                .is_multiple_of(WAKEUPS_SAMPLE_INTERVAL_TICKS);
        self.wakeups_sample_tick = self.wakeups_sample_tick.wrapping_add(1);
        let mut user_directory_refreshed = self.should_refresh_user_directory(discovery_scan);
        if user_directory_refreshed {
            self.users.refresh();
        }

        let process_sampling_started = std::time::Instant::now();
        let mut next_process_counters = HashMap::with_capacity(self.system.processes().len());
        let all_processes: Vec<_> = self
            .system
            .processes()
            .values()
            .map(|process| {
                let pid = process.pid().as_u32();
                let start_time_millis = process.start_time().saturating_mul(1_000);
                let previous = self
                    .previous_process_counters
                    .get(&pid)
                    .filter(|prev| prev.start_time_millis == start_time_millis);
                // Wakeups and energy come from the same syscall
                // (`proc_pid_rusage`). We sample them on the same slow cadence
                // (every `WAKEUPS_SAMPLE_INTERVAL_TICKS` ticks, ~10 s) to avoid
                // a per-process syscall on every tick, but always read the
                // counters for new PIDs so they show data immediately.
                let sample_rusage = sample_wakeups || refresh_memory || previous.is_none();
                let (wakeups, energy_nj, physical_footprint_bytes) = if sample_rusage {
                    platform::process_counters(pid)
                        .map(|counters| {
                            (
                                counters.wakeups,
                                counters.energy_nj,
                                counters.physical_footprint_bytes,
                            )
                        })
                        .unwrap_or_else(|| {
                            let prev_wakeups = previous.map(|prev| prev.wakeups).unwrap_or(0);
                            let prev_energy = previous.map(|prev| prev.energy_nj).unwrap_or(0);
                            let prev_footprint = previous
                                .map(|prev| prev.physical_footprint_bytes)
                                .unwrap_or(0);
                            (prev_wakeups, prev_energy, prev_footprint)
                        })
                } else {
                    (
                        previous.map(|prev| prev.wakeups).unwrap_or(0),
                        previous.map(|prev| prev.energy_nj).unwrap_or(0),
                        previous
                            .map(|prev| prev.physical_footprint_bytes)
                            .unwrap_or(0),
                    )
                };
                // Use the cumulative byte counters; subtracting the previous
                // tick's cumulative value below yields a true per-tick delta.
                // (`read_bytes` is itself only the delta since the last refresh,
                // so differencing it double-counted and collapsed steady I/O to ~0.)
                let disk_read_total = process.disk_usage().total_read_bytes;
                let disk_write_total = process.disk_usage().total_written_bytes;

                let tick_seconds = TICK_SECONDS;
                let wakeups_per_second = previous
                    .map(|prev| {
                        if sample_wakeups || prev.wakeups == 0 {
                            wakeups.saturating_sub(prev.wakeups) as f32 / tick_seconds
                        } else {
                            prev.wakeups_per_second
                        }
                    })
                    .unwrap_or(0.0);
                let energy_nj_per_s = previous
                    .map(|prev| {
                        if sample_wakeups || prev.energy_nj == 0 {
                            energy_nj.saturating_sub(prev.energy_nj) as f64 / tick_seconds as f64
                        } else {
                            prev.energy_nj_per_s
                        }
                    })
                    .unwrap_or(0.0);
                let disk_read_delta = previous
                    .map(|prev| disk_read_total.saturating_sub(prev.disk_read_bytes))
                    .unwrap_or(0);
                let disk_write_delta = previous
                    .map(|prev| disk_write_total.saturating_sub(prev.disk_write_bytes))
                    .unwrap_or(0);

                next_process_counters.insert(
                    pid,
                    ProcessCounterSample {
                        start_time_millis,
                        wakeups,
                        energy_nj,
                        physical_footprint_bytes,
                        disk_read_bytes: disk_read_total,
                        disk_write_bytes: disk_write_total,
                        wakeups_per_second,
                        energy_nj_per_s,
                    },
                );

                let identity = match self.process_identity_cache.get(&pid) {
                    Some(cached)
                        if cached.start_time_millis == start_time_millis && !metadata_refresh =>
                    {
                        cached.clone()
                    }
                    _ => {
                        let identity = ProcessIdentitySample {
                            start_time_millis,
                            name: process.name().to_string_lossy().into_owned(),
                            exe: process.exe().and_then(path_to_string),
                            cmd: process
                                .cmd()
                                .iter()
                                .map(|segment| segment.to_string_lossy().into_owned())
                                .collect(),
                            user: process.user_id().and_then(|uid| {
                                if let Some(user) = self.users.get_user_by_id(uid) {
                                    return Some(user.name().to_owned());
                                }
                                if !user_directory_refreshed {
                                    self.users.refresh();
                                    user_directory_refreshed = true;
                                    return self
                                        .users
                                        .get_user_by_id(uid)
                                        .map(|u| u.name().to_owned());
                                }
                                None
                            }),
                        };
                        self.process_identity_cache.insert(pid, identity.clone());
                        identity
                    }
                };

                RawProcessSample {
                    pid,
                    parent_pid: process.parent().map(|parent| parent.as_u32()),
                    start_time_millis,
                    name: identity.name,
                    exe: identity.exe,
                    cmd: identity.cmd,
                    cpu_percent: process.cpu_usage(),
                    memory_bytes: process.memory(),
                    memory_physical_footprint_bytes: physical_footprint_bytes,
                    disk_read_bytes: disk_read_delta,
                    disk_write_bytes: disk_write_delta,
                    wakeups_per_second,
                    energy_nj_per_s,
                    cwd: if self.config.full_collection
                        || self.process_metadata_tick.is_multiple_of(2)
                    {
                        // Only probe new PIDs; return cached cwd for known ones.
                        let is_new = self
                            .previous_process_counters
                            .get(&pid)
                            .is_none_or(|prev| prev.start_time_millis != start_time_millis);
                        if is_new {
                            platform::process_cwd(pid)
                        } else {
                            self.cwd_cache.get(&pid).cloned()
                        }
                    } else {
                        self.cwd_cache.get(&pid).cloned()
                    },
                    user: identity.user,
                }
            })
            .collect();
        let process_sampling_millis = process_sampling_started.elapsed().as_secs_f64() * 1000.0;
        let self_process = all_processes
            .iter()
            .find(|process| process.pid == self.self_pid)
            .cloned();
        let processes = all_processes
            .iter()
            .filter(|process| process.pid != self.self_pid)
            .cloned()
            .collect::<Vec<_>>();
        self.previous_process_counters = next_process_counters;

        // Update CWD cache: insert fresh values, prune dead PIDs.
        if self.config.full_collection || self.process_metadata_tick.is_multiple_of(2) {
            let alive: std::collections::HashSet<u32> =
                all_processes.iter().map(|p| p.pid).collect();
            self.cwd_cache.retain(|pid, _| alive.contains(pid));
            self.process_identity_cache.retain(|pid, cached| {
                alive.contains(pid)
                    && all_processes
                        .iter()
                        .any(|p| p.pid == *pid && p.start_time_millis == cached.start_time_millis)
            });
            for process in &all_processes {
                if let Some(ref cwd) = process.cwd {
                    self.cwd_cache.insert(process.pid, cwd.clone());
                }
            }
        }

        let post_process_started = std::time::Instant::now();
        // Per-process `disk_read_bytes` is the bytes moved during the last tick;
        // divide the host total by the tick length to report a per-second rate.
        let host_disk_read_bytes = processes.iter().fold(0u64, |total, process| {
            total.saturating_add(process.disk_read_bytes)
        });
        let host_disk_write_bytes = processes.iter().fold(0u64, |total, process| {
            total.saturating_add(process.disk_write_bytes)
        });
        let host_disk_read_bps = (host_disk_read_bytes as f64 / TICK_SECONDS as f64) as u64;
        let host_disk_write_bps = (host_disk_write_bytes as f64 / TICK_SECONDS as f64) as u64;
        // Host network throughput: sum the per-interface byte deltas straight from
        // `sysinfo` (each is bytes-since-last-refresh = one tick) and convert to
        // bytes/sec. Computed directly from `self.networks` rather than from the
        // per-interface snapshots, which stay empty until interface metadata is
        // first cached. Zero on the first tick, where the elapsed interval is unknown.
        let (host_network_receive_bps, host_network_send_bps): (u64, u64) = if network_first_tick {
            (0, 0)
        } else {
            let received: u64 = self.networks.values().map(|data| data.received()).sum();
            let transmitted: u64 = self.networks.values().map(|data| data.transmitted()).sum();
            (
                (received as f64 / TICK_SECONDS as f64) as u64,
                (transmitted as f64 / TICK_SECONDS as f64) as u64,
            )
        };

        let compressed_memory_bytes = platform::compressed_memory_bytes().unwrap_or(0);
        let host_wakeups_per_second = processes
            .iter()
            .fold(0.0f32, |total, process| total + process.wakeups_per_second);

        let host = RawHostSample {
            cpu_percent: self.system.global_cpu_usage(),
            memory_used_bytes: self.system.used_memory(),
            memory_total_bytes: self.system.total_memory(),
            swap_used_bytes: self.system.used_swap(),
            compressed_memory_bytes,
            disk_read_bps: host_disk_read_bps,
            disk_write_bps: host_disk_write_bps,
            network_receive_bps: host_network_receive_bps,
            network_send_bps: host_network_send_bps,
            wakeups_per_second: host_wakeups_per_second,
            thermal_state: self.cached_host_environment.thermal_state,
            on_battery: self.cached_host_environment.on_battery,
            battery_charge_percent: self.cached_host_environment.battery_charge_percent,
            low_power_mode: self.cached_host_environment.low_power_mode,
            battery_health: self.cached_host_environment.battery_health.clone(),
            boot_session: self.cached_host_environment.boot_session.clone(),
            network_interfaces,
            disks: self.cached_disks.clone(),
            bluetooth_devices: self.cached_bluetooth_devices.clone(),
        };
        let post_process_millis = post_process_started.elapsed().as_secs_f64() * 1000.0;

        RawSnapshot {
            host,
            processes,
            self_process,
            timings: CollectorTimings {
                host_refresh_millis: host_refresh_millis as f32,
                process_refresh_millis: process_refresh_millis as f32,
                environment_refresh_millis: environment_refresh_millis as f32,
                process_sampling_millis: process_sampling_millis as f32,
                post_process_millis: post_process_millis as f32,
                storage_refresh_millis: storage_refresh_millis as f32,
                discovery_scan,
                sampled_rusage: sample_wakeups || refresh_memory,
            },
        }
    }
}

fn collect_network_interface_metadata(networks: &Networks) -> Vec<NetworkInterfaceIdentitySample> {
    let mut identities = Vec::with_capacity(networks.iter().size_hint().0);
    for (name, data) in networks {
        let mac = data.mac_address();
        identities.push(NetworkInterfaceIdentitySample {
            name: name.clone(),
            mac_address: if mac.is_unspecified() {
                String::new()
            } else {
                mac.to_string()
            },
            is_up: !data.ip_networks().is_empty(),
        });
    }
    identities.sort_by(|left, right| left.name.cmp(&right.name));
    identities
}

fn build_network_interface_snapshots(
    networks: &Networks,
    cached_metadata: &[NetworkInterfaceIdentitySample],
    first_tick: bool,
) -> Vec<NetworkInterfaceSnapshot> {
    if cached_metadata.is_empty() {
        return Vec::new();
    }

    // On the first collector tick, `sysinfo`'s `received()` /
    // `transmitted()` return the delta between `Networks::new_with_
    // refreshed_list()` and whatever unknown interval has elapsed
    // before `collect()` was called. Dividing that by `TICK_SECONDS`
    // produces an arbitrary rate that is typically wrong by a factor
    // of two or more. Zero all rates on the first tick so the UI does
    // not flash a misleading initial burst; subsequent ticks land at
    // the steady 2 s cadence and the rate conversion is accurate.
    let throughput_by_name: HashMap<&str, (u64, u64)> = if first_tick {
        HashMap::new()
    } else {
        networks
            .iter()
            .map(|(name, data)| {
                let receive_bps = (data.received() as f64 / TICK_SECONDS as f64) as u64;
                let send_bps = (data.transmitted() as f64 / TICK_SECONDS as f64) as u64;
                (name.as_str(), (receive_bps, send_bps))
            })
            .collect()
    };

    cached_metadata
        .iter()
        .map(|metadata| {
            let (receive_bps, send_bps) = throughput_by_name
                .get(metadata.name.as_str())
                .copied()
                .unwrap_or((0, 0));
            NetworkInterfaceSnapshot {
                name: metadata.name.clone(),
                mac_address: metadata.mac_address.clone(),
                receive_bps,
                send_bps,
                is_up: metadata.is_up,
            }
        })
        .collect()
}

impl Default for Collector {
    fn default() -> Self {
        Self::new()
    }
}

impl Collector {
    fn should_refresh_user_directory(&self, full_scan: bool) -> bool {
        if self.config.full_collection {
            return full_scan;
        }
        if !full_scan {
            return false;
        }

        if self.users.list().is_empty() {
            return self.user_directory_refresh_tick >= USER_DIRECTORY_INITIAL_REFRESH_TICKS;
        }

        self.user_directory_refresh_tick
            .is_multiple_of(USER_DIRECTORY_REFRESH_INTERVAL_TICKS)
    }
}

fn process_refresh_kind(discovery_scan: bool, refresh_memory: bool) -> ProcessRefreshKind {
    let mut kind = ProcessRefreshKind::nothing().with_cpu().with_disk_usage();
    if refresh_memory {
        kind = kind.with_memory();
    }
    if discovery_scan {
        kind = kind
            .with_user(UpdateKind::OnlyIfNotSet)
            .with_cmd(UpdateKind::OnlyIfNotSet)
            .with_exe(UpdateKind::OnlyIfNotSet);
    }
    kind
}

fn path_to_string(path: &std::path::Path) -> Option<String> {
    if path.as_os_str().is_empty() {
        None
    } else {
        Some(path.to_string_lossy().into_owned())
    }
}

pub fn index_processes(processes: &[RawProcessSample]) -> BTreeMap<u32, &RawProcessSample> {
    processes
        .iter()
        .map(|process| (process.pid, process))
        .collect()
}

#[derive(Debug, Clone, Default)]
pub struct HostEnvironment {
    pub thermal_state: ThermalState,
    pub on_battery: bool,
    pub battery_charge_percent: Option<u8>,
    pub low_power_mode: bool,
    pub battery_health: Option<BatteryHealthSnapshot>,
    pub boot_session: Option<BootSessionSnapshot>,
}

pub fn read_environment() -> HostEnvironment {
    platform::read_environment()
}

#[cfg(target_os = "macos")]
mod platform {
    use aetower_model::{
        BatteryCondition, BatteryHealthSnapshot, BluetoothDeviceBattery, BootSessionSnapshot,
        DiskHealthSnapshot, DiskHealthStatus, ThermalState,
    };
    use std::{
        ffi::{CStr, c_char, c_void},
        mem, ptr,
        time::SystemTime,
    };

    use core_foundation_sys::{
        array::{CFArrayGetCount, CFArrayGetValueAtIndex, CFArrayRef},
        base::{CFRelease, CFTypeRef, kCFAllocatorDefault},
        dictionary::{CFDictionaryGetValueIfPresent, CFDictionaryRef},
        number::{
            CFBooleanGetValue, CFBooleanRef, CFNumberGetValue, CFNumberRef, kCFNumberSInt32Type,
        },
        string::{
            CFStringCreateWithCString, CFStringGetCString, CFStringRef, kCFStringEncodingUTF8,
        },
    };

    use super::HostEnvironment;

    const K_IO_PM_AC_POWER_KEY: &str = "AC Power";
    const K_IOPS_CURRENT_CAPACITY_KEY: &str = "Current Capacity";
    const K_IOPS_MAX_CAPACITY_KEY: &str = "Max Capacity";
    const K_IOPS_IS_PRESENT_KEY: &str = "Is Present";
    // Extended battery-health keys exposed by IOPSGetPowerSourceDescription.
    // These are the same keys Apple's System Information reads to render the
    // Battery pane, so they're reasonably stable across macOS versions.
    const K_IOPS_CYCLE_COUNT_KEY: &str = "Cycle Count";
    const K_IOPS_DESIGN_CAPACITY_KEY: &str = "DesignCapacity";
    const K_IOPS_BATTERY_HEALTH_KEY: &str = "BatteryHealth";
    const K_IOPS_TEMPERATURE_KEY: &str = "Temperature";

    #[link(name = "Foundation", kind = "framework")]
    unsafe extern "C" {}

    #[link(name = "IOKit", kind = "framework")]
    unsafe extern "C" {
        fn IOPSCopyPowerSourcesInfo() -> CFTypeRef;
        fn IOPSCopyPowerSourcesList(blob: CFTypeRef) -> CFArrayRef;
        fn IOPSGetPowerSourceDescription(blob: CFTypeRef, ps: CFTypeRef) -> CFDictionaryRef;
        fn IOPSGetProvidingPowerSourceType(snapshot: CFTypeRef) -> CFStringRef;
        fn mach_host_self() -> u32;
        fn host_page_size(host: u32, page_size: *mut u32) -> i32;
        fn host_statistics64(
            host: u32,
            flavor: i32,
            host_info_out: *mut i32,
            host_info_out_cnt: *mut u32,
        ) -> i32;
        fn sysctlbyname(
            name: *const c_char,
            oldp: *mut c_void,
            oldlenp: *mut usize,
            newp: *mut c_void,
            newlen: usize,
        ) -> i32;
        fn proc_pid_rusage(pid: i32, flavor: i32, buffer: *mut c_void) -> i32;
        fn proc_pidinfo(
            pid: i32,
            flavor: i32,
            arg: u64,
            buffer: *mut c_void,
            buffersize: i32,
        ) -> i32;
    }

    #[link(name = "objc")]
    unsafe extern "C" {
        fn objc_getClass(name: *const c_char) -> *mut c_void;
        fn sel_registerName(name: *const c_char) -> *mut c_void;
        fn objc_msgSend();
    }

    pub fn read_environment() -> HostEnvironment {
        let thermal_state = thermal_state();
        let low_power_mode = low_power_mode_enabled();
        let power = power_state();
        let boot_session = boot_session();

        HostEnvironment {
            thermal_state,
            on_battery: power.on_battery,
            battery_charge_percent: power.charge_percent,
            low_power_mode,
            battery_health: power.health,
            boot_session,
        }
    }

    fn boot_session() -> Option<BootSessionSnapshot> {
        let boot_id = sysctl_string("kern.bootsessionuuid");
        let boot_time_millis = sysctl_timeval_millis("kern.boottime");
        let host_uptime_millis = boot_time_millis.and_then(|boot_time| {
            let now = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .ok()?;
            let now_millis = now.as_millis().min(u128::from(u64::MAX)) as u64;
            Some(now_millis.saturating_sub(boot_time))
        });
        if boot_id.is_none() && boot_time_millis.is_none() && host_uptime_millis.is_none() {
            return None;
        }

        Some(BootSessionSnapshot {
            boot_id,
            boot_time_millis,
            host_uptime_millis,
            previous_shutdown: None,
        })
    }

    fn sysctl_string(name: &str) -> Option<String> {
        let name = std::ffi::CString::new(name).ok()?;
        unsafe {
            let mut size = 0usize;
            if sysctlbyname(
                name.as_ptr(),
                ptr::null_mut(),
                &mut size as *mut usize,
                ptr::null_mut(),
                0,
            ) != 0
                || size == 0
            {
                return None;
            }
            let mut buffer = vec![0u8; size];
            if sysctlbyname(
                name.as_ptr(),
                buffer.as_mut_ptr().cast::<c_void>(),
                &mut size as *mut usize,
                ptr::null_mut(),
                0,
            ) != 0
            {
                return None;
            }
            let end = buffer
                .iter()
                .position(|byte| *byte == 0)
                .unwrap_or(buffer.len());
            String::from_utf8(buffer[..end].to_vec())
                .ok()
                .filter(|value| !value.is_empty())
        }
    }

    #[repr(C)]
    struct Timeval {
        tv_sec: i64,
        tv_usec: i32,
    }

    fn sysctl_timeval_millis(name: &str) -> Option<u64> {
        let name = std::ffi::CString::new(name).ok()?;
        unsafe {
            let mut value = Timeval {
                tv_sec: 0,
                tv_usec: 0,
            };
            let mut size = mem::size_of::<Timeval>();
            if sysctlbyname(
                name.as_ptr(),
                &mut value as *mut Timeval as *mut c_void,
                &mut size as *mut usize,
                ptr::null_mut(),
                0,
            ) != 0
            {
                return None;
            }
            let seconds = u64::try_from(value.tv_sec).ok()?;
            let micros = u64::try_from(value.tv_usec).ok()?;
            Some(seconds.saturating_mul(1_000).saturating_add(micros / 1_000))
        }
    }

    /// Composite result of one IOPS power-sources read. Split out so callers
    /// can treat on/off-battery, current charge, and long-lived health
    /// metrics independently.
    struct PowerState {
        on_battery: bool,
        charge_percent: Option<u8>,
        health: Option<BatteryHealthSnapshot>,
    }

    /// Counters extracted from a single `proc_pid_rusage` call.
    ///
    /// Both fields come from the same syscall buffer — the energy
    /// counter is "free" once we upgrade V2 → V4 because the kernel
    /// fills the entire struct in one shot.
    pub struct ProcessRusageCounters {
        pub wakeups: u64,
        pub energy_nj: u64,
        pub physical_footprint_bytes: u64,
    }

    /// Read wakeup count and cumulative energy (nanojoules) for a
    /// process in a single `proc_pid_rusage` call.
    ///
    /// `ri_billed_energy` is the XNU scheduler's per-task energy
    /// counter: cumulative nanojoules of CPU + GPU work billed to this
    /// task since process start. It is the same data source Activity
    /// Monitor's "Energy Impact" column draws from.
    pub fn process_counters(pid: u32) -> Option<ProcessRusageCounters> {
        let mut info = RUsageInfoV4::default();
        let result = unsafe {
            proc_pid_rusage(
                pid as i32,
                RUSAGE_INFO_V4,
                &mut info as *mut RUsageInfoV4 as *mut c_void,
            )
        };
        (result == 0).then_some(ProcessRusageCounters {
            wakeups: info
                .ri_interrupt_wkups
                .saturating_add(info.ri_pkg_idle_wkups),
            energy_nj: info.ri_billed_energy,
            physical_footprint_bytes: info.ri_phys_footprint,
        })
    }

    const PROC_PIDVNODEPATHINFO: i32 = 9;
    const MAXPATHLEN: usize = 1024;

    #[repr(C)]
    struct VnodeInfoPath {
        _vip_vi: [u8; 152],
        vip_path: [u8; MAXPATHLEN],
    }

    #[repr(C)]
    struct ProcVnodePathInfo {
        pvi_cdir: VnodeInfoPath,
        _pvi_rdir: VnodeInfoPath,
    }

    pub fn process_cwd(pid: u32) -> Option<String> {
        unsafe {
            let mut info: ProcVnodePathInfo = mem::zeroed();
            let size = mem::size_of::<ProcVnodePathInfo>() as i32;
            let result = proc_pidinfo(
                pid as i32,
                PROC_PIDVNODEPATHINFO,
                0,
                &mut info as *mut _ as *mut c_void,
                size,
            );
            if result <= 0 {
                return None;
            }
            let cstr = CStr::from_ptr(info.pvi_cdir.vip_path.as_ptr() as *const c_char);
            let path = cstr.to_str().ok()?.to_owned();
            if path.is_empty() { None } else { Some(path) }
        }
    }

    pub fn compressed_memory_bytes() -> Option<u64> {
        unsafe {
            let host = mach_host_self();
            let mut page_size = 0u32;
            if host_page_size(host, &mut page_size as *mut u32) != KERN_SUCCESS {
                return None;
            }

            let mut stats = VmStatistics64::default();
            let mut count = (mem::size_of::<VmStatistics64>() / mem::size_of::<i32>()) as u32;
            let result = host_statistics64(
                host,
                HOST_VM_INFO64,
                &mut stats as *mut VmStatistics64 as *mut i32,
                &mut count as *mut u32,
            );
            (result == KERN_SUCCESS)
                .then_some((stats.compressor_page_count as u64).saturating_mul(page_size as u64))
        }
    }

    const HOST_VM_INFO64: i32 = 4;
    const KERN_SUCCESS: i32 = 0;
    const RUSAGE_INFO_V4: i32 = 4;

    /// macOS `rusage_info_v4` — a strict superset of v2 that adds QoS
    /// time buckets, instruction/cycle counters, and (critically for us)
    /// `ri_billed_energy` / `ri_serviced_energy`. The field layout
    /// matches `/usr/include/sys/resource.h` on macOS 12+.
    ///
    /// We upgrade from v2 → v4 so we can read the energy counter in the
    /// same syscall that already fetches wakeup counts. The kernel fills
    /// the entire struct in one shot regardless of which fields the caller
    /// actually inspects, so the marginal cost of reading 37 fields
    /// instead of 19 is the buffer allocation (still stack, still tiny).
    #[repr(C)]
    #[derive(Default)]
    struct RUsageInfoV4 {
        // ── v0 fields ──
        ri_uuid: [u8; 16],
        ri_user_time: u64,
        ri_system_time: u64,
        ri_pkg_idle_wkups: u64,
        ri_interrupt_wkups: u64,
        ri_pageins: u64,
        ri_wired_size: u64,
        ri_resident_size: u64,
        ri_phys_footprint: u64,
        ri_proc_start_abstime: u64,
        ri_proc_exit_abstime: u64,
        // ── v1 fields (child counters) ──
        ri_child_user_time: u64,
        ri_child_system_time: u64,
        ri_child_pkg_idle_wkups: u64,
        ri_child_interrupt_wkups: u64,
        ri_child_pageins: u64,
        ri_child_elapsed_abstime: u64,
        // ── v2 fields (disk I/O) ──
        ri_diskio_bytesread: u64,
        ri_diskio_byteswritten: u64,
        // ── v3 fields (QoS time buckets) ──
        ri_cpu_time_qos_default: u64,
        ri_cpu_time_qos_maintenance: u64,
        ri_cpu_time_qos_background: u64,
        ri_cpu_time_qos_utility: u64,
        ri_cpu_time_qos_legacy: u64,
        ri_cpu_time_qos_user_initiated: u64,
        ri_cpu_time_qos_user_interactive: u64,
        ri_billed_system_time: u64,
        ri_serviced_system_time: u64,
        // ── v4 fields (counters + energy) ──
        ri_logical_writes: u64,
        ri_lifetime_max_phys_footprint: u64,
        ri_instructions: u64,
        ri_cycles: u64,
        /// Cumulative nanojoules of CPU+GPU energy billed to this task.
        ri_billed_energy: u64,
        ri_serviced_energy: u64,
        ri_interval_max_phys_footprint: u64,
        ri_runnable_time: u64,
    }

    #[repr(C)]
    #[derive(Default)]
    struct VmStatistics64 {
        free_count: u32,
        active_count: u32,
        inactive_count: u32,
        wire_count: u32,
        zero_fill_count: u64,
        reactivations: u64,
        pageins: u64,
        pageouts: u64,
        faults: u64,
        cow_faults: u64,
        lookups: u64,
        hits: u64,
        purges: u64,
        purgeable_count: u32,
        speculative_count: u32,
        decompressions: u64,
        compressions: u64,
        swapins: u64,
        swapouts: u64,
        compressor_page_count: u32,
        throttled_count: u32,
        external_page_count: u32,
        internal_page_count: u32,
        total_uncompressed_pages_in_compressor: u64,
    }

    fn thermal_state() -> ThermalState {
        match unsafe { ns_process_info_thermal_state() } {
            1 => ThermalState::Fair,
            2 => ThermalState::Serious,
            3 => ThermalState::Critical,
            _ => ThermalState::Nominal,
        }
    }

    fn low_power_mode_enabled() -> bool {
        unsafe { ns_process_info_low_power_mode_enabled() }
    }

    /// Read one pass of power-source info from IOKit.
    ///
    /// `IOPSCopyPowerSourcesInfo` / `IOPSGetPowerSourceDescription` return one
    /// CF dictionary per attached source. On laptops the internal battery is
    /// always the first source; on iMacs/Mac minis there is no source at all
    /// (`on_battery=false`, all optional fields `None`).
    ///
    /// This function reads the extended health keys Apple documents for the
    /// same dictionary: `Cycle Count`, `DesignCapacity`, `BatteryHealth`, and
    /// `Temperature`. `Max Capacity` is treated as the current full-charge
    /// value for computing a health ratio against `DesignCapacity`.
    ///
    /// `Temperature` is reported in centi-degrees Kelvin (e.g. `3013` = 28.15°C);
    /// values outside `-40..=100°C` are dropped as obvious garbage.
    #[allow(clippy::collapsible_if)]
    fn power_state() -> PowerState {
        unsafe {
            let snapshot = IOPSCopyPowerSourcesInfo();
            if snapshot.is_null() {
                return PowerState {
                    on_battery: false,
                    charge_percent: None,
                    health: None,
                };
            }

            let providing_type = cfstring_to_rust_string(IOPSGetProvidingPowerSourceType(snapshot));
            let on_battery = providing_type.as_deref() != Some(K_IO_PM_AC_POWER_KEY);
            let mut charge_percent = None;
            let mut health: Option<BatteryHealthSnapshot> = None;

            let list = IOPSCopyPowerSourcesList(snapshot);
            if !list.is_null() {
                let count = CFArrayGetCount(list);
                for index in 0..count {
                    let source = CFArrayGetValueAtIndex(list, index);
                    let description = IOPSGetPowerSourceDescription(snapshot, source);
                    if description.is_null()
                        || !cf_dictionary_bool(description, K_IOPS_IS_PRESENT_KEY)
                    {
                        continue;
                    }

                    let current_capacity =
                        cf_dictionary_i32(description, K_IOPS_CURRENT_CAPACITY_KEY);
                    let max_capacity = cf_dictionary_i32(description, K_IOPS_MAX_CAPACITY_KEY);
                    if let (Some(current_capacity), Some(max_capacity)) =
                        (current_capacity, max_capacity)
                        && max_capacity > 0
                    {
                        let percent = ((current_capacity as f32 / max_capacity as f32) * 100.0)
                            .round()
                            .clamp(0.0, 100.0) as u8;
                        charge_percent = Some(percent);
                    }

                    // Extended health fields — each is optional because older
                    // Macs or non-Apple batteries may not expose them all.
                    //
                    // `None` and `Some(0)` are deliberately distinct here.
                    // `None` means the IOPS dictionary did not provide the
                    // key on this tick (could be a Mac mini, an older
                    // macOS, a third-party USB-C battery, etc.). `Some(0)`
                    // would only arise from a genuinely-zero reading and
                    // we do not currently have a code path that produces
                    // one — but the type distinction prevents downstream
                    // code from confusing "brand-new battery" with
                    // "sensor not reporting".
                    let cycle_count = cf_dictionary_i32(description, K_IOPS_CYCLE_COUNT_KEY)
                        .and_then(|value| u32::try_from(value).ok());
                    let design_capacity =
                        cf_dictionary_i32(description, K_IOPS_DESIGN_CAPACITY_KEY)
                            .and_then(|value| u32::try_from(value).ok())
                            .filter(|value| *value > 0);
                    let max_capacity_mah = max_capacity
                        .and_then(|value| u32::try_from(value).ok())
                        .filter(|value| *value > 0);
                    let health_percent = match (design_capacity, max_capacity_mah) {
                        (Some(design), Some(current)) if design > 0 => {
                            Some((current as f32 / design as f32 * 100.0).clamp(0.0, 100.0))
                        }
                        _ => None,
                    };
                    let condition = cf_dictionary_string(description, K_IOPS_BATTERY_HEALTH_KEY)
                        .map(|value| match value.as_str() {
                            "Good" => BatteryCondition::Good,
                            "Fair" => BatteryCondition::Fair,
                            "Poor" => BatteryCondition::Poor,
                            "Check Battery" | "Service Battery" | "Replace Soon"
                            | "Replace Now" => BatteryCondition::ServiceBattery,
                            _ => BatteryCondition::Unknown,
                        })
                        .unwrap_or(BatteryCondition::Unknown);
                    let temperature_celsius =
                        cf_dictionary_i32(description, K_IOPS_TEMPERATURE_KEY).and_then(
                            |centi_kelvin| {
                                let celsius = (centi_kelvin as f32 / 100.0) - 273.15;
                                (-40.0..=100.0).contains(&celsius).then_some(celsius)
                            },
                        );

                    // Only emit a BatteryHealthSnapshot when we actually
                    // have at least one meaningful field — otherwise a
                    // desktop Mac with no battery would still surface an
                    // empty health record instead of `None`.
                    if design_capacity.is_some()
                        || cycle_count.is_some()
                        || condition != BatteryCondition::Unknown
                    {
                        health = Some(BatteryHealthSnapshot {
                            cycle_count,
                            design_capacity_mah: design_capacity,
                            max_capacity_mah,
                            health_percent,
                            condition,
                            temperature_celsius,
                        });
                    }
                    break;
                }
                CFRelease(list.cast());
            }

            CFRelease(snapshot);
            PowerState {
                on_battery,
                charge_percent,
                health,
            }
        }
    }

    unsafe fn ns_process_info_thermal_state() -> isize {
        unsafe {
            let process_info = ns_process_info();
            let selector = sel_registerName(c"thermalState".as_ptr().cast());
            let send: unsafe extern "C" fn(*mut c_void, *mut c_void) -> isize =
                mem::transmute(objc_msgSend as *const ());
            send(process_info, selector)
        }
    }

    unsafe fn ns_process_info_low_power_mode_enabled() -> bool {
        unsafe {
            let process_info = ns_process_info();
            let selector = sel_registerName(c"isLowPowerModeEnabled".as_ptr().cast());
            let send: unsafe extern "C" fn(*mut c_void, *mut c_void) -> i8 =
                mem::transmute(objc_msgSend as *const ());
            send(process_info, selector) != 0
        }
    }

    unsafe fn ns_process_info() -> *mut c_void {
        unsafe {
            let cls = objc_getClass(c"NSProcessInfo".as_ptr().cast());
            let selector = sel_registerName(c"processInfo".as_ptr().cast());
            let send: unsafe extern "C" fn(*mut c_void, *mut c_void) -> *mut c_void =
                mem::transmute(objc_msgSend as *const ());
            send(cls, selector)
        }
    }

    fn cf_dictionary_bool(dictionary: CFDictionaryRef, key: &str) -> bool {
        unsafe {
            let Some(key_ref) = cfstring_from_str(key) else {
                return false;
            };
            let mut value: *const c_void = ptr::null();
            let found = CFDictionaryGetValueIfPresent(
                dictionary,
                key_ref.cast(),
                &mut value as *mut *const c_void,
            );
            CFRelease(key_ref.cast());

            if found == 0 || value.is_null() {
                return false;
            }

            CFBooleanGetValue(value.cast::<c_void>() as CFBooleanRef)
        }
    }

    fn cf_dictionary_i32(dictionary: CFDictionaryRef, key: &str) -> Option<i32> {
        unsafe {
            let key_ref = cfstring_from_str(key)?;
            let mut value: *const c_void = ptr::null();
            let found = CFDictionaryGetValueIfPresent(
                dictionary,
                key_ref.cast(),
                &mut value as *mut *const c_void,
            );
            CFRelease(key_ref.cast());
            if found == 0 || value.is_null() {
                return None;
            }

            let mut number = 0i32;
            let success = CFNumberGetValue(
                value.cast::<c_void>() as CFNumberRef,
                kCFNumberSInt32Type,
                &mut number as *mut i32 as *mut c_void,
            );
            success.then_some(number)
        }
    }

    /// Read a string-valued entry from a CF dictionary.
    ///
    /// Used for `BatteryHealth` which returns human-readable condition
    /// strings ("Good", "Fair", "Poor", "Service Battery"). The caller maps
    /// those strings to our typed `BatteryCondition` enum.
    fn cf_dictionary_string(dictionary: CFDictionaryRef, key: &str) -> Option<String> {
        unsafe {
            let key_ref = cfstring_from_str(key)?;
            let mut value: *const c_void = ptr::null();
            let found = CFDictionaryGetValueIfPresent(
                dictionary,
                key_ref.cast(),
                &mut value as *mut *const c_void,
            );
            CFRelease(key_ref.cast());
            if found == 0 || value.is_null() {
                return None;
            }
            cfstring_to_rust_string(value.cast::<c_void>() as CFStringRef)
        }
    }

    fn cfstring_to_rust_string(value: CFStringRef) -> Option<String> {
        unsafe {
            if value.is_null() {
                return None;
            }

            let mut buffer = [0i8; 256];
            let success = CFStringGetCString(
                value,
                buffer.as_mut_ptr(),
                buffer.len() as isize,
                kCFStringEncodingUTF8,
            );
            (success != 0).then(|| {
                CStr::from_ptr(buffer.as_ptr())
                    .to_string_lossy()
                    .into_owned()
            })
        }
    }

    fn cfstring_from_str(value: &str) -> Option<CFStringRef> {
        let bytes = std::ffi::CString::new(value).ok()?;
        unsafe {
            let string = CFStringCreateWithCString(
                kCFAllocatorDefault,
                bytes.as_ptr(),
                kCFStringEncodingUTF8,
            );
            (!string.is_null()).then_some(string)
        }
    }

    /// Sample SMART data for every physical disk known to diskutil.
    ///
    /// macOS does not publish NVMe SMART data through a public Rust-friendly
    /// API — `IONVMeSMARTUserClient` requires opening an IOUserClient with
    /// privileges that the sandboxed app does not hold. The pragmatic path is
    /// to shell out to `diskutil info -plist`, which Apple documents and
    /// supports on every recent macOS version. Its output is a standard
    /// Apple plist XML blob.
    ///
    /// We parse two commands:
    /// - `diskutil list -plist physical` enumerates whole-disk identifiers
    ///   (`disk0`, `disk1`, ...). Partitions are ignored because SMART is
    ///   per-physical-device, not per-volume.
    /// - `diskutil info -plist <id>` returns the SMART status and a nested
    ///   `SMARTDeviceSpecificKeysMayVaryNotGuaranteed` dictionary with the
    ///   actual counters (percentage used, spare, temperature, hours).
    ///
    /// Each call that fails (no such device, unsupported drive) is skipped
    /// silently rather than aborting the whole sample — a single flaky
    /// external enclosure shouldn't black out the disk panel.
    pub fn sample_disks() -> Vec<DiskHealthSnapshot> {
        let Some(list_plist) = run_diskutil(&["list", "-plist", "physical"]) else {
            return Vec::new();
        };
        let device_ids = parse_whole_disk_identifiers(&list_plist);
        let mut disks = Vec::with_capacity(device_ids.len());
        for device_id in device_ids {
            let Some(info_plist) = run_diskutil(&["info", "-plist", &device_id]) else {
                continue;
            };
            if let Some(snapshot) = parse_disk_info(&device_id, &info_plist) {
                disks.push(snapshot);
            }
        }
        disks
    }

    fn run_diskutil(args: &[&str]) -> Option<String> {
        let output = std::process::Command::new("/usr/sbin/diskutil")
            .args(args)
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        String::from_utf8(output.stdout).ok()
    }

    /// Extract the whole-disk identifiers (`disk0`, `disk1`, ...) from
    /// `diskutil list -plist physical` output.
    ///
    /// The plist holds a top-level array `WholeDisks` whose entries are the
    /// identifiers we need. We avoid pulling in a plist parser crate and
    /// instead walk the XML line-by-line — the format is dead-simple and
    /// stable:
    ///
    /// ```xml
    /// <key>WholeDisks</key>
    /// <array>
    ///     <string>disk0</string>
    ///     <string>disk1</string>
    /// </array>
    /// ```
    fn parse_whole_disk_identifiers(plist: &str) -> Vec<String> {
        let mut identifiers = Vec::new();
        let mut inside_whole_disks = false;
        for line in plist.lines().map(str::trim) {
            if line == "<key>WholeDisks</key>" {
                // The matching <array> opens on the next line.
                inside_whole_disks = true;
                continue;
            }
            if inside_whole_disks {
                if line.starts_with("</array>") {
                    break;
                }
                if let Some(id) = line
                    .strip_prefix("<string>")
                    .and_then(|rest| rest.strip_suffix("</string>"))
                {
                    identifiers.push(id.to_owned());
                }
            }
        }
        identifiers
    }

    /// Parse the interesting fields out of `diskutil info -plist <id>` output.
    ///
    /// Two tiers of keys:
    /// - Top-level scalars (`IORegistryEntryName`, `TotalSize`, `SMARTStatus`)
    ///   map to model fields directly. These are read **at depth 1** — the
    ///   immediate children of the root `<dict>`. `diskutil` sometimes nests
    ///   duplicate keys inside sub-dicts like `ParentWholeDisk`, so a naive
    ///   first-match lexical scan can silently return the wrong value.
    /// - The nested `SMARTDeviceSpecificKeysMayVaryNotGuaranteed` dict holds
    ///   NVMe-specific counters (`PERCENTAGE_USED`, `TEMPERATURE`, etc.).
    ///   Missing keys are `None` so the UI can render "—" instead of
    ///   inventing a zero.
    ///
    /// Returns `None` when the disk has no identifying fields at all — a
    /// plist with blank model, zero total size, and no SMART status is
    /// almost certainly garbage or a format change in `diskutil` and we
    /// would rather skip the drive than render a mystery row.
    ///
    /// `TEMPERATURE` is reported in tenths of a Kelvin, matching the NVMe
    /// spec. The conversion to Celsius is `(value / 10) - 273.15`. The
    /// accepted range is `-40..=120°C` — distressed NVMe drives under
    /// sustained load can legitimately hit 100-110°C, so clipping at 100°C
    /// would silently drop the exact readings we most want to surface.
    fn parse_disk_info(device_id: &str, plist: &str) -> Option<DiskHealthSnapshot> {
        let top_level = extract_dict_body(plist, 1)?;
        let model = find_string_in_body(&top_level, "IORegistryEntryName").unwrap_or_default();
        let total_size_bytes = find_integer_in_body(&top_level, "TotalSize")
            .and_then(|value| u64::try_from(value).ok())
            .unwrap_or(0);
        let smart_status = find_string_in_body(&top_level, "SMARTStatus").unwrap_or_default();

        // Garbage-in guard: if we got zero identifying information, there is
        // nothing useful to report. Reporting a phantom drive would be worse
        // than skipping one.
        if model.is_empty() && total_size_bytes == 0 && smart_status.is_empty() {
            return None;
        }

        let status = match smart_status.as_str() {
            "Verified" => DiskHealthStatus::Healthy,
            "Failing" => DiskHealthStatus::Failing,
            "Not Supported" | "" => DiskHealthStatus::NotSupported,
            _ => DiskHealthStatus::Unknown,
        };

        // SMART counters live inside the nested dict. Scoped extraction
        // means every key lookup is unambiguous — no accidental collisions
        // with top-level or other sub-dict entries.
        let smart_body =
            extract_named_section_body(plist, "SMARTDeviceSpecificKeysMayVaryNotGuaranteed")
                .unwrap_or_default();
        let percentage_used = find_integer_in_body(&smart_body, "PERCENTAGE_USED")
            .and_then(|value| u8::try_from(value).ok());
        let available_spare = find_integer_in_body(&smart_body, "AVAILABLE_SPARE")
            .and_then(|value| u8::try_from(value).ok());
        let power_on_hours = find_integer_in_body(&smart_body, "POWER_ON_HOURS_0")
            .and_then(|value| u64::try_from(value).ok());
        let power_cycles = find_integer_in_body(&smart_body, "POWER_CYCLES_0")
            .and_then(|value| u64::try_from(value).ok());
        let media_errors = find_integer_in_body(&smart_body, "MEDIA_ERRORS_0")
            .and_then(|value| u64::try_from(value).ok());
        let temperature_celsius =
            find_integer_in_body(&smart_body, "TEMPERATURE").and_then(|value| {
                // NVMe TEMPERATURE is 1/10 Kelvin.
                let celsius = (value as f32 / 10.0) - 273.15;
                (-40.0..=120.0).contains(&celsius).then_some(celsius)
            });

        // Elevate "healthy but nearing wear-out" to Warning so the UI can
        // distinguish "all good" from "back up your data soon" even while
        // macOS still considers the drive Verified.
        let status = if matches!(status, DiskHealthStatus::Healthy)
            && percentage_used.map(|value| value >= 80).unwrap_or(false)
        {
            DiskHealthStatus::Warning
        } else {
            status
        };

        Some(DiskHealthSnapshot {
            device_identifier: device_id.to_owned(),
            model,
            total_size_bytes,
            status,
            temperature_celsius,
            percentage_used,
            available_spare_percent: available_spare,
            power_on_hours,
            power_cycles,
            media_errors,
        })
    }

    /// Return the body of a `<dict>` opened at a specific nesting depth.
    ///
    /// Depth 1 is the first `<dict>` encountered — the root dict in a
    /// standard plist. Depth 2 would be a `<dict>` nested inside the root.
    /// We track nesting with a stack of open/close counters so the returned
    /// slice contains only the content at `target_depth`, with all nested
    /// dicts **removed entirely** rather than flattened. That makes
    /// `find_string_in_body` / `find_integer_in_body` unable to cross a
    /// nested boundary by accident.
    fn extract_dict_body(plist: &str, target_depth: usize) -> Option<String> {
        let mut depth = 0usize;
        let mut collecting_depth: Option<usize> = None;
        let mut skip_nested_depth: Option<usize> = None;
        let mut collected = String::new();
        for raw in plist.lines() {
            let line = raw.trim();
            let is_dict_open = line == "<dict>" || line.ends_with("<dict>");
            let is_dict_close = line == "</dict>";

            // Entering a new dict: increment depth, decide what to do with
            // the new level.
            if is_dict_open {
                depth += 1;
                if depth == target_depth && collecting_depth.is_none() {
                    collecting_depth = Some(target_depth);
                    continue;
                }
                // If we're already collecting and this is a nested dict,
                // record it as a skip region so its keys don't leak in.
                if collecting_depth.is_some() && skip_nested_depth.is_none() {
                    skip_nested_depth = Some(depth);
                }
                continue;
            }

            if is_dict_close {
                if let Some(skip_at) = skip_nested_depth
                    && depth == skip_at
                {
                    skip_nested_depth = None;
                }
                if let Some(collecting_at) = collecting_depth
                    && depth == collecting_at
                {
                    // Closing the dict we were collecting — we're done.
                    return Some(collected);
                }
                depth = depth.saturating_sub(1);
                continue;
            }

            if collecting_depth.is_some() && skip_nested_depth.is_none() {
                collected.push_str(line);
                collected.push('\n');
            }
        }
        // Reached EOF without a matching close — return what we have so a
        // malformed-but-mostly-complete plist still yields data.
        (!collected.is_empty()).then_some(collected)
    }

    /// Walk the original plist text and return the body of the `<dict>`
    /// that immediately follows `<key>SECTION</key>`.
    ///
    /// This is distinct from `extract_dict_body` which operates on an
    /// already-scoped body. `extract_named_section_body` is used for the
    /// one nested section we actually care about (the NVMe SMART counters),
    /// and avoids the ambiguity of a general "find any key" scan by
    /// anchoring on the key's exact name before consuming the matching
    /// dict body.
    ///
    /// The returned body has further-nested dicts removed the same way
    /// `extract_dict_body` handles them, so `find_integer_in_body` on the
    /// result is unambiguous.
    fn extract_named_section_body(plist: &str, section_key: &str) -> Option<String> {
        let key_line = format!("<key>{section_key}</key>");
        let mut found_key = false;
        let mut depth = 0usize;
        let mut collecting_at: Option<usize> = None;
        let mut skip_nested_depth: Option<usize> = None;
        let mut collected = String::new();
        for raw in plist.lines() {
            let line = raw.trim();
            if !found_key {
                if line == key_line {
                    found_key = true;
                }
                continue;
            }

            let is_dict_open = line == "<dict>" || line.ends_with("<dict>");
            let is_dict_close = line == "</dict>";

            if is_dict_open {
                depth += 1;
                if collecting_at.is_none() {
                    // The dict that opens right after our target key —
                    // start collecting its body.
                    collecting_at = Some(depth);
                    continue;
                }
                // Nested dict inside our target — skip its contents.
                if skip_nested_depth.is_none() {
                    skip_nested_depth = Some(depth);
                }
                continue;
            }

            if is_dict_close {
                if let Some(skip_at) = skip_nested_depth
                    && depth == skip_at
                {
                    skip_nested_depth = None;
                }
                if let Some(collecting) = collecting_at
                    && depth == collecting
                {
                    return Some(collected);
                }
                depth = depth.saturating_sub(1);
                continue;
            }

            if collecting_at.is_some() && skip_nested_depth.is_none() {
                collected.push_str(line);
                collected.push('\n');
            }
        }
        None
    }

    /// Return the `<string>…</string>` body that follows a given `<key>` in
    /// a flattened (depth-scoped) body string.
    ///
    /// The input is the output of `extract_dict_body`, so nested dicts have
    /// already been removed and every key match is unambiguous within its
    /// scope.
    fn find_string_in_body(body: &str, key: &str) -> Option<String> {
        let key_line = format!("<key>{key}</key>");
        let mut lines = body.lines().map(str::trim);
        while let Some(line) = lines.next() {
            if line == key_line {
                let next = lines.next()?;
                return next
                    .strip_prefix("<string>")
                    .and_then(|rest| rest.strip_suffix("</string>"))
                    .map(|value| value.to_owned());
            }
        }
        None
    }

    /// Return the `<integer>` value that follows a given `<key>` in a
    /// flattened body string.
    fn find_integer_in_body(body: &str, key: &str) -> Option<i64> {
        let key_line = format!("<key>{key}</key>");
        let mut lines = body.lines().map(str::trim);
        while let Some(line) = lines.next() {
            if line == key_line {
                let next = lines.next()?;
                return next
                    .strip_prefix("<integer>")
                    .and_then(|rest| rest.strip_suffix("</integer>"))
                    .and_then(|value| value.parse().ok());
            }
        }
        None
    }

    /// Sample battery levels for wireless input peripherals.
    ///
    /// macOS exposes these through the IORegistry under
    /// `AppleDeviceManagementHIDEventService` — one node per connected HID
    /// device with `BatteryPercent`, `Product`, `DeviceAddress`, and
    /// `Transport` keys. We parse `ioreg` text output rather than linking
    /// `IOBluetooth.framework` (deprecated on newer macOS) or opening a
    /// raw IOKit iterator (requires more C FFI than the value justifies).
    ///
    /// Built-in trackpads/keyboards are filtered out via the
    /// `Transport == "FIFO"` check — they have no battery and would clutter
    /// the UI. Everything else with a `BatteryPercent` is included.
    pub fn sample_bluetooth_devices() -> Vec<BluetoothDeviceBattery> {
        let Some(output) = run_ioreg(&["-r", "-l", "-c", "AppleDeviceManagementHIDEventService"])
        else {
            return Vec::new();
        };
        parse_bluetooth_devices(&output)
    }

    fn run_ioreg(args: &[&str]) -> Option<String> {
        let output = std::process::Command::new("/usr/sbin/ioreg")
            .args(args)
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        String::from_utf8(output.stdout).ok()
    }

    /// Split ioreg output into per-device blocks and extract the fields we care
    /// about.
    ///
    /// `ioreg -r -l` emits one node per `+-o` marker and prints its
    /// properties inside a `{ … }` block. On Apple Silicon, matching
    /// `AppleDeviceManagementHIDEventService` can return both the
    /// top-level HID parent **and** a nested `IOHIDEventServiceUserClient`
    /// child whose properties live inside a sub-block indented further
    /// right:
    ///
    /// ```text
    /// +-o AppleDeviceManagementHIDEventService ...
    ///   | {
    ///   |   "Product" = "Magic Keyboard"
    ///   |   "BatteryPercent" = 42
    ///   | }
    ///   |
    ///   +-o IOHIDEventServiceUserClient ...
    ///       {
    ///         "IOUserClientCreator" = "pid 386, WindowServer"
    ///       }
    /// ```
    ///
    /// The previous implementation only split on column-zero `+-o` markers,
    /// so the nested child's property block stayed glued onto the parent.
    /// That is a silent-failure waiting to happen: if a future ioreg output
    /// ever puts a conflicting `Product` or `BatteryPercent` inside a child
    /// node, the parent row would silently absorb it.
    ///
    /// The fix is two-fold:
    /// 1. Split device blocks on **any** `+-o` marker regardless of indent.
    /// 2. Inside each block, only collect property lines that lie between
    ///    that block's own `{` and `}` — anything after the closing brace
    ///    belongs to a sibling or child and is discarded by the block
    ///    boundary, even though the parser already isolates it.
    fn parse_bluetooth_devices(ioreg_output: &str) -> Vec<BluetoothDeviceBattery> {
        let mut devices = Vec::new();
        let mut current: Option<Vec<&str>> = None;
        for line in ioreg_output.lines() {
            if is_ioreg_node_marker(line) {
                if let Some(block) = current.take()
                    && let Some(device) = parse_bluetooth_block(&block)
                {
                    devices.push(device);
                }
                current = Some(Vec::new());
                continue;
            }
            if let Some(block) = current.as_mut() {
                block.push(line);
            }
        }
        if let Some(block) = current
            && let Some(device) = parse_bluetooth_block(&block)
        {
            devices.push(device);
        }
        devices
    }

    /// Detect a `+-o` node marker at any indent level.
    ///
    /// `ioreg` prefixes nested children with whitespace and optional `|`
    /// continuation bars. A top-level node looks like `+-o Foo`, a first-
    /// level child looks like `  +-o Bar`, and deeper descendants add more
    /// indent. We only care about locating the marker itself, so we trim
    /// leading whitespace and the vertical-bar decoration before checking.
    fn is_ioreg_node_marker(line: &str) -> bool {
        let trimmed = line.trim_start_matches([' ', '\t', '|']);
        trimmed.starts_with("+-o ")
    }

    /// Extract property values from a single block's property dict.
    ///
    /// Each `+-o` node emits its properties between `{` and `}` markers
    /// (sometimes prefixed with `|`). Anything outside those markers is
    /// either the class header line or trailing whitespace, and we ignore
    /// it. Tracking this inner scope means even if a future ioreg format
    /// emits property-shaped lines outside the dict block, they cannot
    /// pollute the device record.
    fn parse_bluetooth_block(lines: &[&str]) -> Option<BluetoothDeviceBattery> {
        let mut name = None;
        let mut address = None;
        let mut battery_percent = None;
        let mut transport = None;
        let mut inside_property_dict = false;
        for raw in lines {
            let stripped = raw.trim_start_matches([' ', '|', '\t']);
            let stripped_trimmed = stripped.trim();

            // `{` and `}` delimit the property dict. Some lines are just
            // `{` or `}` by themselves, others are leaf values with those
            // characters embedded — only act on the plain delimiters.
            if stripped_trimmed == "{" {
                inside_property_dict = true;
                continue;
            }
            if stripped_trimmed == "}" {
                inside_property_dict = false;
                continue;
            }
            if !inside_property_dict {
                continue;
            }

            if let Some(value) = extract_string_property(stripped_trimmed, "Product") {
                name = Some(value);
            } else if let Some(value) = extract_string_property(stripped_trimmed, "DeviceAddress") {
                address = Some(value);
            } else if let Some(value) = extract_string_property(stripped_trimmed, "Transport") {
                transport = Some(value);
            } else if let Some(value) = extract_integer_property(stripped_trimmed, "BatteryPercent")
            {
                battery_percent = u8::try_from(value).ok();
            }
        }
        // Built-in HID devices have no battery and would only clutter the
        // list. The internal trackpad reports `Transport = "FIFO"` on Apple
        // Silicon — that's the cleanest filter.
        if matches!(transport.as_deref(), Some("FIFO")) {
            return None;
        }
        let name = name.unwrap_or_else(|| "Unknown device".to_owned());
        let address = address.unwrap_or_default();
        // We only surface devices that report a battery level — users don't
        // want a pile of rows for wired keyboards, docks, etc. that happen
        // to match the HID class.
        battery_percent?;
        let device_type = classify_bluetooth_device(&name);
        Some(BluetoothDeviceBattery {
            name,
            address,
            battery_percent,
            device_type,
        })
    }

    fn extract_string_property(line: &str, key: &str) -> Option<String> {
        let prefix = format!("\"{key}\" = \"");
        let rest = line.strip_prefix(&prefix)?;
        let end = rest.rfind('"')?;
        Some(rest[..end].to_owned())
    }

    fn extract_integer_property(line: &str, key: &str) -> Option<i64> {
        let prefix = format!("\"{key}\" = ");
        let rest = line.strip_prefix(&prefix)?;
        // Integers are unquoted; a quoted value would be a different key type.
        rest.trim_end_matches(|ch: char| !ch.is_ascii_digit() && ch != '-')
            .parse()
            .ok()
    }

    fn classify_bluetooth_device(name: &str) -> aetower_model::BluetoothDeviceType {
        use aetower_model::BluetoothDeviceType;
        let lowered = name.to_ascii_lowercase();
        if lowered.contains("trackpad") {
            BluetoothDeviceType::Trackpad
        } else if lowered.contains("keyboard") {
            BluetoothDeviceType::Keyboard
        } else if lowered.contains("mouse") || lowered.contains("mickey") {
            BluetoothDeviceType::Mouse
        } else if lowered.contains("airpods")
            || lowered.contains("headphones")
            || lowered.contains("beats")
            || lowered.contains("headset")
        {
            BluetoothDeviceType::Headphones
        } else if lowered.contains("controller")
            || lowered.contains("gamepad")
            || lowered.contains("dualshock")
            || lowered.contains("xbox")
        {
            BluetoothDeviceType::GameController
        } else {
            BluetoothDeviceType::Other
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn parses_whole_disk_identifiers() {
            let plist = r#"<?xml version="1.0"?>
<plist version="1.0">
<dict>
    <key>WholeDisks</key>
    <array>
        <string>disk0</string>
        <string>disk1</string>
    </array>
</dict>
</plist>"#;
            assert_eq!(
                parse_whole_disk_identifiers(plist),
                vec!["disk0".to_owned(), "disk1".to_owned()]
            );
        }

        /// A realistic plist skeleton used by the disk-parser tests. Takes
        /// an inner SMART body so individual tests can vary the counters
        /// without repeating the surrounding structure.
        fn disk_plist(
            model: &str,
            total_size: u64,
            smart_status: &str,
            smart_body: &str,
        ) -> String {
            format!(
                r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0">
<dict>
    <key>IORegistryEntryName</key>
    <string>{model}</string>
    <key>TotalSize</key>
    <integer>{total_size}</integer>
    <key>SMARTStatus</key>
    <string>{smart_status}</string>
    <key>SMARTDeviceSpecificKeysMayVaryNotGuaranteed</key>
    <dict>
{smart_body}
    </dict>
    <key>ParentWholeDisk</key>
    <dict>
        <key>IORegistryEntryName</key>
        <string>SHOULD NOT SHOW UP</string>
        <key>TotalSize</key>
        <integer>999</integer>
    </dict>
</dict>
</plist>"#
            )
        }

        #[test]
        fn parses_disk_info_fields() {
            let plist = disk_plist(
                "APPLE SSD AP0512Z Media",
                500_277_792_768,
                "Verified",
                r#"        <key>PERCENTAGE_USED</key>
        <integer>6</integer>
        <key>AVAILABLE_SPARE</key>
        <integer>100</integer>
        <key>POWER_ON_HOURS_0</key>
        <integer>1807</integer>
        <key>POWER_CYCLES_0</key>
        <integer>199</integer>
        <key>MEDIA_ERRORS_0</key>
        <integer>0</integer>
        <key>TEMPERATURE</key>
        <integer>3021</integer>"#,
            );
            let Some(snapshot) = parse_disk_info("disk0", &plist) else {
                panic!("parse_disk_info returned None");
            };
            assert_eq!(snapshot.device_identifier, "disk0");
            assert_eq!(snapshot.model, "APPLE SSD AP0512Z Media");
            assert_eq!(snapshot.total_size_bytes, 500_277_792_768);
            assert_eq!(snapshot.status, DiskHealthStatus::Healthy);
            assert_eq!(snapshot.percentage_used, Some(6));
            assert_eq!(snapshot.available_spare_percent, Some(100));
            assert_eq!(snapshot.power_on_hours, Some(1807));
            assert_eq!(snapshot.power_cycles, Some(199));
            assert_eq!(snapshot.media_errors, Some(0));
            // 3021 deci-Kelvin = 302.1 K = 28.95°C
            let Some(temp) = snapshot.temperature_celsius else {
                panic!("temperature missing from parsed plist");
            };
            assert!((temp - 28.95).abs() < 0.01, "got {temp}");
        }

        /// Regression: the parser must not leak nested-dict values up to
        /// top-level lookups. Prior to the depth-aware rewrite, scanning
        /// for `IORegistryEntryName` would first hit the nested entry in
        /// `ParentWholeDisk` if it appeared before the top-level copy in
        /// `diskutil` output.
        #[test]
        fn top_level_keys_do_not_collide_with_nested_dict_keys() {
            let plist = disk_plist(
                "APPLE SSD Primary",
                1_000_000_000_000,
                "Verified",
                r#"        <key>PERCENTAGE_USED</key>
        <integer>10</integer>"#,
            );
            let Some(snapshot) = parse_disk_info("disk0", &plist) else {
                panic!("parse_disk_info returned None");
            };
            assert_eq!(
                snapshot.model, "APPLE SSD Primary",
                "top-level IORegistryEntryName must not be shadowed by ParentWholeDisk"
            );
            assert_eq!(
                snapshot.total_size_bytes, 1_000_000_000_000,
                "top-level TotalSize must not be shadowed by the nested 999"
            );
        }

        #[test]
        fn high_percentage_used_escalates_to_warning() {
            let plist = disk_plist(
                "Old SSD",
                512_000_000_000,
                "Verified",
                r#"        <key>PERCENTAGE_USED</key>
        <integer>85</integer>"#,
            );
            let Some(snapshot) = parse_disk_info("disk9", &plist) else {
                panic!("parse_disk_info returned None");
            };
            assert_eq!(snapshot.status, DiskHealthStatus::Warning);
        }

        /// Regression: a plist with no identifying fields at all must be
        /// dropped entirely so we don't render a phantom drive with blank
        /// model and zero size.
        #[test]
        fn parse_disk_info_returns_none_on_empty_fields() {
            let plist = r#"<?xml version="1.0"?>
<plist version="1.0">
<dict>
</dict>
</plist>"#;
            assert!(parse_disk_info("disk99", plist).is_none());
        }

        /// Regression: negative integers in the SMART counters used to be
        /// cast with `as u64`, which wraps `-1` to `u64::MAX` and renders
        /// in the UI as a 584-million-year power-on hours count. They must
        /// now become `None`.
        #[test]
        fn negative_integer_counters_become_none_not_u64_max() {
            let plist = disk_plist(
                "Test Drive",
                100,
                "Verified",
                r#"        <key>POWER_ON_HOURS_0</key>
        <integer>-1</integer>
        <key>POWER_CYCLES_0</key>
        <integer>-2</integer>
        <key>MEDIA_ERRORS_0</key>
        <integer>-3</integer>"#,
            );
            let Some(snapshot) = parse_disk_info("disk0", &plist) else {
                panic!("parse_disk_info returned None");
            };
            assert_eq!(snapshot.power_on_hours, None);
            assert_eq!(snapshot.power_cycles, None);
            assert_eq!(snapshot.media_errors, None);
        }

        /// Regression: NVMe drives under sustained load reach 100-115°C
        /// before Apple flips SMART status to Failing. The previous
        /// `-40..=100°C` clip silently discarded exactly the readings the
        /// user most needs to see.
        #[test]
        fn temperature_up_to_120c_is_accepted() {
            // 3831 deci-Kelvin = 383.1 K = 109.95°C
            let plist = disk_plist(
                "Hot NVMe",
                1_000_000_000,
                "Verified",
                r#"        <key>TEMPERATURE</key>
        <integer>3831</integer>"#,
            );
            let Some(snapshot) = parse_disk_info("disk0", &plist) else {
                panic!("parse_disk_info returned None");
            };
            let Some(temp) = snapshot.temperature_celsius else {
                panic!("temperature must be within the valid NVMe range");
            };
            assert!((temp - 109.95).abs() < 0.01, "got {temp}");
        }

        #[test]
        fn parses_bluetooth_block_with_battery() {
            let ioreg = r#"+-o AppleDeviceManagementHIDEventService  <class ...>
    {
      "Product" = "Magic Keyboard with Touch ID and Numeric Keypad"
      "DeviceAddress" = "38-09-fb-02-17-14"
      "Transport" = "Bluetooth"
      "BatteryPercent" = 42
    }

+-o AppleDeviceManagementHIDEventService  <class ...>
    {
      "Product" = "Apple Internal Keyboard / Trackpad"
      "Transport" = "FIFO"
    }

+-o AppleDeviceManagementHIDEventService  <class ...>
    {
      "Product" = "Mickey"
      "DeviceAddress" = "f8-73-df-c5-93-9f"
      "Transport" = "Bluetooth"
      "BatteryPercent" = 70
    }
"#;
            let devices = parse_bluetooth_devices(ioreg);
            assert_eq!(devices.len(), 2);
            assert_eq!(
                devices[0].name,
                "Magic Keyboard with Touch ID and Numeric Keypad"
            );
            assert_eq!(devices[0].battery_percent, Some(42));
            assert_eq!(
                devices[0].device_type,
                aetower_model::BluetoothDeviceType::Keyboard
            );
            assert_eq!(devices[1].name, "Mickey");
            assert_eq!(devices[1].battery_percent, Some(70));
            // "Mickey" is caught by the mouse heuristic.
            assert_eq!(
                devices[1].device_type,
                aetower_model::BluetoothDeviceType::Mouse
            );
        }

        #[test]
        fn bluetooth_block_without_battery_is_dropped() {
            let ioreg = r#"+-o AppleDeviceManagementHIDEventService  <class ...>
    {
      "Product" = "Dock Station"
      "Transport" = "USB"
    }
"#;
            let devices = parse_bluetooth_devices(ioreg);
            assert!(devices.is_empty());
        }

        /// Regression: a top-level Bluetooth node on Apple Silicon often
        /// has a nested `IOHIDEventServiceUserClient` child. The old
        /// parser only split on column-zero `+-o`, so the child's
        /// property block was glued to the parent. If the child ever
        /// emitted a conflicting `Product` or `BatteryPercent`, the
        /// parent row would silently absorb the wrong value.
        ///
        /// This test uses an indented child whose property looks like
        /// `Product = "WRONG LEAK"` and asserts the parent keeps its
        /// real Product string.
        #[test]
        fn nested_child_device_does_not_leak_into_parent() {
            let ioreg = r#"+-o AppleDeviceManagementHIDEventService  <class ...>
  | {
  |   "Product" = "Magic Mouse"
  |   "DeviceAddress" = "aa-bb-cc-dd-ee-ff"
  |   "Transport" = "Bluetooth"
  |   "BatteryPercent" = 55
  | }
  |
  +-o IOHIDEventServiceUserClient  <class ...>
      {
        "Product" = "WRONG LEAK"
        "BatteryPercent" = 1
      }
"#;
            let devices = parse_bluetooth_devices(ioreg);
            // Parent parsed correctly, with its own values.
            let parent = devices
                .iter()
                .find(|device| device.name == "Magic Mouse")
                .unwrap_or_else(|| panic!("parent device missing: {devices:?}"));
            assert_eq!(parent.battery_percent, Some(55));
            // The nested child has no valid Transport or address, but the
            // key test is that the parent's Product was NOT overwritten
            // to "WRONG LEAK" and its BatteryPercent was NOT overwritten
            // to 1.
            assert_ne!(parent.name, "WRONG LEAK");
            assert_ne!(parent.battery_percent, Some(1));
        }

        /// Regression: `is_ioreg_node_marker` must detect `+-o` at any
        /// indent, not just column zero.
        #[test]
        fn ioreg_marker_detected_at_any_indent() {
            assert!(is_ioreg_node_marker("+-o Foo"));
            assert!(is_ioreg_node_marker("  +-o Foo"));
            assert!(is_ioreg_node_marker("  | +-o Foo"));
            assert!(is_ioreg_node_marker("      | | +-o Foo"));
            assert!(!is_ioreg_node_marker("+foo"));
            assert!(!is_ioreg_node_marker("  \"Key\" = \"Value\""));
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod platform {
    use super::{BluetoothDeviceBattery, DiskHealthSnapshot, HostEnvironment};

    pub fn read_environment() -> HostEnvironment {
        HostEnvironment::default()
    }

    pub struct ProcessRusageCounters {
        pub wakeups: u64,
        pub energy_nj: u64,
        pub physical_footprint_bytes: u64,
    }

    pub fn process_counters(_pid: u32) -> Option<ProcessRusageCounters> {
        None
    }

    pub fn process_cwd(_pid: u32) -> Option<String> {
        None
    }

    pub fn compressed_memory_bytes() -> Option<u64> {
        None
    }

    pub fn sample_disks() -> Vec<DiskHealthSnapshot> {
        Vec::new()
    }

    pub fn sample_bluetooth_devices() -> Vec<BluetoothDeviceBattery> {
        Vec::new()
    }
}

#[cfg(test)]
mod top_level_tests {
    use super::{
        NetworkInterfaceIdentitySample, build_network_interface_snapshots, process_refresh_kind,
    };
    use sysinfo::{Networks, UpdateKind};

    /// Regression: the first `collect()` call on a freshly-constructed
    /// `Collector` must not report non-zero per-interface bps. Before
    /// this fix, sysinfo's `received()` returned the delta between the
    /// constructor's initial refresh and the first `collect()` call —
    /// an unknown interval that divided by `TICK_SECONDS` produced a
    /// misleading rate (typically off by a factor of two or more).
    /// With the first-tick seed, all rates must be zero.
    #[test]
    fn first_tick_network_rates_are_seeded_to_zero() {
        let networks = Networks::new_with_refreshed_list();
        // Pull a real interface name so the cache match succeeds, but
        // mirror it as a local `NetworkInterfaceIdentitySample` so the
        // test does not depend on the platform-specific MAC format.
        let cached: Vec<NetworkInterfaceIdentitySample> = networks
            .iter()
            .take(1)
            .map(|(name, _data)| NetworkInterfaceIdentitySample {
                name: name.clone(),
                mac_address: String::new(),
                is_up: true,
            })
            .collect();
        if cached.is_empty() {
            // No interfaces exposed in the test environment; nothing to
            // assert but the code path must still not panic.
            return;
        }

        let snapshots = build_network_interface_snapshots(&networks, &cached, true);
        assert_eq!(snapshots.len(), 1);
        assert_eq!(
            snapshots[0].receive_bps, 0,
            "first-tick receive_bps must be zero to avoid the misleading startup rate"
        );
        assert_eq!(
            snapshots[0].send_bps, 0,
            "first-tick send_bps must be zero to avoid the misleading startup rate"
        );
    }

    /// A subsequent tick (first_tick=false) is allowed to report any
    /// non-negative rate. We can't easily assert a specific value
    /// without mocking sysinfo, so this test just verifies the call
    /// does not panic and produces the same number of rows as the
    /// cached metadata.
    #[test]
    fn subsequent_ticks_produce_one_snapshot_per_cached_interface() {
        let networks = Networks::new_with_refreshed_list();
        let cached: Vec<NetworkInterfaceIdentitySample> = networks
            .iter()
            .take(2)
            .map(|(name, _data)| NetworkInterfaceIdentitySample {
                name: name.clone(),
                mac_address: String::new(),
                is_up: false,
            })
            .collect();
        let snapshots = build_network_interface_snapshots(&networks, &cached, false);
        assert_eq!(snapshots.len(), cached.len());
    }

    #[test]
    fn discovery_scan_refreshes_memory_and_identity_once() {
        let kind = process_refresh_kind(true, true);
        assert!(kind.cpu());
        assert!(kind.disk_usage());
        assert!(kind.memory());
        assert_eq!(kind.user(), UpdateKind::OnlyIfNotSet);
        assert_eq!(kind.cmd(), UpdateKind::OnlyIfNotSet);
        assert_eq!(kind.exe(), UpdateKind::OnlyIfNotSet);
    }

    #[test]
    fn selective_scan_can_refresh_memory_without_identity() {
        let kind = process_refresh_kind(false, true);
        assert!(kind.cpu());
        assert!(kind.disk_usage());
        assert!(kind.memory());
        assert_eq!(kind.user(), UpdateKind::Never);
    }

    #[test]
    fn selective_scan_without_memory_stays_on_cpu_and_disk_only() {
        let kind = process_refresh_kind(false, false);
        assert!(kind.cpu());
        assert!(kind.disk_usage());
        assert!(!kind.memory());
        assert_eq!(kind.user(), UpdateKind::Never);
    }
}
