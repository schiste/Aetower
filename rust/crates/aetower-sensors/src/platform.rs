use std::ffi::c_void;

use aetower_model::{FanReading, PowerReading, PowerUnit, TemperatureReading};

use super::SensorSample;

// ---------------------------------------------------------------------------
// IOKit FFI
// ---------------------------------------------------------------------------

type IoReturn = i32;
type IoConnect = u32;
type MachPort = u32;

const KERN_SUCCESS: IoReturn = 0;
const SMC_CMD_READ_KEYINFO: u8 = 9;
const SMC_CMD_READ_BYTES: u8 = 5;
const SMC_CMD_WRITE_BYTES: u8 = 6;
const MAX_PLAUSIBLE_FAN_RPM: f32 = 10_000.0;

#[repr(C)]
#[derive(Default, Clone)]
struct SmcKeyData {
    key: u32,
    vers: [u8; 6],
    p_limit_data: [u8; 16],
    key_info: SmcKeyDataKeyInfo,
    result: u8,
    status: u8,
    data8: u8,
    data32: u32,
    bytes: [u8; 32],
}

#[repr(C)]
#[derive(Default, Clone, Copy)]
struct SmcKeyDataKeyInfo {
    data_size: u32,
    data_type: u32,
    data_attributes: u8,
}

#[link(name = "IOKit", kind = "framework")]
unsafe extern "C" {
    fn IOServiceGetMatchingService(masterPort: MachPort, matching: *const c_void) -> u32;
    fn IOServiceMatching(name: *const u8) -> *const c_void;
    fn IOServiceOpen(
        service: u32,
        owningTask: u32,
        type_: u32,
        connect: *mut IoConnect,
    ) -> IoReturn;
    fn IOServiceClose(connect: IoConnect) -> IoReturn;
    fn IOConnectCallStructMethod(
        connection: IoConnect,
        selector: u32,
        input: *const c_void,
        input_size: usize,
        output: *mut c_void,
        output_size: *mut usize,
    ) -> IoReturn;
    fn mach_task_self() -> u32;
}

// ---------------------------------------------------------------------------
// SMC Connection
// ---------------------------------------------------------------------------

struct SmcConnection {
    conn: IoConnect,
}

impl SmcConnection {
    fn open() -> Option<Self> {
        unsafe {
            let matching = IOServiceMatching(c"AppleSMC".as_ptr().cast());
            if matching.is_null() {
                return None;
            }
            let service = IOServiceGetMatchingService(0, matching);
            if service == 0 {
                return None;
            }
            let mut conn: IoConnect = 0;
            let result = IOServiceOpen(service, mach_task_self(), 0, &mut conn);
            if result != KERN_SUCCESS {
                return None;
            }
            Some(Self { conn })
        }
    }

    #[allow(clippy::field_reassign_with_default)]
    fn read_key(&self, key: [u8; 4]) -> Option<SmcValue> {
        let key_u32 = u32::from_be_bytes(key);

        // Step 1: get key info (data size + type)
        let mut input = SmcKeyData::default();
        input.key = key_u32;
        input.data8 = SMC_CMD_READ_KEYINFO;

        let mut output = SmcKeyData::default();
        if !self.call_smc(&input, &mut output) {
            return None;
        }

        let data_size = output.key_info.data_size as usize;
        let data_type = output.key_info.data_type;
        if data_size == 0 || data_size > 32 {
            return None;
        }

        // Step 2: read bytes
        input.key_info.data_size = output.key_info.data_size;
        input.data8 = SMC_CMD_READ_BYTES;

        output = SmcKeyData::default();
        if !self.call_smc(&input, &mut output) {
            return None;
        }

        Some(SmcValue {
            data_type,
            data_size: data_size as u8,
            bytes: output.bytes,
        })
    }

    #[allow(clippy::field_reassign_with_default)]
    fn write_key(&self, key: [u8; 4], value: &SmcValue) -> bool {
        let key_u32 = u32::from_be_bytes(key);

        // Get key info first
        let mut input = SmcKeyData::default();
        input.key = key_u32;
        input.data8 = SMC_CMD_READ_KEYINFO;

        let mut output = SmcKeyData::default();
        if !self.call_smc(&input, &mut output) {
            return false;
        }

        // Write bytes
        input.data8 = SMC_CMD_WRITE_BYTES;
        input.key_info = output.key_info;
        input.bytes[..value.data_size as usize]
            .copy_from_slice(&value.bytes[..value.data_size as usize]);

        output = SmcKeyData::default();
        self.call_smc(&input, &mut output)
    }

