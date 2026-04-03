#[derive(Debug, Clone, Default)]
pub struct GpuSample {
    pub gpu_percent: f32,
    pub ane_percent: f32,
    pub gpu_memory_bytes: u64,
}

/// Sample GPU, Neural Engine, and Media Engine utilization.
///
/// Uses the IOReport private framework on macOS Apple Silicon.
/// Returns `None` if IOReport is unavailable or the machine is Intel.
pub fn sample_gpu() -> Option<GpuSample> {
    platform::sample_gpu()
}

#[cfg(target_os = "macos")]
mod platform {
    use super::GpuSample;

    // IOReport integration placeholder.
    //
    // Full implementation requires:
    // 1. Link IOReport.framework from /System/Library/PrivateFrameworks/
    // 2. Call IOReportCopyChannelsInGroup("GPU Energy Model", nil)
    //    or IOReportCopyChannelsInGroup("Energy Model", nil) for ANE
    // 3. Sample two snapshots ~100ms apart
    // 4. Compute delta utilization from IOReportChannelGetScalarValue
    //
    // For now, return None to avoid private framework linkage issues.
    pub fn sample_gpu() -> Option<GpuSample> {
        None
    }
}

#[cfg(not(target_os = "macos"))]
mod platform {
    use super::GpuSample;

    pub fn sample_gpu() -> Option<GpuSample> {
        None
    }
}
