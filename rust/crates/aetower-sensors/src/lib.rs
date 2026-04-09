pub use aetower_model::{FanReading, PowerReading, PowerUnit, TemperatureReading};

/// Combined sensor sample from SMC and other hardware interfaces.
#[derive(Debug, Clone, Default)]
pub struct SensorSample {
    pub fans: Vec<FanReading>,
    pub cpu_temperatures: Vec<TemperatureReading>,
    pub power_readings: Vec<PowerReading>,
}

/// Read all available hardware sensors (fans, temperatures, power).
/// Returns `None` if the platform has no SMC or sensor access fails.
pub fn sample_sensors() -> Option<SensorSample> {
    platform::sample_sensors()
}

/// Write a fan minimum RPM via SMC. Requires root privileges.
pub fn set_fan_min_rpm(fan_id: u8, rpm: f32) -> Result<(), String> {
    platform::set_fan_min_rpm(fan_id, rpm)
}

/// Reset a fan to automatic (OS-controlled) mode.
pub fn reset_fan_auto(fan_id: u8) -> Result<(), String> {
    platform::reset_fan_auto(fan_id)
}

#[cfg(target_os = "macos")]
mod platform;

#[cfg(not(target_os = "macos"))]
mod platform {
    use super::SensorSample;

    pub fn sample_sensors() -> Option<SensorSample> {
        None
    }

    pub fn set_fan_min_rpm(_fan_id: u8, _rpm: f32) -> Result<(), String> {
        Err("fan control not supported on this platform".to_owned())
    }

    pub fn reset_fan_auto(_fan_id: u8) -> Result<(), String> {
        Err("fan control not supported on this platform".to_owned())
    }
}
