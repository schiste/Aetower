use std::collections::{BTreeMap, HashMap};

use aetower_model::{
    BatteryHealthSnapshot, BluetoothDeviceBattery, DiskHealthSnapshot, NetworkInterfaceSnapshot,
    ThermalState,
};
use serde::{Deserialize, Serialize};
use sysinfo::{Networks, ProcessRefreshKind, ProcessesToUpdate, System, Users};

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
    pub disk_read_bytes: u64,
    pub disk_write_bytes: u64,
    #[serde(default)]
    pub wakeups_per_second: f32,
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
}

#[derive(Debug, Clone, Copy, Default)]
struct NetworkTotals {
    received: u64,
    transmitted: u64,
}

#[derive(Debug, Clone, Copy, Default)]
struct ProcessCounterSample {
    start_time_millis: u64,
    wakeups: u64,
    disk_read_bytes: u64,
    disk_write_bytes: u64,
    wakeups_per_second: f32,
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

pub struct Collector {
    config: CollectorConfig,
    system: System,
    networks: Networks,
    previous_network_totals: NetworkTotals,
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
            previous_network_totals: NetworkTotals::default(),
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
        }
    }

    pub fn configure(&mut self, config: CollectorConfig) {
        self.config = config;
    }

    pub fn collect(&mut self) -> RawSnapshot {
        self.system.refresh_cpu_all();
        self.system.refresh_memory();
        self.networks.refresh(true);
        // Full PID scan every 10th tick (~20s); selective refresh in between.
        let full_scan =
            self.config.full_collection || self.process_metadata_tick.is_multiple_of(10);
        if full_scan || self.known_pids.is_empty() {
            self.system.refresh_processes_specifics(
                ProcessesToUpdate::All,
                true,
                process_refresh_kind(self.process_metadata_tick),
            );
            self.known_pids = self.system.processes().keys().copied().collect();
        } else {
            self.system.refresh_processes_specifics(
                ProcessesToUpdate::Some(&self.known_pids),
                false,
                process_refresh_kind(self.process_metadata_tick),
            );
        }
        self.process_metadata_tick = self.process_metadata_tick.wrapping_add(1);
        self.user_directory_refresh_tick = self.user_directory_refresh_tick.wrapping_add(1);
        let refresh_host_environment = self.host_environment_refresh_tick == 0;
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
        let refresh_bluetooth = self.cached_bluetooth_devices.is_empty()
            || self.config.full_collection
            || self.bluetooth_refresh_tick == 0;
        if refresh_bluetooth {
            self.cached_bluetooth_devices = platform::sample_bluetooth_devices();
        }
        self.bluetooth_refresh_tick =
            (self.bluetooth_refresh_tick + 1) % BLUETOOTH_REFRESH_INTERVAL_TICKS;

        let mut network_totals = NetworkTotals::default();
        for (_name, data) in &self.networks {
            network_totals.received = network_totals.received.saturating_add(data.received());
            network_totals.transmitted = network_totals
                .transmitted
                .saturating_add(data.transmitted());
        }
        let network_interfaces =
            build_network_interface_snapshots(&self.networks, &self.cached_network_interfaces);

        let metadata_refresh =
            self.config.full_collection || self.process_metadata_tick == 1 || full_scan;
        let sample_wakeups =
            self.config.full_collection || self.wakeups_sample_tick.is_multiple_of(3);
        self.wakeups_sample_tick = self.wakeups_sample_tick.wrapping_add(1);
        let mut user_directory_refreshed = self.should_refresh_user_directory(full_scan);
        if user_directory_refreshed {
            self.users.refresh();
        }

        let mut next_process_counters = HashMap::with_capacity(self.system.processes().len());
        let processes: Vec<_> = self
            .system
            .processes()
            .values()
            .filter(|process| process.pid().as_u32() != self.self_pid)
            .map(|process| {
                let pid = process.pid().as_u32();
                let start_time_millis = process.start_time().saturating_mul(1_000);
                let previous = self
                    .previous_process_counters
                    .get(&pid)
                    .filter(|prev| prev.start_time_millis == start_time_millis);
                let wakeups = if sample_wakeups || previous.is_none() {
                    platform::process_wakeups(pid)
                        .or_else(|| previous.map(|prev| prev.wakeups))
                        .unwrap_or(0)
                } else {
                    previous.map(|prev| prev.wakeups).unwrap_or(0)
                };
                let disk_read_total = process.disk_usage().read_bytes;
                let disk_write_total = process.disk_usage().written_bytes;

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
                        disk_read_bytes: disk_read_total,
                        disk_write_bytes: disk_write_total,
                        wakeups_per_second,
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
                    disk_read_bytes: disk_read_delta,
                    disk_write_bytes: disk_write_delta,
                    wakeups_per_second,
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
        self.previous_process_counters = next_process_counters;

        // Update CWD cache: insert fresh values, prune dead PIDs.
        if self.config.full_collection || self.process_metadata_tick.is_multiple_of(2) {
            let alive: std::collections::HashSet<u32> = processes.iter().map(|p| p.pid).collect();
            self.cwd_cache.retain(|pid, _| alive.contains(pid));
            self.process_identity_cache.retain(|pid, cached| {
                alive.contains(pid)
                    && processes
                        .iter()
                        .any(|p| p.pid == *pid && p.start_time_millis == cached.start_time_millis)
            });
            for process in &processes {
                if let Some(ref cwd) = process.cwd {
                    self.cwd_cache.insert(process.pid, cwd.clone());
                }
            }
        }

        let host_disk_read_bps = processes.iter().fold(0u64, |total, process| {
            total.saturating_add(process.disk_read_bytes)
        });
        let host_disk_write_bps = processes.iter().fold(0u64, |total, process| {
            total.saturating_add(process.disk_write_bytes)
        });

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
            network_receive_bps: network_totals
                .received
                .saturating_sub(self.previous_network_totals.received),
            network_send_bps: network_totals
                .transmitted
                .saturating_sub(self.previous_network_totals.transmitted),
            wakeups_per_second: host_wakeups_per_second,
            thermal_state: self.cached_host_environment.thermal_state,
            on_battery: self.cached_host_environment.on_battery,
            battery_charge_percent: self.cached_host_environment.battery_charge_percent,
            low_power_mode: self.cached_host_environment.low_power_mode,
            battery_health: self.cached_host_environment.battery_health.clone(),
            network_interfaces,
            disks: self.cached_disks.clone(),
            bluetooth_devices: self.cached_bluetooth_devices.clone(),
        };
        self.previous_network_totals = network_totals;

        RawSnapshot { host, processes }
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
) -> Vec<NetworkInterfaceSnapshot> {
    if cached_metadata.is_empty() {
        return Vec::new();
    }

    let throughput_by_name: HashMap<&str, (u64, u64)> = networks
        .iter()
        .map(|(name, data)| {
            let receive_bps = (data.received() as f64 / TICK_SECONDS as f64) as u64;
            let send_bps = (data.transmitted() as f64 / TICK_SECONDS as f64) as u64;
            (name.as_str(), (receive_bps, send_bps))
        })
        .collect();

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

fn process_refresh_kind(metadata_tick: u8) -> ProcessRefreshKind {
    if metadata_tick == 0 {
        ProcessRefreshKind::everything()
    } else {
        ProcessRefreshKind::nothing().with_cpu().with_disk_usage()
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
}

pub fn read_environment() -> HostEnvironment {
    platform::read_environment()
}

#[cfg(target_os = "macos")]
mod platform {
    use aetower_model::{
        BatteryCondition, BatteryHealthSnapshot, BluetoothDeviceBattery, DiskHealthSnapshot,
        DiskHealthStatus, ThermalState,
    };
    use std::{
        ffi::{CStr, c_char, c_void},
        mem, ptr,
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

        HostEnvironment {
            thermal_state,
            on_battery: power.on_battery,
            battery_charge_percent: power.charge_percent,
            low_power_mode,
            battery_health: power.health,
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

    pub fn process_wakeups(pid: u32) -> Option<u64> {
        let mut info = RUsageInfoV2::default();
        let result = unsafe {
            proc_pid_rusage(
                pid as i32,
                RUSAGE_INFO_V2,
                &mut info as *mut RUsageInfoV2 as *mut c_void,
            )
        };
        (result == 0).then_some(
            info.ri_interrupt_wkups
                .saturating_add(info.ri_pkg_idle_wkups),
        )
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
    const RUSAGE_INFO_V2: i32 = 2;

    #[repr(C)]
    #[derive(Default)]
    struct RUsageInfoV2 {
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
        ri_child_user_time: u64,
        ri_child_system_time: u64,
        ri_child_pkg_idle_wkups: u64,
        ri_child_interrupt_wkups: u64,
        ri_child_pageins: u64,
        ri_child_elapsed_abstime: u64,
        ri_diskio_bytesread: u64,
        ri_diskio_byteswritten: u64,
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
                    let cycle_count = cf_dictionary_i32(description, K_IOPS_CYCLE_COUNT_KEY)
                        .filter(|value| *value >= 0)
                        .map(|value| value as u32)
                        .unwrap_or(0);
                    let design_capacity =
                        cf_dictionary_i32(description, K_IOPS_DESIGN_CAPACITY_KEY)
                            .filter(|value| *value > 0)
                            .map(|value| value as u32)
                            .unwrap_or(0);
                    let max_capacity_mah = max_capacity
                        .filter(|value| *value > 0)
                        .map(|value| value as u32)
                        .unwrap_or(0);
                    let health_percent = if design_capacity > 0 && max_capacity_mah > 0 {
                        (max_capacity_mah as f32 / design_capacity as f32 * 100.0).clamp(0.0, 100.0)
                    } else {
                        0.0
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

                    if design_capacity > 0
                        || cycle_count > 0
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
    ///   map to model fields directly.
    /// - The nested `SMARTDeviceSpecificKeysMayVaryNotGuaranteed` dict holds
    ///   NVMe-specific counters (`PERCENTAGE_USED`, `TEMPERATURE`, etc.).
    ///   Missing keys are `None` so the UI can render "—" instead of
    ///   inventing a zero.
    ///
    /// `TEMPERATURE` is reported in tenths of a Kelvin, matching the NVMe
    /// spec. The conversion to Celsius is `(value / 10) - 273.15`; values
    /// outside `-40..=100°C` are treated as garbage.
    fn parse_disk_info(device_id: &str, plist: &str) -> Option<DiskHealthSnapshot> {
        let model = find_string_value(plist, "IORegistryEntryName").unwrap_or_default();
        let total_size_bytes = find_integer_value(plist, "TotalSize")
            .and_then(|value| u64::try_from(value).ok())
            .unwrap_or(0);
        let smart_status = find_string_value(plist, "SMARTStatus").unwrap_or_default();
        let status = match smart_status.as_str() {
            "Verified" => DiskHealthStatus::Healthy,
            "Failing" => DiskHealthStatus::Failing,
            "Not Supported" | "" => DiskHealthStatus::NotSupported,
            _ => DiskHealthStatus::Unknown,
        };
        let percentage_used =
            find_integer_value(plist, "PERCENTAGE_USED").and_then(|value| u8::try_from(value).ok());
        let available_spare =
            find_integer_value(plist, "AVAILABLE_SPARE").and_then(|value| u8::try_from(value).ok());
        let power_on_hours =
            find_integer_value(plist, "POWER_ON_HOURS_0").map(|value| value as u64);
        let power_cycles = find_integer_value(plist, "POWER_CYCLES_0").map(|value| value as u64);
        let media_errors = find_integer_value(plist, "MEDIA_ERRORS_0").map(|value| value as u64);
        let temperature_celsius = find_integer_value(plist, "TEMPERATURE").and_then(|value| {
            // NVMe TEMPERATURE is 1/10 Kelvin.
            let celsius = (value as f32 / 10.0) - 273.15;
            (-40.0..=100.0).contains(&celsius).then_some(celsius)
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

    /// Return the `<string>…</string>` body that follows a given `<key>`.
    ///
    /// This is intentionally a simple linear scan — the plists we parse have
    /// tens of keys, not thousands, and staying off a plist parser keeps the
    /// dependency footprint small.
    fn find_string_value(plist: &str, key: &str) -> Option<String> {
        let key_line = format!("<key>{key}</key>");
        let mut lines = plist.lines().map(str::trim);
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

    /// Return the `<integer>` value that follows a given `<key>`.
    fn find_integer_value(plist: &str, key: &str) -> Option<i64> {
        let key_line = format!("<key>{key}</key>");
        let mut lines = plist.lines().map(str::trim);
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
    /// `ioreg -r` emits one top-level block per matching node, separated by
    /// blank lines. Inside a block, property lines look like:
    ///
    /// ```text
    ///       "Product" = "Magic Keyboard"
    ///       "BatteryPercent" = 42
    ///       "DeviceAddress" = "38-09-fb-02-17-14"
    /// ```
    ///
    /// The leading indent varies (`|   "..."` vs `    "..."`) depending on
    /// whether the node is part of a chain or a leaf, so we normalise by
    /// trimming and skipping the decorative characters before looking at
    /// content.
    fn parse_bluetooth_devices(ioreg_output: &str) -> Vec<BluetoothDeviceBattery> {
        let mut devices = Vec::new();
        let mut block: Vec<&str> = Vec::new();
        for line in ioreg_output.lines() {
            // Top-level nodes start with `+-o` — that's our block separator.
            if line.starts_with("+-o") {
                if !block.is_empty() {
                    if let Some(device) = parse_bluetooth_block(&block) {
                        devices.push(device);
                    }
                    block.clear();
                }
                continue;
            }
            block.push(line);
        }
        if !block.is_empty()
            && let Some(device) = parse_bluetooth_block(&block)
        {
            devices.push(device);
        }
        devices
    }

    fn parse_bluetooth_block(lines: &[&str]) -> Option<BluetoothDeviceBattery> {
        let mut name = None;
        let mut address = None;
        let mut battery_percent = None;
        let mut transport = None;
        for raw in lines {
            let line = raw.trim_start_matches(|ch: char| {
                matches!(ch, ' ' | '|' | '\t' | '{' | '+' | '-' | 'o')
            });
            let line = line.trim();
            if let Some(value) = extract_string_property(line, "Product") {
                name = Some(value);
            } else if let Some(value) = extract_string_property(line, "DeviceAddress") {
                address = Some(value);
            } else if let Some(value) = extract_string_property(line, "Transport") {
                transport = Some(value);
            } else if let Some(value) = extract_integer_property(line, "BatteryPercent") {
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

        #[test]
        fn parses_disk_info_fields() {
            let plist = r#"<plist>
<dict>
    <key>IORegistryEntryName</key>
    <string>APPLE SSD AP0512Z Media</string>
    <key>TotalSize</key>
    <integer>500277792768</integer>
    <key>SMARTStatus</key>
    <string>Verified</string>
    <key>PERCENTAGE_USED</key>
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
    <integer>3021</integer>
</dict>
</plist>"#;
            let Some(snapshot) = parse_disk_info("disk0", plist) else {
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

        #[test]
        fn high_percentage_used_escalates_to_warning() {
            let plist = r#"<plist>
<dict>
    <key>IORegistryEntryName</key>
    <string>Old SSD</string>
    <key>TotalSize</key>
    <integer>0</integer>
    <key>SMARTStatus</key>
    <string>Verified</string>
    <key>PERCENTAGE_USED</key>
    <integer>85</integer>
</dict>
</plist>"#;
            let Some(snapshot) = parse_disk_info("disk9", plist) else {
                panic!("parse_disk_info returned None");
            };
            assert_eq!(snapshot.status, DiskHealthStatus::Warning);
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
    }
}

#[cfg(not(target_os = "macos"))]
mod platform {
    use super::{BluetoothDeviceBattery, DiskHealthSnapshot, HostEnvironment};

    pub fn read_environment() -> HostEnvironment {
        HostEnvironment::default()
    }

    pub fn process_wakeups(_pid: u32) -> Option<u64> {
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
