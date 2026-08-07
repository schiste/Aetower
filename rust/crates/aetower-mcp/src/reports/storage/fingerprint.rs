use std::os::unix::fs::MetadataExt;

use super::*;

pub(super) const STORAGE_PATH_FINGERPRINT_VERSION: u8 = 1;
const STORAGE_PATH_FINGERPRINT_LEN: usize = 70;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct StoragePathFingerprint {
    pub(super) kind_code: u8,
    pub(super) mode: u32,
    pub(super) device: u64,
    pub(super) inode: u64,
    pub(super) logical_bytes: u64,
    pub(super) physical_bytes: u64,
    pub(super) modified_millis: u64,
    pub(super) changed_millis: u64,
    pub(super) link_count: u64,
    hash: u64,
}

impl StoragePathFingerprint {
    pub(super) fn from_metadata(metadata: &fs::Metadata) -> Self {
        let kind_code = if metadata.file_type().is_symlink() {
            3
        } else if metadata.is_dir() {
            2
        } else if metadata.is_file() {
            1
        } else {
            0
        };
        let modified_millis = metadata_time_millis(metadata.mtime(), metadata.mtime_nsec());
        let changed_millis = metadata_time_millis(metadata.ctime(), metadata.ctime_nsec());
        let mut fingerprint = Self {
            kind_code,
            mode: metadata.mode(),
            device: metadata.dev(),
            inode: metadata.ino(),
            logical_bytes: metadata.len(),
            physical_bytes: metadata.blocks().saturating_mul(512),
            modified_millis,
            changed_millis,
            link_count: metadata.nlink(),
            hash: 0,
        };
        fingerprint.hash = fingerprint.compute_hash();
        fingerprint
    }