    fn call_smc(&self, input: &SmcKeyData, output: &mut SmcKeyData) -> bool {
        unsafe {
            let mut output_size = std::mem::size_of::<SmcKeyData>();
            let result = IOConnectCallStructMethod(
                self.conn,
                2, // kSMCHandleYPCEvent
                input as *const SmcKeyData as *const c_void,
                std::mem::size_of::<SmcKeyData>(),
                output as *mut SmcKeyData as *mut c_void,
                &mut output_size,
            );
            result == KERN_SUCCESS
        }
    }
}

impl Drop for SmcConnection {
    fn drop(&mut self) {
        unsafe {
            IOServiceClose(self.conn);
        }
    }
}

// ---------------------------------------------------------------------------
// SMC Value parsing
// ---------------------------------------------------------------------------

struct SmcValue {
    data_type: u32,
    data_size: u8,
    bytes: [u8; 32],
}

impl SmcValue {
    fn as_f32(&self) -> Option<f32> {
        let type_bytes = self.data_type.to_be_bytes();
        match &type_bytes {
            b"fpe2" => Some(fpe2_to_f32([self.bytes[0], self.bytes[1]])),
            b"sp78" => Some(sp78_to_f32([self.bytes[0], self.bytes[1]])),
            b"flt " => {
                if self.data_size >= 4 {
                    Some(f32::from_be_bytes([
                        self.bytes[0],
                        self.bytes[1],
                        self.bytes[2],
                        self.bytes[3],
                    ]))
                } else {
                    None
                }
            }
            b"fp2e" => Some(fp2e_to_f32([self.bytes[0], self.bytes[1]])),
            b"sp96" => Some(sp96_to_f32([self.bytes[0], self.bytes[1]])),
            b"ui8 " => Some(self.bytes[0] as f32),
            b"ui16" => Some(u16::from_be_bytes([self.bytes[0], self.bytes[1]]) as f32),
            b"ui32" => Some(u32::from_be_bytes([
                self.bytes[0],
                self.bytes[1],
                self.bytes[2],
                self.bytes[3],
            ]) as f32),
            _ => None,
        }
    }

    fn as_string(&self) -> Option<String> {
        let len = self.data_size as usize;
        if len == 0 || len > 32 {
            return None;
        }
        let slice = &self.bytes[..len];
        let trimmed = slice
            .iter()
            .copied()
            .take_while(|&b| b != 0 && b.is_ascii())
            .collect::<Vec<u8>>();
        String::from_utf8(trimmed).ok()
    }
}

/// fpe2: unsigned 14.2 fixed-point (value / 4)
fn fpe2_to_f32(bytes: [u8; 2]) -> f32 {
    u16::from_be_bytes(bytes) as f32 / 4.0
}

/// sp78: signed 7.8 fixed-point (value / 256)
fn sp78_to_f32(bytes: [u8; 2]) -> f32 {
    i16::from_be_bytes(bytes) as f32 / 256.0
}

/// fp2e: unsigned 2.14 fixed-point (value / 16384)
fn fp2e_to_f32(bytes: [u8; 2]) -> f32 {
    u16::from_be_bytes(bytes) as f32 / 16384.0
}

/// sp96: signed 9.6 fixed-point (value / 64) — used for power (watts)
fn sp96_to_f32(bytes: [u8; 2]) -> f32 {
    i16::from_be_bytes(bytes) as f32 / 64.0
}

// ---------------------------------------------------------------------------
// SMC Key constants
// ---------------------------------------------------------------------------

fn fan_key(fan_id: u8, suffix: &[u8; 2]) -> [u8; 4] {
    [b'F', b'0' + fan_id, suffix[0], suffix[1]]
}

// Temperature keys to probe (label, key bytes)
const TEMP_KEYS_APPLE_SILICON: &[(&str, [u8; 4])] = &[
    ("CPU Performance", *b"Tp09"),
    ("CPU Efficiency", *b"Tp0T"),
    ("CPU P-Core 1", *b"Tp01"),
    ("CPU P-Core 2", *b"Tp02"),
    ("CPU P-Core 3", *b"Tp03"),
    ("CPU P-Core 4", *b"Tp04"),
    ("CPU E-Core 1", *b"Tp0h"),
    ("CPU E-Core 2", *b"Tp0j"),
    ("System", *b"Ts0P"),
    ("SSD", *b"Ts1S"),
];

