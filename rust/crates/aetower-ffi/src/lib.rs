use std::{
    ffi::{CStr, CString},
    os::raw::c_char,
};

use aetower_core::Engine;
use aetower_model::{CapabilityKind, CapabilityState};

#[no_mangle]
pub extern "C" fn aetower_engine_new() -> *mut Engine {
    Box::into_raw(Box::new(Engine::new()))
}

#[no_mangle]
pub unsafe extern "C" fn aetower_engine_start(handle: *mut Engine) {
    if let Some(engine) = handle.as_mut() {
        engine.start();
    }
}

#[no_mangle]
pub unsafe extern "C" fn aetower_engine_stop(handle: *mut Engine) {
    if let Some(engine) = handle.as_mut() {
        engine.stop();
    }
}

#[no_mangle]
pub unsafe extern "C" fn aetower_engine_free(handle: *mut Engine) {
    if !handle.is_null() {
        drop(Box::from_raw(handle));
    }
}

#[no_mangle]
pub unsafe extern "C" fn aetower_engine_get_snapshot_json(handle: *mut Engine) -> *mut c_char {
    if let Some(engine) = handle.as_ref() {
        return to_c_string(engine.latest_snapshot_json());
    }
    to_c_string("{}".to_owned())
}

#[no_mangle]
pub unsafe extern "C" fn aetower_engine_set_capability_state(
    handle: *mut Engine,
    kind: *const c_char,
    state: *const c_char,
    detail: *const c_char,
) {
    let Some(engine) = handle.as_ref() else {
        return;
    };
    let Some(kind) = parse_capability(kind) else {
        return;
    };
    let Some(state) = parse_capability_state(state) else {
        return;
    };
    let detail = if detail.is_null() {
        None
    } else {
        Some(CStr::from_ptr(detail).to_string_lossy().into_owned())
    };
    engine.set_capability_state(kind, state, detail);
}

#[no_mangle]
pub unsafe extern "C" fn aetower_engine_free_string(value: *mut c_char) {
    if !value.is_null() {
        drop(CString::from_raw(value));
    }
}

unsafe fn parse_capability(value: *const c_char) -> Option<CapabilityKind> {
    let raw = CStr::from_ptr(value).to_string_lossy();
    match raw.as_ref() {
        "accessibility" => Some(CapabilityKind::Accessibility),
        "full-disk-access" => Some(CapabilityKind::FullDiskAccess),
        "apple-automation" => Some(CapabilityKind::AppleAutomation),
        "chromium-debug" => Some(CapabilityKind::ChromiumDebug),
        "docker-socket" => Some(CapabilityKind::DockerSocket),
        _ => None,
    }
}

unsafe fn parse_capability_state(value: *const c_char) -> Option<CapabilityState> {
    let raw = CStr::from_ptr(value).to_string_lossy();
    match raw.as_ref() {
        "unknown" => Some(CapabilityState::Unknown),
        "granted" => Some(CapabilityState::Granted),
        "denied" => Some(CapabilityState::Denied),
        "requested" => Some(CapabilityState::Requested),
        "unavailable" => Some(CapabilityState::Unavailable),
        _ => None,
    }
}

fn to_c_string(value: String) -> *mut c_char {
    CString::new(value).unwrap_or_default().into_raw()
}