    pub(super) fn encode(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(STORAGE_PATH_FINGERPRINT_LEN);
        bytes.push(STORAGE_PATH_FINGERPRINT_VERSION);
        bytes.push(self.kind_code);
        bytes.extend_from_slice(&self.mode.to_le_bytes());
        for value in [
            self.device,
            self.inode,
            self.logical_bytes,
            self.physical_bytes,
            self.modified_millis,
            self.changed_millis,
            self.link_count,
            self.hash,
        ] {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        bytes
    }

    #[cfg(test)]
    pub(super) fn decode(bytes: &[u8]) -> Option<Self> {
        if bytes.len() != STORAGE_PATH_FINGERPRINT_LEN
            || bytes.first().copied()? != STORAGE_PATH_FINGERPRINT_VERSION
        {
            return None;
        }
        let kind_code = bytes[1];
        let mode = read_u32_le(bytes, 2)?;
        let mut offset = 6;
        let device = read_u64_le(bytes, offset)?;
        offset += 8;
        let inode = read_u64_le(bytes, offset)?;
        offset += 8;
        let logical_bytes = read_u64_le(bytes, offset)?;
        offset += 8;
        let physical_bytes = read_u64_le(bytes, offset)?;
        offset += 8;
        let modified_millis = read_u64_le(bytes, offset)?;
        offset += 8;
        let changed_millis = read_u64_le(bytes, offset)?;
        offset += 8;
        let link_count = read_u64_le(bytes, offset)?;
        offset += 8;
        let hash = read_u64_le(bytes, offset)?;
        let fingerprint = Self {
            kind_code,
            mode,
            device,
            inode,
            logical_bytes,
            physical_bytes,
            modified_millis,
            changed_millis,
            link_count,
            hash,
        };
        (fingerprint.compute_hash() == hash).then_some(fingerprint)
    }

    #[cfg(test)]
    pub(super) fn stable_hash(&self) -> u64 {
        self.hash
    }

    #[cfg(test)]
    pub(super) fn changed_since(&self, previous: &Self) -> bool {
        self.hash != previous.hash
    }

    fn compute_hash(&self) -> u64 {
        let mut hash = FNV_OFFSET_BASIS;
        mix_u8(&mut hash, STORAGE_PATH_FINGERPRINT_VERSION);
        mix_u8(&mut hash, self.kind_code);
        mix_u32(&mut hash, self.mode);
        for value in [
            self.device,
            self.inode,
            self.logical_bytes,
            self.physical_bytes,
            self.modified_millis,
            self.changed_millis,
            self.link_count,
        ] {
            mix_u64(&mut hash, value);
        }
        hash
    }
}

fn metadata_time_millis(seconds: i64, nanoseconds: i64) -> u64 {
    seconds
        .saturating_mul(1000)
        .saturating_add(nanoseconds.max(0) / 1_000_000)
        .max(0) as u64
}

const FNV_OFFSET_BASIS: u64 = 0xcbf29ce484222325;
const FNV_PRIME: u64 = 0x100000001b3;

fn mix_u8(hash: &mut u64, value: u8) {
    *hash ^= value as u64;
    *hash = hash.wrapping_mul(FNV_PRIME);
}

fn mix_u32(hash: &mut u64, value: u32) {
    for byte in value.to_le_bytes() {
        mix_u8(hash, byte);
    }
}

fn mix_u64(hash: &mut u64, value: u64) {
    for byte in value.to_le_bytes() {
        mix_u8(hash, byte);
    }
}

#[cfg(test)]
fn read_u32_le(bytes: &[u8], offset: usize) -> Option<u32> {
    let slice: [u8; 4] = bytes.get(offset..offset + 4)?.try_into().ok()?;
    Some(u32::from_le_bytes(slice))
}

#[cfg(test)]
fn read_u64_le(bytes: &[u8], offset: usize) -> Option<u64> {
    let slice: [u8; 8] = bytes.get(offset..offset + 8)?.try_into().ok()?;
    Some(u64::from_le_bytes(slice))
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;

    fn temp_test_dir(name: &str) -> PathBuf {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let id = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "aetower-storage-fingerprint-{}-{name}-{id}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("create test directory");
        path
    }

    #[test]
    fn storage_path_fingerprint_round_trips_binary_encoding() {
        let root = temp_test_dir("roundtrip");
        let file = root.join("artifact.bin");
        fs::write(&file, b"baseline").expect("write fixture");
        let metadata = fs::symlink_metadata(&file).expect("metadata");

        let fingerprint = StoragePathFingerprint::from_metadata(&metadata);
        let encoded = fingerprint.encode();
        let decoded = StoragePathFingerprint::decode(&encoded).expect("decode");

        assert_eq!(decoded, fingerprint);
        assert_eq!(decoded.stable_hash(), fingerprint.stable_hash());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn storage_path_fingerprint_changes_when_file_content_changes() {
        let root = temp_test_dir("changes");
        let file = root.join("artifact.bin");
        fs::write(&file, b"baseline").expect("write baseline");
        let baseline = StoragePathFingerprint::from_metadata(
            &fs::symlink_metadata(&file).expect("baseline metadata"),
        );

        fs::write(&file, b"baseline plus more bytes").expect("write changed");
        let changed = StoragePathFingerprint::from_metadata(
            &fs::symlink_metadata(&file).expect("changed metadata"),
        );

        assert!(changed.changed_since(&baseline));
        assert_ne!(changed.stable_hash(), baseline.stable_hash());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn storage_path_fingerprint_rejects_corrupt_encoding() {
        let root = temp_test_dir("corrupt");
        let file = root.join("artifact.bin");
        fs::write(&file, b"baseline").expect("write fixture");
        let metadata = fs::symlink_metadata(&file).expect("metadata");
        let mut encoded = StoragePathFingerprint::from_metadata(&metadata).encode();
        let last = encoded.len() - 1;
        encoded[last] ^= 0xff;

        assert!(StoragePathFingerprint::decode(&encoded).is_none());
        let _ = fs::remove_dir_all(root);
    }
}
