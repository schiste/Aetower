#[derive(Debug, Clone, Default)]
pub struct GpuSample {
    pub gpu_percent: f32,
    pub ane_percent: f32,
    pub gpu_memory_bytes: u64,
    pub gpu_temperature_celsius: Option<f32>,
}

/// Sample GPU, Neural Engine, and memory counters directly from IORegistry on macOS.
pub fn sample_gpu() -> Option<GpuSample> {
    platform::sample_gpu()
}

#[cfg(target_os = "macos")]
mod platform {
    use std::{
        ffi::{CString, c_void},
        ptr,
    };

    use core_foundation_sys::{
        base::{CFRelease, CFTypeRef, kCFAllocatorDefault},
        dictionary::{CFDictionaryGetValueIfPresent, CFDictionaryRef, CFMutableDictionaryRef},
        number::{CFNumberGetValue, CFNumberRef, kCFNumberSInt64Type},
        string::{
            CFStringCreateWithCString, CFStringGetCString, CFStringRef, kCFStringEncodingUTF8,
        },
    };

    use super::GpuSample;

    type KernReturn = i32;
    type IoObject = u32;
    type IoIterator = IoObject;
    type IoRegistryEntry = IoObject;
    type MachPort = u32;

    const KERN_SUCCESS: KernReturn = 0;

    #[link(name = "IOKit", kind = "framework")]
    unsafe extern "C" {
        fn IOServiceMatching(name: *const i8) -> CFMutableDictionaryRef;
        fn IOServiceGetMatchingServices(
            master_port: MachPort,
            matching: CFMutableDictionaryRef,
            existing: *mut IoIterator,
        ) -> KernReturn;
        fn IOIteratorNext(iterator: IoIterator) -> IoObject;
        fn IOObjectRelease(object: IoObject) -> KernReturn;
        fn IORegistryEntryCreateCFProperties(
            entry: IoRegistryEntry,
            properties: *mut CFMutableDictionaryRef,
            allocator: *const c_void,
            options: u32,
        ) -> KernReturn;
    }

    pub fn sample_gpu() -> Option<GpuSample> {
        let class_name = CString::new("IOAccelerator").ok()?;
        let matching = unsafe { IOServiceMatching(class_name.as_ptr()) };
        if matching.is_null() {
            return None;
        }

        let mut iterator: IoIterator = 0;
        let result = unsafe { IOServiceGetMatchingServices(0, matching, &mut iterator) };
        if result != KERN_SUCCESS || iterator == 0 {
            return None;
        }

        let mut merged = GpuSample::default();
        let mut found = false;

        loop {
            let entry = unsafe { IOIteratorNext(iterator) };
            if entry == 0 {
                break;
            }
            if let Some(sample) = read_entry_sample(entry) {
                // Utilisation is the busiest accelerator (summing percentages is
                // meaningless), but memory is the total in use across all GPUs —
                // max under-reported VRAM on integrated+discrete Macs.
                merged.gpu_percent = merged.gpu_percent.max(sample.gpu_percent);
                merged.ane_percent = merged.ane_percent.max(sample.ane_percent);
                merged.gpu_memory_bytes = merged
                    .gpu_memory_bytes
                    .saturating_add(sample.gpu_memory_bytes);
                if merged.gpu_temperature_celsius.is_none() {
                    merged.gpu_temperature_celsius = sample.gpu_temperature_celsius;
                }
                found = true;
            }
            let _ = unsafe { IOObjectRelease(entry) };
        }
        let _ = unsafe { IOObjectRelease(iterator) };

        found.then_some(merged)
    }

    fn read_entry_sample(entry: IoRegistryEntry) -> Option<GpuSample> {
        let mut properties: CFMutableDictionaryRef = ptr::null_mut();
        let result = unsafe {
            IORegistryEntryCreateCFProperties(entry, &mut properties, kCFAllocatorDefault, 0)
        };
        if result != KERN_SUCCESS || properties.is_null() {
            return None;
        }

        let sample = read_sample_from_properties(properties);
        unsafe { CFRelease(properties as CFTypeRef) };
        sample
    }

