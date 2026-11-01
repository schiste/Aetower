//! Read ordinary string versions without a subprocess. Other inputs keep PlistBuddy.
use core_foundation_sys::{
    base::{CFGetTypeID, CFRange, CFRelease, CFTypeRef, kCFAllocatorDefault},
    data::CFDataCreate,
    dictionary::{CFDictionaryGetTypeID, CFDictionaryGetValue, CFDictionaryRef},
    propertylist::{CFPropertyListCreateWithData, kCFPropertyListImmutable},
    string::{
        CFStringCreateWithBytes, CFStringGetBytes, CFStringGetLength,
        CFStringGetMaximumSizeForEncoding, CFStringGetTypeID, CFStringRef, kCFStringEncodingUTF8,
    },
};
use std::{fs::File, io::Read, path::Path, ptr};

struct OwnedCf(CFTypeRef);
impl OwnedCf {
    fn new(value: CFTypeRef) -> Option<Self> {
        if value.is_null() {
            None
        } else {
            Some(Self(value))
        }
    }
}
impl Drop for OwnedCf {
    fn drop(&mut self) {
        unsafe { CFRelease(self.0) };
    }
}

pub(super) fn read(bundle: &str) -> Option<String> {
    // Bound both I/O and conversion. Unusual files/types use the existing CLI.
    let mut bytes = Vec::new();
    File::open(Path::new(bundle).join("Contents/Info.plist"))
        .ok()?
        .take(1_048_577)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() > 1_048_576 {
        return None;
    }
    let data = OwnedCf::new(
        unsafe {
            CFDataCreate(
                kCFAllocatorDefault,
                bytes.as_ptr(),
                bytes.len().try_into().ok()?,
            )
        }
        .cast(),
    )?;
    let mut error = ptr::null_mut();
    let plist = OwnedCf::new(unsafe {
        CFPropertyListCreateWithData(
            kCFAllocatorDefault,
            data.0.cast(),
            kCFPropertyListImmutable,
            ptr::null_mut(),
            &mut error,
        )
    });
    let _error = OwnedCf::new(error.cast());
    let plist = plist?;
    if unsafe { CFGetTypeID(plist.0) != CFDictionaryGetTypeID() } {
        return None;
    }
    let key_bytes = b"CFBundleShortVersionString";
    let key = OwnedCf::new(
        unsafe {
            CFStringCreateWithBytes(
                kCFAllocatorDefault,
                key_bytes.as_ptr(),
                key_bytes.len().try_into().ok()?,
                kCFStringEncodingUTF8,
                0,
            )
        }
        .cast(),
    )?;
    let value = unsafe { CFDictionaryGetValue(plist.0 as CFDictionaryRef, key.0) };
    if value.is_null() || unsafe { CFGetTypeID(value) != CFStringGetTypeID() } {
        return None;
    }
    let value = value as CFStringRef;
    let length = unsafe { CFStringGetLength(value) };
    let capacity = unsafe { CFStringGetMaximumSizeForEncoding(length, kCFStringEncodingUTF8) };
    if !(0..=65536).contains(&capacity) {
        return None;
    }
    let mut result = vec![0; capacity as usize];
    let mut used = 0;
    let converted = unsafe {
        CFStringGetBytes(
            value,
            CFRange {
                location: 0,
                length,
            },
            kCFStringEncodingUTF8,
            0,
            0,
            result.as_mut_ptr(),
            capacity,
            &mut used,
        )
    };
    if converted != length || used < 0 || used > capacity {
        return None;
    }
    result.truncate(used as usize);
    let value = String::from_utf8(result).ok()?;
    if value.contains('\0') {
        return None;
    }
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_owned())
}

#[cfg(test)]
mod tests {
    use super::super::{resolve_app_version, resolve_app_version_cli};
    use std::{
        fs,
        path::PathBuf,
        process::Command,
        sync::atomic::{AtomicU64, Ordering},
    };
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "aetower version {} {}.app",
                std::process::id(),
                SEQUENCE.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(path.join("Contents")).unwrap();
            Self(path)
        }
        fn write(&self, value: &str) {
            fs::write(self.0.join("Contents/Info.plist"), format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?><!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\"><plist version=\"1.0\"><dict>{value}</dict></plist>")).unwrap();
        }
        fn path(&self) -> &str {
            self.0.to_str().unwrap()
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    #[test]
    fn string_versions_match_plistbuddy_for_xml_binary_and_symlinks() {
        let fixture = Fixture::new();
        for value in ["1.2.3", "  2.0 β  \n", "版本 &amp; &lt;test&gt;", ""] {
            for binary in [false, true] {
                fixture.write(&format!(
                    "<key>CFBundleShortVersionString</key><string>{value}</string>"
                ));
                if binary {
                    assert!(
                        Command::new("/usr/bin/plutil")
                            .args(["-convert", "binary1"])
                            .arg(fixture.0.join("Contents/Info.plist"))
                            .status()
                            .unwrap()
                            .success()
                    );
                }
                let expected = resolve_app_version_cli(fixture.path());
                assert_eq!(resolve_app_version(fixture.path()), expected);
                if !value.is_empty() {
                    assert_eq!(super::read(fixture.path()), expected);
                }
            }
        }
        let alias = Fixture::new();
        fs::remove_dir_all(&alias.0).unwrap();
        std::os::unix::fs::symlink(&fixture.0, &alias.0).unwrap();
        assert_eq!(
            resolve_app_version(alias.path()),
            resolve_app_version_cli(alias.path())
        );
        fs::remove_file(&alias.0).unwrap();
    }
    #[test]
    fn unsupported_missing_and_malformed_versions_keep_cli_results() {
        let fixture = Fixture::new();
        for value in [
            "",
            "<key>CFBundleShortVersionString</key><integer>42</integer>",
            "<key>CFBundleShortVersionString</key><true/>",
            "<key>CFBundleShortVersionString</key><array><string>one</string></array>",
        ] {
            fixture.write(value);
            assert_eq!(super::read(fixture.path()), None);
            assert_eq!(
                resolve_app_version(fixture.path()),
                resolve_app_version_cli(fixture.path())
            );
        }
        for bytes in [b"".as_slice(), b"not a plist", b"bplist00truncated"] {
            fs::write(fixture.0.join("Contents/Info.plist"), bytes).unwrap();
            assert_eq!(super::read(fixture.path()), None);
            assert_eq!(
                resolve_app_version(fixture.path()),
                resolve_app_version_cli(fixture.path())
            );
        }
        fixture.write(&format!("<key>padding</key><string>{}</string><key>CFBundleShortVersionString</key><string>3.0</string>", "x".repeat(1_048_576)));
        assert_eq!(super::read(fixture.path()), None);
        assert_eq!(resolve_app_version(fixture.path()), Some("3.0".to_owned()));
        assert_eq!(
            resolve_app_version(fixture.path()),
            resolve_app_version_cli(fixture.path())
        );
        fs::remove_file(fixture.0.join("Contents/Info.plist")).unwrap();
        assert_eq!(
            resolve_app_version(fixture.path()),
            resolve_app_version_cli(fixture.path())
        );
    }
}
