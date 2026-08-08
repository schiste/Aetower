use super::*;

pub(super) const STORAGE_LEDGER_FSEVENTS_SOURCE: &str = "aetower-fsevents";
pub(super) const STORAGE_NATIVE_FSEVENTS_SOURCE: &str = "aetower-native-fsevents";

const NATIVE_FSEVENTS_POLL_MILLIS: u64 = 25;
const NATIVE_FSEVENTS_MAX_EVENTS: usize = 1024;

const FSEVENT_FLAG_MUST_SCAN_SUBDIRS: u64 = 0x0000_0001;
const FSEVENT_FLAG_USER_DROPPED: u64 = 0x0000_0002;
const FSEVENT_FLAG_KERNEL_DROPPED: u64 = 0x0000_0004;
const FSEVENT_FLAG_EVENT_IDS_WRAPPED: u64 = 0x0000_0008;

#[derive(Clone, Debug, Default)]
pub(super) struct NativeStorageEventBatch {
    pub(super) records: Vec<StorageFilesystemEventRecord>,
    pub(super) cursor: Option<u64>,
    pub(super) unknown_gap_roots: BTreeSet<String>,
    pub(super) status: String,
    pub(super) detail: String,
}

pub(super) fn storage_event_flags_indicate_unknown_gap(flags: u64) -> bool {
    flags
        & (FSEVENT_FLAG_MUST_SCAN_SUBDIRS
            | FSEVENT_FLAG_USER_DROPPED
            | FSEVENT_FLAG_KERNEL_DROPPED
            | FSEVENT_FLAG_EVENT_IDS_WRAPPED)
        != 0
}

pub(super) fn poll_native_storage_filesystem_events(
    roots: &[PathBuf],
    since_event_id: Option<u64>,
    now_millis: u64,
) -> NativeStorageEventBatch {
    native::poll(roots, since_event_id, now_millis)
}

#[cfg(target_os = "macos")]
mod native {
    use std::{
        ffi::{CStr, CString},
        os::raw::{c_char, c_double, c_long, c_void},
    };

    use super::*;

    type CFAllocatorRef = *const c_void;
    type CFArrayRef = *const c_void;
    type CFIndex = c_long;
    type CFRunLoopMode = *const c_void;
    type CFRunLoopRef = *mut c_void;
    type CFStringEncoding = u32;
    type CFStringRef = *const c_void;
    type CFTimeInterval = c_double;
    type FSEventStreamEventFlags = u32;
    type FSEventStreamEventId = u64;
    type FSEventStreamRef = *mut c_void;

    const K_CF_STRING_ENCODING_UTF8: CFStringEncoding = 0x0800_0100;
    const K_FSEVENT_STREAM_CREATE_FLAG_FILE_EVENTS: u32 = 0x0000_0010;
    const K_CF_RUN_LOOP_RUN_TIMED_OUT: i32 = 3;

    #[repr(C)]
    struct FSEventStreamContext {
        version: CFIndex,
        info: *mut c_void,
        retain: Option<unsafe extern "C" fn(*const c_void) -> *const c_void>,
        release: Option<unsafe extern "C" fn(*const c_void)>,
        copy_description: Option<unsafe extern "C" fn(*const c_void) -> CFStringRef>,
    }

    type FSEventStreamCallback = unsafe extern "C" fn(
        FSEventStreamRef,
        *mut c_void,
        usize,
        *mut c_void,
        *const FSEventStreamEventFlags,
        *const FSEventStreamEventId,
    );

    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        static kCFRunLoopDefaultMode: CFRunLoopMode;

