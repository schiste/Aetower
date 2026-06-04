use serde::{Deserialize, Serialize};

pub const MEMORY_PRESSURE_WARNING_RATIO: f64 = 0.80;
pub const MEMORY_PRESSURE_CRITICAL_RATIO: f64 = 0.90;
pub const COMPRESSED_MEMORY_WARNING_BYTES: u64 = 4 * 1024 * 1024 * 1024;
pub const COMPRESSED_MEMORY_CRITICAL_BYTES: u64 = 6 * 1024 * 1024 * 1024;
pub const SWAP_WARNING_BYTES: u64 = 8 * 1024 * 1024 * 1024;
pub const SWAP_CRITICAL_BYTES: u64 = 16 * 1024 * 1024 * 1024;
pub const WAKEUPS_WARNING: f32 = 12_000.0;
pub const WAKEUPS_CRITICAL: f32 = 25_000.0;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "kebab-case")]
pub enum SeverityBand {
    Info,
    Warning,
    Critical,
}

impl SeverityBand {
    pub fn score(self) -> u8 {
        match self {
            Self::Info => 1,
            Self::Warning => 2,
            Self::Critical => 3,
        }
    }
}