    fn read_sample_from_properties(properties: CFDictionaryRef) -> Option<GpuSample> {
        let performance_stats = dictionary_value(properties, "PerformanceStatistics")
            .map(|value| value as CFDictionaryRef)
            .or(Some(properties))?;

        let gpu_percent = number_value(performance_stats, "Device Utilization %")
            .or_else(|| number_value(performance_stats, "Renderer Utilization %"))
            .unwrap_or(0) as f32;
        let gpu_memory_bytes = number_value(performance_stats, "In use system memory")
            .or_else(|| number_value(performance_stats, "Alloc system memory"))
            .unwrap_or(0);
        let ane_percent = number_value(properties, "ane-device-utilization")
            .or_else(|| number_value(properties, "ANE Utilization %"))
            .or_else(|| number_value(performance_stats, "ane-device-utilization"))
            .unwrap_or(0) as f32;

        // GPU temperature from PerformanceStatistics
        let gpu_temperature_celsius = number_value(performance_stats, "GPU Die Temperature")
            .or_else(|| number_value(performance_stats, "gpu-die-temperature"))
            .or_else(|| number_value(performance_stats, "Temperature(C)"))
            .map(|v| {
                let temp = v as f32;
                if temp > 200.0 { temp / 10.0 } else { temp } // some GPUs report in tenths
            })
            .filter(|&t| t > 0.0 && t < 150.0); // sanity check

        Some(GpuSample {
            gpu_percent,
            ane_percent,
            gpu_memory_bytes,
            gpu_temperature_celsius,
        })
    }

    fn dictionary_value(dict: CFDictionaryRef, key: &str) -> Option<*const c_void> {
        let cf_key = cf_string(key)?;
        let mut value: *const c_void = ptr::null();
        let found = unsafe {
            CFDictionaryGetValueIfPresent(
                dict,
                cf_key as *const c_void,
                &mut value as *mut *const c_void,
            )
        };
        unsafe { CFRelease(cf_key as CFTypeRef) };
        (found != 0 && !value.is_null()).then_some(value)
    }

    fn number_value(dict: CFDictionaryRef, key: &str) -> Option<u64> {
        let value = dictionary_value(dict, key)?;
        number_from_cf(value as CFNumberRef)
            .or_else(|| string_from_cf(value as CFStringRef).and_then(|text| text.parse().ok()))
    }

    fn number_from_cf(number: CFNumberRef) -> Option<u64> {
        let mut value = 0i64;
        let ok = unsafe {
            CFNumberGetValue(
                number,
                kCFNumberSInt64Type,
                &mut value as *mut i64 as *mut c_void,
            )
        };
        (ok && value >= 0).then_some(value as u64)
    }

    fn cf_string(value: &str) -> Option<CFStringRef> {
        let c_string = CString::new(value).ok()?;
        let string = unsafe {
            CFStringCreateWithCString(
                kCFAllocatorDefault,
                c_string.as_ptr(),
                kCFStringEncodingUTF8,
            )
        };
        (!string.is_null()).then_some(string)
    }

    fn string_from_cf(value: CFStringRef) -> Option<String> {
        let mut buffer = vec![0i8; 256];
        let ok = unsafe {
            CFStringGetCString(
                value,
                buffer.as_mut_ptr(),
                buffer.len() as isize,
                kCFStringEncodingUTF8,
            )
        };
        if ok == 0 {
            return None;
        }
        let bytes = buffer
            .into_iter()
            .take_while(|byte| *byte != 0)
            .map(|byte| byte as u8)
            .collect::<Vec<_>>();
        String::from_utf8(bytes).ok()
    }
}

#[cfg(not(target_os = "macos"))]
mod platform {
    use super::GpuSample;

    pub fn sample_gpu() -> Option<GpuSample> {
        None
    }
}
