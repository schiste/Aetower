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
    use std::process::Command;

    use super::GpuSample;

    pub fn sample_gpu() -> Option<GpuSample> {
        let output = Command::new("ioreg")
            .args(["-l", "-r", "-c", "IOAccelerator"])
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        parse_ioreg_output(&String::from_utf8_lossy(&output.stdout))
    }

    fn parse_ioreg_output(output: &str) -> Option<GpuSample> {
        let stats_start = output.find("\"PerformanceStatistics\" = {")?;
        let stats = &output[stats_start..];
        let gpu_percent = extract_numeric_field(stats, "Device Utilization %")
            .or_else(|| extract_numeric_field(stats, "Renderer Utilization %"))
            .unwrap_or(0) as f32;
        let gpu_memory_bytes = extract_numeric_field(stats, "In use system memory")
            .or_else(|| extract_numeric_field(stats, "Alloc system memory"))
            .unwrap_or(0);
        let ane_percent = extract_numeric_field(output, "ane-device-utilization")
            .or_else(|| extract_numeric_field(output, "ANE Utilization %"))
            .unwrap_or(0) as f32;

        Some(GpuSample {
            gpu_percent,
            ane_percent,
            gpu_memory_bytes,
        })
    }

    fn extract_numeric_field(haystack: &str, key: &str) -> Option<u64> {
        let needle = format!("\"{key}\"=");
        let index = haystack.find(&needle)?;
        let mut digits = String::new();
        for ch in haystack[index + needle.len()..].chars() {
            if ch.is_ascii_digit() {
                digits.push(ch);
                continue;
            }
            if !digits.is_empty() {
                break;
            }
        }
        digits.parse().ok()
    }

    #[cfg(test)]
    mod tests {
        use super::parse_ioreg_output;

        #[test]
        fn parses_ioreg_performance_statistics() {
            let sample = r#"
            |   "PerformanceStatistics" = {"In use system memory"=681213952,"Renderer Utilization %"=14,"Device Utilization %"=18}
            "#;
            let parsed = parse_ioreg_output(sample).expect("gpu sample");
            assert_eq!(parsed.gpu_percent, 18.0);
            assert_eq!(parsed.gpu_memory_bytes, 681_213_952);
            assert_eq!(parsed.ane_percent, 0.0);
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod platform {
    use super::GpuSample;

    pub fn sample_gpu() -> Option<GpuSample> {
        None
    }
}
