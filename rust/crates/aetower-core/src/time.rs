use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const FAST_TICK: Duration = Duration::from_millis(2000);

pub fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_millis() as u64
}
