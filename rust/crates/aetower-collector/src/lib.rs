use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use sysinfo::{
    CpuExt, CpuRefreshKind, NetworkExt, PidExt, ProcessExt, ProcessRefreshKind, RefreshKind,
    System, SystemExt,
};

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
    pub virtual_memory_bytes: u64,
    pub disk_read_bytes: u64,
    pub disk_write_bytes: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RawHostSample {
    pub cpu_percent: f32,
    pub memory_used_bytes: u64,
    pub memory_total_bytes: u64,
    pub swap_used_bytes: u64,
    pub disk_read_bps: u64,
    pub disk_write_bps: u64,
    pub network_receive_bps: u64,
    pub network_send_bps: u64,
    pub thermal_state: String,
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

pub struct Collector {
    system: System,
    previous_network_totals: NetworkTotals,
    self_pid: u32,
    process_metadata_tick: u8,
    host_environment_refresh_tick: u8,
    cached_host_environment: HostEnvironment,
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
            self_pid: std::process::id(),
            process_metadata_tick: 0,
            host_environment_refresh_tick: 0,
            cached_host_environment: HostEnvironment::default(),
        }
    }

    pub fn collect(&mut self) -> RawSnapshot {
        self.system.refresh_cpu();
        self.system.refresh_memory();
        self.system.refresh_networks();
        self.system
            .refresh_processes_specifics(process_refresh_kind(self.process_metadata_tick));
        self.process_metadata_tick = self.process_metadata_tick.wrapping_add(1);
        if self.host_environment_refresh_tick == 0 {
            self.cached_host_environment = read_environment();
        }
        self.host_environment_refresh_tick = (self.host_environment_refresh_tick + 1) % 5;

        let mut network_totals = NetworkTotals::default();
        for (_name, data) in self.system.networks() {
            network_totals.received = network_totals.received.saturating_add(data.received());
            network_totals.transmitted = network_totals
                .transmitted
                .saturating_add(data.transmitted());
        }

        let processes: Vec<_> = self
            .system
            .processes()
            .values()
            .filter(|process| process.pid().as_u32() != self.self_pid)
            .map(|process| RawProcessSample {
                pid: process.pid().as_u32(),
                parent_pid: process.parent().map(|parent| parent.as_u32()),
                start_time_millis: process.start_time().saturating_mul(1_000),
                name: process.name().to_owned(),
                exe: path_to_string(process.exe()),
                cmd: process
                    .cmd()
                    .iter()
                    .map(|segment| segment.to_string())
                    .collect(),
                cpu_percent: process.cpu_usage(),
                memory_bytes: process.memory(),
                virtual_memory_bytes: process.virtual_memory(),
                disk_read_bytes: process.disk_usage().read_bytes,
                disk_write_bytes: process.disk_usage().written_bytes,
            })
            .collect();

        let host_disk_read_bps = processes.iter().fold(0u64, |total, process| {
            total.saturating_add(process.disk_read_bytes)
        });
        let host_disk_write_bps = processes.iter().fold(0u64, |total, process| {
            total.saturating_add(process.disk_write_bytes)
        });

        let host = RawHostSample {
            cpu_percent: self.system.global_cpu_info().cpu_usage(),
            memory_used_bytes: self.system.used_memory(),
            memory_total_bytes: self.system.total_memory(),
            swap_used_bytes: self.system.used_swap(),
            disk_read_bps: host_disk_read_bps,
            disk_write_bps: host_disk_write_bps,
            network_receive_bps: network_totals
                .received
                .saturating_sub(self.previous_network_totals.received),
            network_send_bps: network_totals
                .transmitted
                .saturating_sub(self.previous_network_totals.transmitted),
            thermal_state: self.cached_host_environment.thermal_state.clone(),
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

fn process_refresh_kind(metadata_tick: u8) -> ProcessRefreshKind {
    if metadata_tick == 0 {
        ProcessRefreshKind::everything()
    } else {
        ProcessRefreshKind::new().with_cpu().with_disk_usage()
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
    pub thermal_state: String,
    pub on_battery: bool,
    pub battery_charge_percent: Option<u8>,
    pub low_power_mode: bool,
}

impl Default for HostEnvironment {
    fn default() -> Self {
        Self {
            thermal_state: "nominal".to_owned(),
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
    use std::{
        ffi::{c_char, c_void, CStr},
        mem, ptr,
    };

    use core_foundation_sys::{
        array::{CFArrayGetCount, CFArrayGetValueAtIndex, CFArrayRef},
        base::{kCFAllocatorDefault, CFRelease, CFTypeRef},
        dictionary::{CFDictionaryGetValueIfPresent, CFDictionaryRef},
        number::{
            kCFNumberSInt32Type, CFBooleanGetValue, CFBooleanRef, CFNumberGetValue, CFNumberRef,
        },
        string::{
            kCFStringEncodingUTF8, CFStringCreateWithCString, CFStringGetCString, CFStringRef,
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

    fn thermal_state() -> String {
        match unsafe { ns_process_info_thermal_state() } {
            1 => "fair".to_owned(),
            2 => "serious".to_owned(),
            3 => "critical".to_owned(),
            _ => "nominal".to_owned(),
        }
    }

    fn low_power_mode_enabled() -> bool {
        unsafe { ns_process_info_low_power_mode_enabled() }
    }

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
                    {
                        if max_capacity > 0 {
                            let percent = ((current_capacity as f32 / max_capacity as f32) * 100.0)
                                .round()
                                .clamp(0.0, 100.0) as u8;
                            battery_charge_percent = Some(percent);
                            break;
                        }
                    }
                }
                CFRelease(list.cast());
            }

            CFRelease(snapshot);
            (on_battery, battery_charge_percent)
        }
    }

    unsafe fn ns_process_info_thermal_state() -> isize {
        let process_info = ns_process_info();
        let selector = sel_registerName(c"thermalState".as_ptr().cast());
        let send: unsafe extern "C" fn(*mut c_void, *mut c_void) -> isize =
            mem::transmute(objc_msgSend as *const ());
        send(process_info, selector)
    }

    unsafe fn ns_process_info_low_power_mode_enabled() -> bool {
        let process_info = ns_process_info();
        let selector = sel_registerName(c"isLowPowerModeEnabled".as_ptr().cast());
        let send: unsafe extern "C" fn(*mut c_void, *mut c_void) -> i8 =
            mem::transmute(objc_msgSend as *const ());
        send(process_info, selector) != 0
    }

    unsafe fn ns_process_info() -> *mut c_void {
        let cls = objc_getClass(c"NSProcessInfo".as_ptr().cast());
        let selector = sel_registerName(c"processInfo".as_ptr().cast());
        let send: unsafe extern "C" fn(*mut c_void, *mut c_void) -> *mut c_void =
            mem::transmute(objc_msgSend as *const ());
        send(cls, selector)
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
}