const TEMP_KEYS_INTEL: &[(&str, [u8; 4])] = &[
    ("CPU Die", *b"TC0C"),
    ("CPU Core 1", *b"TC1C"),
    ("CPU Core 2", *b"TC2C"),
    ("CPU Core 3", *b"TC3C"),
    ("CPU Core 4", *b"TC4C"),
    ("CPU Core 5", *b"TC5C"),
    ("CPU Core 6", *b"TC6C"),
    ("CPU Core 7", *b"TC7C"),
    ("CPU Proximity", *b"TCXC"),
    ("GPU Die", *b"TG0D"),
];

// Power keys to probe
const POWER_KEYS_APPLE_SILICON: &[(&str, [u8; 4], PowerUnit)] = &[
    ("System Total", *b"PSTR", PowerUnit::Watts),
    ("CPU Package", *b"PCPT", PowerUnit::Watts),
    ("GPU Package", *b"PCPG", PowerUnit::Watts),
    ("Main Rail V", *b"VD0R", PowerUnit::Volts),
    ("Main Rail A", *b"ID0R", PowerUnit::Amps),
];

const POWER_KEYS_INTEL: &[(&str, [u8; 4], PowerUnit)] = &[
    ("System Total", *b"PSLP", PowerUnit::Watts),
    ("CPU Package", *b"PCPT", PowerUnit::Watts),
    ("CPU Voltage", *b"VC0C", PowerUnit::Volts),
    ("CPU Current", *b"IC0C", PowerUnit::Amps),
    ("GPU Power", *b"PCPG", PowerUnit::Watts),
];

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

pub fn sample_sensors() -> Option<SensorSample> {
    let smc = SmcConnection::open()?;

    let fans = read_fans(&smc);
    let cpu_temperatures = read_temperatures(&smc);
    let power_readings = read_power(&smc);

    Some(SensorSample {
        fans,
        cpu_temperatures,
        power_readings,
    })
}

/// Pin a fan to a manual minimum RPM.
///
/// Writes two SMC keys in sequence: `F{n}Md = 1` to switch the fan into
/// "forced" mode, then `F{n}Mn = rpm*4` to set the new minimum. If the
/// second write fails, the fan is left in forced mode at whatever `Mn`
/// was previously configured — possibly zero, meaning no cooling under
/// load. That is catastrophic for a user who tried to *increase* cooling
/// and now has the fan actively disabled.
///
/// The fix is a two-write rollback: on second-write failure, attempt to
/// write `F{n}Md = 0` to return the fan to automatic mode. Three outcomes:
///
/// - Happy path: both writes succeed, `Ok(())`.
/// - Second write fails, rollback succeeds: return an error noting that
///   the fan has been restored to automatic mode so the user knows the
///   machine is safe to continue using.
/// - Second write fails AND rollback fails: return a critical error
///   asking the user to reboot, because the fan may be stuck in forced
///   mode at the previous `Mn`. This is the worst case and the only path
///   where the user needs to take manual action.
pub fn set_fan_min_rpm(fan_id: u8, rpm: f32) -> Result<(), String> {
    let smc = SmcConnection::open().ok_or("failed to open SMC")?;

    // Set fan mode to forced (1)
    let mode_key = fan_key(fan_id, b"Md");
    let mode_val = SmcValue {
        data_type: u32::from_be_bytes(*b"ui8 "),
        data_size: 1,
        bytes: {
            let mut b = [0u8; 32];
            b[0] = 1;
            b
        },
    };
    if !smc.write_key(mode_key, &mode_val) {
        return Err("failed to set fan mode".to_owned());
    }

    // Set minimum RPM
    let min_key = fan_key(fan_id, b"Mn");
    let rpm_bytes = ((rpm * 4.0) as u16).to_be_bytes();
    let rpm_val = SmcValue {
        data_type: u32::from_be_bytes(*b"fpe2"),
        data_size: 2,
        bytes: {
            let mut b = [0u8; 32];
            b[0] = rpm_bytes[0];
            b[1] = rpm_bytes[1];
            b
        },
    };
    if !smc.write_key(min_key, &rpm_val) {
        // Rollback: return the fan to automatic mode so it isn't left
        // pinned to whatever `Mn` was before this call (possibly zero).
        let auto_mode_val = SmcValue {
            data_type: u32::from_be_bytes(*b"ui8 "),
            data_size: 1,
            bytes: [0u8; 32], // byte 0 is already 0 which means automatic
        };
        if smc.write_key(mode_key, &auto_mode_val) {
            return Err("failed to set fan RPM; fan restored to automatic mode".to_owned());
        }
        return Err(
            "failed to set fan RPM AND rollback failed; fan may be stuck \
             in forced mode — please reboot to restore automatic control"
                .to_owned(),
        );
    }

    Ok(())
}