        fn CFArrayCreate(
            allocator: CFAllocatorRef,
            values: *const *const c_void,
            num_values: CFIndex,
            callbacks: *const c_void,
        ) -> CFArrayRef;
        fn CFRelease(cf: *const c_void);
        fn CFRunLoopGetCurrent() -> CFRunLoopRef;
        fn CFRunLoopRunInMode(
            mode: CFRunLoopMode,
            seconds: CFTimeInterval,
            return_after_source_handled: u8,
        ) -> i32;
        fn CFStringCreateWithCString(
            allocator: CFAllocatorRef,
            c_str: *const c_char,
            encoding: CFStringEncoding,
        ) -> CFStringRef;
    }

    #[link(name = "CoreServices", kind = "framework")]
    unsafe extern "C" {
        fn FSEventStreamCreate(
            allocator: CFAllocatorRef,
            callback: FSEventStreamCallback,
            context: *mut FSEventStreamContext,
            paths_to_watch: CFArrayRef,
            since_when: FSEventStreamEventId,
            latency: CFTimeInterval,
            flags: u32,
        ) -> FSEventStreamRef;
        fn FSEventStreamInvalidate(stream_ref: FSEventStreamRef);
        fn FSEventStreamRelease(stream_ref: FSEventStreamRef);
        fn FSEventStreamScheduleWithRunLoop(
            stream_ref: FSEventStreamRef,
            run_loop: CFRunLoopRef,
            run_loop_mode: CFRunLoopMode,
        );
        fn FSEventStreamStart(stream_ref: FSEventStreamRef) -> u8;
        fn FSEventStreamStop(stream_ref: FSEventStreamRef);
        fn FSEventsGetCurrentEventId() -> FSEventStreamEventId;
    }

    struct CallbackState {
        roots: Vec<String>,
        now_millis: u64,
        records: Vec<StorageFilesystemEventRecord>,
        unknown_gap_roots: BTreeSet<String>,
        latest_event_id: Option<u64>,
        truncated: bool,
    }

    pub(super) fn poll(
        roots: &[PathBuf],
        since_event_id: Option<u64>,
        now_millis: u64,
    ) -> NativeStorageEventBatch {
        if roots.is_empty() {
            return NativeStorageEventBatch {
                status: "native_fsevents_no_roots".to_owned(),
                detail: "no roots requested".to_owned(),
                ..NativeStorageEventBatch::default()
            };
        }
        let roots = roots
            .iter()
            .map(|root| root.display().to_string())
            .filter(|root| !root.trim().is_empty())
            .collect::<Vec<_>>();
        if roots.is_empty() {
            return NativeStorageEventBatch {
                status: "native_fsevents_no_roots".to_owned(),
                detail: "no non-empty roots requested".to_owned(),
                ..NativeStorageEventBatch::default()
            };
        }
        let current_event_id = unsafe { FSEventsGetCurrentEventId() };
        let Some(since_event_id) = since_event_id else {
            return NativeStorageEventBatch {
                cursor: Some(current_event_id),
                status: "native_fsevents_cursor_seeded".to_owned(),
                detail: "seeded native FSEvents cursor; future calls ingest from this point"
                    .to_owned(),
                ..NativeStorageEventBatch::default()
            };
        };

        let Some(paths_array) = create_cf_paths_array(&roots) else {
            return NativeStorageEventBatch {
                cursor: Some(since_event_id),
                status: "native_fsevents_paths_failed".to_owned(),
                detail: "could not create FSEvents path array".to_owned(),
                ..NativeStorageEventBatch::default()
            };
        };
        let mut state = CallbackState {
            roots: roots.clone(),
            now_millis,
            records: Vec::new(),
            unknown_gap_roots: BTreeSet::new(),
            latest_event_id: None,
            truncated: false,
        };
        let mut context = FSEventStreamContext {
            version: 0,
            info: (&mut state as *mut CallbackState).cast::<c_void>(),
            retain: None,
            release: None,
            copy_description: None,
        };
        let stream = unsafe {
            FSEventStreamCreate(
                std::ptr::null(),
                fsevents_callback,
                &mut context,
                paths_array.array,
                since_event_id as FSEventStreamEventId,
                NATIVE_FSEVENTS_POLL_MILLIS as CFTimeInterval / 1000.0,
                K_FSEVENT_STREAM_CREATE_FLAG_FILE_EVENTS,
            )
        };
        if stream.is_null() {
            return NativeStorageEventBatch {
                cursor: Some(since_event_id),
                status: "native_fsevents_stream_failed".to_owned(),
                detail: "CoreServices refused to create FSEvents stream".to_owned(),
                ..NativeStorageEventBatch::default()
            };
        }

        unsafe {
            FSEventStreamScheduleWithRunLoop(stream, CFRunLoopGetCurrent(), kCFRunLoopDefaultMode);
        }
        let started = unsafe { FSEventStreamStart(stream) } != 0;
        if started {
            let deadline = Instant::now() + Duration::from_millis(NATIVE_FSEVENTS_POLL_MILLIS);
            while Instant::now() < deadline && state.records.len() < NATIVE_FSEVENTS_MAX_EVENTS {
                let remaining = deadline.saturating_duration_since(Instant::now());
                let result = unsafe {
                    CFRunLoopRunInMode(kCFRunLoopDefaultMode, remaining.as_secs_f64(), 1)
                };
                if result == K_CF_RUN_LOOP_RUN_TIMED_OUT {
                    break;
                }
            }
        }
        unsafe {
            FSEventStreamStop(stream);
            FSEventStreamInvalidate(stream);
            FSEventStreamRelease(stream);
        }

        let cursor = if started {
            state.latest_event_id.or(Some(current_event_id))
        } else {
            Some(since_event_id)
        };
        let status = if !started {
            "native_fsevents_start_failed"
        } else if state.truncated {
            "native_fsevents_backlog"
        } else if state.records.is_empty() {
            "native_fsevents_idle"
        } else if state.unknown_gap_roots.is_empty() {
            "native_fsevents_ingested"
        } else {
            "native_fsevents_unknown_gap"
        };
        NativeStorageEventBatch {
            detail: if state.truncated {
                format!(
                    "native FSEvents poll read {} event(s) for {} root(s); backlog remains",
                    state.records.len(),
                    roots.len()
                )
            } else {
                format!(
                    "native FSEvents poll read {} event(s) for {} root(s)",
                    state.records.len(),
                    roots.len()
                )
            },
            status: status.to_owned(),
            records: state.records,
            unknown_gap_roots: state.unknown_gap_roots,
            cursor,
        }
    }

    unsafe extern "C" fn fsevents_callback(
        _stream: FSEventStreamRef,
        client_info: *mut c_void,
        event_count: usize,
        event_paths: *mut c_void,
        event_flags: *const FSEventStreamEventFlags,
        event_ids: *const FSEventStreamEventId,
    ) {
        if client_info.is_null()
            || event_paths.is_null()
            || event_flags.is_null()
            || event_ids.is_null()
        {
            return;
        }
        let state = unsafe { &mut *(client_info.cast::<CallbackState>()) };
        let paths = event_paths.cast::<*const c_char>();
        for index in 0..event_count {
            if state.records.len() >= NATIVE_FSEVENTS_MAX_EVENTS {
                state.truncated = true;
                break;
            }
            let path_ptr = unsafe { *paths.add(index) };
            if path_ptr.is_null() {
                continue;
            }
            let path = unsafe { CStr::from_ptr(path_ptr) }
                .to_string_lossy()
                .into_owned();
            if path.trim().is_empty() {
                continue;
            }
            let flags = unsafe { *event_flags.add(index) } as u64;
            let event_id = unsafe { *event_ids.add(index) };
            state.latest_event_id = Some(state.latest_event_id.unwrap_or(0).max(event_id));
            if storage_event_flags_indicate_unknown_gap(flags) {
                mark_unknown_gap_roots_for_path(&path, &state.roots, &mut state.unknown_gap_roots);
            }
            state.records.push(StorageFilesystemEventRecord {
                timestamp_millis: Some(state.now_millis),
                path: Some(path),
                event_id: Some(event_id),
                flags: Some(flags),
                source: Some(STORAGE_NATIVE_FSEVENTS_SOURCE.to_owned()),
            });
        }
    }

    fn mark_unknown_gap_roots_for_path(
        path: &str,
        roots: &[String],
        unknown_gap_roots: &mut BTreeSet<String>,
    ) {
        if roots.is_empty() {
            unknown_gap_roots.insert(path.to_owned());
            return;
        }
        let path = Path::new(path);
        for root in roots {
            let root_path = Path::new(root);
            if path_is_under_root(&path.display().to_string(), root_path)
                || path_is_under_root(root, path)
            {
                unknown_gap_roots.insert(root.clone());
            }
        }
    }

    struct CFPathArray {
        array: CFArrayRef,
        strings: Vec<CFStringRef>,
    }

    impl Drop for CFPathArray {
        fn drop(&mut self) {
            unsafe {
                CFRelease(self.array);
                for string in &self.strings {
                    CFRelease(*string);
                }
            }
        }
    }

    fn create_cf_paths_array(roots: &[String]) -> Option<CFPathArray> {
        let mut strings = Vec::with_capacity(roots.len());
        let mut values = Vec::with_capacity(roots.len());
        for root in roots {
            let c_root = CString::new(root.as_str()).ok()?;
            let string = unsafe {
                CFStringCreateWithCString(
                    std::ptr::null(),
                    c_root.as_ptr(),
                    K_CF_STRING_ENCODING_UTF8,
                )
            };
            if string.is_null() {
                return None;
            }
            values.push(string.cast::<c_void>());
            strings.push(string);
        }
        let array = unsafe {
            CFArrayCreate(
                std::ptr::null(),
                values.as_ptr(),
                values.len() as CFIndex,
                std::ptr::null(),
            )
        };
        if array.is_null() {
            return None;
        }
        Some(CFPathArray { array, strings })
    }
}

#[cfg(not(target_os = "macos"))]
mod native {
    use super::*;

    pub(super) fn poll(
        _roots: &[PathBuf],
        _since_event_id: Option<u64>,
        _now_millis: u64,
    ) -> NativeStorageEventBatch {
        NativeStorageEventBatch {
            status: "native_fsevents_unsupported".to_owned(),
            detail: "native FSEvents ingestion is available on macOS only".to_owned(),
            ..NativeStorageEventBatch::default()
        }
    }
}
