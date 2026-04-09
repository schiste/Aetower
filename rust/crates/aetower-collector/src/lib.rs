use std::collections::{BTreeMap, HashMap};

use aetower_model::ThermalState;
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

const USER_DIRECTORY_INITIAL_REFRESH_TICKS: u8 = 10;
const USER_DIRECTORY_REFRESH_INTERVAL_TICKS: u8 = 120;

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
        if self.host_environment_refresh_tick == 0 {
            self.cached_host_environment = read_environment();
        }
        self.host_environment_refresh_tick = (self.host_environment_refresh_tick + 1) % 5;

        let mut network_totals = NetworkTotals::default();
        for (_name, data) in &self.networks {
            network_totals.received = network_totals.received.saturating_add(data.received());
            network_totals.transmitted = network_totals
                .transmitted
                .saturating_add(data.transmitted());
        }

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

                let tick_seconds = 2.0_f32;
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
        };
        self.previous_network_totals = network_totals;

        RawSnapshot { host, processes }
    }
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

#[derive(Debug, Clone)]
pub struct HostEnvironment {
    pub thermal_state: ThermalState,
    pub on_battery: bool,
    pub battery_charge_percent: Option<u8>,
    pub low_power_mode: bool,
}

impl Default for HostEnvironment {
    fn default() -> Self {
        Self {
            thermal_state: ThermalState::Nominal,
            on_battery: false,
            battery_charge_percent: None,
            low_power_mode: false,
        }
    }
}

pub fn read_environment() -> HostEnvironment {
    platform::read_environment()
}

#[cfg(target_os = "macos")]
mod platform {
    use aetower_model::ThermalState;
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
        let (on_battery, battery_charge_percent) = power_state();

        HostEnvironment {
            thermal_state,
            on_battery,
            battery_charge_percent,
            low_power_mode,
        }
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

    #[allow(clippy::collapsible_if)]
    fn power_state() -> (bool, Option<u8>) {
        unsafe {
            let snapshot = IOPSCopyPowerSourcesInfo();
            if snapshot.is_null() {
                return (false, None);
            }

            let providing_type = cfstring_to_rust_string(IOPSGetProvidingPowerSourceType(snapshot));
            let on_battery = providing_type.as_deref() != Some(K_IO_PM_AC_POWER_KEY);
            let mut battery_charge_percent = None;

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
                        battery_charge_percent = Some(percent);
                        break;
                    }
                }
                CFRelease(list.cast());
            }

            CFRelease(snapshot);
            (on_battery, battery_charge_percent)
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
}

#[cfg(not(target_os = "macos"))]
mod platform {
    use super::HostEnvironment;

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
}