pub fn reset_fan_auto(fan_id: u8) -> Result<(), String> {
    let smc = SmcConnection::open().ok_or("failed to open SMC")?;

    let mode_key = fan_key(fan_id, b"Md");
    let mode_val = SmcValue {
        data_type: u32::from_be_bytes(*b"ui8 "),
        data_size: 1,
        bytes: [0u8; 32], // mode 0 = auto
    };
    if !smc.write_key(mode_key, &mode_val) {
        return Err("failed to reset fan mode".to_owned());
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Internal readers
// ---------------------------------------------------------------------------

fn read_fans(smc: &SmcConnection) -> Vec<FanReading> {
    let fan_count = smc
        .read_key(*b"FNum")
        .and_then(|v| v.as_f32())
        .filter(|value| value.is_finite() && *value >= 0.0)
        .unwrap_or(0.0) as u8;

    (0..fan_count.min(8))
        .filter_map(|id| {
            let current = smc
                .read_key(fan_key(id, b"Ac"))?
                .as_f32()
                .and_then(plausible_fan_rpm)?;
            let min = smc
                .read_key(fan_key(id, b"Mn"))
                .and_then(|v| v.as_f32())
                .and_then(plausible_fan_rpm)
                .unwrap_or(0.0);
            let max = smc
                .read_key(fan_key(id, b"Mx"))
                .and_then(|v| v.as_f32())
                .and_then(plausible_fan_rpm)
                .unwrap_or(0.0);
            let name = smc
                .read_key(fan_key(id, b"ID"))
                .and_then(|v| v.as_string())
                .unwrap_or_else(|| format!("Fan {id}"));

            Some(FanReading {
                id,
                name,
                current_rpm: current,
                min_rpm: min,
                max_rpm: max,
            })
        })
        .collect()
}

fn plausible_fan_rpm(value: f32) -> Option<f32> {
    if value.is_finite() && (0.0..=MAX_PLAUSIBLE_FAN_RPM).contains(&value) {
        Some(value)
    } else {
        None
    }
}

fn read_temperatures(smc: &SmcConnection) -> Vec<TemperatureReading> {
    // Try Apple Silicon keys first, then Intel
    let mut temps = probe_keys(smc, TEMP_KEYS_APPLE_SILICON);
    if temps.is_empty() {
        temps = probe_keys(smc, TEMP_KEYS_INTEL);
    }
    temps
}

fn probe_keys(smc: &SmcConnection, keys: &[(&str, [u8; 4])]) -> Vec<TemperatureReading> {
    keys.iter()
        .filter_map(|(label, key)| {
            let celsius = smc.read_key(*key)?.as_f32()?;
            // Sanity: ignore nonsensical temperatures
            if !(-40.0..=150.0).contains(&celsius) {
                return None;
            }
            Some(TemperatureReading {
                label: label.to_string(),
                celsius,
            })
        })
        .collect()
}

fn read_power(smc: &SmcConnection) -> Vec<PowerReading> {
    let mut readings = probe_power_keys(smc, POWER_KEYS_APPLE_SILICON);
    if readings.is_empty() {
        readings = probe_power_keys(smc, POWER_KEYS_INTEL);
    }
    readings
}

fn probe_power_keys(smc: &SmcConnection, keys: &[(&str, [u8; 4], PowerUnit)]) -> Vec<PowerReading> {
    keys.iter()
        .filter_map(|(label, key, unit)| {
            let value = smc.read_key(*key)?.as_f32()?;
            if !(0.0..=500.0).contains(&value) {
                return None;
            }
            Some(PowerReading {
                label: label.to_string(),
                value,
                unit: unit.clone(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fpe2_conversion() {
        // 6000 RPM = 6000 * 4 = 24000 = 0x5DC0
        assert!((fpe2_to_f32([0x5D, 0xC0]) - 6000.0).abs() < 0.5);
        assert!((fpe2_to_f32([0x00, 0x00]) - 0.0).abs() < 0.01);
    }

    #[test]
    fn sp78_conversion() {
        // 25.0°C = 25 * 256 = 6400 = 0x1900
        assert!((sp78_to_f32([0x19, 0x00]) - 25.0).abs() < 0.01);
        // -10.0°C = -10 * 256 = -2560 = 0xF600
        assert!((sp78_to_f32([0xF6, 0x00]) - (-10.0)).abs() < 0.01);
    }
}
